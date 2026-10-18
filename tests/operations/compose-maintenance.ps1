$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$project = "kp-maintenance-$([Guid]::NewGuid().ToString('N').Substring(0, 12))"
$temporary = Join-Path ([IO.Path]::GetTempPath()) $project
$names = @('KNOWLEDGE_PUBLIC_ORIGIN','KNOWLEDGE_SITE_ADDRESS','KNOWLEDGE_MASTER_KEY_FILE','KNOWLEDGE_DATABASE_PASSWORD_FILE','KNOWLEDGE_OBJECT_ACCESS_KEY_FILE','KNOWLEDGE_OBJECT_SECRET_KEY_FILE','KNOWLEDGE_MAINTENANCE_UID','KNOWLEDGE_MAINTENANCE_GID')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    $secret = Join-Path $temporary 'synthetic-secret'
    [IO.File]::WriteAllText($secret, 'test-only-not-a-real-secret')
    $env:KNOWLEDGE_PUBLIC_ORIGIN = 'http://localhost:8080'
    $env:KNOWLEDGE_SITE_ADDRESS = ':80'
    foreach ($name in $names | Where-Object { $_ -like '*_FILE' }) {
        [Environment]::SetEnvironmentVariable($name, $secret, 'Process')
    }
    $compose = @('compose','-f',(Join-Path $repoRoot 'deploy/compose.yaml'),'-p',$project)
    foreach ($owner in @('10002','1001')) {
        # Unset interpolation tests the ordinary service default; the second case
        # exercises an unprivileged native Linux maintenance caller, as in CI.
        $env:KNOWLEDGE_MAINTENANCE_UID = $(if ($owner -eq '1001') { $owner } else { $null })
        $env:KNOWLEDGE_MAINTENANCE_GID = $env:KNOWLEDGE_MAINTENANCE_UID
        $probe = 'test "$(stat -c %u:%g /tmp)" = ' + "${owner}:${owner}" + '; test "$(stat -c %a /tmp)" = 700; umask 077; mkdir -p "$MC_CONFIG_DIR"; printf test-only >"$MC_CONFIG_DIR/config.json"; test "$(stat -c %a "$MC_CONFIG_DIR/config.json")" = 600; printf "private tmpfs PASS\n"'
        & docker @compose run --rm --no-deps -T --user "${owner}:${owner}" --entrypoint /bin/sh minio-init -ec $probe
        if ($LASTEXITCODE -ne 0) { throw "Compose private tmpfs probe failed for UID/GID $owner" }
    }
    Write-Output 'Compose maintenance ownership PASS (default and native caller)'
} finally {
    if ($null -ne $compose) {
        # The unpredictable project name is exclusive to this probe. Remove only
        # resources whose Compose ownership label matches that exact project.
        foreach ($resource in @('volume','network')) {
            $owned = @(& docker $resource ls --filter "label=com.docker.compose.project=$project" --format '{{.Name}}')
            if ($LASTEXITCODE -ne 0) { throw 'could not inventory maintenance probe resources' }
            foreach ($name in $owned | Where-Object { $_ }) {
                if (-not $name.StartsWith("${project}_", [StringComparison]::Ordinal)) { throw 'unexpected maintenance resource name' }
                & docker $resource rm $name | Out-Null
                if ($LASTEXITCODE -ne 0) { throw 'could not remove maintenance probe resource' }
            }
        }
    }
    if (Test-Path -LiteralPath $temporary) {
        $expected = Join-Path ([IO.Path]::GetTempPath()) $project
        if ([IO.Path]::GetFullPath($temporary) -ne [IO.Path]::GetFullPath($expected) -or $project -notmatch '^kp-maintenance-[a-f0-9]{12}$') { throw 'unsafe maintenance probe cleanup path' }
        Remove-Item -LiteralPath $temporary -Recurse -Force
    }
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
}
