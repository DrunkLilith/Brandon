[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $repoRoot
. (Join-Path $PSScriptRoot 'release-check-utils.ps1')

function Resolve-Bash {
    if ($IsWindows) {
        $gitBash = 'C:\Program Files\Git\bin\bash.exe'
        if (-not (Test-Path -LiteralPath $gitBash -PathType Leaf)) { throw "Git Bash is required at $gitBash" }
        return $gitBash
    }
    return Resolve-NativeApplication -Name bash
}

function Invoke-RustTests {
    $suffix = [Guid]::NewGuid().ToString('N').Substring(0, 12)
    $container = "kp-rust-tests-$suffix"
    $temporary = Join-Path ([IO.Path]::GetTempPath()) $container
    $password = [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(24)).ToLowerInvariant()
    $passwordFile = Join-Path $temporary 'database_password'
    $previousDatabaseUrl = $env:DATABASE_URL
    if ($container -notmatch '^kp-rust-tests-[a-f0-9]{12}$') { throw 'generated unsafe Rust test database name' }
    try {
        New-Item -ItemType Directory -Path $temporary -ErrorAction Stop | Out-Null
        if ($IsWindows) {
            Invoke-Native 'Protect Rust test database directory' icacls @($temporary, '/inheritance:r', '/grant:r', "$($env:USERNAME):(OI)(CI)F")
        } else {
            Invoke-Native 'Protect Rust test database directory' chmod @('700', $temporary)
        }
        [IO.File]::WriteAllText($passwordFile, $password, [Text.UTF8Encoding]::new($false))
        if (-not $IsWindows) { Invoke-Native 'Protect Rust test database secret' chmod @('600', $passwordFile) }
        Invoke-Native 'Start isolated Rust test database' docker @(
            'run', '--detach', '--name', $container,
            '--label', 'knowledge-platform.release-owned=true',
            '--env', 'POSTGRES_DB=postgres', '--env', 'POSTGRES_USER=knowledge',
            '--env', 'POSTGRES_PASSWORD_FILE=/run/secrets/database_password',
            '--mount', "type=bind,source=$passwordFile,target=/run/secrets/database_password,readonly",
            '--publish', '127.0.0.1::5432',
            'postgres:18.6-alpine@sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2'
        )
        $deadline = [DateTimeOffset]::UtcNow.AddMinutes(2)
        do {
            & docker exec $container pg_isready -U knowledge -d postgres *> $null
            if ($LASTEXITCODE -eq 0) { break }
            Start-Sleep -Seconds 1
        } while ([DateTimeOffset]::UtcNow -lt $deadline)
        if ($LASTEXITCODE -ne 0) { throw 'isolated Rust test database did not become ready' }
        $mapping = (& docker port $container '5432/tcp' | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $mapping -notmatch ':(\d+)$') { throw 'could not resolve isolated Rust test database port' }
        $env:DATABASE_URL = "postgres://knowledge:$password@127.0.0.1:$($Matches[1])/postgres"
        Invoke-Native 'Rust tests' cargo @('test', '--workspace')
        Invoke-Native 'Seeded bounded security properties' pnpm @('property:check')
    } finally {
        $env:DATABASE_URL = $previousDatabaseUrl
        $owned = (& docker ps -a --filter "name=^/$container`$" --filter 'label=knowledge-platform.release-owned=true' --format '{{.Names}}' | Out-String).Trim()
        if ($owned -eq $container) {
            $anonymousVolumes = @(& docker inspect $container --format '{{range .Mounts}}{{if eq .Type "volume"}}{{println .Name}}{{end}}{{end}}' | Where-Object { $_ })
            if ($LASTEXITCODE -ne 0) { throw 'could not inventory Rust test database volumes' }
            & docker rm --force --volumes $container *> $null
            if ($LASTEXITCODE -ne 0) { throw 'could not remove Rust test database container and volumes' }
            foreach ($volume in $anonymousVolumes) {
                & docker volume inspect $volume *> $null
                if ($LASTEXITCODE -eq 0) { throw "Rust test database volume cleanup failed: $volume" }
            }
        }
        $expected = Join-Path ([IO.Path]::GetTempPath()) $container
        if ([IO.Path]::GetFullPath($temporary) -ne [IO.Path]::GetFullPath($expected)) { throw 'refusing Rust test cleanup outside its exact temporary directory' }
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
    }
}

try {
    Assert-CleanTree
    Assert-ExactVersion rustc '1.98.0'
    Assert-ExactVersion cargo '1.98.0'
    Assert-ExactVersion node '24.19.0'
    Assert-ExactVersion pnpm '11.19.0'
    Assert-ExactVersion cargo-audit '0.22.2'
    Assert-ExactVersion cargo-deny '0.20.2'
    Assert-ExactVersion gitleaks '8.30.1' @('version')
    Invoke-Native 'Docker Compose availability' docker @('compose', 'version')
    $bash = Resolve-Bash

    Invoke-Native 'Desktop frontend lint' pnpm @('--dir', 'apps/desktop', 'lint')
    Invoke-Native 'Desktop Canvas unit tests' pnpm @('--dir', 'apps/desktop', 'test:canvas')
    Invoke-Native 'Desktop search request unit tests' pnpm @('--dir', 'apps/desktop', 'test:search')
    Invoke-Native 'Desktop command, settings and navigation contracts' pnpm @('--dir', 'apps/desktop', 'test:r1a')
    Invoke-Native 'Desktop workspace and session contracts' pnpm @('--dir', 'apps/desktop', 'test:r1b')
    Invoke-Native 'Desktop incremental metadata contracts' pnpm @('--dir', 'apps/desktop', 'test:r2a')
    Invoke-Native 'Desktop bounded Base query contracts' pnpm @('--dir', 'apps/desktop', 'test:r2b')
    Invoke-Native 'Desktop frontend production build' pnpm @('--dir', 'apps/desktop', 'build')
    Invoke-Native 'Rust formatting' cargo @('fmt', '--all', '--', '--check')
    Invoke-Native 'Rust lint' cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
    Invoke-RustTests
    Invoke-Native 'Frontend lint' pnpm @('--filter', '@knowledge/web', 'lint')
    Invoke-Native 'Frontend tests' pnpm @('--filter', '@knowledge/web', 'test', '--run')
    Invoke-Native 'Frontend typecheck' pnpm @('--filter', '@knowledge/web', 'typecheck')
    Invoke-Native 'Frontend production build' pnpm @('--filter', '@knowledge/web', 'build')
    Invoke-Native 'E2E lint' pnpm @('e2e:lint')
    Invoke-Native 'E2E typecheck' pnpm @('e2e:typecheck')
    Invoke-Native 'Release policy unit regressions' pnpm @('release:unit')
    Invoke-Native 'PowerShell release contract' pwsh @('-NoProfile', '-File', 'tests/operations/release-check-contract.ps1')
    Invoke-Native 'Native Linux permission semantics' $bash @('tests/operations/linux-permissions.sh')
    Invoke-Native 'Generated OpenAPI drift' pnpm @('api:check')
    Invoke-Native 'Rust vulnerability audit' cargo @('audit')
    Invoke-Native 'Rust dependency, license and source policy' cargo @('deny', 'check', 'bans', 'licenses', 'sources', '--hide-inclusion-graph')
    Invoke-Native 'Production npm vulnerability audit' pnpm @('audit', '--prod')
    Invoke-Native 'Production npm license policy' pnpm @('licenses:check')
    Assert-FullGitHistory
    Invoke-Native 'Repository secret scan' gitleaks @('git', '--redact', '--no-banner', '--no-color', '--config', '.gitleaks.toml', '.')

    if ($IsWindows) {
        Invoke-Native 'Fresh Compose acceptance (PowerShell)' pwsh @('-NoProfile', '-File', 'tests/operations/compose-smoke.ps1', '-DeleteVolumes')
    } else {
        Invoke-Native 'Fresh Compose acceptance (Bash)' $bash @('tests/operations/compose-smoke.sh', '--delete-volumes')
    }
    Invoke-Native 'Compose maintenance tmpfs ownership' pwsh @('-NoProfile', '-File', 'tests/operations/compose-maintenance.ps1')
    Invoke-Native 'PowerShell restore drill' pwsh @('-NoProfile', '-File', 'scripts/verify-backup.ps1', '-TestMode')
    Invoke-Native 'Bash restore drill' $bash @('scripts/verify-backup.sh', '--test-mode')
    Invoke-Native 'Production browser E2E' pnpm @('exec', 'playwright', 'test', '--workers=1')

    Invoke-Native 'Generated-file diff check' git @('diff', '--exit-code')
    Invoke-Native 'Index diff check' git @('diff', '--cached', '--exit-code')
    Assert-CleanTree
    Write-Host 'Release gate PASS'
    exit 0
} catch {
    Write-Error ("Release gate FAILED: {0}" -f $_.Exception.Message)
    exit 1
}
