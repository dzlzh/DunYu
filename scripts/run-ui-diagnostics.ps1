param([switch]$SelfTest)
$ErrorActionPreference = 'Stop'

function Invoke-DiagnosticStage {
    param([string]$Name, [string]$Executable, [string]$Arguments,
          [int]$TimeoutSeconds, [string]$Directory, [string]$Log)
    $stdout = Join-Path $Directory "$Name.stdout.log"
    $stderr = Join-Path $Directory "$Name.stderr.log"
    $start = "STAGE_START $Name $([DateTime]::UtcNow.ToString('o'))"
    Write-Host $start
    Add-Content -LiteralPath $Log -Value $start -Encoding UTF8
    $process = $null
    $code = 1
    $status = 'launch-error'
    try {
        $process = Start-Process -FilePath $Executable -ArgumentList $Arguments -WorkingDirectory $Directory -NoNewWindow -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        # Keep the handle available for ExitCode on Windows PowerShell 5.1.
        $processHandle = $process.Handle
        if ($process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.WaitForExit()
            $code = $process.ExitCode
            if ($null -eq $code) { throw 'Process exited without a readable exit code' }
            $status = 'exited'
        } else {
            $status = 'timeout'
            $code = 124
            if (!$process.HasExited) { $process.Kill() }
            $process.WaitForExit()
        }
    } catch {
        $status = "runner-error: $($_.Exception.Message)"
        $code = 1
    } finally {
        if ($null -ne $process) {
            if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
            $process.Dispose()
        }
    }
    foreach ($stream in @($stdout, $stderr)) {
        if (Test-Path -LiteralPath $stream) {
            $heading = "--- $(Split-Path $stream -Leaf) ---"
            $content = Get-Content -LiteralPath $stream -Raw
            Write-Host $heading
            Write-Host $content
            Add-Content -LiteralPath $Log -Value $heading, $content -Encoding UTF8
        }
    }
    $end = "STAGE_END $Name $([DateTime]::UtcNow.ToString('o')) status=$status exit_code=$code"
    Write-Host $end
    Add-Content -LiteralPath $Log -Value $end -Encoding UTF8
    return [int]$code
}

if ($SelfTest) {
    $directory = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
    New-Item $directory -ItemType Directory | Out-Null
    try {
        $shell = Join-Path $PSHOME 'powershell.exe'
        if (!(Test-Path $shell)) { $shell = Join-Path $PSHOME 'pwsh.exe' }
        $log = Join-Path $directory 'runner.log'
        foreach ($case in @(
            @{ Name='success'; Command="Write-Output 'checks completed'; exit 0"; Timeout=15; Expected=0 },
            @{ Name='failure'; Command="Write-Output 'checks completed'; [Console]::Error.WriteLine('failure detail'); exit 1"; Timeout=15; Expected=1 },
            @{ Name='timeout'; Command='Start-Sleep -Seconds 30'; Timeout=1; Expected=124 }
        )) {
            $code = Invoke-DiagnosticStage $case.Name $shell ('-NoProfile -NonInteractive -Command "' + $case.Command + '"') $case.Timeout $directory $log
            if ($code -ne $case.Expected) { throw "Runner self-test failed: $($case.Name), code=$code" }
        }
        $text = Get-Content $log -Raw
        if ($text -notmatch 'failure detail' -or $text -notmatch 'status=timeout exit_code=124') { throw 'Runner lost failure/timeout details' }
        Write-Host 'Runner self-tests passed.'
    } finally { Remove-Item $directory -Recurse -Force }
    exit 0
}

$log = Join-Path $PSScriptRoot 'diagnostic-run.log'
Set-Content -LiteralPath $log -Value 'DunYu diagnostics: process exit and test runner result determine success.' -Encoding UTF8
$exe = Join-Path $PSScriptRoot 'dunyu-ui-tests.exe'
$stages = @(@{Name='buffer'; Filter='desktop::buffering::tests::buffer_reuses_size_replaces_and_releases_gdi_objects'; Timeout=120; Ignored=''})
$stages += @{Name='rest-pixels'; Filter='desktop::rest_ui::tests::cached_rest_matches_original'; Timeout=600; Ignored='--ignored'}
foreach ($i in 1..3) {
    $stages += @{Name="navigation-$i"; Filter='desktop::theme::navigation_tests::cached_navigation_matches_original'; Timeout=120; Ignored='--ignored'}
}
$stages += @{Name='navigation-real'; Filter='desktop::ui::capture_tests::real_navigation_uses_cache'; Timeout=120; Ignored='--ignored'}
$stages += @{Name='static-pixels'; Filter='desktop::ui::capture_tests::static_controls_match_original'; Timeout=120; Ignored='--ignored'}
$stages += @{Name='transitions' ; Filter='desktop::ui::diagnostics::ui_transition_diagnostics'; Timeout=600; Ignored='--ignored'}
foreach ($stage in $stages) {
    $arguments = "--exact $($stage.Filter) $($stage.Ignored) --test-threads=1 --nocapture"
    $code = Invoke-DiagnosticStage $stage.Name $exe $arguments $stage.Timeout $PSScriptRoot $log
    if ($code -ne 0) {
        Write-Host "FAILED: $($stage.Name), exit_code=$code. See diagnostic-run.log, stage logs and any rest-pixel-diagnostics-* folder. Later stages were not run."
        exit $code
    }
}
$baseline = Join-Path $PSScriptRoot 'dunyu-ui-baseline.exe'
if (Test-Path -LiteralPath $baseline) {
    $previousReport = $env:DUNYU_BENCH_REPORT
    try {
        foreach ($i in 1..3) {
            foreach ($variant in @('baseline','candidate')) {
                $name = "benchmark-$variant-$i"
                $env:DUNYU_BENCH_REPORT = Join-Path $PSScriptRoot "$name.json"
                Remove-Item -LiteralPath $env:DUNYU_BENCH_REPORT -ErrorAction SilentlyContinue
                $program = if ($variant -eq 'baseline') { $baseline } else { $exe }
                $code = Invoke-DiagnosticStage $name $program '--exact desktop::ui::diagnostics::ui_performance_benchmark --ignored --test-threads=1 --nocapture' 600 $PSScriptRoot $log
                if ($code -ne 0) { exit $code }
            }
        }
        & (Join-Path $PSScriptRoot 'compare-performance.ps1') -Directory $PSScriptRoot
    } finally { $env:DUNYU_BENCH_REPORT = $previousReport }
} else {
    Write-Host 'No calibrated baseline included; performance comparison was not run.'
}
Write-Host 'ALL STAGES COMPLETED. Review report.json, capture gaps and frames before visual/performance acceptance.'
exit 0
