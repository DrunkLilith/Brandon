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
if ($joined -like '* ps -q backend *') { 'backend-id'; exit 0 }
if ($joined -like '* stop backend *') { exit 0 }
if ($joined -like '* start backend *') {
    if ($env:FAKE_DOCKER_MODE -eq 'start-fail') { [Console]::Error.WriteLine('simulated backend start failure'); exit 1 }
    exit 0
}
if ($joined -like '* run *') {
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
    Write-Output 'backup restart safety passed'
} finally {
    $env:PATH = $oldPath
    Remove-Item Env:FAKE_DOCKER_MODE -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
