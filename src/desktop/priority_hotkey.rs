//! A dedicated hook thread only filters input and posts commands to the host.
use super::hotkey_diagnostics::{self as diagnostics, Event};
use super::*;
use dunyu::priority_hotkey::{Delivery, KeyState, deliver};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
pub const ACTION: u32 = WM_APP + 9;
pub static RECORDING: AtomicBool = AtomicBool::new(false);
pub static RECORDING_EPOCH: AtomicUsize = AtomicUsize::new(0);
#[derive(Default)]
struct Routes {
    bindings: Vec<(Hotkey, Command)>,
    generation: usize,
    diagnostic_routes: Vec<(usize, usize)>,
}
struct Context {
    routes: Arc<Mutex<Routes>>,
    state: KeyState,
    epoch: usize,
    host: usize,
}
thread_local! {static CONTEXT:RefCell<Option<Context>>=const{RefCell::new(None)};}
pub struct Runtime {
    routes: Arc<Mutex<Routes>>,
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Runtime {
    pub fn new(host: HWND) -> Result<Self, String> {
        let routes = Arc::new(Mutex::new(Routes::default()));
        let shared = routes.clone();
        let host = host as usize;
        let (tx, rx) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || unsafe {
            let mut msg: MSG = std::mem::zeroed();
            PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
            CONTEXT.with(|c| {
                *c.borrow_mut() = Some(Context {
                    routes: shared,
                    state: KeyState::default(),
                    epoch: RECORDING_EPOCH.load(Ordering::SeqCst),
                    host,
                })
            });
            let hook =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), GetModuleHandleW(null()), 0);
            if hook.is_null() {
                let _ = tx.send(Err("无法安装快捷键接管钩子".to_string()));
                return;
            }
            diagnostics::emit(Event {
                source: "priority",
                kind: "hook_installed",
                ..Default::default()
            });
            let _ = tx.send(Ok(GetCurrentThreadId()));
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            diagnostics::emit(Event {
                source: "priority",
                kind: "hook_thread_exit",
                ..Default::default()
            });
            UnhookWindowsHookEx(hook);
            CONTEXT.with(|c| *c.borrow_mut() = None);
        });
        match rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                routes,
                thread_id,
                thread: Some(thread),
            }),
            _ => {
                let _ = thread.join();
                Err("无法启动快捷键接管线程".into())
            }
        }
    }
    pub fn configure(&self, bindings: Vec<(Hotkey, Command)>) {
        let mut r = self.routes.lock().unwrap();
        if r.bindings == bindings {
            return;
        }
        r.bindings = bindings;
        r.generation = r.generation.wrapping_add(1);
        let generation = r.generation;
        let watched: Vec<_> = r
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| diagnostics::watches(key))
            .map(|(i, _)| (generation, i))
            .collect();
        for &(generation, index) in &watched {
            diagnostics::emit(Event {
                source: "priority",
                kind: "route_configured",
                generation,
                index,
                count: match r.bindings[index].1 {
                    Command::Summon(rule) => rule as u32,
                    _ => u32::MAX,
                },
                ..Default::default()
            });
        }
        diagnostics::emit(Event {
            source: "priority",
            kind: "routes_updated",
            generation,
            count: watched.len() as u32,
            ..Default::default()
        });
        r.diagnostic_routes.extend(watched);
    }
    pub fn diagnostic_route(&self, generation: usize, index: usize) -> bool {
        self.routes
            .lock()
            .unwrap()
            .diagnostic_routes
            .contains(&(generation, index))
    }
    pub fn command(&self, generation: usize, index: usize) -> Option<Command> {
        let r = self.routes.lock().unwrap();
        (r.generation == generation)
            .then(|| r.bindings.get(index).map(|(_, c)| *c))
            .flatten()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.configure(vec![]);
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        // Do not join on the UI thread: another hook can require its message pump.
        self.thread.take();
    }
}
unsafe fn mask_menu() -> (u32, u32) {
    let mut events: [INPUT; 2] = std::mem::zeroed();
    for (i, event) in events.iter_mut().enumerate() {
        event.r#type = INPUT_KEYBOARD;
        event.Anonymous.ki = KEYBDINPUT {
            wVk: 0xe8,
            wScan: 0,
            dwFlags: if i == 1 { KEYEVENTF_KEYUP } else { 0 },
            time: 0,
            dwExtraInfo: 0x44554e59,
        };
    }
    SetLastError(0);
    let count = SendInput(2, events.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    let error = GetLastError();
    (count, error)
}
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code < 0 {
        diagnostics::emit(Event {
            source: "priority",
            kind: "hook_bypass",
            code,
            message: w as u32,
            ..Default::default()
        });
        return CallNextHookEx(null_mut(), code, w, l);
    }
    let event = &*(l as *const KBDLLHOOKSTRUCT);
    let traced = diagnostics::watches_key(event.vkCode);
    let mut trace = Event {
        source: "priority",
        kind: "key_decision",
        code,
        input_time: event.time,
        scan_code: event.scanCode,
        key: event.vkCode,
        message: w as u32,
        flags: event.flags,
        ..Default::default()
    };
    if traced {
        trace.callback_id = diagnostics::callback_id();
        diagnostics::snapshot(&mut trace);
        diagnostics::emit(Event {
            source: "priority",
            kind: "hook_entry",
            ..trace
        });
    }
    if event.flags & LLKHF_INJECTED != 0 {
        if traced {
            trace.reason = "injected_passthrough";
            trace.duration_us = diagnostics::elapsed_since(trace.micros);
            diagnostics::emit(trace);
        }
        return CallNextHookEx(null_mut(), code, w, l);
    }
    let consumed = CONTEXT.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            trace.reason = "context_busy";
            return false;
        };
        let Some(c) = guard.as_mut() else {
            trace.reason = "context_missing";
            return false;
        };
        let epoch = RECORDING_EPOCH.load(Ordering::SeqCst);
        trace.epoch_before = c.epoch;
        trace.epoch_after = epoch;
        if epoch != c.epoch {
            c.epoch = epoch;
            c.state.clear();
            if traced {
                diagnostics::emit(Event {
                    source: "priority",
                    kind: "recording_state_reset",
                    ..trace
                });
            }
        }
        trace.recording = RECORDING.load(Ordering::SeqCst);
        if trace.recording {
            trace.reason = "recording_paused";
            return false;
        }
        let mut modifiers = 0;
        for (keys, flag) in [
            ([VK_LCONTROL, VK_RCONTROL], MOD_CONTROL),
            ([VK_LMENU, VK_RMENU], MOD_ALT),
            ([VK_LSHIFT, VK_RSHIFT], MOD_SHIFT),
            ([VK_LWIN, VK_RWIN], MOD_WIN),
        ] {
            if keys.iter().any(|k| GetAsyncKeyState(*k as i32) < 0) {
                modifiers |= flag;
            }
        }
        trace.modifiers = modifiers;
        trace.repeated = GetAsyncKeyState(event.vkCode as i32) < 0;
        let routes = c.routes.lock().unwrap();
        trace.generation = routes.generation;
        let (handled, command) = c.state.event(
            event.vkCode,
            matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN),
            trace.repeated,
            modifiers,
            &routes.bindings,
        );
        let delivery = command.map(|command| {
            (
                routes.generation,
                routes
                    .bindings
                    .iter()
                    .position(|(_, cmd)| *cmd == command)
                    .unwrap(),
            )
        });
        drop(routes);
        trace.reason = if handled {
            "owned_key"
        } else {
            "unmatched_or_repeat"
        };
        if let Some((generation, index)) = delivery {
            trace.index = index;
            let result = deliver(
                modifiers,
                || {
                    let (count, error) = mask_menu();
                    if traced {
                        diagnostics::emit(Event {
                            source: "priority",
                            kind: "menu_mask",
                            callback_id: trace.callback_id,
                            key: event.vkCode,
                            generation,
                            index,
                            count,
                            error,
                            ..Default::default()
                        });
                    }
                    count == 2
                },
                || {
                    SetLastError(0);
                    let posted = PostMessageW(handle(c.host), ACTION, generation, index as isize);
                    let error = GetLastError();
                    if traced {
                        diagnostics::emit(Event {
                            source: "priority",
                            kind: "command_posted",
                            callback_id: trace.callback_id,
                            key: event.vkCode,
                            generation,
                            index,
                            count: posted as u32,
                            error,
                            ..Default::default()
                        });
                    }
                    posted != 0
                },
            );
            trace.reason = match result {
                Delivery::Accepted => "matched_delivered",
                Delivery::MaskFailed => "menu_mask_failed",
                Delivery::PostFailed => "command_post_failed",
            };
            if result != Delivery::Accepted {
                c.state.release(event.vkCode);
                return false;
            }
        }
        handled
    });
    if traced {
        trace.consumed = consumed;
        trace.duration_us = diagnostics::elapsed_since(trace.micros);
        diagnostics::emit(trace);
    }
    if consumed {
        1
    } else {
        CallNextHookEx(null_mut(), code, w, l)
    }
}
