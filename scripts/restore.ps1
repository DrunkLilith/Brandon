param(
    [Parameter(Mandatory)][string]$ProjectName,
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$TargetDirectory,
    [string]$ComposeFile,
    [ValidateRange(1, 3600)][int]$StorageTimeoutSeconds = 120
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($ComposeFile)) { $ComposeFile = Join-Path $repoRoot 'deploy/compose.yaml' }
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
if ($IsLinux) {
    & chmod 700 -- $target
    if ($LASTEXITCODE -ne 0) { throw 'could not protect restore target permissions' }
}
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
$nativeRun = @()
if ($IsLinux) {
    $hostUid = (& id -u).Trim(); $hostGid = (& id -g).Trim()
    if ($LASTEXITCODE -ne 0 -or $hostUid -notmatch '^\d+$' -or $hostGid -notmatch '^\d+$') { throw 'could not resolve invoking Linux UID/GID' }
    $nativeRun = @('--user', "${hostUid}:${hostGid}", '--tmpfs', "/tmp:rw,noexec,nosuid,size=8m,mode=0700,uid=$hostUid,gid=$hostGid")
}
if (-not [string]::IsNullOrWhiteSpace((& docker compose -f $ComposeFile -p $ProjectName ps -aq))) { throw 'restore project already has containers' }
$configLines = @(& docker compose -f $ComposeFile -p $ProjectName config --format json 2>$null)
if ($LASTEXITCODE -ne 0 -or $configLines.Count -eq 0) { throw 'restore Compose volume configuration is invalid' }
try {
    $config = ($configLines -join "`n") | ConvertFrom-Json
} catch {
    throw 'restore Compose volume configuration is invalid'
}
$volumeProperties = @($config.volumes.PSObject.Properties)
if ($null -eq $config.volumes -or $volumeProperties.Count -eq 0) { throw 'restore Compose file declares no volumes' }
$resolvedNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$expectedVolumes = foreach ($property in $volumeProperties) {
    $key = $property.Name
    $name = $property.Value.name
    if ($key -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_.-]*$' -or $name -isnot [string] -or $name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_.-]*$' -or -not $resolvedNames.Add($name)) {
        throw 'restore Compose volume configuration is invalid'
    }
    [pscustomobject]@{ Key = $key; Name = $name }
}
foreach ($volume in $expectedVolumes) {
    & docker volume inspect $volume.Name *> $null
    if ($LASTEXITCODE -eq 0) { throw "restore project already has volume: $($volume.Name)" }
}
[IO.File]::WriteAllText((Join-Path $target 'restore-state.json'), "{`"project`":`"$ProjectName`",`"source`":`"$($source.Replace('\','/'))`"}`n", [Text.UTF8Encoding]::new($false))

function Get-OwnedRestoreVolumes {
    $owned = [Collections.Generic.List[string]]::new()
    foreach ($volume in $expectedVolumes) {
        $labels = & docker volume inspect --format '{{ index .Labels "com.docker.compose.project" }}|{{ index .Labels "com.docker.compose.volume" }}' $volume.Name 2>$null
        if ($LASTEXITCODE -eq 0) {
            if ($labels.Trim() -ne "$ProjectName|$($volume.Key)") { throw 'restore volume ownership could not be established' }
            $owned.Add($volume.Name)
        }
    }
    return $owned.ToArray()
}

$completed = $false
try {
    Invoke-Compose up --detach postgres minio
    $null = Get-OwnedRestoreVolumes
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($StorageTimeoutSeconds)
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
    Invoke-Compose run --rm -T @nativeRun --volume $postgresMount --entrypoint /bin/sh postgres -ec 'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec psql --no-psqlrc --set=ON_ERROR_STOP=1 --host=postgres --username=knowledge --dbname=knowledge --file=/backup/database.sql'
    $objectsMount = "$(Join-Path $source 'objects'):/backup-objects"
    Invoke-Compose run --rm -T @nativeRun --volume $objectsMount --entrypoint /bin/sh minio-init -ec @'
mc-credentials http://minio:9000 | mc alias import local >/dev/null
mc mirror --overwrite /backup-objects local/knowledge
'@
    $completed = $true
} finally {
    if (-not $completed) {
        & docker compose -f $ComposeFile -p $ProjectName down --remove-orphans | Out-Null
        $ownedVolumes = @(Get-OwnedRestoreVolumes)
        if ($ownedVolumes.Count -gt 0) {
            & docker volume rm @ownedVolumes | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'restore could not remove its owned volumes after failure' }
        }
    }
}
Write-Output "restore completed: project=$ProjectName target=$target"
