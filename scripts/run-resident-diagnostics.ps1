param([ValidateRange(1,3)][int]$Rounds=3, [string]$BaselineDirectory, [string]$CandidateDirectory, [switch]$SelfTest,
 [ValidateSet('cold','warm','rest','message','fullscreen','pending1','pending5')][string[]]$Scenarios=@('cold','warm','rest','message','fullscreen','pending1','pending5'),
 [string]$BaselineExecutable, [switch]$ProfileFullscreen, [string]$SummarizeDirectory)
if ($SummarizeDirectory -and @($PSBoundParameters.Keys | Where-Object { $_ -ne 'SummarizeDirectory' }).Count) { throw 'Summary recovery cannot be combined with other options' }
if ($ProfileFullscreen -and ($BaselineExecutable -or $BaselineDirectory -or $CandidateDirectory)) { throw 'Profiling cannot be combined with comparison' }
if ($ProfileFullscreen) { $Scenarios=@('fullscreen') }
$Scenarios=@($Scenarios | Select-Object -Unique)
if (!$Scenarios.Count) { throw 'Select at least one scenario' }
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
function Stats($values) {
    $v=@($values | Sort-Object)
    if (!$v.Count) { throw 'Missing samples' }
    return [ordered]@{median=$v[[int][Math]::Floor($v.Count/2)]; p95=$v[[int][Math]::Ceiling($v.Count*.95)-1]; max=$v[-1]; min=$v[0]}
}
function EnvironmentKey($report) {
    @($report.environment,$report.monitors,$report.dpi,$report.high_contrast,$report.motion_enabled,$report.parameters) | ConvertTo-Json -Depth 12 -Compress
}
function Wait-ResidentProcess($process, [int]$timeoutSeconds, $sample) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while (!$process.WaitForExit(1000)) {
        if ($clock.Elapsed.TotalSeconds -gt $timeoutSeconds) { throw 'Resident process timeout' }
        if ($null -ne $sample) { & $sample }
    }
    $process.WaitForExit()
    if ($null -eq $process.ExitCode) { throw 'Missing child exit code' }
    return [int]$process.ExitCode
}
function Comparable($baseline,$candidate) {
    return $baseline.environment_key -eq $candidate.environment_key -and $baseline.rounds -eq $candidate.rounds
}
function Validate-StrokeCache($report) {
    if (!$report.PSObject.Properties['stroke_cache']) { return 'not_reported' }
    $cache=$report.stroke_cache
    if (!$cache.passed) { throw 'Stroke cache validation failed' }
    $windows=@($cache.measurement.PSObject.Properties)
    if ($report.scenario -eq 'fullscreen' -and !$windows.Count) { throw 'Missing stroke cache windows' }
    $applicable=$false
    foreach ($window in $windows) {
        $v=$window.Value
        $warm=$cache.warmup.PSObject.Properties[$window.Name].Value
        foreach ($phase in @($warm,$v)) {
            foreach ($name in @('builds','hits','draws','overlap_fallbacks','build_failures','copy_failures')) {
                $value=$phase.$name
                if ($null -eq $value -or [double]::IsNaN([double]$value) -or [double]::IsInfinity([double]$value) -or $value -lt 0) { throw 'Invalid stroke cache count' }
            }
        }
        if ($v.applicable) {
            $applicable=$true
            if (!$v.ready -or $v.hits -le 0 -or ($warm.builds+$v.builds) -le 0 -or
                ($warm.build_failures+$v.build_failures+$warm.copy_failures+$v.copy_failures) -ne 0) { throw 'Stroke cache validation failed' }
        }
    }
    if ($applicable) { return 'passed' }
    return 'not_applicable'
}
function Summarize($directory, $protocol = 'resident-v1', [switch]$Recover) {
    $manifest=Get-Content (Join-Path $directory 'manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.protocol -ne $protocol -or (!$manifest.complete -and !$Recover)) { throw 'Incomplete resident run' }
    $summary=[ordered]@{}
    $selected=@(if ($manifest.PSObject.Properties['scenarios']) { $manifest.scenarios } else { 'cold','warm','rest','message','fullscreen','pending1','pending5' })
    if (!$selected.Count) { throw 'Missing scenarios' }
    foreach ($name in $selected) { if ($name -notin @('cold','warm','rest','message','fullscreen','pending1','pending5')) { throw 'Unknown scenario' } }
    if ($Recover) {
        if ($manifest.rounds -notin @(1,2,3)) { throw 'Invalid rounds' }
        $logPath=Join-Path $directory 'run.log'; $variant='candidate'
        if (!(Test-Path $logPath)) { $variant=Split-Path $directory -Leaf; $logPath=Join-Path (Split-Path $directory -Parent) 'run.log' }
        $runLog=Get-Content $logPath -Raw
    }
    foreach ($scenario in $selected) {
        $reports=@(foreach ($i in 1..$manifest.rounds) {
            $r=Get-Content (Join-Path $directory "$scenario-$i.json") -Raw | ConvertFrom-Json
            if ($r.protocol -ne $protocol -or !$r.complete -or $r.scenario -ne $scenario) { throw "Incomplete $scenario-$i" }
            if ($Recover) {
                if (!$manifest.executable_sha256 -or $r.executable_sha256 -ne $manifest.executable_sha256) { throw 'Executable hash mismatch' }
                if ($runLog -notmatch ('(?m)^END '+[regex]::Escape("$variant/$scenario-$i")+' exit=0\r?$')) { throw 'Missing successful exit record' }
                $side=Get-Content (Join-Path $directory "$scenario-$i.samples.json") -Raw | ConvertFrom-Json
                if (($side | ConvertTo-Json -Depth 8 -Compress) -cne ($r.samples | ConvertTo-Json -Depth 8 -Compress)) { throw 'Resource sample files differ' }
            }
            foreach ($value in @($r.wall_ms,$r.cpu_percent_one_core,$r.start_ms,$r.end_ms)) {
                if ($null -eq $value -or [double]::IsNaN([double]$value) -or [double]::IsInfinity([double]$value) -or $value -lt 0) { throw 'Invalid timing metric' }
            }
            if ($r.end_ms -le $r.start_ms) { throw 'Invalid observation timestamps' }
            if ($r.wall_ms -lt $r.parameters.sample_seconds*950 -or $r.wall_ms -gt $r.parameters.sample_seconds*1100) { throw 'Invalid observation interval' }
            $observed=@($r.samples | Where-Object { $_.at_ms -ge $r.start_ms -and $_.at_ms -le $r.end_ms })
            if ($observed.Count -lt [Math]::Max(3,$r.parameters.sample_seconds-3)) { throw 'Insufficient resource samples' }
            $previous=$r.start_ms
            foreach ($point in $observed) {
                if ($point.at_ms -lt $previous -or $point.at_ms-$previous -gt 3000) { throw 'Resource sampling gap' }
                foreach ($metric in @('working_set_bytes','private_bytes','gdi','user','handles')) { if ($null -eq $point.$metric -or [double]::IsNaN([double]$point.$metric) -or [double]::IsInfinity([double]$point.$metric) -or [double]$point.$metric -lt 0) { throw 'Invalid resource metric' } }
                $previous=$point.at_ms
            }
            if ($r.end_ms-$previous -gt 3000) { throw 'Missing end samples' }
            Validate-StrokeCache $r | Out-Null
            $r
        })
        $builds=@($reports | ForEach-Object build | Select-Object -Unique)
        if ($builds.Count -ne 1 -or !$builds[0]) { throw 'Missing or mixed build revisions' }
        $keys=@($reports | ForEach-Object { EnvironmentKey $_ } | Select-Object -Unique)
        if ($keys.Count -ne 1) { throw "Environment changed within $scenario" }
        $samples=@($reports | ForEach-Object { $r=$_; $_.samples | Where-Object { $_.at_ms -ge $r.start_ms -and $_.at_ms -le $r.end_ms } })
        if (!$samples.Count) { throw "No measurement samples for $scenario" }
        $metrics=[ordered]@{cpu_percent_one_core=(Stats @($reports | ForEach-Object cpu_percent_one_core))}
        foreach ($name in @('working_set_bytes','private_bytes','gdi','user','handles')) {
            $metrics[$name]=Stats @($samples | ForEach-Object { $_.$name })
        }
        foreach ($counter in @('window_enumerations','process_queries','startup_ticks')) {
            $available=@($reports | Where-Object { $_.PSObject.Properties['counts'] -and $_.counts.PSObject.Properties[$counter] })
            if ($available.Count -eq $reports.Count) {
                $metrics[$counter]=Stats @($reports | ForEach-Object { $_.counts.$counter })
                $metrics[$counter+'_per_second']=Stats @($reports | ForEach-Object { $_.counts.$counter * 1000.0 / $_.wall_ms })
            }
        }
        $entry=[ordered]@{environment_key=$keys[0]; metrics=$metrics; rounds=$reports.Count}
        $entry['stroke_cache']=[ordered]@{status=@($reports | ForEach-Object { Validate-StrokeCache $_ }); rounds=@($reports | ForEach-Object { if ($_.PSObject.Properties['stroke_cache']) { $_.stroke_cache } else { $null } })}
        if ($protocol -eq 'resident-profile-v1') {
            $groups=@{}
            foreach ($r in $reports) {
                if (!@($r.profile.windows.PSObject.Properties).Count) { throw 'Missing profile stages' }
                if ($scenario -eq 'fullscreen') {
                    $paints=0
                    foreach ($window in $r.profile.windows.PSObject.Properties) {
                        if ($window.Name -eq 'host') { continue }
                        $paints+=@($window.Value.'paint.total'.samples_ms).Count
                    }
                    if ($paints -le 0 -or $paints -ne $r.counts.rest_paints) { throw 'Profile paint count mismatch' }
                }
                foreach ($window in $r.profile.windows.PSObject.Properties) {
                    foreach ($stage in $window.Value.PSObject.Properties) {
                        $key=$window.Name+'/'+$stage.Name
                        if (!$groups.ContainsKey($key)) { $groups[$key]=New-Object 'System.Collections.Generic.List[double]' }
                        $stageSamples=@($stage.Value.samples_ms)
                        if (!$stageSamples.Count) { throw 'Empty profile stage' }
                        foreach ($ms in $stageSamples) {
                            $value=[double]$ms
                            if ($null -eq $ms -or [double]::IsNaN($value) -or [double]::IsInfinity($value) -or $value -lt 0) { throw 'Invalid stage duration' }
                            $groups[$key].Add($value)
                        }
                    }
                }
            }
            $layers=[ordered]@{}
            foreach ($key in @($groups.Keys | Sort-Object)) {
                $values=$groups[$key].ToArray()
                $layers[$key]=[ordered]@{count=$values.Count; total_ms=($values | Measure-Object -Sum).Sum; duration_ms=(Stats $values)}
            }
            $entry['profile_stages']=$layers
        }
        $summary[$scenario]=$entry
    }
    return $summary
}
function Save-ResidentSummary($directory,$summary,$protocol) {
    $json=Join-Path $directory 'summary.json'; $txt=Join-Path $directory 'summary.txt'
    $summary | ConvertTo-Json -Depth 20 | Set-Content ($json+'.tmp') -Encoding UTF8
    $lines=@("$protocol; CPU: one logical core = 100%. Stage times are inclusive wall time, not CPU shares.")
    foreach ($name in $summary.Keys) { $lines+="$name : " + ($summary[$name] | ConvertTo-Json -Depth 10 -Compress) }
    $lines | Set-Content ($txt+'.tmp') -Encoding UTF8
    Move-Item ($json+'.tmp') $json -Force; Move-Item ($txt+'.tmp') $txt -Force
}
if ($SummarizeDirectory) {
    $directory=(Resolve-Path $SummarizeDirectory).Path
    $manifestPath=Join-Path $directory 'manifest.json'
    $manifest=Get-Content $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.protocol -notin @('resident-v1','resident-profile-v1')) { throw 'Unknown resident protocol' }
    $summary=Summarize $directory $manifest.protocol -Recover
    $backup=Join-Path $directory 'manifest.before-summary.json'
    if (!(Test-Path $backup)) { Copy-Item $manifestPath $backup }
    Save-ResidentSummary $directory $summary $manifest.protocol
    $manifest.complete=$true
    $manifest | ConvertTo-Json -Depth 10 | Set-Content ($manifestPath+'.tmp') -Encoding UTF8
    Move-Item ($manifestPath+'.tmp') $manifestPath -Force
    Write-Host "SUMMARY RECOVERED: $directory"
    exit 0
}
if ($SelfTest) {
    $fixture='{"scenario":"fullscreen","stroke_cache":{"passed":true,"warmup":{"window":{"applicable":true,"ready":true,"builds":1,"hits":0,"draws":1,"overlap_fallbacks":1,"build_failures":0,"copy_failures":0}},"measurement":{"window":{"applicable":true,"ready":true,"builds":0,"hits":10,"draws":2,"overlap_fallbacks":2,"build_failures":0,"copy_failures":0}}}}' | ConvertFrom-Json
    if ((Validate-StrokeCache $fixture) -ne 'passed') { throw 'Warm cache rejected' }
    $fixture.stroke_cache.measurement.window.hits=0
    $rejected=$false; try { Validate-StrokeCache $fixture } catch { $rejected=$true }; if (!$rejected) { throw 'Zero cache hits accepted' }
    $fixture.stroke_cache.measurement.window.applicable=$false
    if ((Validate-StrokeCache $fixture) -ne 'not_applicable') { throw 'Inapplicable cache rejected' }
    if ((Validate-StrokeCache ('{"scenario":"fullscreen"}' | ConvertFrom-Json)) -ne 'not_reported') { throw 'Legacy cache report rejected' }
    $fixture.stroke_cache.measurement.window.applicable=$true; $fixture.stroke_cache.measurement.window.hits=10
    $fixture.stroke_cache.warmup.window.build_failures=1
    $rejected=$false; try { Validate-StrokeCache $fixture } catch { $rejected=$true }; if (!$rejected) { throw 'Warmup cache failure accepted' }
    $s=Stats @(1,2,3,4,5)
    if ($s.median -ne 3 -or $s.p95 -ne 5 -or $s.min -ne 1) { throw 'Stats failed' }
    $rejected=$false; try { Stats @() } catch { $rejected=$true }; if (!$rejected) { throw 'Missing data accepted' }
    $dir=Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item $dir -ItemType Directory | Out-Null
    try {
        if (Comparable @{environment_key='one';rounds=3} @{environment_key='two';rounds=3}) { throw 'Environment mismatch accepted' }
        if (Comparable @{environment_key='one';rounds=3} @{environment_key='one';rounds=1}) { throw 'Round mismatch accepted' }
        $shell=Join-Path $PSHOME 'powershell.exe'
        if (!(Test-Path $shell)) { $shell=Join-Path $PSHOME 'pwsh.exe' }
        foreach ($case in @(@{name='success';body='exit 0';code=0;timeout=15},@{name='failure';body="[Console]::Error.WriteLine('failure detail'); exit 7";code=7;timeout=15},@{name='timeout';body='Start-Sleep -Seconds 30';code=-1;timeout=1})) {
            $script=Join-Path $dir ($case.name+'.ps1'); Set-Content $script $case.body
            $stderr=Join-Path $dir ($case.name+'.stderr')
            $child=Start-Process $shell -ArgumentList ('-NoProfile -NonInteractive -File "'+$script+'"') -PassThru -NoNewWindow -RedirectStandardError $stderr
            $childHandle=$child.Handle
            try {
                $timedOut=$false; $code=$null
                try { $code=Wait-ResidentProcess $child $case.timeout $null } catch { if ($_.Exception.Message -ne 'Resident process timeout') { throw }; $timedOut=$true }
                if ($case.code -eq -1) { if (!$timedOut) { throw 'Timeout not detected' } }
                elseif ($timedOut -or $code -ne $case.code) { throw 'Wrong process result' }
                if ($case.name -eq 'failure' -and (Get-Content $stderr -Raw) -notmatch 'failure detail') { throw 'Lost stderr' }
            } finally { if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }; $child.Dispose() }
        }
        '{"protocol":"resident-v1","complete":false}'  | Set-Content (Join-Path $dir 'manifest.json')
        $rejected=$false; try { Summarize $dir } catch { $rejected=$true }; if (!$rejected) { throw 'Incomplete run accepted' }
        '{"protocol":"resident-v1","complete":true,"rounds":1}' | Set-Content (Join-Path $dir 'manifest.json')
        foreach ($scenario in @('cold','warm','rest','message','fullscreen','pending1','pending5')) {
            $r=@{protocol='resident-v1';complete=$true;build='fixture';scenario=$scenario;wall_ms=5000;start_ms=0;end_ms=5000;cpu_percent_one_core=1;environment=@{machine='fixture'};monitors=@();dpi=96;high_contrast=$false;motion_enabled=$true;parameters=@{sample_seconds=5};samples=@(foreach($t in 1..5){@{at_ms=$t*1000;working_set_bytes=10;private_bytes=20;handles=3;gdi=4;user=5}})}
            $r | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $dir "$scenario-1.json")
        }
        $summary=Summarize $dir
        if ($summary.Count -ne 7 -or $summary['cold'].metrics.private_bytes.median -ne 20) { throw 'Report summary failed' }
        '{"protocol":"resident-v1","complete":true,"rounds":1,"scenarios":["cold"]}' | Set-Content (Join-Path $dir 'manifest.json')
        $subset=Summarize $dir
        if ($subset.Count -ne 1 -or !$subset.Contains('cold')) { throw 'Scenario subset failed' }
        foreach ($shape in @('"cold"','["cold","warm"]')) {
            ('{"protocol":"resident-v1","complete":true,"rounds":1,"scenarios":'+$shape+'}') | Set-Content (Join-Path $dir 'manifest.json')
            $shapeSummary=Summarize $dir
            if (!$shapeSummary.Contains('cold')) { throw 'Scenario normalization failed' }
        }
        '{"protocol":"resident-v1","complete":true,"rounds":1,"scenarios":["cold"]}' | Set-Content (Join-Path $dir 'manifest.json')
        $rejected=$false; try { Summarize $dir 'resident-profile-v1' } catch { $rejected=$true }; if (!$rejected) { throw 'Profile/normal mixing accepted' }
        $path=Join-Path $dir 'cold-1.json'; $r=Get-Content $path -Raw | ConvertFrom-Json
        $r.protocol='resident-profile-v1'
        $r | Add-Member -NotePropertyName profile -NotePropertyValue @{windows=@{host=@{tick=@{samples_ms=@(1.0,2.0)}}}}
        $r | ConvertTo-Json -Depth 15 | Set-Content $path
        '{"protocol":"resident-profile-v1","complete":true,"rounds":1,"scenarios":["cold"]}' | Set-Content (Join-Path $dir 'manifest.json')
        $profileSummary=Summarize $dir 'resident-profile-v1'
        if ($profileSummary['cold'].profile_stages['host/tick']['count'] -ne 2) { throw 'Profile summary failed' }
        $r | Add-Member -NotePropertyName executable_sha256 -NotePropertyValue 'fixture-hash'
        $r | ConvertTo-Json -Depth 15 | Set-Content $path
        $r.samples | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $dir 'cold-1.samples.json')
        'END candidate/cold-1 exit=0' | Set-Content (Join-Path $dir 'run.log')
        $manifest=@{protocol='resident-profile-v1';complete=$false;rounds=1;scenarios=@('cold');executable_sha256='fixture-hash'}
        $manifest | ConvertTo-Json | Set-Content (Join-Path $dir 'manifest.json')
        $recovered=Summarize $dir 'resident-profile-v1' -Recover
        if ($recovered['cold'].profile_stages['host/tick'].count -ne 2) { throw 'Recovery failed' }
        Save-ResidentSummary $dir $recovered 'resident-profile-v1'
        foreach ($case in @('empty','missing-round','hash','exit','gap','profile')) {
            $bad=$r | ConvertTo-Json -Depth 15 | ConvertFrom-Json
            $badManifest=$manifest.Clone()
            switch ($case) {
                'empty' { $badManifest.scenarios=@() }
                'missing-round' { $badManifest.rounds=2 }
                'hash' { $bad.executable_sha256='wrong' }
                'exit' { 'END candidate/cold-1 exit=7' | Set-Content (Join-Path $dir 'run.log') }
                'gap' { $bad.samples=@($bad.samples[0],$bad.samples[-1]) }
                'profile' { $bad.profile.windows=@{} }
            }
            $badManifest | ConvertTo-Json | Set-Content (Join-Path $dir 'manifest.json')
            $bad | ConvertTo-Json -Depth 15 | Set-Content $path
            $rejected=$false; try { Summarize $dir 'resident-profile-v1' -Recover } catch { $rejected=$true }
            if (!$rejected) { throw "Recovery accepted $case" }
            'END candidate/cold-1 exit=0' | Set-Content (Join-Path $dir 'run.log')
        }
        $r.protocol='resident-v1'; $r.samples=@()
        '{"protocol":"resident-v1","complete":true,"rounds":1,"scenarios":["cold"]}' | Set-Content (Join-Path $dir 'manifest.json')
        $r | ConvertTo-Json -Depth 10 | Set-Content $path
        $rejected=$false; try { Summarize $dir } catch { $rejected=$true }; if (!$rejected) { throw 'Missing resource samples accepted' }

    } finally { Remove-Item $dir -Recurse -Force }
    Write-Host 'Resident report self-tests passed'; exit 0
}
function Compare-Resident($BaselineDirectory,$CandidateDirectory) {
    $b=Summarize $BaselineDirectory; $c=Summarize $CandidateDirectory
    $comparison=[ordered]@{}
    if ((@($b.Keys | Sort-Object) -join '|') -cne (@($c.Keys | Sort-Object) -join '|')) { throw 'Scenario sets differ' }
    foreach ($name in $b.Keys) {
        $same=Comparable $b[$name] $c[$name]
        $comparison[$name]=[ordered]@{comparable=$same; baseline=$b[$name].metrics; candidate=$c[$name].metrics; differences=$null}
        if ($same) {
            $delta=[ordered]@{}
            foreach ($metric in $b[$name].metrics.Keys) { if ($c[$name].metrics.Contains($metric)) { $delta[$metric]=$c[$name].metrics[$metric].median-$b[$name].metrics[$metric].median } }
            $comparison[$name].differences=$delta
        }
    }
    $comparison | ConvertTo-Json -Depth 15 | Set-Content (Join-Path $CandidateDirectory 'comparison.json') -Encoding UTF8
    Write-Host 'Comparison saved. Values and differences only; no optimization pass threshold.'
}
if ($BaselineDirectory -or $CandidateDirectory) {
    if (!$BaselineDirectory -or !$CandidateDirectory) { throw 'Both comparison directories are required' }
    Compare-Resident $BaselineDirectory $CandidateDirectory
    exit 0
}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ResidentResources {
 [DllImport("user32.dll", SetLastError=true)] public static extern uint GetGuiResources(IntPtr process, uint flags);
 [DllImport("kernel32.dll")] public static extern void SetLastError(uint error);
}
'@
function GuiCount($handle,$flag) {
    [ResidentResources]::SetLastError(0)
    $count=[ResidentResources]::GetGuiResources($handle,$flag)
    if ($count -eq 0 -and [Runtime.InteropServices.Marshal]::GetLastWin32Error() -ne 0) { throw 'GetGuiResources failed' }
    return $count
}
$directory=Join-Path $PSScriptRoot ('resident-results-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'-'+$PID)
New-Item $directory -ItemType Directory | Out-Null
$log=Join-Path $directory 'run.log'
$protocol=if ($ProfileFullscreen) { 'resident-profile-v1' } else { 'resident-v1' }
$variants=[ordered]@{candidate=(Join-Path $PSScriptRoot 'dunyu-ui-tests.exe')}
if ($BaselineExecutable) { $variants=[ordered]@{baseline=(Resolve-Path $BaselineExecutable).Path; candidate=$variants.candidate} }
$directories=@{}; $manifests=@{}
foreach ($variant in $variants.Keys) {
    $dest=if ($BaselineExecutable) { Join-Path $directory $variant } else { $directory }
    New-Item $dest -ItemType Directory -Force | Out-Null
    $directories[$variant]=$dest
    $manifests[$variant]=[ordered]@{protocol=$protocol;complete=$false;rounds=$Rounds;scenarios=$Scenarios;executable_sha256=(Get-FileHash $variants[$variant] -Algorithm SHA256).Hash}
    $manifests[$variant] | ConvertTo-Json | Set-Content (Join-Path $dest 'manifest.json') -Encoding UTF8
}
$os=Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$environment=[ordered]@{machine=$env:COMPUTERNAME; windows_build=$os.CurrentBuildNumber; ubr=$os.UBR; architecture=$env:PROCESSOR_ARCHITECTURE; cpu=$env:PROCESSOR_IDENTIFIER; logical_processors=[Environment]::ProcessorCount}
$oldProfile=$env:DUNYU_RESIDENT_PROFILE; $env:DUNYU_RESIDENT_PROFILE=if ($ProfileFullscreen) { '1' } else { '0' }
$oldScenario=$env:DUNYU_RESIDENT_SCENARIO; $oldReport=$env:DUNYU_RESIDENT_REPORT
try {
    foreach ($i in 1..$Rounds) {
        foreach ($scenario in $Scenarios) {
          foreach ($variant in $variants.Keys) {
            $dest=$directories[$variant]; $manifest=$manifests[$variant]
            $name="$scenario-$i"; $reportPath=Join-Path $dest "$name.json"
            $env:DUNYU_RESIDENT_SCENARIO=$scenario; $env:DUNYU_RESIDENT_REPORT=$reportPath
            $stdout=Join-Path $dest "$name.stdout.log"; $stderr=Join-Path $dest "$name.stderr.log"
            "START $variant/$name $([DateTime]::UtcNow.ToString('o'))" | Tee-Object -FilePath $log -Append
            $process=$null; $samples=New-Object 'System.Collections.Generic.List[object]'; $samplingError=$null
            try {
                $process=Start-Process -FilePath $variants[$variant] -ArgumentList '--exact desktop::ui::resident_diagnostics::resident_baseline --ignored --test-threads=1 --nocapture' -WorkingDirectory $dest -NoNewWindow -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
                $handle=$process.Handle
                $samplingErrors=New-Object 'System.Collections.Generic.List[string]'
                $code=Wait-ResidentProcess $process 120 {
                    try {
                        $process.Refresh()
                        $samples.Add([ordered]@{at_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); working_set_bytes=$process.WorkingSet64; private_bytes=$process.PrivateMemorySize64; handles=$process.HandleCount; gdi=(GuiCount $handle 0); user=(GuiCount $handle 1)})
                    } catch { $samplingErrors.Add($_.Exception.Message) }
                }
                if ($samplingErrors.Count) { $samplingError=$samplingErrors -join '; ' }
                if ($null -eq $code -or $code -ne 0) { throw "Test failed: $name exit=$code" }
                if ($samplingError) { throw "Resource sampling failed: $samplingError" }
                $r=Get-Content $reportPath -Raw | ConvertFrom-Json
                if (!$r.complete) { throw "Incomplete report: $name" }
                $r | Add-Member -NotePropertyName executable_sha256 -NotePropertyValue $manifest.executable_sha256
                $r | Add-Member -NotePropertyName environment -NotePropertyValue $environment
                $r | Add-Member -NotePropertyName samples -NotePropertyValue @($samples.ToArray())
                $r | ConvertTo-Json -Depth 15 | Set-Content $reportPath -Encoding UTF8
                "END $variant/$name exit=$code" | Tee-Object -FilePath $log -Append
            } finally {
                $samples.ToArray() | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $dest "$name.samples.json") -Encoding UTF8
                if ($null -ne $process) { if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }; $process.Dispose() }
            }
        }
    }
    }
    foreach ($variant in $variants.Keys) {
        $dest=$directories[$variant]; $manifest=$manifests[$variant]
        $manifest.complete=$true
        $manifest | ConvertTo-Json | Set-Content (Join-Path $dest 'manifest.json') -Encoding UTF8
        $summary=Summarize $dest $protocol
        Save-ResidentSummary $dest $summary $protocol
    }
    if ($BaselineExecutable) { Compare-Resident $directories.baseline $directories.candidate }
    Write-Host "COMPLETED: $directory"
} catch {
    foreach ($variant in $variants.Keys) {
        $manifests[$variant].complete=$false
        $manifests[$variant] | ConvertTo-Json | Set-Content (Join-Path $directories[$variant] 'manifest.json') -Encoding UTF8
    }
    "FAILED: $($_.Exception.Message)" | Tee-Object -FilePath $log -Append
    throw
} finally { $env:DUNYU_RESIDENT_PROFILE=$oldProfile; $env:DUNYU_RESIDENT_SCENARIO=$oldScenario; $env:DUNYU_RESIDENT_REPORT=$oldReport }
