// Included in capture_tests so this exercises the production window and dispatcher.
#[test]
#[ignore = "requires an interactive Windows desktop; run with the recording diagnostic launcher"]
fn recording_real_input() {
    unsafe {
        use super::super::recorder;
        use std::io::Write;
        use std::time::Duration;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        struct Report {
            dir: PathBuf,
            file: std::fs::File,
            failures: usize,
            incomplete_cases: usize,
            complete: bool,
        }
        impl Report {
            fn write(&mut self, value: serde_json::Value) {
                writeln!(self.file, "{value}").unwrap();
                self.file.flush().unwrap();
                println!("{value}");
            }
            fn check(&mut self, case: &str, stage: &str, passed: bool) {
                self.failures += usize::from(!passed);
                self.write(serde_json::json!({"case":case,"stage":stage,"passed":passed}));
            }
            unsafe fn value(&mut self, case: &str, stage: &str, field: HWND, expected: &str) {
                let actual = title(field);
                let passed = actual == expected;
                self.failures += usize::from(!passed);
                self.write(serde_json::json!({"case":case,"stage":stage,"expected":expected,"actual":actual,"passed":passed}));
            }
            fn incomplete(&mut self, case: &str, reason: &str) {
                self.incomplete_cases += 1;
                self.write(serde_json::json!({"case":case,"status":"INCOMPLETE","reason":reason}));
            }
        }
        impl Drop for Report {
            fn drop(&mut self) {
                // Also preserve earlier failures if a later setup/assertion unexpectedly panics.
                let status = if self.failures > 0 {
                    "FAIL"
                } else if !self.complete || self.incomplete_cases > 0 {
                    "INCOMPLETE"
                } else {
                    "PASS"
                };
                let summary = serde_json::json!({"status":status,"complete":self.complete,
                    "failures":self.failures,"incomplete_cases":self.incomplete_cases});
                let _ = std::fs::write(self.dir.join("summary.json"), summary.to_string());
                let _ = writeln!(self.file, "{summary}");
                println!("Recording summary: {summary}");
            }
        }
        unsafe fn pump(ms: u64) {
            let until = Instant::now() + Duration::from_millis(ms);
            let mut msg: MSG = std::mem::zeroed();
            while Instant::now() < until {
                if PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    super::super::dispatch_message(&msg);
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        unsafe fn focus(window: HWND, field: HWND) -> bool {
            SetForegroundWindow(window);
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if GetForegroundWindow() == window {
                    SetFocus(field);
                }
                pump(10);
                if GetForegroundWindow() == window
                    && GetFocus() == field
                    && IsWindowVisible(field) != 0
                {
                    return true;
                }
            }
            false
        }
        // SendInput may wait for hook delivery. Keep the installing UI thread pumping.
        unsafe fn inject(keys: Vec<(u16, bool)>) -> Result<(), &'static str> {
            let worker = std::thread::spawn(move || {
                let inputs: Vec<INPUT> = keys
                    .into_iter()
                    .map(|(key, up)| INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: key,
                                wScan: 0,
                                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }
                                    | if matches!(key, 0x5b | 0x5c) {
                                        KEYEVENTF_EXTENDEDKEY
                                    } else {
                                        0
                                    },
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    })
                    .collect();
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                ) == inputs.len() as u32
            });
            let deadline = Instant::now() + Duration::from_secs(3);
            while !worker.is_finished() && Instant::now() < deadline {
                pump(5);
            }
            if !worker.is_finished() {
                return Err("input delivery timed out");
            }
            if !worker.join().unwrap_or(false) {
                return Err("SendInput failed");
            }
            pump(200);
            Ok(())
        }
        struct ReleaseKeys;
        impl Drop for ReleaseKeys {
            fn drop(&mut self) {
                unsafe {
                    let _ = inject(
                        [0xa2, 0xa4, 0x5b, 0x5c, 0x51, 0x57]
                            .into_iter()
                            .map(|k| (k, true))
                            .collect(),
                    );
                    recorder::stop();
                }
            }
        }
        let mode = std::env::var("DUNYU_RECORDING_MODE").unwrap_or_else(|_| "none".into());
        let observers = match mode.as_str() {
            "none" | "log" => hotkey_diagnostics::Observers::default(),
            "foreground" => hotkey_diagnostics::Observers {
                foreground: true,
                ..Default::default()
            },
            "raw" => hotkey_diagnostics::Observers {
                raw_input: true,
                ..Default::default()
            },
            _ => panic!("unknown recording comparison mode"),
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let output = std::env::var_os("DUNYU_RECORDING_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap()
                    .join(format!("recording-results-{stamp}-{mode}"))
            });
        std::fs::create_dir_all(&output).unwrap();
        let mut report = Report {
            file: std::fs::File::create(output.join("checks.jsonl")).unwrap(),
            dir: output.clone(),
            failures: 0,
            incomplete_cases: 0,
            complete: false,
        };
        report.write(serde_json::json!({"mode":mode,"process":std::process::id()}));
        println!("Recording report: {}", output.display());
        for key in [0xa2, 0xa3, 0xa4, 0xa5, 0xa0, 0xa1, 0x5b, 0x5c, 0x51, 0x57] {
            if GetAsyncKeyState(key) < 0 {
                report.incomplete("setup", "release keyboard keys before testing");
                panic!("INCOMPLETE: keyboard keys held before testing");
            }
        }
        let mut f = fixture();
        std::fs::create_dir_all(&f.app.dir).unwrap();
        APP.with(|cell| {
            *cell.borrow_mut() = Some(std::mem::replace(
                &mut f.app,
                test_app(null_mut(), PathBuf::new()),
            ))
        });
        let _active = ActiveFixture(&mut f);
        let window = APP.with(|cell| cell.borrow().as_ref().unwrap().ui.as_ref().unwrap().window);
        static HOTKEY_DELIVERED: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        unsafe extern "system" fn observer_host(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            if m == WM_HOTKEY {
                HOTKEY_DELIVERED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            if m == WM_INPUT {
                hotkey_diagnostics::raw_input(l);
            }
            DefWindowProcW(h, m, w, l)
        }
        let host = APP.with(|cell| cell.borrow().as_ref().unwrap().host);
        SetWindowLongPtrW(host, GWLP_WNDPROC, observer_host as *const () as isize);
        let _trace = if mode == "none" {
            None
        } else {
            let trace = hotkey_diagnostics::start_with_observers(
                &output,
                Hotkey::new(8, 81),
                host,
                observers,
            )
            .unwrap();
            if !trace.observers_ready(observers) {
                report.incomplete("setup", "requested observer registration failed");
                panic!("INCOMPLETE: observer registration failed");
            }
            Some(trace)
        };
        let _release = ReleaseKeys;
        for (name, mods, key, preview, expected) in [
            (
                "fallback_ctrl_alt",
                vec![0xa2, 0xa4],
                0x57,
                "Ctrl+Alt+…",
                "Ctrl+Alt+W",
            ),
            (
                "ctrl_alt",
                vec![0xa2, 0xa4],
                0x57,
                "Ctrl+Alt+…",
                "Ctrl+Alt+W",
            ),
            ("left_win", vec![0x5b], 0x51, "Win+…", "Win+Q"),
            ("right_win", vec![0x5c], 0x51, "Win+…", "Win+Q"),
            ("registered_message", vec![], 0x57, "", "Ctrl+Alt+W"),
        ] {
            let result = (|| -> Result<(), &'static str> {
                let name_field = item(window, 200);
                if !focus(window, name_field) {
                    return Err("cannot focus the rule name");
                }
                APP.with(|cell| cell.borrow_mut().as_mut().unwrap().fill_rule(None));
                set_text(window, 200, name);
                set_text(window, 201, "fixture.exe");
                check(window, 208, false); // Save without reserving a global shortcut.
                let field = item(window, 207);
                SetWindowTextW(field, wide("Ctrl+Alt+A").as_ptr());
                if !focus(window, field) {
                    return Err("cannot focus the visible shortcut field");
                }
                if name == "fallback_ctrl_alt" {
                    recorder::disable_hook_for_test();
                }
                let generation = recorder::generation_for_test();
                let before: Vec<_> = mods
                    .iter()
                    .chain(std::iter::once(&key))
                    .map(|k| (*k, recorder::hook_count(*k as u32)))
                    .collect();
                if PostMessageW(host, WM_HOTKEY, 1, (0x41 << 16) | 3) == 0 {
                    return Err("cannot post existing binding message");
                }
                pump(100);
                report.value(name, "existing_binding_message", field, "Ctrl+Alt+A");
                report.check(
                    name,
                    "existing_binding_consumed",
                    HOTKEY_DELIVERED.load(std::sync::atomic::Ordering::SeqCst) == 0,
                );
                if name == "registered_message" {
                    if PostMessageW(host, WM_HOTKEY, 1, (0x57 << 16) | 3) == 0 {
                        return Err("cannot post replacement binding message");
                    }
                    pump(100);
                    report.value(name, "complete_chord", field, expected);
                } else {
                    inject(mods.iter().map(|k| (*k, false)).collect())?;
                    report.value(name, "modifier_preview", field, preview);
                    // Do not inject more keydowns into an unrelated foreground application.
                    if GetForegroundWindow() != window || GetFocus() != field {
                        return Err("focus lost before main key");
                    }
                    inject(vec![(key, false), (key, true)])?;
                    report.value(name, "complete_chord", field, expected);
                    inject(mods.iter().rev().map(|k| (*k, true)).collect())?;
                    report.value(name, "released", field, expected);
                    let delivered = before
                        .iter()
                        .all(|(k, count)| recorder::hook_count(*k as u32) > *count);
                    report.check(
                        name,
                        "hook_delivery",
                        if name == "fallback_ctrl_alt" {
                            before
                                .iter()
                                .all(|(k, count)| recorder::hook_count(*k as u32) == *count)
                        } else {
                            delivered
                        },
                    );
                }
                recorder::deliver(field, generation.wrapping_sub(1), (0x5a << 16) | 3);
                report.value(name, "stale_session_ignored", field, expected);
                if !focus(window, name_field) {
                    return Err("cannot focus rule name for blur check");
                }
                report.value(name, "blurred", field, expected);
                recorder::deliver(field, generation, (0x5a << 16) | 3);
                recorder::deliver(name_field, generation, (0x5a << 16) | 3);
                report.value(name, "stopped_session_ignored", field, expected);
                report.value(name, "other_field_unchanged", name_field, name);
                let (passed, error) = APP.with(|cell| {
                    let mut guard = cell.borrow_mut();
                    let app = guard.as_mut().unwrap();
                    let result = app.ui_action(113);
                    let saved = std::fs::read(app.dir.join("config.json"))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<AppConfig>(&bytes).ok())
                        .is_some_and(|c| {
                            c.rules
                                .iter()
                                .any(|r| r.name == name && hotkey_text(&r.hotkey) == expected)
                        });
                    (result.is_ok() && saved, result.err())
                });
                report.check(name, "saved", passed);
                if let Some(error) = error {
                    report.write(serde_json::json!({"case":name,"error":error}));
                }
                Ok(())
            })();
            if let Err(reason) = result {
                report.incomplete(name, reason);
                // Balance any injected modifiers even when focus or delivery failed mid-case.
                let _ = inject(mods.iter().map(|k| (*k, true)).collect());
                recorder::stop();
                if reason == "input delivery timed out" {
                    break;
                }
            }
        }
        if focus(window, item(window, 207)) {
            APP.with(|cell| cell.borrow_mut().as_mut().unwrap().page(1));
            report.check(
                "lifecycle",
                "page_stops_recording",
                !super::super::priority_hotkey::RECORDING.load(std::sync::atomic::Ordering::SeqCst),
            );
            APP.with(|cell| cell.borrow_mut().as_mut().unwrap().page(0));
            if focus(window, item(window, 207)) {
                DestroyWindow(window);
                report.check(
                    "lifecycle",
                    "close_stops_recording",
                    !super::super::priority_hotkey::RECORDING
                        .load(std::sync::atomic::Ordering::SeqCst),
                );
            } else {
                report.incomplete("lifecycle", "cannot focus field before close");
            }
        } else {
            report.incomplete("lifecycle", "cannot focus field before page transition");
        }
        report.complete = report.incomplete_cases == 0;
        assert_eq!(
            report.failures, 0,
            "FAIL: recording checks failed; see checks.jsonl"
        );
        assert!(
            report.complete,
            "INCOMPLETE: some recording cases could not run"
        );
    }
}
