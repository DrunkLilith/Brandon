param(
    [Parameter(Mandatory)][string]$ProjectName,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [string]$ComposeFile
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($ComposeFile)) { $ComposeFile = Join-Path $repoRoot 'deploy\compose.yaml' }
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

function Invoke-Compose {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    & docker compose -f $ComposeFile -p $ProjectName @Arguments
    if ($LASTEXITCODE -ne 0) { throw "docker compose failed: $($Arguments -join ' ')" }
}

$postgres = (& docker compose -f $ComposeFile -p $ProjectName ps -q postgres).Trim()
$minio = (& docker compose -f $ComposeFile -p $ProjectName ps -q minio).Trim()
if (-not $postgres -or -not $minio) { throw 'source project PostgreSQL and MinIO must be running' }

New-Item -ItemType Directory -Path (Join-Path $target 'postgres'),(Join-Path $target 'objects'),(Join-Path $target 'metadata') | Out-Null
[IO.File]::WriteAllText((Join-Path $target 'bundle-version.txt'), "knowledge-platform-backup-v1`n", [Text.UTF8Encoding]::new($false))
$backendWasRunning = -not [string]::IsNullOrWhiteSpace((& docker compose -f $ComposeFile -p $ProjectName ps -q backend))
try {
    if ($backendWasRunning) { Invoke-Compose stop backend | Out-Null }
    $postgresMount = "$(Join-Path $target 'postgres'):/backup"
    Invoke-Compose run --rm -T --volume $postgresMount --entrypoint /bin/sh postgres -ec 'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec pg_dump --host=postgres --username=knowledge --dbname=knowledge --format=plain --no-owner --no-privileges --no-comments --restrict-key=KnowledgePlatformBackupV1 --file=/backup/database.sql'
    $bundleMount = "${target}:/backup"
    Invoke-Compose run --rm -T --volume $bundleMount --entrypoint /bin/sh minio-init -ec @'
access=$(cat /run/secrets/object_access_key)
secret=$(cat /run/secrets/object_secret_key)
mc alias set --quiet local http://minio:9000 "$access" "$secret"
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
} finally {
    if ($backendWasRunning) { & docker compose -f $ComposeFile -p $ProjectName start backend | Out-Null }
}
Write-Output "backup completed: project=$ProjectName target=$target"
