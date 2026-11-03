//! A dedicated hook thread only filters input and posts commands to the host.
use super::*;
use dunyu::priority_hotkey::KeyState;
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
            let _ = tx.send(Ok(GetCurrentThreadId()));
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
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
unsafe fn mask_menu() -> bool {
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
    SendInput(2, events.as_ptr(), std::mem::size_of::<INPUT>() as i32) == 2
}
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(null_mut(), code, w, l);
    }
    let event = &*(l as *const KBDLLHOOKSTRUCT);
    if event.flags & LLKHF_INJECTED != 0 {
        return CallNextHookEx(null_mut(), code, w, l);
    }
    let consumed = CONTEXT.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(c) = guard.as_mut() else {
            return false;
        };
        let epoch = RECORDING_EPOCH.load(Ordering::SeqCst);
        if epoch != c.epoch {
            c.epoch = epoch;
            c.state.clear();
        }
        if RECORDING.load(Ordering::SeqCst) {
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
        let routes = c.routes.lock().unwrap();
        let (handled, command) = c.state.event(
            event.vkCode,
            matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN),
            GetAsyncKeyState(event.vkCode as i32) < 0,
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
        if let Some((generation, index)) = delivery {
            if modifiers & (MOD_WIN | MOD_ALT) != 0 && !mask_menu() {
                c.state.release(event.vkCode);
                return false;
            }
            if PostMessageW(handle(c.host), ACTION, generation, index as isize) == 0 {
                c.state.release(event.vkCode);
                return false;
            }
        }
        handled
    });
    if consumed {
        1
    } else {
        CallNextHookEx(null_mut(), code, w, l)
    }
}
