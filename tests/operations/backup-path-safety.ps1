$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$root = Join-Path ([IO.Path]::GetTempPath()) "kp-path-safety-$([Guid]::NewGuid().ToString('N'))"
$bundle = Join-Path $root 'bundle'
$target = Join-Path $root 'target'
function Invoke-RejectedRestore([string]$Expected, [string]$Source = $bundle) {
    $output = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName kp-restore-0123456789ab -SourceDirectory $Source -TargetDirectory $target 2>&1
    if ($LASTEXITCODE -eq 0 -or "$output" -notmatch $Expected) {
        throw "unsafe restore input was not rejected with '$Expected': $output"
    }
}

try {
    New-Item -ItemType Directory -Path (Join-Path $bundle 'postgres'),(Join-Path $bundle 'metadata'),$target | Out-Null
    [IO.File]::WriteAllText((Join-Path $root 'escape'), "outside`n")
    [IO.File]::WriteAllText((Join-Path $bundle 'postgres\database.sql'), "placeholder`n")
    [IO.File]::WriteAllText((Join-Path $bundle 'metadata\object-index.jsonl'), "")
    [IO.File]::WriteAllText((Join-Path $bundle 'bundle-version.txt'), "knowledge-platform-backup-v1`n")
    $hash = (Get-FileHash -LiteralPath (Join-Path $root 'escape') -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.sha256'), "$hash  ../escape`n", [Text.UTF8Encoding]::new($false))
    Invoke-RejectedRestore 'backup manifest is invalid'

    $dumpHash = (Get-FileHash -LiteralPath (Join-Path $bundle 'postgres\database.sql') -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.sha256'), "$dumpHash  postgres/database.sql`n$dumpHash  postgres/database.sql`n", [Text.UTF8Encoding]::new($false))
    Invoke-RejectedRestore 'backup manifest is invalid'

    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.sha256'), "$dumpHash  postgres/database.sql`n", [Text.UTF8Encoding]::new($false))
    Invoke-RejectedRestore 'backup manifest is invalid'

    $versionHash = (Get-FileHash -LiteralPath (Join-Path $bundle 'bundle-version.txt') -Algorithm SHA256).Hash.ToLowerInvariant()
    $metadataHash = (Get-FileHash -LiteralPath (Join-Path $bundle 'metadata\object-index.jsonl') -Algorithm SHA256).Hash.ToLowerInvariant()
    $unsorted = "$dumpHash  postgres/database.sql`n$versionHash  bundle-version.txt`n$metadataHash  metadata/object-index.jsonl`n"
    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.sha256'), $unsorted, [Text.UTF8Encoding]::new($false))
    Invoke-RejectedRestore 'backup manifest is invalid'

    $junction = Join-Path $root 'repo-junction'
    New-Item -ItemType Junction -Path $junction -Target $repoRoot | Out-Null
    Invoke-RejectedRestore 'reparse point|unsafe root directory' $junction
    $backupOutput = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\backup.ps1') -ProjectName kp-path-safety -TargetDirectory $junction 2>&1
    if ($LASTEXITCODE -eq 0 -or "$backupOutput" -notmatch 'reparse point|unsafe root directory') {
        throw "backup target junction was not rejected at the path boundary: $backupOutput"
    }
    Write-Output 'backup path safety passed'
} finally {
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
