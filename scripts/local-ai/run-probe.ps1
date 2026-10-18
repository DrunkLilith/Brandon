#requires -Version 7.0
param(
    [ValidateSet('BuildOcr', 'BuildAsr', 'Fixtures', 'Ocr', 'Asr')][string]$Action = 'Ocr',
    [ValidateSet('0.6b', '1.7b')][string]$Model = '0.6b',
    [ValidateSet('synthetic', 'fleurs-ru')][string]$Corpus = 'synthetic',
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$sourceDirectory = [IO.Path]::GetFullPath($PSScriptRoot)
$outputDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../target/qa/local-ai'))
[IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
if ($Action -in @('BuildOcr', 'BuildAsr')) {
    $stage = if ($Action -eq 'BuildOcr') { 'ocr' } else { 'asr' }
    & docker build --target $stage -t "brandon/local-ai-$stage`:20261018" $sourceDirectory
} else {
    $stage = if ($Action -eq 'Asr') { 'asr' } else { 'ocr' }
    $command = $Action.ToLowerInvariant()
    $arguments = @('run', '--rm', '--gpus', 'all', '--cpus', '8', '--memory', '12g', '--shm-size', '1g',
        '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges', '--read-only', '--tmpfs', '/tmp:rw,size=536870912',
        '--mount', "type=bind,src=$sourceDirectory,dst=/work,readonly",
        '--mount', "type=bind,src=$outputDirectory,dst=/results",
        '--mount', 'type=volume,src=brandon-local-ai-cache,dst=/cache')
    if ($Offline) { $arguments += @('--network', 'none') }
    $arguments += @("brandon/local-ai-$stage`:20261018", '/work/probe.py', $command, '--model', $Model, '--corpus', $Corpus)
    if ($Offline) { $arguments += '--offline' }
    & docker @arguments
}
if ($LASTEXITCODE -ne 0) { throw "Local AI probe failed with exit code $LASTEXITCODE; no user vault is mounted." }
