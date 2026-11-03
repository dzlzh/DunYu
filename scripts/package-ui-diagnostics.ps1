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
    Copy-Item LICENSE $package
    Copy-Item resources/fonts/OFL.txt $package
    Copy-Item docs/UI-DIAGNOSTICS.md "$package/README.md"
    Compress-Archive -Path "$package/dunyu-ui-tests.exe", "$package/run-ui-diagnostics.cmd", "$package/run-ui-diagnostics.ps1", "$package/LICENSE", "$package/OFL.txt", "$package/README.md", "$package/compare-ui-diagnostics.py" -DestinationPath target/DunYu-ui-diagnostics.zip -Force
    Write-Host 'Created target/DunYu-ui-diagnostics.zip'
} finally {
    $env:DUNYU_DIAGNOSTIC_REVISION = $previousRevision
    Pop-Location
}
