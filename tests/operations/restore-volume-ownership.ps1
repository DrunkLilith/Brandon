$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$suffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
$cleanupSuffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
$collisionProject = "kp-restore-$suffix"
$cleanupProject = "kp-restore-$cleanupSuffix"
$root = Join-Path ([IO.Path]::GetTempPath()) "kp-volume-ownership-$suffix"
$bundle = Join-Path $root 'bundle'
$collisionTarget = Join-Path $root 'collision-target'
$cleanupTarget = Join-Path $root 'cleanup-target'
$collisionCompose = Join-Path $root 'collision-compose.yaml'
$cleanupCompose = Join-Path $root 'cleanup-compose.yaml'
$collisionVolume = "${collisionProject}_postgres_data"
$preservedVolume = "kp-preserved-$suffix"

try {
    New-Item -ItemType Directory -Path (Join-Path $bundle 'postgres'),(Join-Path $bundle 'objects'),(Join-Path $bundle 'metadata'),$collisionTarget,$cleanupTarget | Out-Null
    [IO.File]::WriteAllText((Join-Path $bundle 'postgres\database.sql'), "placeholder`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $bundle 'metadata\object-index.jsonl'), '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $bundle 'bundle-version.txt'), "knowledge-platform-backup-v1`n", [Text.UTF8Encoding]::new($false))
    $lines = foreach ($relative in 'bundle-version.txt','metadata/object-index.jsonl','postgres/database.sql') {
        $path = Join-Path $bundle ($relative.Replace('/', '\'))
        "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $relative"
    }
    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.sha256'), (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($collisionCompose, @'
services:
  postgres:
    image: alpine:3.22.1
    network_mode: "container:kp-volume-owner-missing"
    volumes:
      - postgres_data:/data
  minio:
    image: alpine:3.22.1
    network_mode: "container:kp-volume-owner-missing"
    volumes:
      - minio_data:/data
volumes:
  postgres_data:
  minio_data:
'@, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($cleanupCompose, @'
services:
  postgres:
    image: alpine:3.22.1
    command: ["sh", "-c", "sleep 300"]
    healthcheck:
      test: ["CMD", "false"]
      interval: 1s
      timeout: 1s
      retries: 1
    volumes:
      - postgres_data:/data
  minio:
    image: alpine:3.22.1
    command: ["sh", "-c", "sleep 300"]
    healthcheck:
      test: ["CMD", "false"]
      interval: 1s
      timeout: 1s
      retries: 1
    volumes:
      - minio_data:/data
volumes:
  postgres_data:
  minio_data:
'@, [Text.UTF8Encoding]::new($false))

    & docker volume create $collisionVolume | Out-Null
    $collisionOutput = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName $collisionProject -SourceDirectory $bundle -TargetDirectory $collisionTarget -ComposeFile $collisionCompose 2>&1
    if ($LASTEXITCODE -eq 0 -or "$collisionOutput" -notmatch 'restore project already has volume') {
        throw "exact-name collision was not rejected before restore: $collisionOutput"
    }
    & docker volume inspect $collisionVolume | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'restore deleted a pre-existing exact-name volume' }

    & docker volume create $preservedVolume | Out-Null
    $cleanupOutput = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName $cleanupProject -SourceDirectory $bundle -TargetDirectory $cleanupTarget -ComposeFile $cleanupCompose -StorageTimeoutSeconds 1 2>&1
    if ($LASTEXITCODE -eq 0 -or "$cleanupOutput" -notmatch 'restore storage did not become healthy') {
        throw "controlled restore failure did not occur: $cleanupOutput"
    }
    & docker volume inspect $preservedVolume | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'restore cleanup deleted an unrelated volume' }
    foreach ($volume in "${cleanupProject}_postgres_data","${cleanupProject}_minio_data") {
        & docker volume inspect $volume *> $null
        if ($LASTEXITCODE -eq 0) { throw "restore cleanup retained its owned volume: $volume" }
    }
    Write-Output 'restore volume ownership passed'
} finally {
    & docker compose -f $collisionCompose -p $collisionProject down --remove-orphans *> $null
    & docker compose -f $cleanupCompose -p $cleanupProject down --remove-orphans *> $null
    & docker volume rm $collisionVolume $preservedVolume "${cleanupProject}_postgres_data" "${cleanupProject}_minio_data" *> $null
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
