//! Isolated resident workloads; compiled only into the Windows test executable.
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static COUNTS: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];
pub(crate) fn count(index: usize) {
    if ACTIVE.load(Ordering::Relaxed) {
        COUNTS[index].fetch_add(1, Ordering::Relaxed);
    }
}
fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
unsafe fn pump(seconds: u64) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            assert_ne!(msg.message, WM_QUIT, "unexpected quit");
            assert!(
                !matches!(
                    msg.message,
                    WM_DISPLAYCHANGE | WM_POWERBROADCAST | WM_WTSSESSION_CHANGE
                ),
                "environment changed"
            );
            dispatch_message(&msg);
            if Instant::now() >= deadline {
                return;
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        assert_ne!(
            MsgWaitForMultipleObjectsEx(
                0,
                null(),
                remaining.as_millis().max(1) as u32,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE
            ),
            WAIT_FAILED
        );
    }
}
unsafe fn cpu() -> u64 {
    let mut created = std::mem::zeroed();
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    assert_ne!(
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exit,
            &mut kernel,
            &mut user
        ),
        0
    );
    let value = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    value(kernel) + value(user)
}
unsafe extern "system" fn monitor(
    monitor: HMONITOR,
    _: HDC,
    rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let r = *rect;
    let (mut x, mut y) = (0, 0);
    assert_eq!(
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y),
        0
    );
    (*(data as *mut Vec<[i32; 6]>)).push([r.left, r.top, r.right, r.bottom, x as i32, y as i32]);
    1
}
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Relaxed);
    }
}
#[test]
#[ignore = "requires an interactive Windows desktop; use run-resident-diagnostics.cmd"]
fn resident_baseline() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let profiling = std::env::var("DUNYU_RESIDENT_PROFILE").is_ok_and(|v| v == "1");
        let scenario = std::env::var("DUNYU_RESIDENT_SCENARIO").unwrap();
        assert!(
            [
                "cold",
                "warm",
                "rest",
                "message",
                "fullscreen",
                "pending1",
                "pending5"
            ]
            .contains(&scenario.as_str())
        );
        assert!(!profiling || scenario == "fullscreen");
        let path = std::env::var("DUNYU_RESIDENT_REPORT").unwrap();
        std::fs::write(&path, b"{\"protocol\":\"resident-v1\",\"complete\":false}").unwrap();
        let mut fixture = super::capture_tests::fixture_with_ui(scenario == "warm");
        if let Some(u) = &fixture.app.ui {
            ShowWindow(u.window, SW_HIDE);
        }
        std::fs::create_dir_all(&fixture.app.dir).unwrap();
        let host = fixture.app.host;
        SetWindowLongPtrW(host, GWLP_WNDPROC, procedure as *const () as isize);
        windows_sys::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(host, 0);
        let app = std::mem::replace(
            &mut fixture.app,
            super::capture_tests::test_app(null_mut(), PathBuf::new()),
        );
        APP.with(|a| *a.borrow_mut() = Some(app));
        // Guard restores ownership even when an assertion fails.
        struct Owner<'a>(&'a mut super::capture_tests::Fixture);
        impl Drop for Owner<'_> {
            fn drop(&mut self) {
                APP.with(|a| { if let Some(mut app)=a.borrow_mut().take() { unsafe { KillTimer(app.host,TIMER); KillTimer(app.host,rest_ui::TIMER_ID); app.rest.close(); windows_sys::Win32::System::RemoteDesktop::WTSUnRegisterSessionNotification(app.host); } self.0.app=app; } });
            }
        }
        let _owner = Owner(&mut fixture);
        let pending = scenario.starts_with("pending");
        APP.with(|a| {
            let mut guard = a.borrow_mut();
            let app = guard.as_mut().unwrap();
            if matches!(scenario.as_str(), "rest" | "fullscreen") {
                app.rest.configure(
                    dunyu::rest::RestConfig {
                        enabled: true,
                        short_enabled: true,
                        long_enabled: false,
                        interval_minutes: 60,
                        short_seconds: 120,
                        fullscreen: scenario == "fullscreen",
                        ..Default::default()
                    },
                    host,
                );
                if scenario == "fullscreen" {
                    app.rest.action(1, None, host);
                }
            }
            if pending {
                for i in 0..if scenario == "pending1" { 1 } else { 5 } {
                    app.config.rules.push(SummonRule {
                        name: format!("Fixture {i}"),
                        enabled: false,
                        executable: "unused.exe".into(),
                        arguments: String::new(),
                        working_directory: String::new(),
                        process_path: format!(
                            "DunYu-resident-nonexistent-{}-{i}",
                            GetCurrentProcessId()
                        ),
                        class: String::new(),
                        title: String::new(),
                        hotkey: Hotkey::new(3, 65 + i as u32),
                        minimize: false,
                        priority_hotkey: false,
                    });
                    app.pending.push(Pending {
                        rule: i,
                        started: Instant::now(),
                    });
                }
                assert_ne!(SetTimer(host, TIMER, 100, None), 0);
            }
        });
        struct Listener(Option<(std::sync::Arc<AtomicBool>, std::thread::JoinHandle<()>)>);
        impl Drop for Listener {
            fn drop(&mut self) {
                if let Some((cancel, thread)) = self.0.take() {
                    cancel.store(true, Ordering::SeqCst);
                    thread.join().unwrap();
                }
            }
        }
        let mut listener = Listener(None);
        if scenario == "message" {
            let db = APP.with(|a| a.borrow().as_ref().unwrap().dir.join("notifications.db"));
            let writer = rusqlite::Connection::open(&db).unwrap();
            writer.execute_batch("PRAGMA journal_mode=WAL;CREATE TABLE Notification(id INTEGER PRIMARY KEY,Payload BLOB,type TEXT);").unwrap();
            listener.0 = Some(message::resident_listener(&db, host));
        }
        let mut monitors = Vec::<[i32; 6]>::new();
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(monitor),
            &mut monitors as *mut _ as isize,
        );
        pump(if pending { 1 } else { 5 });
        for c in &COUNTS {
            c.store(0, Ordering::Relaxed);
        }
        ACTIVE.store(true, Ordering::Relaxed);
        let _active = Active;
        if profiling {
            rest_profile::start();
        }
        let start_ms = timestamp();
        let begin = cpu();
        let wall = Instant::now();
        pump(if pending { 5 } else { 60 });
        let wall_ms = wall.elapsed().as_secs_f64() * 1000.;
        let cpu_ms = (cpu() - begin) as f64 / 10000.;
        let end_ms = timestamp();
        let profile = if profiling {
            Some(rest_profile::finish())
        } else {
            None
        };
        ACTIVE.store(false, Ordering::Relaxed);
        let counts: Vec<_> = COUNTS.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        match scenario.as_str() {
            "rest" => assert!(counts[4] >= 50, "rest scheduler did not run"),
            "fullscreen" => {
                assert!(
                    counts[4] >= 50 && counts[5] > 0,
                    "fullscreen workload did not run"
                );
                APP.with(|a| {
                    assert_eq!(
                        a.borrow().as_ref().unwrap().rest.resident_visible_windows(),
                        monitors.len()
                    )
                });
                assert!(!monitors.is_empty());
            }
            "message" => assert!(counts[2] >= 5 && counts[3] > 0, "listener did not run"),
            "pending1" | "pending5" => assert!(counts[1] >= 30, "startup queries did not run"),
            _ => assert_eq!(counts[4], 0),
        }
        drop(listener);
        APP.with(|a| {
            let mut a = a.borrow_mut();
            let app = a.as_mut().unwrap();
            app.pending.clear();
            KillTimer(host, TIMER);
            KillTimer(host, rest_ui::TIMER_ID);
            app.rest.close();
        });
        let cleanup_ms = timestamp();
        pump(2);
        let report = json!({"protocol":if profiling {"resident-profile-v1"} else {"resident-v1"},"profile":profile,"complete":true,"build":option_env!("DUNYU_DIAGNOSTIC_REVISION"),"scenario":scenario,"start_ms":start_ms,"end_ms":end_ms,"cleanup_ms":cleanup_ms,"wall_ms":wall_ms,"cpu_ms":cpu_ms,"cpu_percent_one_core":cpu_ms/wall_ms*100.,"monitors":monitors,"multi_monitor_covered":monitors.len()>1,"dpi":GetDpiForWindow(host),"high_contrast":theme::high_contrast(),"motion_enabled":rest_ui::motion_enabled(),"parameters":{"poll_seconds":10,"delay_seconds":60,"sample_seconds":if pending {5}else{60}},"counts":{"process_queries":counts[0],"window_enumerations":counts[1],"startup_ticks":counts[6],"database_scans":counts[2],"message_wait_timeouts":counts[3],"rest_ticks":counts[4],"rest_paints":counts[5]}});
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
