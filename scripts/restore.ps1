param(
    [Parameter(Mandatory)][string]$ProjectName,
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [string]$ComposeFile
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($ComposeFile)) { $ComposeFile = Join-Path $repoRoot 'deploy\compose.yaml' }
$ComposeFile = [IO.Path]::GetFullPath($ComposeFile)
if ($ProjectName -notmatch '^kp-restore-[a-f0-9]{12}$') { throw 'restore project must match kp-restore-<12 hex>' }

function Assert-NoReparsePoint([string]$Label, [string]$Value) {
    $cursor = [IO.Path]::GetFullPath($Value)
    while ($null -ne $cursor) {
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label must not contain a filesystem reparse point"
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
}

function Resolve-SafeDirectory([string]$Label, [string]$Value) {
    if (-not [IO.Path]::IsPathFullyQualified($Value) -or $Value.IndexOfAny([char[]]'*?[]') -ge 0 -or -not (Test-Path -LiteralPath $Value -PathType Container)) {
        throw "$Label must be an existing absolute directory without glob characters"
    }
    Assert-NoReparsePoint $Label $Value
    $resolved = (Resolve-Path -LiteralPath $Value).Path.TrimEnd([IO.Path]::DirectorySeparatorChar)
    $root = [IO.Path]::GetPathRoot($resolved).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $userProfile = [IO.Path]::GetFullPath([Environment]::GetFolderPath('UserProfile')).TrimEnd([IO.Path]::DirectorySeparatorChar)
    if ($resolved -eq $root -or $resolved -eq $userProfile -or $resolved -eq $repoRoot.TrimEnd([IO.Path]::DirectorySeparatorChar)) { throw "$Label is an unsafe root directory" }
    return $resolved
}

$source = Resolve-SafeDirectory 'backup source' $SourceDirectory
$target = Resolve-SafeDirectory 'restore target' $TargetDirectory
if ($source -eq $target) { throw 'backup source and restore target must differ' }
if (Get-ChildItem -LiteralPath $target -Force | Select-Object -First 1) { throw 'restore target must be empty' }
if (-not (Test-Path -LiteralPath (Join-Path $source 'manifest.sha256') -PathType Leaf)) { throw 'backup bundle is incomplete' }

$listed = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$sourcePrefix = $source + [IO.Path]::DirectorySeparatorChar
$previousRelative = $null
foreach ($line in [IO.File]::ReadAllLines((Join-Path $source 'manifest.sha256'))) {
    if ($line -notmatch '^([a-f0-9]{64})  (.+)$') { throw 'backup manifest is invalid' }
    $expectedHash = $Matches[1]
    $relative = $Matches[2]
    if ([IO.Path]::IsPathFullyQualified($relative) -or $relative.Contains('\') -or $relative.Contains(':') -or $relative -match '[\x00-\x1f\x7f]' -or $relative.IndexOfAny([char[]]'*?[]') -ge 0) {
        throw 'backup manifest is invalid'
    }
    $segments = $relative.Split('/')
    if ($segments.Count -eq 0 -or ($segments | Where-Object { $_ -eq '' -or $_ -eq '.' -or $_ -eq '..' })) { throw 'backup manifest is invalid' }
    if ($null -ne $previousRelative -and [StringComparer]::Ordinal.Compare($previousRelative, $relative) -ge 0) { throw 'backup manifest is invalid' }
    $previousRelative = $relative
    if (-not $listed.Add($relative) -or $relative -eq 'manifest.sha256') { throw 'backup manifest is invalid' }
    $path = [IO.Path]::GetFullPath([IO.Path]::Combine($source, $relative.Replace('/', [IO.Path]::DirectorySeparatorChar)))
    if (-not $path.StartsWith($sourcePrefix, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'backup manifest is invalid' }
    Assert-NoReparsePoint 'backup manifest entry' $path
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedHash) { throw 'backup manifest verification failed' }
}

$actual = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$pending = [Collections.Generic.Stack[string]]::new()
$pending.Push($source)
while ($pending.Count -gt 0) {
    $directory = $pending.Pop()
    foreach ($item in Get-ChildItem -LiteralPath $directory -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'backup manifest is invalid' }
        if ($item.PSIsContainer) {
            $pending.Push($item.FullName)
        } else {
            $relative = [IO.Path]::GetRelativePath($source, $item.FullName).Replace('\','/')
            if ($relative -ne 'manifest.sha256' -and -not $actual.Add($relative)) { throw 'backup manifest is invalid' }
        }
    }
}
foreach ($required in 'bundle-version.txt','postgres/database.sql','metadata/object-index.jsonl') {
    if (-not $actual.Contains($required)) { throw 'backup bundle is incomplete' }
}
if (-not $actual.SetEquals($listed)) { throw 'backup manifest is invalid' }
if ([IO.File]::ReadAllText((Join-Path $source 'bundle-version.txt')) -ne "knowledge-platform-backup-v1`n") {
    throw 'unsupported backup bundle version'
}

function Invoke-Compose {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    & docker compose -f $ComposeFile -p $ProjectName @Arguments
    if ($LASTEXITCODE -ne 0) { throw "docker compose failed: $($Arguments -join ' ')" }
}
if (-not [string]::IsNullOrWhiteSpace((& docker compose -f $ComposeFile -p $ProjectName ps -aq))) { throw 'restore project already has containers' }
if (-not [string]::IsNullOrWhiteSpace((& docker volume ls --quiet --filter "label=com.docker.compose.project=$ProjectName"))) { throw 'restore project already has volumes' }
[IO.File]::WriteAllText((Join-Path $target 'restore-state.json'), "{`"project`":`"$ProjectName`",`"source`":`"$($source.Replace('\','/'))`"}`n", [Text.UTF8Encoding]::new($false))

$completed = $false
try {
    Invoke-Compose up --detach postgres minio
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(120)
    do {
        $postgresId = (& docker compose -f $ComposeFile -p $ProjectName ps -q postgres).Trim()
        $minioId = (& docker compose -f $ComposeFile -p $ProjectName ps -q minio).Trim()
        if ($postgresId -and $minioId) {
            $postgresHealth = (& docker inspect -f '{{.State.Health.Status}}' $postgresId).Trim()
            $minioHealth = (& docker inspect -f '{{.State.Health.Status}}' $minioId).Trim()
            if ($postgresHealth -eq 'healthy' -and $minioHealth -eq 'healthy') { break }
        }
        Start-Sleep -Seconds 2
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    if ($postgresHealth -ne 'healthy' -or $minioHealth -ne 'healthy') { throw 'restore storage did not become healthy' }
    Invoke-Compose run --rm -T minio-init
    $postgresMount = "$(Join-Path $source 'postgres'):/backup"
    Invoke-Compose run --rm -T --volume $postgresMount --entrypoint /bin/sh postgres -ec 'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec psql --no-psqlrc --set=ON_ERROR_STOP=1 --host=postgres --username=knowledge --dbname=knowledge --file=/backup/database.sql'
    $objectsMount = "$(Join-Path $source 'objects'):/backup-objects"
    Invoke-Compose run --rm -T --volume $objectsMount --entrypoint /bin/sh minio-init -ec @'
access=$(cat /run/secrets/object_access_key)
secret=$(cat /run/secrets/object_secret_key)
mc alias set --quiet local http://minio:9000 "$access" "$secret"
mc mirror --overwrite /backup-objects local/knowledge
'@
    $completed = $true
} finally {
    if (-not $completed) { & docker compose -f $ComposeFile -p $ProjectName down --remove-orphans --volumes | Out-Null }
}
Write-Output "restore completed: project=$ProjectName target=$target"
