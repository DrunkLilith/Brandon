function Invoke-Native {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter()][string[]]$ArgumentList = @()
    )
    Write-Host "==> $Label"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE" }
}

function Get-SemanticVersion {
    param([Parameter(Mandatory)][string]$Observed)
    $matches = [Regex]::Matches($Observed, '(?<!\S)v?([0-9]+\.[0-9]+\.[0-9]+)(?!\S)')
    if ($matches.Count -ne 1) { throw 'version output must contain exactly one stable semantic version' }
    return $matches[0].Groups[1].Value
}

function Assert-ObservedVersion {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Expected,
        [Parameter(Mandatory)][string]$Observed
    )
    $actual = Get-SemanticVersion $Observed
    if (-not [StringComparer]::Ordinal.Equals($actual, $Expected)) {
        throw "$Name $Expected is required; observed semantic version: $actual"
    }
}

function Assert-ExactVersion {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Expected,
        [Parameter()][string[]]$Arguments = @('--version')
    )
    $command = Get-Command $Name -CommandType Application -ErrorAction Stop
    $observed = (& $command.Source @Arguments 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw "$Name version probe failed" }
    Assert-ObservedVersion -Name $Name -Expected $Expected -Observed $observed
}

function Assert-CleanTree {
    $status = (& git status --porcelain=v1 --untracked-files=all | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'git status failed' }
    if ($status) { throw 'release gate requires a clean tracked and untracked worktree' }
}

function Assert-FullGitHistory {
    $shallow = (& git rev-parse --is-shallow-repository | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $shallow -ne 'false') { throw 'release gate requires a non-shallow Git checkout before secret history scan' }
    $countText = (& git rev-list --count HEAD | Out-String).Trim()
    $count = 0
    if ($LASTEXITCODE -ne 0 -or -not [int]::TryParse($countText, [ref]$count) -or $count -lt 2) {
        throw 'release gate requires available Git history before secret history scan'
    }
}
