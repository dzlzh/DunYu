$ErrorActionPreference = 'Stop'
$directory = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item $directory -ItemType Directory | Out-Null
try {
    foreach ($variant in @('baseline','candidate')) {
        $factor = if ($variant -eq 'baseline') { 1.0 } else { 0.5 }
        $pages = @(foreach ($i in 1..12) {
            @{handler_ms=7; measurement=@{counters=@{paint=@(1,30); 'paint.tab'=@(1,7)}}}
        })
        $runs = @(@{scenario='first_fullscreen'; handler_ms=100})
        foreach ($kind in @('reopen','card_to_fullscreen')) {
            foreach ($i in 1..10) { $runs += @{scenario=$kind;handler_ms=(100*$factor)} }
        }
        $samples = @(foreach ($i in 1..100) { 10*$factor })
        $report = @{protocol='performance-v2'; os='windows'; pages=$pages; rest=@{runs=$runs; monitors=@(@{rect=@(0,0,1920,1080);dpi=96}); high_contrast=$false; motion_enabled=$true; wall_ms=5000; cpu_ms=(100*$factor); animation=@{samples_ms=@{'paint.rest'=$samples}}}}
        foreach ($i in 1..3) { $report | ConvertTo-Json -Depth 15 | Set-Content (Join-Path $directory "benchmark-$variant-$i.json") -Encoding UTF8 }
    }
    & "$PSScriptRoot/compare-performance.ps1" -Directory $directory
    $result = Get-Content (Join-Path $directory 'performance-comparison.json') -Raw | ConvertFrom-Json
    if (!$result.all_performance_goals_met) { throw 'Expected passing comparison' }
    $file = Join-Path $directory 'benchmark-candidate-1.json'
    $report = Get-Content $file -Raw | ConvertFrom-Json
    $report.rest.cpu_ms = 10000
    $report | ConvertTo-Json -Depth 15 | Set-Content $file -Encoding UTF8
    # Median intentionally tolerates one outlier. Two slow runs must fail.
    Copy-Item $file (Join-Path $directory 'benchmark-candidate-2.json') -Force
    & "$PSScriptRoot/compare-performance.ps1" -Directory $directory
    $result = Get-Content (Join-Path $directory 'performance-comparison.json') -Raw | ConvertFrom-Json
    if ($result.goals.animation_cpu -or $result.all_performance_goals_met) { throw 'Regression was accepted' }
    $report.protocol = 'old'
    $report | ConvertTo-Json -Depth 15 | Set-Content $file -Encoding UTF8
    $rejected = $false
    try { & "$PSScriptRoot/compare-performance.ps1" -Directory $directory } catch { $rejected = $true }
    if (!$rejected) { throw 'Incompatible protocol accepted' }
    Write-Host 'Performance comparison tests passed.'
} finally { Remove-Item $directory -Recurse -Force }
