//! Opt-in, bounded, asynchronous tracing of one configured hotkey.
use super::*;
use std::io::Write;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

#[derive(Default, Clone, Copy)]
pub(super) struct Event {
    pub source: &'static str,
    pub field: usize,
    pub focus: usize,
    pub control_id: i32,
    pub recording_session: usize,
    pub seq: u64,
    pub duration_us: u64,
    pub callback_id: u64,
    pub code: i32,
    pub input_time: u32,
    pub scan_code: u32,
    pub epoch_before: usize,
    pub epoch_after: usize,
    pub target: usize,
    pub kind: &'static str,
    pub reason: &'static str,
    pub key: u32,
    pub message: u32,
    pub flags: u32,
    pub modifiers: u32,
    pub generation: usize,
    pub index: usize,
    pub count: u32,
    pub error: u32,
    pub recording: bool,
    pub repeated: bool,
    pub consumed: bool,
    pub foreground: usize,
    pub pid: u32,
    pub micros: u64,
}
struct Trace {
    key: Hotkey,
    sender: mpsc::SyncSender<Event>,
    counters: dunyu::hotkey_trace::Counters,
    callbacks: AtomicU64,
    in_flight: AtomicUsize,
    stopped: AtomicBool,
    start: Instant,
}
static TRACE: OnceLock<Trace> = OnceLock::new();
pub(super) struct Session(Option<std::thread::JoinHandle<()>>, HWINEVENTHOOK, bool);
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            if !self.1.is_null() {
                UnhookWinEvent(self.1);
            }
            if self.2 {
                use windows_sys::Win32::UI::Input::*;
                let device = RAWINPUTDEVICE {
                    usUsagePage: 1,
                    usUsage: 6,
                    dwFlags: RIDEV_REMOVE,
                    hwndTarget: null_mut(),
                };
                RegisterRawInputDevices(&device, 1, std::mem::size_of_val(&device) as u32);
            }
        }
        TRACE.get().unwrap().stopped.store(true, Ordering::SeqCst);
        if let Some(thread) = self.0.take() {
            let _ = thread.join();
        }
    }
}
pub(super) fn watches(key: &Hotkey) -> bool {
    TRACE
        .get()
        .is_some_and(|t| t.key == *key && !t.stopped.load(Ordering::Relaxed))
}
pub(super) fn watches_key(key: u32) -> bool {
    TRACE.get().is_some_and(|t| {
        !t.stopped.load(Ordering::Relaxed) && dunyu::hotkey_trace::watches_key(t.key.key, key)
    })
}
pub(super) fn emit(mut event: Event) {
    let Some(t) = TRACE.get() else { return };
    t.in_flight.fetch_add(1, Ordering::SeqCst);
    if t.stopped.load(Ordering::SeqCst) {
        t.in_flight.fetch_sub(1, Ordering::SeqCst);
        return;
    }
    if event.micros == 0 {
        snapshot(&mut event);
    }
    t.counters.send(&t.sender, |seq| {
        event.seq = seq;
        event
    });
    t.in_flight.fetch_sub(1, Ordering::SeqCst);
}
pub(super) fn snapshot(event: &mut Event) {
    let Some(t) = TRACE.get() else { return };
    event.micros = t.start.elapsed().as_micros() as u64;
    unsafe {
        let h = GetForegroundWindow();
        event.foreground = h as usize;
        GetWindowThreadProcessId(h, &mut event.pid);
    }
}
unsafe fn identity(pid: u32) -> serde_json::Value {
    use windows_sys::Win32::Security::*;
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return serde_json::json!({"pid":pid,"name":null,"integrity_rid":null});
    }
    let mut path = vec![0u16; 32768];
    let mut size = path.len() as u32;
    let name =
        (QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) != 0).then(|| {
            String::from_utf16_lossy(&path[..size as usize])
                .rsplit('\\')
                .next()
                .unwrap_or("")
                .to_owned()
        });
    let mut token = null_mut();
    let mut integrity = None;
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) != 0 {
        let mut buffer = [0u64; 128];
        let mut needed = 0;
        if GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buffer.as_mut_ptr() as _,
            std::mem::size_of_val(&buffer) as u32,
            &mut needed,
        ) != 0
        {
            let label = &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let count = *GetSidSubAuthorityCount(label.Label.Sid);
            if count > 0 {
                integrity = Some(*GetSidSubAuthority(label.Label.Sid, count as u32 - 1));
            }
        }
        CloseHandle(token);
    }
    CloseHandle(process);
    serde_json::json!({"pid":pid,"name":name,"integrity_rid":integrity})
}
#[derive(Default, Clone, Copy)]
pub(super) struct Observers {
    pub raw_input: bool,
    pub foreground: bool,
}
// Production tracing observes existing events without registering extra input sources.
pub(super) fn start(dir: &std::path::Path, key: Hotkey, host: HWND) -> Result<Session, String> {
    start_with_observers(dir, key, host, Observers::default())
}
pub(super) fn start_with_observers(
    dir: &std::path::Path,
    key: Hotkey,
    host: HWND,
    observers: Observers,
) -> Result<Session, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = dir.join(format!(
        "hotkey-diagnostics-{stamp}-{}.jsonl",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", serde_json::json!({"tag":"DEBUG-hotkey","kind":"session_start","version":env!("CARGO_PKG_VERSION"),"revision":"winq-trace-v4","observers":{"raw_input":observers.raw_input,"foreground":observers.foreground},"windows":unsafe {windows_version()},"remote_session":unsafe {GetSystemMetrics(SM_REMOTESESSION) != 0},"unix_ms":stamp,"hotkey":ui::hotkey_text(&key),"process":unsafe {identity(GetCurrentProcessId())}})).map_err(|e| e.to_string())?;
    let (sender, receiver) = mpsc::sync_channel(256);
    TRACE
        .set(Trace {
            key,
            sender,
            counters: Default::default(),
            callbacks: AtomicU64::new(0),
            in_flight: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
            start: Instant::now(),
        })
        .map_err(|_| "诊断已经启动")?;
    let thread = std::thread::spawn(move || {
        let t = TRACE.get().unwrap();
        let write = |file: &mut std::fs::File, e: Event| -> std::io::Result<()> {
            let foreground = unsafe { identity(e.pid) };
            writeln!(
                file,
                "{}",
                serde_json::json!({"source":e.source,"field_hwnd":e.field,"focus_hwnd":e.focus,"control_id":e.control_id,"recording_session":e.recording_session,"seq":e.seq,"duration_us":e.duration_us,"callback_id":e.callback_id,"n_code":e.code,"input_time_ms":e.input_time,"scan_code":e.scan_code,"epoch_before":e.epoch_before,"epoch_after":e.epoch_after,"target_hwnd":e.target,"async_key_down_before_event":e.repeated,"dropped_so_far":t.counters.snapshot().1,"tag":"DEBUG-hotkey","kind":e.kind,"reason":e.reason,"us":e.micros,"key":e.key,"message":e.message,"flags":e.flags,"modifiers":e.modifiers,"generation":e.generation,"route_index":if matches!(e.kind, "summon_result" | "window_operation") {None} else {Some(e.index)},"rule_index":match e.kind {"route_configured" => Some(e.count as usize), "summon_result" | "window_operation" => Some(e.index), _ => None},"count":e.count,"error":e.error,"recording":e.recording,"repeated":e.repeated,"consumed":e.consumed,"foreground_hwnd":e.foreground,"foreground":foreground})
            )
        };
        let mut written = 0u64;
        loop {
            let event = if t.stopped.load(Ordering::Relaxed) {
                receiver.try_recv().ok()
            } else {
                receiver.recv_timeout(Duration::from_millis(100)).ok()
            };
            if let Some(event) = event {
                if write(&mut file, event).is_err() {
                    t.stopped.store(true, Ordering::SeqCst);
                    return; // Missing session_end makes this capture incomplete.
                }
                written += 1;
            } else if t.stopped.load(Ordering::SeqCst) && t.in_flight.load(Ordering::SeqCst) == 0 {
                // Recheck after producer quiescence: a final send may have raced try_recv.
                for event in receiver.try_iter() {
                    if write(&mut file, event).is_err() {
                        return;
                    }
                    written += 1;
                }
                break;
            } else {
                std::thread::yield_now();
            }
        }
        let dropped = t.counters.snapshot().1;
        let _ = writeln!(
            file,
            "{}",
            serde_json::json!({"tag":"DEBUG-hotkey","kind":"session_end","dropped":dropped,"written_events":written,"complete":dropped==0 && written==t.counters.snapshot().0,"attempted_events":t.counters.snapshot().0})
        );
        let _ = file.sync_all();
    });
    let (hook, raw) = unsafe {
        use windows_sys::Win32::UI::Input::*;
        let raw = if observers.raw_input {
            let device = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: host,
            };
            let raw =
                RegisterRawInputDevices(&device, 1, std::mem::size_of_val(&device) as u32) != 0;
            emit(Event {
                kind: "raw_input_registration",
                count: raw as u32,
                error: if raw { 0 } else { GetLastError() },
                ..Default::default()
            });
            raw
        } else {
            false
        };
        let hook = if observers.foreground {
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                null_mut(),
                Some(foreground_changed),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            emit(Event {
                kind: "foreground_registration",
                count: (!hook.is_null()) as u32,
                error: if hook.is_null() { GetLastError() } else { 0 },
                ..Default::default()
            });
            hook
        } else {
            null_mut()
        };
        emit(Event {
            kind: "foreground_initial",
            ..Default::default()
        });
        (hook, raw)
    };
    Ok(Session(Some(thread), hook, raw))
}

#[cfg(test)]
impl Session {
    pub(super) fn observers_ready(&self, observers: Observers) -> bool {
        (!observers.raw_input || self.2) && (!observers.foreground || !self.1.is_null())
    }
}

// IDs only relate hook entry to its decision, not unrelated UI commands.
pub(super) fn callback_id() -> u64 {
    TRACE
        .get()
        .map_or(0, |t| t.callbacks.fetch_add(1, Ordering::Relaxed) + 1)
}
unsafe extern "system" fn foreground_changed(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    _: u32,
    time: u32,
) {
    let mut e = Event {
        kind: "foreground_changed",
        input_time: time,
        ..Default::default()
    };
    snapshot(&mut e);
    e.foreground = hwnd as usize;
    e.pid = 0;
    GetWindowThreadProcessId(hwnd, &mut e.pid);
    emit(e);
}
pub(super) unsafe fn raw_input(l: LPARAM) {
    use windows_sys::Win32::UI::Input::*;
    if TRACE.get().is_none() {
        return;
    }
    let mut input = RAWINPUT::default();
    let mut size = std::mem::size_of_val(&input) as u32;
    let read = GetRawInputData(
        l as HRAWINPUT,
        RID_INPUT,
        &mut input as *mut _ as _,
        &mut size,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );
    if read == u32::MAX {
        emit(Event {
            kind: "raw_input_error",
            error: GetLastError(),
            ..Default::default()
        });
        return;
    }
    if read < (std::mem::size_of::<RAWINPUTHEADER>() + std::mem::size_of::<RAWKEYBOARD>()) as u32
        || input.header.dwType != RIM_TYPEKEYBOARD
    {
        return;
    }
    let k = input.data.keyboard;
    let key = match k.VKey as u32 {
        0x10 => {
            if k.MakeCode == 0x36 {
                0xa1
            } else {
                0xa0
            }
        }
        0x11 => {
            if k.Flags & 2 != 0 {
                0xa3
            } else {
                0xa2
            }
        }
        0x12 => {
            if k.Flags & 2 != 0 {
                0xa5
            } else {
                0xa4
            }
        }
        key => key,
    };
    if watches_key(key) {
        emit(Event {
            kind: "raw_input",
            key,
            message: k.Message,
            flags: k.Flags as u32,
            scan_code: k.MakeCode as u32,
            input_time: GetMessageTime() as u32,
            ..Default::default()
        });
    }
}
unsafe fn windows_version() -> serde_json::Value {
    use windows_sys::Win32::System::Registry::*;
    let path = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
    let mut build = [0u16; 64];
    let mut size = std::mem::size_of_val(&build) as u32;
    let ok = RegGetValueW(
        HKEY_LOCAL_MACHINE,
        path.as_ptr(),
        wide("CurrentBuildNumber").as_ptr(),
        RRF_RT_REG_SZ,
        null_mut(),
        build.as_mut_ptr() as _,
        &mut size,
    ) == 0;
    let mut ubr = 0u32;
    let mut size = 4;
    let ubr_ok = RegGetValueW(
        HKEY_LOCAL_MACHINE,
        path.as_ptr(),
        wide("UBR").as_ptr(),
        RRF_RT_REG_DWORD,
        null_mut(),
        &mut ubr as *mut _ as _,
        &mut size,
    ) == 0;
    serde_json::json!({"build":ok.then(|| String::from_utf16_lossy(&build[..build.iter().position(|c| *c == 0).unwrap_or(build.len())])),"ubr":ubr_ok.then_some(ubr)})
}
thread_local! { static SUMMON: std::cell::Cell<Option<usize>> = const {std::cell::Cell::new(None)}; }
pub(super) struct SummonScope(Option<usize>);
impl SummonScope {
    pub fn new(rule: Option<usize>) -> Self {
        Self(SUMMON.with(|s| s.replace(rule)))
    }
}
impl Drop for SummonScope {
    fn drop(&mut self) {
        SUMMON.with(|s| s.set(self.0));
    }
}
pub(super) fn window_operation(target: HWND, action: &'static str, result: i32) {
    if let Some(index) = SUMMON.with(|s| s.get()) {
        emit(Event {
            kind: "window_operation",
            reason: action,
            target: target as usize,
            index,
            count: result as u32,
            ..Default::default()
        });
    }
}

pub(super) fn elapsed_since(micros: u64) -> u64 {
    TRACE.get().map_or(0, |t| {
        (t.start.elapsed().as_micros() as u64).saturating_sub(micros)
    })
}
