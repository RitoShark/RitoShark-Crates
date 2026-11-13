Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/test-corpus.ps1"

function Assert-Fails {
    param([scriptblock] $Action, [string] $Message)
    $failure = $null
    try { & $Action } catch { $failure = $_.Exception.Message }
    if ($null -eq $failure -or $failure -notlike "*$Message*") {
        throw "Expected failure containing '$Message'; got '$failure'."
    }
}

function cargo {
    $script:cargoCalls++
    $script:lastArgs = @($args)
    $global:LASTEXITCODE = $script:mockExit
    $script:mockOutput
}

$script:cargoCalls = 0
$script:mockExit = 0
$script:mockOutput = 'test result: ok. 1 passed; 0 failed; 0 ignored;'
$script:lastArgs = @()
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ("ritoshark-corpus-test-" + [guid]::NewGuid())
[IO.Directory]::CreateDirectory((Join-Path $root 'Sample-Files')) | Out-Null
[IO.Directory]::CreateDirectory((Join-Path $root 'docs')) | Out-Null
[IO.Directory]::CreateDirectory((Join-Path $root 'crates/rs_probe/tests')) | Out-Null
$fixture = Join-Path $root 'Sample-Files/probe.bin'
$manifestPath = Join-Path $root 'docs/fixtures.json'
try {
    [IO.File]::WriteAllBytes($fixture, [byte[]]@(1, 2, 3, 4))
    [IO.File]::WriteAllText((Join-Path $root 'crates/rs_probe/tests/real_files.rs'), '')
    $manifest = @{
        version = 1
        packages = @(@{
            name = 'rs_probe'
            tests = @('real_files')
            fixtures = @(@{
                name = 'probe.bin'
                bytes = 4
                sha256 = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash
            })
        })
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath
    Invoke-Corpus -Root $root -PreflightOnly
    if ($script:cargoCalls -ne 0) { throw 'Preflight ran cargo.' }

    Remove-Item -LiteralPath $fixture
    Assert-Fails { Invoke-Corpus -Root $root } 'probe.bin'
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    Assert-Fails { Invoke-Corpus -Root $root } 'expected a regular file'
    Remove-Item -LiteralPath $fixture
    [IO.File]::WriteAllBytes($fixture, [byte[]]@())
    Assert-Fails { Invoke-Corpus -Root $root } 'size mismatch'
    [IO.File]::WriteAllBytes($fixture, [byte[]]@(4, 3, 2, 1))
    Assert-Fails { Invoke-Corpus -Root $root } 'SHA-256 mismatch'
    if ($script:cargoCalls -ne 0) { throw 'Invalid fixtures reached cargo.' }

    [IO.File]::WriteAllBytes($fixture, [byte[]]@(1, 2, 3, 4))
    Assert-Fails { Invoke-Corpus -Root $root -Crates rs_typo } 'Unknown corpus crate'
    $script:mockExit = 7
    Assert-Fails { Invoke-Corpus -Root $root } 'cargo exit 7'
    $script:mockExit = 0
    $script:mockOutput = 'test result: ok. 0 passed; 0 failed; 0 ignored;'
    Assert-Fails { Invoke-Corpus -Root $root } 'nonempty test results'
    $script:mockOutput = @('skipping: fixture missing', 'test result: ok. 1 passed; 0 failed; 0 ignored;')
    Assert-Fails { Invoke-Corpus -Root $root } 'skipped coverage'
    $script:mockOutput = 'test result: ok. 1 passed; 0 failed; 0 ignored;'
    Invoke-Corpus -Root $root -Crates rs_probe
    if (($script:lastArgs -join ' ') -ne 'test -p rs_probe --all-features --locked --test real_files -- --nocapture --test-threads=1') {
        throw "Unexpected cargo invocation: $($script:lastArgs -join ' ')"
    }
    $manifest.packages[0].tests = @('real_files', 'extra')
    [IO.File]::WriteAllText((Join-Path $root 'crates/rs_probe/tests/extra.rs'), '')
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath
    Assert-Fails { Invoke-Corpus -Root $root } 'nonempty test results'
    $script:mockOutput = @($script:mockOutput, $script:mockOutput)
    Invoke-Corpus -Root $root
    $manifest.version = 2
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath
    Assert-Fails { Invoke-Corpus -Root $root } 'version-1 corpus manifest'
    $manifest.version = 1
    [IO.Directory]::CreateDirectory((Join-Path $root 'crates/rs_other/tests')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $root 'crates/rs_other/tests/real_files.rs'), '')
    $manifest.packages += @{
        name = 'rs_other'
        tests = @('real_files')
        fixtures = @(@{ name = 'absent.bin'; bytes = 4; sha256 = $manifest.packages[0].fixtures[0].sha256 })
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath
    Invoke-Corpus -Root $root -Crates rs_probe -PreflightOnly
    $callsBefore = $script:cargoCalls
    Assert-Fails { Invoke-Corpus -Root $root } 'absent.bin'
    if ($script:cargoCalls -ne $callsBefore) { throw 'Cargo ran before every selected package passed preflight.' }
    Write-Host 'Corpus runner regression checks passed.'
} finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if (-not $resolved.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -notlike 'ritoshark-corpus-test-*') {
        throw "Refusing to remove unexpected test directory: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
