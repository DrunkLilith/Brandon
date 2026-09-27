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
$collisionVolume = "kp-custom-collision-caddy-config-$suffix"
$collisionPostgresVolume = "kp-custom-collision-postgres-$suffix"
$collisionMinioVolume = "kp-custom-collision-minio-$suffix"
$collisionCaddyDataVolume = "kp-custom-collision-caddy-data-$suffix"
$cleanupPostgresVolume = "kp-custom-cleanup-postgres-$cleanupSuffix"
$cleanupMinioVolume = "kp-custom-cleanup-minio-$cleanupSuffix"
$cleanupCaddyDataVolume = "kp-custom-cleanup-caddy-data-$cleanupSuffix"
$cleanupCaddyConfigVolume = "kp-custom-cleanup-caddy-config-$cleanupSuffix"
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
    [IO.File]::WriteAllText($collisionCompose, @"
services:
  postgres:
    image: alpine:3.22.1
    command: ["sh", "-c", "printf touched > /config/marker; sleep 300"]
    healthcheck:
      test: ["CMD", "false"]
      interval: 1s
      timeout: 1s
      retries: 1
    volumes:
      - postgres_data:/data
      - caddy_config:/config
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
      - caddy_data:/caddy
volumes:
  postgres_data:
    name: kp-custom-collision-postgres-$suffix
  minio_data:
    name: kp-custom-collision-minio-$suffix
  caddy_data:
    name: kp-custom-collision-caddy-data-$suffix
  caddy_config:
    name: kp-custom-collision-caddy-config-$suffix
"@, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($cleanupCompose, @"
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
      - caddy_config:/config
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
      - caddy_data:/caddy
volumes:
  postgres_data:
    name: kp-custom-cleanup-postgres-$cleanupSuffix
  minio_data:
    name: kp-custom-cleanup-minio-$cleanupSuffix
  caddy_data:
    name: kp-custom-cleanup-caddy-data-$cleanupSuffix
  caddy_config:
    name: kp-custom-cleanup-caddy-config-$cleanupSuffix
"@, [Text.UTF8Encoding]::new($false))

    & docker volume create $collisionVolume | Out-Null
    & docker run --rm --volume "${collisionVolume}:/marker-volume" alpine:3.22.1 sh -c 'printf original > /marker-volume/marker' | Out-Null
    $collisionOutput = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName $collisionProject -SourceDirectory $bundle -TargetDirectory $collisionTarget -ComposeFile $collisionCompose -StorageTimeoutSeconds 1 2>&1
    if ($LASTEXITCODE -eq 0 -or "$collisionOutput" -notmatch 'restore project already has volume') {
        throw "exact-name collision was not rejected before restore: $collisionOutput"
    }
    & docker volume inspect $collisionVolume | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'restore deleted a pre-existing exact-name volume' }
    $marker = & docker run --rm --volume "${collisionVolume}:/marker-volume:ro" alpine:3.22.1 cat /marker-volume/marker
    if ($LASTEXITCODE -ne 0 -or "$marker" -ne 'original') { throw 'restore wrote to a pre-existing exact-name volume before rejection' }

    & docker volume create $preservedVolume | Out-Null
    $cleanupOutput = & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName $cleanupProject -SourceDirectory $bundle -TargetDirectory $cleanupTarget -ComposeFile $cleanupCompose -StorageTimeoutSeconds 1 2>&1
    if ($LASTEXITCODE -eq 0 -or "$cleanupOutput" -notmatch 'restore storage did not become healthy') {
        throw "controlled restore failure did not occur: $cleanupOutput"
    }
    & docker volume inspect $preservedVolume | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'restore cleanup deleted an unrelated volume' }
    foreach ($volume in $cleanupPostgresVolume,$cleanupMinioVolume,$cleanupCaddyDataVolume,$cleanupCaddyConfigVolume) {
        & docker volume inspect $volume *> $null
        if ($LASTEXITCODE -eq 0) { throw "restore cleanup retained its owned volume: $volume" }
    }
    Write-Output 'restore volume ownership passed'
} finally {
    & docker compose -f $collisionCompose -p $collisionProject down --remove-orphans *> $null
    & docker compose -f $cleanupCompose -p $cleanupProject down --remove-orphans *> $null
    & docker volume rm $collisionVolume $collisionPostgresVolume $collisionMinioVolume $collisionCaddyDataVolume $preservedVolume $cleanupPostgresVolume $cleanupMinioVolume $cleanupCaddyDataVolume $cleanupCaddyConfigVolume *> $null
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
