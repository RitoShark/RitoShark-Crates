[CmdletBinding()]
param(
    [string[]] $Crate = @(),
    [switch] $CheckOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-Corpus {
    param(
        [string] $Root,
        [string[]] $Crates = @(),
        [switch] $PreflightOnly
    )

    $manifest = Get-Content -LiteralPath (Join-Path $Root 'docs/fixtures.json') -Raw | ConvertFrom-Json
    if ($manifest.version -ne 1 -or @($manifest.packages).Count -eq 0) {
        throw 'Expected a version-1 corpus manifest with at least one package.'
    }
    $known = @($manifest.packages | ForEach-Object { $_.name })
    if (@($known | Sort-Object -Unique).Count -ne $known.Count) {
        throw 'Duplicate packages in corpus manifest.'
    }
    foreach ($name in $Crates) {
        if ($name -notin $known) { throw "Unknown corpus crate: $name" }
    }
    $packages = @($manifest.packages | Where-Object { $Crates.Count -eq 0 -or $_.name -in $Crates })
    $problems = [Collections.Generic.List[string]]::new()
    $verified = 0
    foreach ($package in $packages) {
        if ($package.name -notmatch '^rs_[a-z0-9_]+$' -or @($package.tests).Count -eq 0 -or @($package.fixtures).Count -eq 0) {
            throw 'Every corpus package needs a valid name, test targets, and fixtures.'
        }
        foreach ($target in $package.tests) {
            if ($target -notmatch '^[a-z0-9_]+$' -or -not (Test-Path -LiteralPath (Join-Path $Root "crates/$($package.name)/tests/$target.rs") -PathType Leaf)) {
                throw "Invalid or missing test target: $($package.name)/$target"
            }
        }
        foreach ($fixture in $package.fixtures) {
            if ($fixture.name -match '[/\\]' -or $fixture.name -in @('', '.', '..') -or $fixture.bytes -le 0 -or $fixture.sha256 -notmatch '^[0-9a-fA-F]{64}$') {
                throw "Invalid fixture manifest entry in $($package.name)."
            }
            $path = Join-Path $Root "Sample-Files/$($fixture.name)"
            try {
                $file = Get-Item -LiteralPath $path -ErrorAction Stop
                if ($file.PSIsContainer) { throw 'expected a regular file' }
                if ($file.Length -ne $fixture.bytes) {
                    throw "size mismatch: expected $($fixture.bytes), got $($file.Length)"
                }
                if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $fixture.sha256) {
                    throw 'SHA-256 mismatch'
                }
                $verified++
            } catch {
                $problems.Add("$($package.name): $path -- $($_.Exception.Message)")
            }
        }
    }
    if ($problems.Count -ne 0) {
        throw "Required corpus preflight failed ($($problems.Count) files):`n$($problems -join "`n")"
    }
    Write-Host "Corpus preflight passed: $verified fixture checks across $($packages.Count) crates."
    if ($PreflightOnly) {
        Write-Host 'Preflight only; no compatibility tests ran.'
        return
    }

    Push-Location $Root
    try {
        foreach ($package in $packages) {
            $cargoArgs = @('test', '-p', $package.name, '--all-features', '--locked')
            foreach ($target in $package.tests) { $cargoArgs += @('--test', $target) }
            $cargoArgs += @('--', '--nocapture', '--test-threads=1')
            Write-Host "Running cargo $($cargoArgs -join ' ')"
            $savedPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $output = @(& cargo @cargoArgs 2>&1 | ForEach-Object {
                    $line = $_.ToString()
                    Write-Host $line
                    $line
                })
                $cargoExit = $LASTEXITCODE
            } finally {
                $ErrorActionPreference = $savedPreference
            }
            if ($cargoExit -ne 0) { throw "Corpus tests failed for $($package.name): cargo exit $cargoExit" }
            $log = $output -join "`n"
            if ($log -match '(?im)\bskip(?:ping)?(?:[: ]|$)|nothing verified') {
                throw "Corpus tests reported skipped coverage for $($package.name)."
            }
            $summaries = [regex]::Matches($log, 'test result: ok\. ([0-9]+) passed;')
            if ($summaries.Count -ne @($package.tests).Count -or @($summaries | Where-Object { [int]$_.Groups[1].Value -eq 0 }).Count -ne 0) {
                throw "Expected nonempty test results for every $($package.name) corpus target."
            }
        }
    } finally {
        Pop-Location
    }
    Write-Host 'Required corpus tests passed.'
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        Invoke-Corpus -Root (Split-Path $PSScriptRoot -Parent) -Crates $Crate -PreflightOnly:$CheckOnly
    } catch {
        Write-Error $_ -ErrorAction Continue
        exit 1
    }
}
