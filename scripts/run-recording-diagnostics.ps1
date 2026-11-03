$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$exe = Join-Path $PSScriptRoot 'dunyu-recording-tests.exe'
if (!(Test-Path $exe)) { throw 'Missing dunyu-recording-tests.exe; extract the entire ZIP.' }
$run = Join-Path $PSScriptRoot ('recording-results-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $run | Out-Null
$oldMode = $env:DUNYU_RECORDING_MODE
$oldOutput = $env:DUNYU_RECORDING_OUTPUT
$results = @()
try {
    # Fresh processes avoid OnceLock state, observer registration and hook lifetimes leaking between modes.
    foreach ($mode in @('none', 'log', 'foreground', 'raw')) {
        $dir = Join-Path $run $mode
        New-Item -ItemType Directory -Path $dir | Out-Null
        $stdout = Join-Path $dir 'stdout.log'
        $stderr = Join-Path $dir 'stderr.log'
        $env:DUNYU_RECORDING_MODE = $mode
        $env:DUNYU_RECORDING_OUTPUT = $dir
        Write-Host "Running comparison: $mode"
        $proc = Start-Process -FilePath $exe -ArgumentList '--exact desktop::ui::capture_tests::recording_real_input --ignored --test-threads=1 --nocapture' -PassThru -NoNewWindow -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $handle = $proc.Handle
        $timedOut = !$proc.WaitForExit(60000)
        if ($timedOut) { $proc.Kill() }
        $proc.WaitForExit()
        $code = $proc.ExitCode
        Get-Content $stdout
        Get-Content $stderr
        $summary = $null
        $failures = 0
        $checks = Join-Path $dir 'checks.jsonl'
        if (Test-Path $checks) {
            # Survive a truncated final line, and preserve failures even without a summary.
            foreach ($line in Get-Content $checks) {
                try {
                    $check = $line | ConvertFrom-Json
                    if ($null -ne $check.passed -and $check.passed -eq $false) { $failures++ }
                } catch { }
            }
        }
        $summaryPath = Join-Path $dir 'summary.json'
        if (Test-Path $summaryPath) {
            try { $summary = Get-Content $summaryPath -Raw | ConvertFrom-Json } catch { }
        }
        if ($null -ne $summary) { $failures = [Math]::Max($failures, [int]$summary.failures) }
        $complete = $null -ne $summary -and $summary.complete -eq $true -and !$timedOut
        if ($failures -gt 0 -or ($null -ne $summary -and $summary.status -eq 'FAIL')) { $status = 'FAIL' }
        elseif ($complete -and $summary.status -eq 'PASS' -and $null -ne $code -and $code -eq 0) { $status = 'PASS' }
        else { $status = 'INCOMPLETE' }
        $results += [pscustomobject]@{
            mode = $mode; status = $status; complete = $complete; failures = $failures
            incomplete_cases = $(if ($null -ne $summary) { $summary.incomplete_cases } else { $null })
            exit_code = $code; timed_out = $timedOut
        }
        Write-Host "$mode : $status"
    }
} finally {
    $env:DUNYU_RECORDING_MODE = $oldMode
    $env:DUNYU_RECORDING_OUTPUT = $oldOutput
}
$status = 'PASS'
if (@($results | Where-Object { $_.status -eq 'FAIL' }).Count -gt 0) { $status = 'FAIL' }
elseif (@($results | Where-Object { $_.status -ne 'PASS' }).Count -gt 0) { $status = 'INCOMPLETE' }
[pscustomobject]@{ status = $status; modes = $results } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $run 'comparison.json') -Encoding UTF8
Write-Host "Comparison: $status. Send the entire directory: $run"
Write-Host 'Automated recording checks do not verify physical Win+Q takeover.'
if ($status -eq 'FAIL') { exit 1 }
if ($status -eq 'INCOMPLETE') { exit 2 }
exit 0
