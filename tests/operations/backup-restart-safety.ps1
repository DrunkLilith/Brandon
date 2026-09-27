$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$root = Join-Path ([IO.Path]::GetTempPath()) "kp-backup-restart-$([Guid]::NewGuid().ToString('N'))"
$bin = Join-Path $root 'bin'
$composeFile = Join-Path $root 'compose.yaml'

try {
    New-Item -ItemType Directory -Path $bin | Out-Null
    [IO.File]::WriteAllText($composeFile, "services: {}`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $bin 'docker.ps1'), @'
$joined = ' ' + ($args -join ' ') + ' '
if ($joined -like '* ps -q postgres *') { 'postgres-id'; exit 0 }
if ($joined -like '* ps -q minio *') { 'minio-id'; exit 0 }
if ($joined -like '* ps -q backend *') {
    if ($env:FAKE_DOCKER_MODE -eq 'state-query-fail') { [Console]::Error.WriteLine('BACKEND_STATE_QUERY_FAILURE'); exit 42 }
    'backend-id'; exit 0
}
if ($joined -like '* stop backend *') { exit 0 }
if ($joined -like '* start backend *') {
    if ($env:FAKE_DOCKER_MODE -eq 'start-fail') { [Console]::Error.WriteLine('simulated backend start failure'); exit 1 }
    if ($env:FAKE_DOCKER_MODE -eq 'primary-and-restart-fail') { [Console]::Error.WriteLine('SECONDARY_RESTART_FAILURE'); exit 1 }
    exit 0
}
if ($joined -like '* run *') {
    if ($env:FAKE_DOCKER_MODE -eq 'primary-and-restart-fail' -and $joined -like '* postgres *') {
        [Console]::Error.WriteLine('PRIMARY_BACKUP_FAILURE'); exit 17
    }
    $mount = $null
    for ($index = 0; $index -lt $args.Count - 1; $index++) {
        if ($args[$index] -eq '--volume') { $mount = $args[$index + 1]; break }
    }
    if ($mount -notmatch '^(.+):/backup$') { [Console]::Error.WriteLine('missing backup mount'); exit 1 }
    $hostPath = $Matches[1]
    if ($joined -like '* postgres *') {
        New-Item -ItemType Directory -Force -Path $hostPath | Out-Null
        [IO.File]::WriteAllText((Join-Path $hostPath 'database.sql'), "database dump`n")
    } else {
        New-Item -ItemType Directory -Force -Path (Join-Path $hostPath 'objects'),(Join-Path $hostPath 'metadata') | Out-Null
        [IO.File]::WriteAllText((Join-Path $hostPath 'objects\object'), "ciphertext`n")
        [IO.File]::WriteAllText((Join-Path $hostPath 'metadata\object-index.jsonl'), "{}`n")
    }
    exit 0
}
if ($args.Count -gt 0 -and $args[0] -eq 'inspect') { 'unhealthy'; exit 0 }
exit 0
'@, [Text.UTF8Encoding]::new($false))

    $oldPath = $env:PATH
    $env:PATH = "$bin$([IO.Path]::PathSeparator)$oldPath"
    foreach ($case in @(
        @{ Mode = 'start-fail'; Expected = 'backend failed to restart after backup' },
        @{ Mode = 'readiness-fail'; Expected = 'backend failed to return ready after backup' }
    )) {
        $target = Join-Path $root $case.Mode
        New-Item -ItemType Directory -Path $target | Out-Null
        $env:FAKE_DOCKER_MODE = $case.Mode
        $arguments = @('-NoProfile','-File',(Join-Path $repoRoot 'scripts\backup.ps1'),'-ProjectName','kp-restart-test','-TargetDirectory',$target,'-ComposeFile',$composeFile)
        if ($case.Mode -ne 'start-fail') { $arguments += @('-RestartTimeoutSeconds','1') }
        $output = & pwsh @arguments 2>&1
        if ($LASTEXITCODE -eq 0 -or "$output" -notmatch [Regex]::Escape($case.Expected) -or "$output" -match 'backup completed:') {
            throw "backup did not report $($case.Mode) truthfully: status=$LASTEXITCODE output=$output"
        }
    }

    $target = Join-Path $root 'primary-and-restart-fail'
    New-Item -ItemType Directory -Path $target | Out-Null
    $env:FAKE_DOCKER_MODE = 'primary-and-restart-fail'
    $output = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\backup.ps1') -ProjectName kp-restart-test -TargetDirectory $target -ComposeFile $composeFile 2>&1
    if ($LASTEXITCODE -eq 0 -or "$output" -notmatch 'PRIMARY_BACKUP_FAILURE' -or "$output" -notmatch 'docker compose failed: run' -or "$output" -notmatch 'SECONDARY_RESTART_FAILURE' -or "$output" -match 'backup completed:') {
        throw "backup did not preserve primary and recovery failures: status=$LASTEXITCODE output=$output"
    }

    $target = Join-Path $root 'state-query-fail'
    New-Item -ItemType Directory -Path $target | Out-Null
    $env:FAKE_DOCKER_MODE = 'state-query-fail'
    $output = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\backup.ps1') -ProjectName kp-restart-test -TargetDirectory $target -ComposeFile $composeFile 2>&1
    if ($LASTEXITCODE -eq 0 -or "$output" -notmatch 'backend state could not be determined' -or "$output" -match 'BACKEND_STATE_QUERY_FAILURE|backup completed:' -or (Test-Path -LiteralPath (Join-Path $target 'postgres\database.sql')) -or (Test-Path -LiteralPath (Join-Path $target 'objects\object'))) {
        throw "backup did not fail closed on backend state query failure: status=$LASTEXITCODE output=$output"
    }
    Write-Output 'backup restart safety passed'
} finally {
    $env:PATH = $oldPath
    Remove-Item Env:FAKE_DOCKER_MODE -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
