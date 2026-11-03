//! Opt-in interactive diagnostic harness. Never included in the application binary.
use super::*;
use serde_json::json;
use std::{
    cell::RefCell,
    path::Path,
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[path = "diagnostic_pixels.rs"]
mod pixels;
thread_local! {
    static SAMPLES: RefCell<HashMap<&'static str, Vec<f64>>> = RefCell::new(HashMap::new());
    static COUNTERS: RefCell<Option<HashMap<&'static str, (u64, f64)>>> = const { RefCell::new(None) };
}
// Additive candidate-only diagnostics. Old baselines have no producer and report
// available=false, rather than misleading zero cache misses.
thread_local! {
    static PAPER_STATS: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
}
pub(crate) fn paper_cache_event(key: (i32, i32, i32), event: &str, bytes: u64, entries: usize) {
    if !COUNTERS.with(|c| c.borrow().is_some()) {
        return;
    }
    PAPER_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let stats = stats
            .get_or_insert_with(|| json!({"available":true,"keys":{},"peak_retained_bytes":0}));
        stats["retained_bytes"] = bytes.into();
        stats["entries"] = entries.into();
        stats["peak_retained_bytes"] = stats["peak_retained_bytes"]
            .as_u64()
            .unwrap()
            .max(bytes)
            .into();
        let keys = stats["keys"].as_object_mut().unwrap();
        let name = format!("{}x{}:scene{}", key.0, key.1, key.2);
        if keys.len() < 256 || keys.contains_key(&name) {
            let counts = keys.entry(name).or_insert_with(|| json!({}));
            counts[event] = (counts[event].as_u64().unwrap_or(0) + 1).into();
        } else {
            stats["truncated"] = true.into();
        }
    });
}
pub(crate) fn capture_counters(action: impl FnOnce()) -> HashMap<&'static str, (u64, f64)> {
    PAPER_STATS.with(|s| *s.borrow_mut() = None);
    COUNTERS.with(|c| assert!(c.borrow_mut().replace(HashMap::new()).is_none()));
    action();
    COUNTERS.with(|c| c.borrow_mut().take().unwrap())
}
pub(crate) fn sample_action(action: impl FnOnce()) -> serde_json::Value {
    SAMPLES.with(|s| s.borrow_mut().clear());
    let counters = capture_counters(action);
    let samples = SAMPLES.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let paper = PAPER_STATS
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(|| json!({"available":false}));
    json!({"counters":counters,"samples_ms":samples,"paper_cache":paper})
}
pub(crate) struct Measurement {
    name: &'static str,
    start: Option<Instant>,
}
pub(crate) fn measure(name: &'static str) -> Measurement {
    Measurement {
        name,
        start: COUNTERS.with(|c| c.borrow().as_ref().map(|_| Instant::now())),
    }
}
impl Drop for Measurement {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            COUNTERS.with(|c| {
                if let Some(c) = c.borrow_mut().as_mut() {
                    let entry = c.entry(self.name).or_default();
                    entry.0 += 1;
                    let ms = start.elapsed().as_secs_f64() * 1000.;
                    entry.1 += ms;
                    SAMPLES.with(|samples| {
                        let mut samples = samples.borrow_mut();
                        let values = samples.entry(self.name).or_default();
                        if values.len() < 100_000 {
                            values.push(ms);
                        }
                    });
                }
            });
        }
    }
}
unsafe fn pump_for(duration: Duration) {
    let start = Instant::now();
    loop {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if start.elapsed() >= duration {
                break;
            }
        }
        if start.elapsed() >= duration {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
unsafe fn displayed(h: HWND) -> bool {
    IsWindowVisible(h) != 0 || !GetPropW(h, wide("SysSetRedraw").as_ptr()).is_null()
}
unsafe extern "system" fn owned_window(h: HWND, data: LPARAM) -> BOOL {
    let mut pid = 0;
    GetWindowThreadProcessId(h, &mut pid);
    if pid == GetCurrentProcessId()
        && displayed(h)
        && IsIconic(h) == 0
        && matches!(class(h).as_str(), "DunYu.Settings.1" | "DunYu.Rest.1")
    {
        (*(data as *mut Vec<usize>)).push(h as usize);
    }
    1
}
// Reject any intersecting visible window above the target (conservative for shaped
// windows). Check both before and after copying; moving occluders still require review.
unsafe fn unobscured(h: HWND, r: RECT) -> bool {
    if !displayed(h) || IsIconic(h) != 0 {
        return false;
    }
    let mut above = GetWindow(h, GW_HWNDPREV);
    while !above.is_null() {
        if displayed(above) && IsIconic(above) == 0 {
            let mut bounds: RECT = std::mem::zeroed();
            GetWindowRect(above, &mut bounds);
            if bounds.left < r.right
                && bounds.right > r.left
                && bounds.top < r.bottom
                && bounds.bottom > r.top
            {
                return false;
            }
        }
        above = GetWindow(above, GW_HWNDPREV);
    }
    r.left >= GetSystemMetrics(SM_XVIRTUALSCREEN)
        && r.top >= GetSystemMetrics(SM_YVIRTUALSCREEN)
        && r.right <= GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN)
        && r.bottom <= GetSystemMetrics(SM_YVIRTUALSCREEN) + GetSystemMetrics(SM_CYVIRTUALSCREEN)
}
struct Frame {
    ms: f64,
    capture_ms: f64,
    hwnd: usize,
    width: i32,
    height: i32,
    dpi: u32,
    data: Vec<u8>,
}
unsafe fn capture(h: HWND, ms: f64) -> Result<Frame, &'static str> {
    let capture_start = Instant::now();
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    MapWindowPoints(h, null_mut(), &mut r as *mut _ as *mut POINT, 2);
    let dpi = GetDpiForWindow(h).max(96);
    // Exclude the rounded popup corners and window frame.
    let inset = 16 * dpi as i32 / 96;
    r.left += inset;
    r.top += inset;
    r.right -= inset;
    r.bottom -= inset;
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    if width <= 0 || height <= 0 || !unobscured(h, r) {
        return Err("hidden, obscured or outside desktop");
    }
    if width as usize * height as usize > 16_000_000 {
        return Err("capture exceeds pixel budget");
    }
    let screen = GetDC(null_mut());
    let dc = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    if screen.is_null() || dc.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        if !screen.is_null() {
            ReleaseDC(null_mut(), screen);
        }
        return Err("GDI allocation failed");
    }
    let old = SelectObject(dc, bitmap);
    let copied = BitBlt(dc, 0, 0, width, height, screen, r.left, r.top, SRCCOPY);
    SelectObject(dc, old);
    let mut data = vec![0; width as usize * height as usize * 4];
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = 40;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let rows = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        data.as_mut_ptr() as _,
        &mut info,
        DIB_RGB_COLORS,
    );
    DeleteObject(bitmap);
    DeleteDC(dc);
    ReleaseDC(null_mut(), screen);
    if copied == 0 || rows != height || !unobscured(h, r) {
        return Err("capture failed or visibility changed");
    }
    Ok(Frame {
        ms,
        capture_ms: capture_start.elapsed().as_secs_f64() * 1000.,
        hwnd: h as usize,
        width,
        height,
        dpi,
        data,
    })
}
fn write_frame(frame: &Frame, path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"BM")?;
    f.write_all(&(54 + frame.data.len() as u32).to_le_bytes())?;
    f.write_all(&[0; 4])?;
    f.write_all(&54u32.to_le_bytes())?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&frame.width.to_le_bytes())?;
    f.write_all(&(-frame.height).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(&[0; 24])?;
    f.write_all(&frame.data)
}
unsafe fn action(app: &mut App, index: usize) -> String {
    let h = app.ui.as_ref().unwrap().window;
    match index {
        0..=17 => {
            app.page(index % 6);
            format!("page-{}", index % 6)
        }
        18..=21 => {
            app.ui_action(116).unwrap();
            "advanced-toggle".into()
        }
        22..=25 => {
            app.fill_rule(Some(index % 2));
            "rule-switch".into()
        }
        26 | 28 => {
            ShowWindow(h, SW_HIDE);
            "main-hide".into()
        }
        27 | 29 => {
            app.open_ui();
            "main-restore".into()
        }
        30 => {
            app.rest
                .configure(dunyu::rest::RestConfig::default(), app.host);
            app.rest.action(23, None, app.host);
            "rest-card".into()
        }
        31 => {
            app.rest.configure(
                dunyu::rest::RestConfig {
                    fullscreen: true,
                    ..Default::default()
                },
                app.host,
            );
            app.rest.action(23, None, app.host);
            "rest-fullscreen".into()
        }
        _ => {
            app.rest.close();
            "rest-close".into()
        }
    }
}
#[test]
#[ignore = "requires a visible unobscured interactive Windows desktop"]
fn ui_transition_diagnostics() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut fixture = super::capture_tests::fixture();
        let app = &mut fixture.app;
        // Test data only; never write configuration or register global shortcuts.
        app.config.rules = (0..2)
            .map(|i| SummonRule {
                name: format!("Diagnostic rule {i}"),
                enabled: true,
                executable: "diagnostic.exe".into(),
                arguments: String::new(),
                working_directory: String::new(),
                process_path: String::new(),
                class: String::new(),
                title: String::new(),
                hotkey: Hotkey::new(3, 65 + i),
                minimize: false,
                priority_hotkey: false,
            })
            .collect();
        app.refresh_list();
        app.fill_rule(Some(0));
        app.layout_ui();
        let h = app.ui.as_ref().unwrap().window;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let dir =
            std::path::PathBuf::from(format!("ui-diagnostics-{stamp}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        println!(
            "Diagnostics: {}. Keep all test windows visible; do not interact until finished.",
            dir.display()
        );
        pump_for(Duration::from_millis(800));
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(h, &mut client);
        let mut report = json!({"measurement_protocol":"transitions-v2","version":env!("CARGO_PKG_VERSION"),"build":option_env!("DUNYU_DIAGNOSTIC_REVISION"),
            "os":std::process::Command::new("cmd").args(["/C","ver"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()),
            "dpi":GetDpiForWindow(h),"client_size":[client.right,client.bottom],"requested_interval_ms":16,
            "notes":"Direct production action methods with normal message pumping. Handler time is not input-to-display latency. Layout/paint totals may overlap handler time. Mixed-frame flags require human review; animation and focus can trigger them. Timing pass runs before sampling, so sampling is warmed. Hidden/obscured windows are not captured. First use means first use within this prepared fixture, not cold application startup.","runs":[]});
        let mut timings = vec![];
        let mut visual_failures = Vec::new();
        let mut navigation_cache_failures = Vec::new();
        for sampling in [false, true] {
            app.rest.close();
            app.open_ui();
            app.page(0);
            app.ui.as_mut().unwrap().advanced = false;
            app.layout_ui();
            SetFocus(h);
            let mut window: RECT = std::mem::zeroed();
            GetWindowRect(h, &mut window);
            SetCursorPos(window.left + 5, window.top + 5);
            pump_for(Duration::from_millis(200));
            for index in 0..33 {
                if matches!(index, 18..=25) {
                    app.page(0);
                    pump_for(Duration::from_millis(200));
                }
                let start = Instant::now();
                let stop = Arc::new(AtomicBool::new(false));
                let ready = Arc::new(Barrier::new(2));
                let recorder = sampling.then(|| {
                    let stop = stop.clone(); let ready = ready.clone();
                    std::thread::spawn(move || {
                        let mut frames = Vec::new(); let mut gaps = Vec::new(); let mut used = 0;
                        ready.wait();
                        while !stop.load(Ordering::Relaxed) {
                            let mut windows = Vec::<usize>::new();
                            EnumWindows(Some(owned_window), &mut windows as *mut _ as LPARAM);
                            if windows.is_empty() { gaps.push(json!({"ms":start.elapsed().as_secs_f64()*1000.,"reason":"no visible test window"})); }
                            for hwnd in windows {
                                let ms = start.elapsed().as_secs_f64()*1000.;
                                match capture(hwnd as HWND,ms) {
                                    Ok(f) if used + f.data.len() <= 128*1024*1024 => { used += f.data.len(); frames.push(f); }
                                    Ok(_) => gaps.push(json!({"ms":ms,"hwnd":hwnd,"reason":"128 MiB segment budget exhausted"})),
                                    Err(e) => gaps.push(json!({"ms":ms,"hwnd":hwnd,"reason":e})),
                                }
                            }
                            std::thread::sleep(Duration::from_millis(16));
                        }
                        (frames,gaps)
                    })
                });
                if sampling {
                    ready.wait();
                    pump_for(Duration::from_millis(50));
                }
                COUNTERS.with(|c| *c.borrow_mut() = Some(HashMap::new()));
                let began = start.elapsed().as_secs_f64() * 1000.;
                let handler = Instant::now();
                let name = action(app, index);
                let handler_ms = handler.elapsed().as_secs_f64() * 1000.;
                pump_for(Duration::from_millis(200));
                let counters = COUNTERS.with(|c| c.borrow_mut().take().unwrap());
                if !sampling
                    && (6..=17).contains(&index)
                    && !theme::high_contrast()
                    && counters.get("paint.tab.cache_copy").map_or(0, |v| v.0) == 0
                {
                    navigation_cache_failures.push(index);
                }
                stop.store(true, Ordering::Relaxed);
                let mut samples = Vec::new();
                let mut gaps = Vec::new();
                if let Some(recorder) = recorder {
                    let (frames, errors) = recorder.join().unwrap();
                    gaps = errors;
                    let folder = dir.join(format!("{index:02}-{name}"));
                    std::fs::create_dir_all(&folder).unwrap();
                    for (i, f) in frames.iter().enumerate() {
                        let first = frames
                            .iter()
                            .find(|r| {
                                r.hwnd == f.hwnd && r.width == f.width && r.height == f.height
                            })
                            .unwrap();
                        let last = frames
                            .iter()
                            .rev()
                            .find(|r| {
                                r.hwnd == f.hwnd && r.width == f.width && r.height == f.height
                            })
                            .unwrap();
                        if let Some(previous) = frames[..i].iter().rev().find(|r| r.hwnd == f.hwnd)
                        {
                            let interval = f.ms - previous.ms;
                            if interval > 32. {
                                gaps.push(json!({"ms":f.ms,"hwnd":f.hwnd,"reason":"sampling interval exceeded 32 ms","interval_ms":interval}));
                            }
                        }
                        let white = !theme::high_contrast()
                            && pixels::white_block(&f.data, f.width as usize, f.dpi);
                        let mixed_content = index < 26
                            && pixels::content_differs(
                                &f.data,
                                &first.data,
                                f.width as usize,
                                f.height as usize,
                                f.dpi,
                            )
                            && pixels::content_differs(
                                &f.data,
                                &last.data,
                                f.width as usize,
                                f.height as usize,
                                f.dpi,
                            );
                        if white || mixed_content {
                            visual_failures.push(format!("{index:02}-{name}/{i:03}.bmp"));
                        }
                        let file = format!("{i:03}.bmp");
                        write_frame(f, &folder.join(&file)).unwrap();
                        samples.push(json!({"file":format!("{index:02}-{name}/{file}"),"ms":f.ms,"capture_end_ms":f.ms+f.capture_ms,"hwnd":f.hwnd,
                            "white_block":white,"mixed_static_content":mixed_content,
                            "suspected_mixed_frame":pixels::differs(&f.data,&first.data) && pixels::differs(&f.data,&last.data)}));
                    }
                }
                if !sampling {
                    timings.push(handler_ms);
                }
                let expected_hidden = matches!(index, 26..=29);
                for gap in &mut gaps {
                    if expected_hidden && gap["reason"] == "no visible test window" {
                        gap["expected"] = json!(true);
                    }
                }
                let pre = samples
                    .iter()
                    .any(|f| f["capture_end_ms"].as_f64().unwrap() < began);
                let post = samples
                    .iter()
                    .any(|f| f["ms"].as_f64().unwrap() > began + handler_ms);
                report["runs"].as_array_mut().unwrap().push(json!({"sampling":sampling,"scenario":name,"index":index,"visit":if index < 6 {"first in pass"} else {"subsequent scenario"},
                    "navigation_geometry":theme::navigation_geometry(item(h,9)),
                    "action_start_ms":began,"handler_ms":handler_ms,"counters":counters,"samples":samples,"gaps":gaps,
                    "coverage":if !sampling {"timing-only"} else if pre && post && gaps.is_empty() {"captured; inspect frames"} else {"incomplete; inspect gaps (hide/close can be intentional)"}}));
                std::fs::write(
                    dir.join("report.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
        }
        app.rest.close();
        timings.sort_by(f64::total_cmp);
        let summary = format!(
            "Handler milliseconds: p50={:.2}, p95={:.2}, max={:.2}\nReview report.json and frame folders. Incomplete coverage is NOT a pass. Mixed-frame flags are candidates, not proof. The sampler did not force redraw; production display paths may repaint synchronously.\n",
            timings[timings.len() / 2],
            timings[(timings.len() * 95 / 100).min(timings.len() - 1)],
            timings.last().unwrap()
        );
        report["visual_failures"] = json!(visual_failures);
        report["navigation_cache_failures"] = json!(navigation_cache_failures);
        std::fs::write(
            dir.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("summary.txt"), &summary).unwrap();
        println!("{summary}");
        assert!(
            navigation_cache_failures.is_empty(),
            "warm page navigation cache unused: {navigation_cache_failures:?}; inspect saved geometry and fallback counters"
        );
        assert!(
            visual_failures.is_empty(),
            "visual regression candidates: {visual_failures:?}; review saved frames"
        );
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn ui_performance_benchmark() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut f = super::capture_tests::fixture();
        let app = &mut f.app;
        // Test data only; never write configuration or register global shortcuts.
        app.config.rules = (0..2)
            .map(|i| SummonRule {
                name: format!("Diagnostic rule {i}"),
                enabled: true,
                executable: "diagnostic.exe".into(),
                arguments: String::new(),
                working_directory: String::new(),
                process_path: String::new(),
                class: String::new(),
                title: String::new(),
                hotkey: Hotkey::new(3, 65 + i),
                minimize: false,
                priority_hotkey: false,
            })
            .collect();
        app.refresh_list();
        app.fill_rule(Some(0));
        app.layout_ui();
        pump_for(Duration::from_millis(500));
        let mut pages = Vec::new();
        for index in 0..18 {
            let mut handler = 0.;
            let measurement = sample_action(|| {
                let start = Instant::now();
                f.app.page(index % 6);
                handler = start.elapsed().as_secs_f64() * 1000.;
                pump_for(Duration::from_millis(200));
            });
            if index >= 6 {
                pages.push(json!({"handler_ms":handler,"measurement":measurement}));
            }
        }
        let rest = super::super::rest_ui::benchmark(&mut f.app.rest, f.app.host);
        let report = json!({"protocol":"performance-v2", "os":std::env::consts::OS, "build":option_env!("DUNYU_DIAGNOSTIC_REVISION"),
            "pages":pages,"rest":rest});
        let path = std::env::var("DUNYU_BENCH_REPORT").expect("DUNYU_BENCH_REPORT required");
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
