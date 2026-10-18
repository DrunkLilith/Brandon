param(
    [string]$OutputPath = (Join-Path ([Environment]::GetFolderPath('UserProfile')) 'Downloads/Brandon-mac-source-2026-10-18.zip')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$outputFile = [IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $outputFile) { throw "Output already exists: $outputFile. Choose a new -OutputPath." }
if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($outputFile)) -PathType Container)) { throw 'The output parent directory must already exist.' }

# Explicit source roots, plus a deny list. Never archive the user's demo vault,
# generated artifacts, credentials, local task databases, or Git history.
$roots = @('apps', 'crates', 'migrations', 'tests/desktop') | Where-Object { Test-Path -LiteralPath (Join-Path $projectRoot $_) }
$rootFiles = @('Cargo.toml', 'Cargo.lock', 'package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml', 'rust-toolchain.toml', 'eslint.config.mjs', '.gitignore', '.editorconfig', 'README.md', 'scripts/build-macos.sh', 'docs/build-macos.md')
Push-Location $projectRoot
try {
    $sourceFiles = @(& rg --files --hidden @roots)
    if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate source files with rg.' }
} finally { Pop-Location }
$files = @($sourceFiles + $rootFiles | ForEach-Object { $_.Replace('\', '/') } | Where-Object {
    $_ -notmatch '(^|/)(node_modules|target|dist|dist-macos|gen|demo-vault|\.git|\.brandon|\.codex-tmp)(/|$)' -and
    $_ -notmatch '(^|/)\.env($|\.)' -and
    $_ -notmatch '\.(sqlite3?|db|log|tsbuildinfo|pem|key|p12|pfx|mobileprovision|cer)$'
} | Sort-Object -Unique)
foreach ($required in $rootFiles) {
    if ($files -notcontains $required -or -not (Test-Path -LiteralPath (Join-Path $projectRoot $required) -PathType Leaf)) { throw "Missing required source: $required" }
}

Add-Type -AssemblyName System.IO.Compression
$stream = [IO.File]::Open($outputFile, [IO.FileMode]::CreateNew)
$archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create)
$manifest = [Collections.Generic.List[object]]::new()
try {
    foreach ($relative in $files) {
        $absolute = [IO.Path]::GetFullPath((Join-Path $projectRoot $relative))
        if (-not $absolute.StartsWith($projectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Source escaped workspace: $relative" }
        if ((Get-Item -LiteralPath $absolute).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing a symlink source: $relative" }
        $bytes = [IO.File]::ReadAllBytes($absolute)
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
        $manifest.Add(@{ path = $relative; bytes = $bytes.Length; sha256 = $hash })
        $entry = $archive.CreateEntry("Brandon-mac-source/$relative", [IO.Compression.CompressionLevel]::Optimal)
        $entryStream = $entry.Open()
        try { $entryStream.Write($bytes, 0, $bytes.Length) } finally { $entryStream.Dispose() }
    }
    $readme = [IO.File]::ReadAllBytes((Join-Path $projectRoot 'docs/build-macos.md'))
    $entry = $archive.CreateEntry('Brandon-mac-source/START-HERE-MACOS.md')
    $entryStream = $entry.Open()
    try { $entryStream.Write($readme, 0, $readme.Length) } finally { $entryStream.Dispose() }
    $json = @{ preparedDate = '2026-10-18'; kind = 'source-only-not-a-built-macOS-app'; files = $manifest.ToArray() } | ConvertTo-Json -Depth 5
    $entry = $archive.CreateEntry('Brandon-mac-source/SOURCE-MANIFEST.json')
    $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
    try { $writer.Write($json) } finally { $writer.Dispose() }
} finally { $archive.Dispose(); $stream.Dispose() }
Get-Item -LiteralPath $outputFile | Select-Object FullName, Length
Get-FileHash -LiteralPath $outputFile -Algorithm SHA256
Write-Output "Source files: $($manifest.Count). No macOS binary has been built on this Windows host."
