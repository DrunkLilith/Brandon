param(
    [switch]$DeleteVolumes,
    [int]$TimeoutSeconds = 420
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$composeFile = Join-Path $repoRoot 'deploy/compose.yaml'
if (-not (Test-Path -LiteralPath $composeFile -PathType Leaf)) {
    throw "production Compose file is missing: $composeFile"
}

$suffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
$project = "kp-smoke-$suffix"
if ($project -notmatch '^kp-smoke-[a-f0-9]{12}$') {
    throw 'generated Compose project name is unsafe'
}
$secretDirectory = Join-Path ([IO.Path]::GetTempPath()) $project
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$httpPort = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
$listener.Stop()

function Invoke-Compose {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    & docker compose -f $composeFile -p $project @Arguments
    if ($LASTEXITCODE -ne 0) { throw "docker compose failed: $($Arguments -join ' ')" }
}

try {
    New-Item -ItemType Directory -Path $secretDirectory | Out-Null
    $masterKey = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
    $databasePassword = "db-$([Guid]::NewGuid().ToString('N'))"
    $objectAccessKey = "kp$suffix"
    $objectSecretKey = "obj-$([Guid]::NewGuid().ToString('N'))"
    $bootstrapPassword = "Bootstrap-$([Guid]::NewGuid().ToString('N'))!"
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'master_key'), $masterKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'database_password'), $databasePassword)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'object_access_key'), $objectAccessKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'object_secret_key'), $objectSecretKey)
    [IO.File]::WriteAllText((Join-Path $secretDirectory 'bootstrap_password'), $bootstrapPassword)

    $env:KNOWLEDGE_MASTER_KEY_FILE = (Join-Path $secretDirectory 'master_key')
    $env:KNOWLEDGE_DATABASE_PASSWORD_FILE = (Join-Path $secretDirectory 'database_password')
    $env:KNOWLEDGE_OBJECT_ACCESS_KEY_FILE = (Join-Path $secretDirectory 'object_access_key')
    $env:KNOWLEDGE_OBJECT_SECRET_KEY_FILE = (Join-Path $secretDirectory 'object_secret_key')
    $env:KNOWLEDGE_SITE_ADDRESS = 'http://localhost'
    $env:KNOWLEDGE_PUBLIC_ORIGIN = "http://localhost:$httpPort"
    $env:KNOWLEDGE_HTTP_PORT = "$httpPort"

    Invoke-Compose config --quiet
    $rendered = (& docker compose -f $composeFile -p $project --profile tools config --format json) | ConvertFrom-Json
    $backendDependencies = @($rendered.services.backend.depends_on.psobject.Properties.Name)
    $bootstrapDependencies = @($rendered.services.bootstrap.depends_on.psobject.Properties.Name)
    if ($backendDependencies -contains 'migrate' -or $bootstrapDependencies -contains 'migrate') {
        throw 'normal backend/bootstrap startup must not run migrations implicitly'
    }
    Invoke-Compose build backend web minio minio-init
    Invoke-Compose up --detach postgres minio
    Invoke-Compose up --detach --no-deps backend
    $preMigrationDeadline = [DateTimeOffset]::UtcNow.AddSeconds(60)
    $preMigrationReady = $null
    while ([DateTimeOffset]::UtcNow -lt $preMigrationDeadline) {
        try {
            $preMigrationReady = (& docker compose -f $composeFile -p $project exec -T backend curl --silent 'http://localhost:3000/api/v1/health/ready') | ConvertFrom-Json
            if ($preMigrationReady.migrations -eq $false) { break }
        } catch {
            Start-Sleep -Seconds 2
        }
    }
    if ($null -eq $preMigrationReady -or $preMigrationReady.migrations -ne $false) {
        throw 'backend did not report pending migrations before the explicit migrate step'
    }
    Invoke-Compose stop backend
    Invoke-Compose run --rm -T minio-init
    Invoke-Compose run --rm -T migrate
    $bootstrapSecret = "$(Join-Path $secretDirectory 'bootstrap_password'):/run/secrets/bootstrap_password:ro"
    & docker compose -f $composeFile -p $project run --rm -T --volume $bootstrapSecret --env 'KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password' bootstrap --email 'admin@example.test' --workspace-name 'Smoke Workspace' --display-name 'Smoke Admin'
    if ($LASTEXITCODE -ne 0) { throw 'bootstrap failed' }
    Invoke-Compose up --detach --no-deps backend web caddy

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    $live = $false
    $ready = $false
    $login = $false
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        try {
            $liveBody = Invoke-RestMethod -Uri "http://localhost:$httpPort/api/v1/health/live" -TimeoutSec 5
            $live = $liveBody.status -eq 'ok'
            $readyBody = Invoke-RestMethod -Uri "http://localhost:$httpPort/api/v1/health/ready" -TimeoutSec 5
            $ready = $readyBody.status -eq 'ready'
            $page = Invoke-WebRequest -Uri "http://localhost:$httpPort/" -UseBasicParsing -TimeoutSec 5
            $login = $page.StatusCode -eq 200 -and $page.Content.Contains('<div id="root"></div>')
            if ($live -and $ready -and $login) { break }
        } catch {
            Start-Sleep -Seconds 2
        }
    }
    if (-not ($live -and $ready -and $login)) {
        Invoke-Compose ps
        Invoke-Compose logs --no-color --tail 120
        throw "production smoke timed out (live=$live ready=$ready login=$login)"
    }
    Write-Output "compose smoke passed: project=$project"
} finally {
    if (Test-Path -LiteralPath $composeFile -PathType Leaf) {
        $down = @('down', '--remove-orphans')
        if ($DeleteVolumes) { $down += '--volumes' }
        & docker compose -f $composeFile -p $project @down | Out-Null
    }
    if ((Test-Path -LiteralPath $secretDirectory) -and
        ([IO.Path]::GetFullPath($secretDirectory) -eq [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) $project)))) {
        Remove-Item -LiteralPath $secretDirectory -Recurse -Force
    }
}
