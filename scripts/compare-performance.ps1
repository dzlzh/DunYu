param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function P95($values) {
    $sorted = @($values | ForEach-Object { [double]$_ } | Sort-Object)
    if (!$sorted.Count) { throw 'Missing performance samples' }
    return $sorted[[Math]::Ceiling($sorted.Count * 0.95) - 1]
}
function Median($values) {
    $sorted = @($values | ForEach-Object { [double]$_ } | Sort-Object)
    if (!$sorted.Count) { throw 'Missing performance samples' }
    return $sorted[[int][Math]::Floor($sorted.Count / 2)]
}
function Summarize($reports) {
    $runs = @($reports | ForEach-Object { $_.rest.runs })
    $pages = @($reports | ForEach-Object { $_.pages })
    $paints = @($reports | ForEach-Object { $_.rest.animation.samples_ms.'paint.rest' })
    return [ordered]@{
        first_fullscreen_median_ms = Median @($runs | Where-Object scenario -eq 'first_fullscreen' | ForEach-Object handler_ms)
        reopen_p95_ms = P95 @($runs | Where-Object scenario -eq 'reopen' | ForEach-Object handler_ms)
        card_to_fullscreen_p95_ms = P95 @($runs | Where-Object scenario -eq 'card_to_fullscreen' | ForEach-Object handler_ms)
        animation_paint_p95_ms = P95 $paints
        animation_cpu_median_ms = Median @($reports | ForEach-Object { $_.rest.cpu_ms })
        animation_frame_count = $paints.Count
        page_handler_p95_ms = P95 @($pages | ForEach-Object handler_ms)
        page_paint_p95_ms = P95 @($pages | ForEach-Object { $_.measurement.counters.paint[1] })
        page_navigation_p95_ms = P95 @($pages | ForEach-Object { $_.measurement.counters.'paint.tab'[1] })
    }
}
$groups = @{}
$environment = $null
foreach ($variant in @('baseline','candidate')) {
    $reports = @(foreach ($i in 1..3) {
        $report = Get-Content -LiteralPath (Join-Path $Directory "benchmark-$variant-$i.json") -Raw | ConvertFrom-Json
        if ($report.protocol -ne 'performance-v2') { throw 'Incompatible measurement protocol' }
        if (@($report.pages).Count -ne 12 -or @($report.rest.runs).Count -ne 21) { throw 'Incomplete benchmark' }
        foreach ($scenario in @('reopen','card_to_fullscreen')) {
            if (@($report.rest.runs | Where-Object scenario -eq $scenario).Count -ne 10) { throw "Missing $scenario samples" }
        }
        if ($report.rest.wall_ms -lt 4900 -or $report.rest.wall_ms -gt 6000) { throw 'Invalid animation observation interval' }
        $settings = @($report.os, $report.rest.monitors, $report.rest.high_contrast, $report.rest.motion_enabled) | ConvertTo-Json -Depth 10 -Compress
        if ($null -eq $environment) { $environment = $settings }
        if ($settings -ne $environment) { throw 'Environment differs between benchmark runs' }
        if ($report.rest.high_contrast -or !$report.rest.motion_enabled) { throw 'Animation performance requires normal contrast and enabled motion' }
        $report
    })
    $groups[$variant] = Summarize $reports
}
$b = $groups.baseline; $c = $groups.candidate
$goals = [ordered]@{
    fullscreen_transition = $c.card_to_fullscreen_p95_ms -le $b.card_to_fullscreen_p95_ms * 0.7
    first_use_no_regression = $c.first_fullscreen_median_ms -le $b.first_fullscreen_median_ms * 1.1
    reopen_no_regression = $c.reopen_p95_ms -le $b.reopen_p95_ms * 1.1
    animation_paint = $c.animation_paint_p95_ms -le $b.animation_paint_p95_ms * 0.7
    animation_cpu = $c.animation_cpu_median_ms -le $b.animation_cpu_median_ms * 0.7
    animation_frames = $c.animation_frame_count -ge $b.animation_frame_count * 0.9
    page_handler = $c.page_handler_p95_ms -le 8.03
    page_paint = $c.page_paint_p95_ms -le 32.94
    page_navigation = $c.page_navigation_p95_ms -le 11.13
}
$result = [ordered]@{baseline=$b; candidate=$c; goals=$goals; all_performance_goals_met=(@($goals.Values | Where-Object { !$_ }).Count -eq 0); note='Three fresh processes per variant; inclusive counters overlap. Review capture gaps and actual images separately.'}
$result | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $Directory 'performance-comparison.json') -Encoding UTF8
$result | ConvertTo-Json -Depth 10 | Write-Host
