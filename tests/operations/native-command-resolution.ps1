$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../../scripts/release-check-utils.ps1')

$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('kp-native-resolution-' + [Guid]::NewGuid().ToString('N'))
$previousPath = $env:PATH
$probeName = 'kp-native-probe-' + [Guid]::NewGuid().ToString('N')
$extension = if ($IsWindows) { '.cmd' } else { '' }
$firstDirectory = Join-Path $fixtureRoot 'first tools'
$secondDirectory = Join-Path $fixtureRoot 'second tools'
$first = Join-Path $firstDirectory ($probeName + $extension)
$second = Join-Path $secondDirectory ($probeName + $extension)
function Write-VersionProbe([string]$Path, [string]$Version, [int]$ExitCode = 0) {
    $content = if ($IsWindows) { "@echo off`r`necho probe $Version`r`nexit /b $ExitCode`r`n" }
        else { "#!/bin/sh`nprintf '%s\n' 'probe $Version'`nexit $ExitCode`n" }
    [IO.File]::WriteAllText($Path, $content, [Text.UTF8Encoding]::new($false))
    if (-not $IsWindows) {
        & chmod 700 $Path
        if ($LASTEXITCODE -ne 0) { throw 'Could not mark generated native probe executable' }
    }
}
try {
    New-Item -ItemType Directory -Path $firstDirectory, $secondDirectory | Out-Null
    Write-VersionProbe $first '24.19.0'
    Write-VersionProbe $second '99.0.0'
    $separator = [IO.Path]::PathSeparator
    $env:PATH = "$firstDirectory$separator$secondDirectory$separator$previousPath"
    $candidates = @(Get-Command $probeName -CommandType Application -ErrorAction Stop)
    if ($candidates.Count -lt 2) { throw 'Regression fixture must expose duplicate executables in PATH' }
    $selected = Resolve-NativeApplication -Name $probeName
    if ($selected -isnot [string] -or $selected -ne $first) { throw 'Resolver must return exactly the first native executable path, including spaces' }
    Assert-ExactVersion -Name $probeName -Expected '24.19.0'
    Write-Output 'PASS duplicate PATH applications resolve to one executable in precedence order'

    Write-VersionProbe $first '24.18.0'
    Write-VersionProbe $second '24.19.0'
    $rejected = $false
    try { Assert-ExactVersion -Name $probeName -Expected '24.19.0' } catch { $rejected = $true }
    if (-not $rejected) { throw 'A later executable must not hide an incorrect version earlier in PATH' }
    Write-Output 'PASS an incorrect first version fails closed instead of selecting a later executable'

    Write-VersionProbe $first '24.19.0' 9
    $rejected = $false
    try { Assert-ExactVersion -Name $probeName -Expected '24.19.0' } catch { $rejected = $_.Exception.Message -like '*version probe failed*' }
    if (-not $rejected) { throw 'A nonzero native version probe must fail' }
    Write-Output 'PASS nonzero native version probes remain failures'
} finally {
    $env:PATH = $previousPath
    $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
    if ([IO.Path]::GetDirectoryName($resolvedFixture).TrimEnd('\','/') -ne [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/') -or -not [IO.Path]::GetFileName($resolvedFixture).StartsWith('kp-native-resolution-')) { throw 'Unsafe native probe cleanup target' }
    if (Test-Path -LiteralPath $resolvedFixture) { Remove-Item -LiteralPath $resolvedFixture -Recurse -Force }
}
