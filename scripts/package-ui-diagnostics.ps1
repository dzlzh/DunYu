param([string]$Baseline, [string]$ResidentBaseline)
$ErrorActionPreference = 'Stop'
$previousRevision = $env:DUNYU_DIAGNOSTIC_REVISION
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $env:DUNYU_DIAGNOSTIC_REVISION = (git rev-parse HEAD).Trim()
    if (git status --porcelain) { $env:DUNYU_DIAGNOSTIC_REVISION += '-dirty' }
    $artifacts = cargo test --locked --release --bin dunyu --no-run --message-format=json
    if ($LASTEXITCODE -ne 0) { throw 'Diagnostic test build failed' }
    $test = $artifacts | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.target.name -eq 'dunyu' -and $_.executable } |
        Select-Object -Last 1
    if (!$test) { throw 'No diagnostic executable found' }
    $package = 'target/ui-diagnostic-package'
    New-Item $package -ItemType Directory -Force | Out-Null
    Copy-Item $test.executable "$package/dunyu-ui-tests.exe"
    Copy-Item scripts/run-ui-diagnostics.cmd $package
    Copy-Item scripts/run-ui-diagnostics.ps1 $package
    Copy-Item scripts/compare-ui-diagnostics.py $package
    Copy-Item scripts/compare-performance.ps1 $package
    Copy-Item LICENSE $package
    Copy-Item resources/fonts/OFL.txt $package
    Copy-Item docs/UI-DIAGNOSTICS.md "$package/README.md"
    if ($Baseline) { Copy-Item $Baseline "$package/dunyu-ui-baseline.exe" }
    $extra = @("$package/compare-performance.ps1")
    if ($Baseline) { $extra += "$package/dunyu-ui-baseline.exe" }
    $inputs = @("$package/dunyu-ui-tests.exe", "$package/run-ui-diagnostics.cmd", "$package/run-ui-diagnostics.ps1", "$package/LICENSE", "$package/OFL.txt", "$package/README.md", "$package/compare-ui-diagnostics.py") + $extra
    Compress-Archive -Path $inputs -DestinationPath target/DunYu-ui-diagnostics.zip -Force
    Write-Host 'Created target/DunYu-ui-diagnostics.zip'
    $resident = 'target/resident-diagnostic-package'
    New-Item $resident -ItemType Directory -Force | Out-Null
    Copy-Item $test.executable "$resident/dunyu-ui-tests.exe"
    Copy-Item scripts/run-resident-diagnostics.cmd,scripts/run-resident-diagnostics.ps1,LICENSE $resident
    Copy-Item resources/fonts/OFL.txt $resident
    Copy-Item docs/RESIDENT-DIAGNOSTICS.md "$resident/README.md"
    if ($ResidentBaseline) { Copy-Item $ResidentBaseline "$resident/dunyu-resident-baseline.exe" }
    else { Remove-Item "$resident/dunyu-resident-baseline.exe" -ErrorAction SilentlyContinue }
    Compress-Archive -Path "$resident/*" -DestinationPath target/DunYu-resident-diagnostics.zip -Force

} finally {
    $env:DUNYU_DIAGNOSTIC_REVISION = $previousRevision
    Pop-Location
}
