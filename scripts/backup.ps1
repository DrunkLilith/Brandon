param(
    [Parameter(Mandatory)][string]$ProjectName,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [string]$ComposeFile,
    [ValidateRange(1, 3600)][int]$RestartTimeoutSeconds = 120
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($ComposeFile)) { $ComposeFile = Join-Path $repoRoot 'deploy/compose.yaml' }
$ComposeFile = [IO.Path]::GetFullPath($ComposeFile)

if ($ProjectName -notmatch '^[a-z0-9][a-z0-9_-]{2,62}$') { throw 'project must be an explicit safe Compose project name' }
if (-not [IO.Path]::IsPathFullyQualified($TargetDirectory) -or $TargetDirectory.IndexOfAny([char[]]'*?[]') -ge 0) {
    throw 'backup target must be an absolute path without glob characters'
}
if (-not (Test-Path -LiteralPath $TargetDirectory -PathType Container)) { throw 'backup target must be an existing empty directory' }
$cursor = [IO.Path]::GetFullPath($TargetDirectory)
while ($null -ne $cursor) {
    $item = Get-Item -LiteralPath $cursor -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'backup target must not contain a filesystem reparse point' }
    $parent = [IO.Directory]::GetParent($cursor)
    if ($null -eq $parent) { break }
    $cursor = $parent.FullName
}
$target = (Resolve-Path -LiteralPath $TargetDirectory).Path.TrimEnd([IO.Path]::DirectorySeparatorChar)
$root = [IO.Path]::GetPathRoot($target).TrimEnd([IO.Path]::DirectorySeparatorChar)
$userProfile = [IO.Path]::GetFullPath([Environment]::GetFolderPath('UserProfile')).TrimEnd([IO.Path]::DirectorySeparatorChar)
if ($target -eq $root -or $target -eq $userProfile -or $target -eq $repoRoot.TrimEnd([IO.Path]::DirectorySeparatorChar)) { throw 'backup target is an unsafe root directory' }
if (Get-ChildItem -LiteralPath $target -Force | Select-Object -First 1) { throw 'backup target must be empty' }
if (-not (Test-Path -LiteralPath $ComposeFile -PathType Leaf)) { throw "Compose file not found: $ComposeFile" }
if ($IsLinux) {
    & chmod 700 -- $target
    if ($LASTEXITCODE -ne 0) { throw 'could not protect backup target permissions' }
}

function Invoke-Compose {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    $previousUid = $env:KNOWLEDGE_MAINTENANCE_UID
    $previousGid = $env:KNOWLEDGE_MAINTENANCE_GID
    try {
        if ($IsLinux -and $Arguments -contains '--user') {
            $env:KNOWLEDGE_MAINTENANCE_UID = $hostUid
            $env:KNOWLEDGE_MAINTENANCE_GID = $hostGid
        }
        & docker compose -f $ComposeFile -p $ProjectName @Arguments
        if ($LASTEXITCODE -ne 0) { throw "docker compose failed: $($Arguments -join ' ')" }
    } finally {
        $env:KNOWLEDGE_MAINTENANCE_UID = $previousUid
        $env:KNOWLEDGE_MAINTENANCE_GID = $previousGid
    }
}
$nativeRun = @()
if ($IsLinux) {
    $hostUid = (& id -u).Trim(); $hostGid = (& id -g).Trim()
    if ($LASTEXITCODE -ne 0 -or $hostUid -notmatch '^\d+$' -or $hostGid -notmatch '^\d+$') { throw 'could not resolve invoking Linux UID/GID' }
    $nativeRun = @('--user', "${hostUid}:${hostGid}")
}

$postgres = (& docker compose -f $ComposeFile -p $ProjectName ps -q postgres).Trim()
$minio = (& docker compose -f $ComposeFile -p $ProjectName ps -q minio).Trim()
if (-not $postgres -or -not $minio) { throw 'source project PostgreSQL and MinIO must be running' }

New-Item -ItemType Directory -Path (Join-Path $target 'postgres'),(Join-Path $target 'objects'),(Join-Path $target 'metadata') | Out-Null
[IO.File]::WriteAllText((Join-Path $target 'bundle-version.txt'), "knowledge-platform-backup-v1`n", [Text.UTF8Encoding]::new($false))
$previousConsoleError = [Console]::Error
try {
    [Console]::SetError([IO.TextWriter]::Null)
    $backendContainer = (& docker compose -f $ComposeFile -p $ProjectName ps -q backend 2>$null)
    $backendStateExitCode = $LASTEXITCODE
} finally {
    [Console]::SetError($previousConsoleError)
}
if ($backendStateExitCode -ne 0) { throw 'backend state could not be determined' }
$backendWasRunning = -not [string]::IsNullOrWhiteSpace($backendContainer)
function Restore-BackendAfterBackup {
    if (-not $backendWasRunning) { return }
    & docker compose -f $ComposeFile -p $ProjectName start backend | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'backend failed to restart after backup' }
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($RestartTimeoutSeconds)
    do {
        $containerId = (& docker compose -f $ComposeFile -p $ProjectName ps -q backend).Trim()
        if ($containerId) {
            $status = (& docker inspect -f '{{.State.Health.Status}}' $containerId 2>$null).Trim()
            if ($LASTEXITCODE -eq 0 -and $status -eq 'healthy') { return }
        }
        Start-Sleep -Seconds 2
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    throw 'backend failed to return ready after backup'
}
$primaryFailure = $null
$recoveryFailure = $null
try {
    if ($backendWasRunning) { Invoke-Compose stop backend | Out-Null }
    $postgresMount = "$(Join-Path $target 'postgres'):/backup"
    Invoke-Compose run --rm -T @nativeRun --volume $postgresMount --entrypoint /bin/sh postgres -ec 'umask 077; export PGPASSWORD="$(cat /run/secrets/database_password)"; exec pg_dump --host=postgres --username=knowledge --dbname=knowledge --format=plain --no-owner --no-privileges --no-comments --restrict-key=KnowledgePlatformBackupV1 --file=/backup/database.sql'
    $bundleMount = "${target}:/backup"
    Invoke-Compose run --rm -T @nativeRun --volume $bundleMount --entrypoint /bin/sh minio-init -ec @'
umask 077
mc-credentials http://minio:9000 | mc alias import local >/dev/null
mc mirror --overwrite local/knowledge /backup/objects
mc ls --recursive --json local/knowledge | LC_ALL=C sort > /backup/metadata/object-index.jsonl
'@

    $entries = @{}
    Get-ChildItem -LiteralPath $target -Recurse -File | Where-Object Name -ne 'manifest.sha256' | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($target, $_.FullName).Replace('\','/')
        $entries[$relative] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    [string[]]$paths = $entries.Keys
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    [string[]]$lines = $paths | ForEach-Object { "$($entries[$_])  $_" }
    [IO.File]::WriteAllText((Join-Path $target 'manifest.sha256'), (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
    if ($IsLinux) {
        & chmod -R 'go-rwx' -- $target
        if ($LASTEXITCODE -ne 0) { throw 'could not protect backup bundle permissions' }
    }
} catch {
    $primaryFailure = $_
} finally {
    try {
        Restore-BackendAfterBackup
    } catch {
        $recoveryFailure = $_
    }
}
if ($null -ne $primaryFailure) {
    if ($null -ne $recoveryFailure) {
        [Console]::Error.WriteLine("backend recovery after failed backup also failed: $($recoveryFailure.Exception.Message)")
    }
    throw $primaryFailure
}
if ($null -ne $recoveryFailure) { throw $recoveryFailure }
Write-Output "backup completed: project=$ProjectName target=$target"
