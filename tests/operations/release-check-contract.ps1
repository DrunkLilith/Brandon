$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../../scripts/release-check-utils.ps1')

if ((Get-SemanticVersion 'pnpm 11.19.0') -ne '11.19.0') { throw 'exact semantic version was not parsed' }
if ((Get-SemanticVersion 'node v24.19.0') -ne '24.19.0') { throw 'v-prefixed semantic version was not normalized' }
foreach ($nearMiss in @(
    'pnpm 11.19.01',
    'pnpm 11.19.0-beta.1',
    'pnpm 11.19.0+meta',
    'pnpm prefix11.19.0',
    'pnpm 11.19.0suffix',
    'versions 11.19.0 and 1.0.0',
    'no version'
)) {
    try {
        $null = Assert-ObservedVersion -Name pnpm -Expected '11.19.0' -Observed $nearMiss
        throw "near miss was accepted: $nearMiss"
    } catch {
        if ($_.Exception.Message -like 'near miss was accepted*') { throw }
    }
}
$null = Assert-ObservedVersion -Name pnpm -Expected '11.19.0' -Observed 'pnpm 11.19.0'
$null = Assert-ObservedVersion -Name cargo-audit -Expected '0.22.2' -Observed 'cargo-audit-audit 0.22.2'

$isShallow = (& git rev-parse --is-shallow-repository).Trim()
if ($LASTEXITCODE -ne 0 -or $isShallow -ne 'false') { throw 'test repository must expose full history' }
$null = Assert-FullGitHistory
Write-Output 'release-check contract PASS'
