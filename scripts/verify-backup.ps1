param(
    [switch]$TestMode,
    [int]$TimeoutSeconds = 420
)

$ErrorActionPreference = 'Stop'
if (-not $TestMode) { throw 'verify-backup requires -TestMode; production restore targets must be operated explicitly' }
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$composeFile = Join-Path $repoRoot 'deploy\compose.yaml'
$suffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
$sourceProject = "kp-verify-$suffix"
$restoreProject = "kp-restore-$suffix"
$testRoot = Join-Path ([IO.Path]::GetTempPath()) $sourceProject
$secretDirectory = Join-Path $testRoot 'secrets'
$backupPowerShell = Join-Path $testRoot 'backup-powershell'
$backupBash = Join-Path $testRoot 'backup-bash'
$restoreTarget = Join-Path $testRoot 'restore-target'
$markerFile = Join-Path $testRoot 'marker.txt'
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$httpPort = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
$listener.Stop()
$publicOrigin = "http://localhost:$httpPort"

function Invoke-Compose([string]$Project, [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments) {
    & docker compose -f $composeFile -p $Project @Arguments
    if ($LASTEXITCODE -ne 0) { throw "docker compose failed for $Project`: $($Arguments -join ' ')" }
}

function Wait-Storage([string]$Project) {
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $postgresId = (& docker compose -f $composeFile -p $Project ps -q postgres).Trim()
        $minioId = (& docker compose -f $composeFile -p $Project ps -q minio).Trim()
        if ($postgresId -and $minioId) {
            $postgresHealth = (& docker inspect -f '{{.State.Health.Status}}' $postgresId).Trim()
            $minioHealth = (& docker inspect -f '{{.State.Health.Status}}' $minioId).Trim()
            if ($postgresHealth -eq 'healthy' -and $minioHealth -eq 'healthy') { return }
        }
        Start-Sleep -Seconds 2
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    throw "storage did not become healthy for $Project"
}

function Wait-PublicReady {
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $ready = Invoke-RestMethod -Uri "$publicOrigin/api/v1/health/ready" -TimeoutSec 5
            if ($ready.status -eq 'ready') { return }
        } catch {}
        Start-Sleep -Seconds 2
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    throw 'public API did not become ready'
}

function Mutation-Headers([string]$Cookie, [string]$Csrf) {
    return @{
        Cookie = $Cookie
        'X-CSRF-Token' = $Csrf
        Origin = $publicOrigin
        'Sec-Fetch-Site' = 'same-origin'
    }
}

function Convert-ToGitBashPath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if ($full -notmatch '^([A-Za-z]):\\(.*)$') { throw "cannot convert path for Git Bash: $full" }
    return "/$($Matches[1].ToLowerInvariant())/$($Matches[2].Replace('\','/'))"
}

$oldEnvironment = @{}
foreach ($name in 'KNOWLEDGE_MASTER_KEY_FILE','KNOWLEDGE_DATABASE_PASSWORD_FILE','KNOWLEDGE_OBJECT_ACCESS_KEY_FILE','KNOWLEDGE_OBJECT_SECRET_KEY_FILE','KNOWLEDGE_SITE_ADDRESS','KNOWLEDGE_PUBLIC_ORIGIN','KNOWLEDGE_HTTP_PORT','KNOWLEDGE_HTTPS_PORT') {
    $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
}

try {
    New-Item -ItemType Directory -Path $secretDirectory,$backupPowerShell,$backupBash,$restoreTarget | Out-Null
    $masterKey = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
    $wrongMasterKey = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
    $databasePassword = "db-$([Guid]::NewGuid().ToString('N'))"
    $objectAccessKey = "kp$suffix"
    $objectSecretKey = "obj-$([Guid]::NewGuid().ToString('N'))"
    $bootstrapPassword = "Bootstrap-$([Guid]::NewGuid().ToString('N'))!"
    $marker = "backup-marker-$([Guid]::NewGuid().ToString('N'))"
    $attachmentMarker = "attachment-$marker`n"
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'master_key'), $masterKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'wrong_master_key'), $wrongMasterKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'database_password'), $databasePassword)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'object_access_key'), $objectAccessKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'object_secret_key'), $objectSecretKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'bootstrap_password'), $bootstrapPassword)
    [IO.File]::WriteAllText($markerFile, $attachmentMarker, [Text.UTF8Encoding]::new($false))

    $env:KNOWLEDGE_MASTER_KEY_FILE = Join-Path $secretDirectory 'master_key'
    $env:KNOWLEDGE_DATABASE_PASSWORD_FILE = Join-Path $secretDirectory 'database_password'
    $env:KNOWLEDGE_OBJECT_ACCESS_KEY_FILE = Join-Path $secretDirectory 'object_access_key'
    $env:KNOWLEDGE_OBJECT_SECRET_KEY_FILE = Join-Path $secretDirectory 'object_secret_key'
    $env:KNOWLEDGE_SITE_ADDRESS = 'http://localhost'
    $env:KNOWLEDGE_PUBLIC_ORIGIN = $publicOrigin
    $env:KNOWLEDGE_HTTP_PORT = "$httpPort"
    $env:KNOWLEDGE_HTTPS_PORT = "$($httpPort + 1)"

    Invoke-Compose $sourceProject config --quiet
    Invoke-Compose $sourceProject build backend web minio minio-init
    Invoke-Compose $sourceProject up --detach postgres minio
    Wait-Storage $sourceProject
    Invoke-Compose $sourceProject run --rm -T minio-init
    Invoke-Compose $sourceProject run --rm -T migrate
    $bootstrapMount = "$(Join-Path $secretDirectory 'bootstrap_password'):/run/secrets/bootstrap_password:ro"
    & docker compose -f $composeFile -p $sourceProject run --rm -T --volume $bootstrapMount --env 'KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password' bootstrap --email 'admin@example.test' --workspace-name 'Backup Verification' --display-name 'Backup Admin'
    if ($LASTEXITCODE -ne 0) { throw 'bootstrap failed' }
    Invoke-Compose $sourceProject up --detach --no-deps backend web caddy
    Wait-PublicReady

    $loginBody = @{ email = 'admin@example.test'; password = $bootstrapPassword } | ConvertTo-Json -Compress
    $login = Invoke-WebRequest -Uri "$publicOrigin/api/v1/auth/login" -Method Post -ContentType 'application/json' -Headers @{ Origin = $publicOrigin; 'Sec-Fetch-Site' = 'same-origin' } -Body $loginBody
    $cookie = (@($login.Headers['Set-Cookie'])[0] -split ';')[0]
    if (-not $cookie.StartsWith('knowledge_session=')) { throw 'login did not return a session cookie' }
    $csrf = (Invoke-RestMethod -Uri "$publicOrigin/api/v1/auth/csrf" -Headers @{ Cookie = $cookie }).token
    $headers = Mutation-Headers $cookie $csrf
    $space = Invoke-RestMethod -Uri "$publicOrigin/api/v1/spaces" -Method Post -ContentType 'application/json' -Headers $headers -Body (@{ name = 'Backup Space'; description = 'restore drill' } | ConvertTo-Json -Compress)
    $page = Invoke-RestMethod -Uri "$publicOrigin/api/v1/pages" -Method Post -ContentType 'application/json' -Headers $headers -Body (@{ spaceId = $space.id; parentId = $null; title = 'Backup Marker'; markdown = $marker; properties = @{}; restricted = $false } | ConvertTo-Json -Compress)
    $attachmentSize = [Text.Encoding]::UTF8.GetByteCount($attachmentMarker)
    $attachment = Invoke-RestMethod -Uri "$publicOrigin/api/v1/pages/$($page.id)/attachments?size=$attachmentSize" -Method Post -Headers $headers -Form @{ file = Get-Item -LiteralPath $markerFile }
    $seedDownload = Join-Path $testRoot 'seed-download.txt'
    Invoke-WebRequest -Uri "$publicOrigin/api/v1/attachments/$($attachment.id)" -Headers @{ Cookie = $cookie } -OutFile $seedDownload
    if ([IO.File]::ReadAllText($seedDownload) -ne $attachmentMarker) { throw 'seed attachment round trip failed before backup' }

    Invoke-Compose $sourceProject stop backend
    & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\backup.ps1') -ProjectName $sourceProject -TargetDirectory $backupPowerShell
    if ($LASTEXITCODE -ne 0) { throw 'PowerShell backup failed' }
    $gitBash = 'C:\Program Files\Git\bin\bash.exe'
    if (-not (Test-Path -LiteralPath $gitBash -PathType Leaf)) { throw "Git Bash is required at $gitBash" }
    & $gitBash (Join-Path $repoRoot 'scripts\backup.sh') --project $sourceProject --target (Convert-ToGitBashPath $backupBash)
    if ($LASTEXITCODE -ne 0) { throw 'Bash backup failed' }
    $powerShellManifestPath = Join-Path $backupPowerShell 'manifest.sha256'
    $bashManifestPath = Join-Path $backupBash 'manifest.sha256'
    $powerShellManifest = Get-Item -LiteralPath $powerShellManifestPath
    $bashManifest = Get-Item -LiteralPath $bashManifestPath
    if ($powerShellManifest.Length -ne $bashManifest.Length -or
        (Get-FileHash -LiteralPath $powerShellManifestPath -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $bashManifestPath -Algorithm SHA256).Hash) {
        $difference = Compare-Object -ReferenceObject ([IO.File]::ReadAllLines($powerShellManifestPath)) -DifferenceObject ([IO.File]::ReadAllLines($bashManifestPath)) | Out-String
        throw "PowerShell and Bash backup manifests differ: $difference"
    }
    if (Get-ChildItem -LiteralPath $backupPowerShell,$backupBash -Recurse -Force | Where-Object Name -Match 'master.?key') { throw 'backup bundle contains master-key material' }

    Invoke-Compose $sourceProject down --remove-orphans
    & pwsh -NoProfile -File (Join-Path $repoRoot 'scripts\restore.ps1') -ProjectName $restoreProject -SourceDirectory $backupPowerShell -TargetDirectory $restoreTarget
    if ($LASTEXITCODE -ne 0) { throw 'restore failed' }

    Invoke-Compose $restoreProject rm --force --stop backend
    $env:KNOWLEDGE_MASTER_KEY_FILE = Join-Path $secretDirectory 'master_key'
    $missingOutput = & docker compose -f $composeFile -p $restoreProject run --rm -T --no-deps --env 'KNOWLEDGE_MASTER_KEY_FILE=/run/secrets/not_supplied' backend 2>&1
    if ($LASTEXITCODE -eq 0 -or "$missingOutput" -notmatch 'KNOWLEDGE_MASTER_KEY_FILE' -or "$missingOutput" -match [Regex]::Escape($masterKey)) {
        throw 'backend did not fail safely without the master-key file'
    }

    $env:KNOWLEDGE_MASTER_KEY_FILE = Join-Path $secretDirectory 'wrong_master_key'
    Invoke-Compose $restoreProject up --detach --no-deps backend
    $wrongKeyDeadline = [DateTimeOffset]::UtcNow.AddSeconds(90)
    $wrongKeyRejected = $false
    do {
        try {
            $body = (& docker compose -f $composeFile -p $restoreProject exec -T backend curl --silent 'http://127.0.0.1:3000/api/v1/health/ready') -join ''
            if ($body -match '"masterKey":false') { $wrongKeyRejected = $true; break }
        } catch {}
        Start-Sleep -Seconds 2
    } while ([DateTimeOffset]::UtcNow -lt $wrongKeyDeadline)
    if (-not $wrongKeyRejected) { throw 'restored backend did not fail readiness with the wrong master key' }
    Invoke-Compose $restoreProject rm --force --stop backend

    $env:KNOWLEDGE_MASTER_KEY_FILE = Join-Path $secretDirectory 'master_key'
    Invoke-Compose $restoreProject up --detach --no-deps backend web caddy
    Wait-PublicReady
    $restoredLogin = Invoke-WebRequest -Uri "$publicOrigin/api/v1/auth/login" -Method Post -ContentType 'application/json' -Headers @{ Origin = $publicOrigin; 'Sec-Fetch-Site' = 'same-origin' } -Body $loginBody
    $restoredCookie = (@($restoredLogin.Headers['Set-Cookie'])[0] -split ';')[0]
    $restoredPage = Invoke-RestMethod -Uri "$publicOrigin/api/v1/pages/$($page.id)" -Headers @{ Cookie = $restoredCookie }
    if ($restoredPage.markdown -ne $marker) { throw 'restored page marker does not match' }
    $restoredDownload = Join-Path $testRoot 'restored-download.txt'
    Invoke-WebRequest -Uri "$publicOrigin/api/v1/attachments/$($attachment.id)" -Headers @{ Cookie = $restoredCookie } -OutFile $restoredDownload
    if ([IO.File]::ReadAllText($restoredDownload) -ne $attachmentMarker) { throw 'restored attachment marker does not match' }
    Write-Output "backup verification passed: source=$sourceProject restore=$restoreProject"
} finally {
    if (Test-Path -LiteralPath $composeFile -PathType Leaf) {
        & docker compose -f $composeFile -p $sourceProject down --remove-orphans --volumes | Out-Null
        & docker compose -f $composeFile -p $restoreProject down --remove-orphans --volumes | Out-Null
    }
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
    foreach ($entry in $oldEnvironment.GetEnumerator()) {
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
    }
}
