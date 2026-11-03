#![allow(unsafe_op_in_unsafe_fn)]
mod assets;
mod buffering;
mod design;
mod fonts;
mod recorder;
mod rest_art;
mod rest_ui;
mod scroll_ui;
mod swim;
mod theme;
mod ui;
use dunyu::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    ptr::{null, null_mut},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{Com::*, LibraryLoader::*, Registry::*, Threading::*},
    UI::{
        Accessibility::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*,
    },
};
use windows_sys::core::BOOL;
const CLASS: &str = "DunYu.Host.1";
const PROP: &str = "DunYu.WindowToken.1";
const TRAY: u32 = WM_APP + 1;
const DESTROYED: u32 = WM_APP + 2;
const OPEN: u32 = WM_APP + 3;
const LAUNCHED: u32 = WM_APP + 4;
const TIMER: usize = 1;
fn hotkey_modifiers(key: &Hotkey, command: Command) -> u32 {
    key.modifiers
        | if matches!(command, Command::Opacity(_)) {
            0
        } else {
            MOD_NOREPEAT
        }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn handle(n: usize) -> HWND {
    n as HWND
}
unsafe fn error(s: &str) {
    MessageBoxW(
        null_mut(),
        wide(s).as_ptr(),
        wide("遁鱼 DunYu").as_ptr(),
        MB_OK | MB_ICONWARNING,
    );
}
unsafe fn title(h: HWND) -> String {
    let mut b = [0u16; 1024];
    let n = GetWindowTextW(h, b.as_mut_ptr(), 1024);
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
unsafe fn class(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = GetClassNameW(h, b.as_mut_ptr(), 256);
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
unsafe fn process(h: HWND) -> Option<(u32, u64, String)> {
    let mut pid = 0;
    GetWindowThreadProcessId(h, &mut pid);
    let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if p.is_null() {
        return None;
    }
    let (mut created, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) = (
        std::mem::zeroed(),
        std::mem::zeroed(),
        std::mem::zeroed(),
        std::mem::zeroed(),
    );
    let mut b = vec![0u16; 32768];
    let mut size = b.len() as u32;
    let ok = GetProcessTimes(p, &mut created, &mut exit, &mut kernel, &mut user) != 0
        && QueryFullProcessImageNameW(p, 0, b.as_mut_ptr(), &mut size) != 0;
    CloseHandle(p);
    ok.then(|| {
        (
            pid,
            ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
            String::from_utf16_lossy(&b[..size as usize]),
        )
    })
}
unsafe fn eligible(h: HWND) -> bool {
    !h.is_null()
        && IsWindow(h) != 0
        && h != GetDesktopWindow()
        && h != GetShellWindow()
        && GetAncestor(h, GA_ROOT) == h
        && ![
            CLASS,
            "DunYu.Settings.1",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "Progman",
            "WorkerW",
        ]
        .contains(&class(h).as_str())
        && GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
        && !title(h).is_empty()
}
#[derive(Clone, Serialize, Deserialize)]
struct Token {
    hwnd: usize,
    pid: u32,
    created: u64,
    tag: usize,
}
impl Token {
    unsafe fn valid(&self) -> bool {
        let h = handle(self.hwnd);
        IsWindow(h) != 0
            && GetPropW(h, wide(PROP).as_ptr()) as usize == self.tag
            && process(h).is_some_and(|(p, c, _)| p == self.pid && c == self.created)
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Alpha {
    layered: bool,
    alpha: u8,
    color: u32,
    flags: u32,
    last: u8,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    token: Token,
    show: Option<u32>,
    alpha: Option<Alpha>,
}
struct Pending {
    rule: usize,
    started: Instant,
}
struct Ui {
    window: HWND,
    theme: std::rc::Rc<theme::Theme>,
    page: usize,
    controls: Vec<(usize, HWND)>,
    content: HWND,
    advanced: bool,
    scroll: i32,
    list_scroll: i32,
    motions: [dunyu::scroll::Motion; 2],
    split_ratio: f64,
    dragging: bool,
    baseline: Vec<String>,
    list: HWND,
    selected: Option<usize>,
    target: Option<Token>,
    opacity: HWND,
    dpi: u32,
    font: HFONT,
    heading_font: HFONT,
    value_font: HFONT,
    pending_opacity: Option<(Token, u8)>,
}
struct App {
    rest: rest_ui::Runtime,
    host: HWND,
    config: AppConfig,
    dir: PathBuf,
    records: Vec<Record>,
    slots: Vec<Option<Token>>,
    targets: HashMap<usize, Token>,
    keys: HashMap<i32, (Hotkey, Command)>,
    next_id: i32,
    pending: Vec<Pending>,
    ui: Option<Ui>,
    hook: HWINEVENTHOOK,
    taskbar: u32,
    can_write: bool,
    launch_results: std::sync::Arc<std::sync::Mutex<Vec<(usize, Result<(), String>)>>>,
}
thread_local! {static APP:RefCell<Option<App>>=const{RefCell::new(None)};}
unsafe extern "system" fn enumerate(h: HWND, p: LPARAM) -> BOOL {
    let v = &mut *(p as *mut Vec<usize>);
    if eligible(h) && IsWindowVisible(h) != 0 {
        v.push(h as usize);
    }
    1
}
unsafe fn windows() -> Vec<usize> {
    let mut v = vec![];
    EnumWindows(Some(enumerate), &mut v as *mut _ as LPARAM);
    v
}
unsafe extern "system" fn event(
    _: HWINEVENTHOOK,
    _: u32,
    h: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if object == 0 && child == 0 {
        let host = FindWindowW(wide(CLASS).as_ptr(), null());
        if !host.is_null() {
            PostMessageW(host, DESTROYED, h as usize, 0);
        }
    }
}
impl App {
    fn journal(&self) -> Result<(), String> {
        if !self.can_write {
            return Err("恢复记录不可写，已停止窗口修改。请检查 data 目录。".into());
        }
        atomic_json(&self.dir.join("recovery.json"), &self.records).map_err(|e| e.to_string())
    }
    unsafe fn track(&mut self, h: HWND) -> Result<usize, String> {
        if let Some(i) = self
            .records
            .iter()
            .position(|r| r.token.hwnd == h as usize && r.token.valid())
        {
            return Ok(i);
        }
        if !eligible(h) {
            return Err("请选择可操作的外部窗口".into());
        }
        let (pid, created, _) = process(h).ok_or("无法读取窗口进程；请检查权限")?;
        let tag = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as usize)
            | 1;
        if SetPropW(h, wide(PROP).as_ptr(), tag as HANDLE) == 0 {
            return Err("无法标记目标窗口；可能需要以管理员身份运行".into());
        }
        self.records.push(Record {
            token: Token {
                hwnd: h as usize,
                pid,
                created,
                tag,
            },
            show: None,
            alpha: None,
        });
        Ok(self.records.len() - 1)
    }
    unsafe fn restore(&mut self, i: usize) -> bool {
        let r = self.records[i].clone();
        if !r.token.valid() {
            return true;
        }
        let h = handle(r.token.hwnd);
        let mut ok = true;
        if let Some(show) = r.show {
            if IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
                ShowWindowAsync(h, show as i32);
                // Async requests are confirmed by the timer, never assumed complete.
                if IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
                    ok = false;
                } else {
                    self.records[i].show = None;
                }
            } else {
                self.records[i].show = None;
            }
        }
        if let Some(a) = r.alpha {
            let mut current = 0;
            let mut color = 0;
            let mut flags = 0;
            if GetLayeredWindowAttributes(h, &mut color, &mut current, &mut flags) == 0
                || current != a.last
            {
                self.records[i].alpha = None;
            } else {
                let restored = if a.layered {
                    SetLayeredWindowAttributes(h, a.color, a.alpha, a.flags) != 0
                } else {
                    SetLastError(0);
                    SetWindowLongPtrW(
                        h,
                        GWL_EXSTYLE,
                        GetWindowLongPtrW(h, GWL_EXSTYLE) & !(WS_EX_LAYERED as isize),
                    );
                    GetLastError() == 0
                };
                if restored {
                    self.records[i].alpha = None;
                } else {
                    ok = false;
                }
            }
        }
        ok
    }
    unsafe fn restore_all(&mut self) {
        self.cancel_opacity();
        self.records.retain(|r| r.token.valid());
        for i in 0..self.records.len() {
            self.restore(i);
        }
        if let Err(e) = self.journal() {
            error(&e);
        }
        self.refresh_state();
    }
    unsafe fn toggle(&mut self, t: Token, minimize: bool) -> Result<(), String> {
        if !t.valid() {
            return Err("目标窗口已关闭，请重新绑定".into());
        }
        let h = handle(t.hwnd);
        let i = self.track(h)?;
        match toggle_action(
            GetForegroundWindow() == h,
            IsWindowVisible(h) != 0,
            IsIconic(h) != 0,
            minimize,
        ) {
            ToggleAction::Hide | ToggleAction::Minimize => {
                if self.records[i].show.is_none() {
                    let mut p: WINDOWPLACEMENT = std::mem::zeroed();
                    p.length = std::mem::size_of_val(&p) as u32;
                    if GetWindowPlacement(h, &mut p) == 0 {
                        return Err("读取窗口状态失败".into());
                    }
                    self.records[i].show = Some(p.showCmd);
                }
                self.journal()?;
                if ShowWindowAsync(h, if minimize { SW_MINIMIZE } else { SW_HIDE }) == 0 {
                    return Err("窗口隐藏请求失败".into());
                }
            }
            ToggleAction::Activate => {
                let show = self.records[i].show.unwrap_or(if IsIconic(h) != 0 {
                    SW_RESTORE as u32
                } else {
                    SW_SHOW as u32
                });
                ShowWindowAsync(h, show as i32);
                if SetForegroundWindow(h) == 0 {
                    let mut f = FLASHWINFO {
                        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                        hwnd: h,
                        dwFlags: FLASHW_TRAY,
                        uCount: 3,
                        dwTimeout: 0,
                    };
                    FlashWindowEx(&mut f);
                }
            }
        }
        Ok(())
    }
    unsafe fn opacity(&mut self, h: HWND, value: Option<u8>, direction: i8) -> Result<(), String> {
        let i = self.track(h)?;
        let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let layered = style as u32 & WS_EX_LAYERED != 0;
        let (mut color, mut alpha, mut flags) = (0, 255, 0);
        if layered && GetLayeredWindowAttributes(h, &mut color, &mut alpha, &mut flags) == 0 {
            return Err("该窗口使用不支持的分层绘制方式".into());
        }
        if flags & LWA_ALPHA == 0 {
            alpha = 255;
        }
        let next = value.unwrap_or_else(|| opacity_step(alpha, direction, self.config.step));
        if next == alpha {
            return Ok(());
        }
        if self.records[i].alpha.is_none() {
            self.records[i].alpha = Some(Alpha {
                layered,
                alpha,
                color,
                flags,
                last: alpha,
            });
        }
        self.records[i].alpha.as_mut().unwrap().last = next;
        self.journal()?;
        SetLastError(0);
        SetWindowLongPtrW(h, GWL_EXSTYLE, style | WS_EX_LAYERED as isize);
        if GetLastError() != 0 {
            return Err("无法调整窗口样式".into());
        }
        if SetLayeredWindowAttributes(h, color, next, flags | LWA_ALPHA) == 0 {
            return Err("设置不透明度失败".into());
        }
        if next == 255 && !self.records[i].alpha.as_ref().unwrap().layered {
            SetLastError(0);
            SetWindowLongPtrW(h, GWL_EXSTYLE, style & !(WS_EX_LAYERED as isize));
            if GetLastError() == 0 {
                self.records[i].alpha = None;
                self.journal()?;
            }
        }
        Ok(())
    }
    unsafe fn find(&self, i: usize) -> Option<HWND> {
        let r = &self.config.rules[i];
        let foreground = GetForegroundWindow();
        let mut candidates = windows();
        candidates.sort_by_key(|h| *h != foreground as usize);
        candidates
            .into_iter()
            .map(handle)
            .find(|h| process(*h).is_some_and(|(_, _, p)| matches(r, &p, &class(*h), &title(*h))))
    }
    unsafe fn summon(&mut self, i: usize) -> Result<(), String> {
        if self.pending.iter().any(|p| p.rule == i) {
            return Ok(());
        }
        let t = self.targets.get(&i).filter(|t| t.valid()).cloned();
        if let Some(t) = t {
            return self.toggle(t, self.config.rules[i].minimize);
        }
        if let Some(h) = self.find(i) {
            let n = self.track(h)?;
            let t = self.records[n].token.clone();
            self.targets.insert(i, t.clone());
            return self.toggle(t, self.config.rules[i].minimize);
        }
        let rule = self.config.rules[i].clone();
        let host = self.host as usize;
        let results = self.launch_results.clone();
        self.pending.push(Pending {
            rule: i,
            started: Instant::now(),
        });
        SetTimer(self.host, TIMER, 100, None);
        std::thread::spawn(move || {
            let result = unsafe {
                let initialized = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) >= 0;
                let mut s: SHELLEXECUTEINFOW = std::mem::zeroed();
                s.cbSize = std::mem::size_of_val(&s) as u32;
                s.fMask = SEE_MASK_FLAG_NO_UI;
                let file = wide(&rule.executable);
                let args = wide(&rule.arguments);
                let dir = wide(&rule.working_directory);
                s.lpFile = file.as_ptr();
                s.lpParameters = args.as_ptr();
                s.lpDirectory = if rule.working_directory.is_empty() {
                    null()
                } else {
                    dir.as_ptr()
                };
                s.nShow = SW_SHOWNORMAL;
                let outcome = if ShellExecuteExW(&mut s) == 0 {
                    Err(std::io::Error::last_os_error().to_string())
                } else {
                    Ok(())
                };
                if initialized {
                    CoUninitialize();
                }
                outcome
            };
            results.lock().unwrap().push((i, result));
            unsafe {
                PostMessageW(handle(host), LAUNCHED, 0, 0);
            }
        });
        Ok(())
    }
    unsafe fn execute(&mut self, c: Command) {
        if matches!(c, Command::Opacity(_) | Command::Opaque | Command::Restore) {
            self.cancel_opacity();
        }
        let h = GetForegroundWindow();
        let result = match c {
            Command::Summon(i) => self.summon(i),
            Command::Bind(i) => {
                if let Some(t) = self.slots[i].clone() {
                    if let Some(n) = self.records.iter().position(|r| r.token.tag == t.tag) {
                        if !self.restore(n) {
                            return;
                        }
                    }
                }
                self.track(h).and_then(|n| {
                    self.slots[i] = Some(self.records[n].token.clone());
                    self.journal()
                })
            }
            Command::Toggle(i) => {
                if let Some(t) = self.slots[i].clone() {
                    self.toggle(t, false)
                } else {
                    Err("槽位为空，请先绑定窗口".into())
                }
            }
            Command::Opacity(d) => self.opacity(h, None, d),
            Command::Opaque => self.opacity(h, Some(255), 0),
            Command::Restore => {
                self.restore_all();
                Ok(())
            }
        };
        if let Err(e) = result {
            error(&e);
        }
        self.refresh_state();
    }
    unsafe fn register(&mut self, c: &AppConfig, save: bool) -> Result<(), String> {
        c.validate()?;
        let desired = c.bindings();
        let mut added = vec![];
        let mut changed = vec![];
        let result = (|| -> Result<(), String> {
            for (key, cmd) in &desired {
                let modifiers = hotkey_modifiers(key, *cmd);
                if let Some((&id, (_, old_cmd))) = self.keys.iter().find(|(_, (k, _))| k == key) {
                    let old_modifiers = hotkey_modifiers(key, *old_cmd);
                    if modifiers != old_modifiers {
                        // Reassigning the same combination can change repeat behavior.
                        // Windows requires unregistering before replacing an existing ID.
                        if UnregisterHotKey(self.host, id) == 0 {
                            return Err("无法更新快捷键的长按设置".into());
                        }
                        changed.push((id, key.clone(), old_modifiers));
                        if RegisterHotKey(self.host, id, modifiers, key.key) == 0 {
                            return Err("无法更新快捷键的长按设置".into());
                        }
                    }
                    continue;
                }
                self.next_id += 1;
                let id = self.next_id;
                if id > 0xbfff || RegisterHotKey(self.host, id, modifiers, key.key) == 0 {
                    return Err(format!(
                        "快捷键 {} 注册失败：可能被系统或其他程序占用",
                        ui::hotkey_text(key)
                    ));
                }
                self.keys.insert(id, (key.clone(), *cmd));
                added.push(id);
            }
            if save {
                atomic_json(&self.dir.join("config.json"), c).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(mut e) = result {
            for id in added {
                UnregisterHotKey(self.host, id);
                self.keys.remove(&id);
            }
            for (id, key, modifiers) in changed.into_iter().rev() {
                UnregisterHotKey(self.host, id);
                if RegisterHotKey(self.host, id, modifiers, key.key) == 0 {
                    self.keys.remove(&id);
                    e.push_str("；旧快捷键恢复失败，请关闭占用它的程序后重新保存设置");
                }
            }
            return Err(e);
        }
        let obsolete: Vec<_> = self
            .keys
            .iter()
            .filter(|(_, (k, _))| !desired.iter().any(|(d, _)| d == k))
            .map(|(id, _)| *id)
            .collect();
        for id in obsolete {
            UnregisterHotKey(self.host, id);
            self.keys.remove(&id);
        }
        for (k, cmd) in self.keys.values_mut() {
            *cmd = desired.iter().find(|(d, _)| d == k).unwrap().1;
        }
        Ok(())
    }
    unsafe fn tick(&mut self) {
        let ids: Vec<_> = self.pending.iter().map(|p| p.rule).collect();
        for i in ids {
            if let Some(h) = self.find(i) {
                if let Ok(n) = self.track(h) {
                    self.targets.insert(i, self.records[n].token.clone());
                    ShowWindowAsync(h, SW_SHOW);
                    SetForegroundWindow(h);
                }
                self.pending.retain(|p| p.rule != i);
            } else if self
                .pending
                .iter()
                .any(|p| p.rule == i && p.started.elapsed().as_secs() >= 10)
            {
                self.pending.retain(|p| p.rule != i);
                error("启动后未找到匹配窗口。请核对实际进程路径、类名与标题。");
            }
        }
        if self.pending.is_empty() {
            KillTimer(self.host, TIMER);
        }
    }
    unsafe fn tray(&self) {
        let mut n: NOTIFYICONDATAW = std::mem::zeroed();
        n.cbSize = std::mem::size_of_val(&n) as u32;
        n.hWnd = self.host;
        n.uID = 1;
        if self.config.hide_tray {
            Shell_NotifyIconW(NIM_DELETE, &n);
            return;
        }
        n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        n.uCallbackMessage = TRAY;
        n.hIcon = LoadIconW(GetModuleHandleW(null()), 101usize as _);
        let tip = wide("遁鱼 DunYu · 摸鱼有道");
        n.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(NIM_ADD, &n);
    }
    unsafe fn menu(&mut self) {
        let m = CreatePopupMenu();
        AppendMenuW(m, MF_STRING, 1, wide("设置").as_ptr());
        AppendMenuW(m, MF_STRING, 2, wide("恢复全部窗口与透明度").as_ptr());
        for (i, r) in self.records.iter().enumerate() {
            if r.show.is_some() && r.token.valid() {
                AppendMenuW(
                    m,
                    MF_STRING,
                    10000 + i,
                    wide(&format!("恢复：{}", title(handle(r.token.hwnd)))).as_ptr(),
                );
            }
        }
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(
            m,
            MF_STRING,
            24,
            wide(&format!("休憩 · {}", self.rest.status())).as_ptr(),
        );
        AppendMenuW(m, MF_STRING, 20, wide("休憩：暂停30分钟").as_ptr());
        AppendMenuW(m, MF_STRING, 21, wide("休憩：暂停1小时").as_ptr());
        AppendMenuW(m, MF_STRING, 22, wide("休憩：恢复提醒").as_ptr());
        AppendMenuW(m, MF_STRING, 23, wide("休憩：预览").as_ptr());
        AppendMenuW(m, MF_STRING, 3, wide("退出并恢复").as_ptr());
        let mut p: POINT = std::mem::zeroed();
        GetCursorPos(&mut p);
        SetForegroundWindow(self.host);
        let id = TrackPopupMenu(
            m,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            self.host,
            null(),
        );
        DestroyMenu(m);
        if id != 0 {
            PostMessageW(self.host, WM_COMMAND, id as usize, 0);
        }
    }
}
unsafe extern "system" fn procedure(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let handled = std::panic::catch_unwind(|| {
        APP.with(|cell| {
            let Ok(mut guard) = cell.try_borrow_mut() else {
                return None;
            };
            let Some(a) = guard.as_mut() else {
                return None;
            };
            if msg == a.taskbar {
                a.tray();
                return Some(0);
            }
            match msg {
                WM_CLOSE => {
                    if h == a.host {
                        PostMessageW(h, WM_COMMAND, 3, 0);
                    } else {
                        a.cancel_opacity();
                        a.refresh_state();
                        ShowWindow(h, SW_HIDE);
                    }
                    Some(0)
                }
                OPEN => {
                    a.open_ui();
                    Some(0)
                }
                WM_HOTKEY => {
                    if let Some((_, cmd)) = a.keys.get(&(w as i32)).cloned() {
                        a.execute(cmd);
                    }
                    Some(0)
                }
                TRAY => {
                    if l as u32 == WM_RBUTTONUP {
                        a.menu();
                    } else if l as u32 == WM_LBUTTONDBLCLK {
                        a.open_ui();
                    }
                    Some(0)
                }
                rest_ui::ACTION => {
                    a.rest.action(w, Some(l as usize), a.host);
                    Some(0)
                }
                WM_WTSSESSION_CHANGE => {
                    if matches!(w, 7 | 2 | 4) {
                        a.rest.locked = true;
                    }
                    if matches!(w, 8 | 1 | 3) {
                        a.rest.locked = false;
                    }
                    a.rest.tick(a.host);
                    Some(0)
                }
                WM_POWERBROADCAST => {
                    if w == PBT_APMSUSPEND as usize {
                        a.rest.suspended = true;
                    }
                    if w == PBT_APMRESUMESUSPEND as usize || w == PBT_APMRESUMEAUTOMATIC as usize {
                        a.rest.suspended = false;
                    }
                    a.rest.tick(a.host);
                    Some(1)
                }
                WM_DISPLAYCHANGE => {
                    if a.rest.scheduler.phase == dunyu::rest::Phase::Warning {
                        a.rest.scheduler.phase = dunyu::rest::Phase::Due;
                    }
                    a.rest.close();
                    Some(0)
                }
                WM_TIMER => {
                    if w == scroll_ui::TIMER {
                        a.scroll_frame();
                    } else if w == rest_ui::TIMER_ID {
                        a.rest.tick(a.host);
                        if let Some(u) = &a.ui {
                            ui::set_text(u.window, 810, &a.rest.status());
                        }
                    } else if w == TIMER {
                        a.tick();
                    } else if w == 900 {
                        KillTimer(h, 900);
                        let pending = a.ui.as_mut().and_then(|u| u.pending_opacity.take());
                        if let Some((token, next)) = pending {
                            let valid = token.valid()
                                && a.ui
                                    .as_ref()
                                    .and_then(|u| u.target.as_ref())
                                    .is_some_and(|t| t.tag == token.tag);
                            if valid {
                                if let Err(e) = a.opacity(
                                    handle(token.hwnd),
                                    Some(((next as u32 * 255 + 50) / 100) as u8),
                                    0,
                                ) {
                                    a.status_error(&e);
                                }
                            } else {
                                a.status_error("目标窗口已失效，请重新捕获");
                            }
                        }
                        a.refresh_state();
                    } else {
                        a.capture(w);
                    }
                    Some(0)
                }
                WM_GETMINMAXINFO if h != a.host && l != 0 => {
                    let dpi = GetDpiForWindow(h).max(96);
                    let mut bounds = RECT {
                        left: 0,
                        top: 0,
                        right: 760 * dpi as i32 / 96,
                        bottom: 420 * dpi as i32 / 96,
                    };
                    AdjustWindowRectExForDpi(
                        &mut bounds,
                        WS_OVERLAPPEDWINDOW,
                        0,
                        WS_EX_CONTROLPARENT,
                        dpi,
                    );
                    let mut monitor: MONITORINFO = std::mem::zeroed();
                    monitor.cbSize = std::mem::size_of_val(&monitor) as u32;
                    GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut monitor);
                    let info = &mut *(l as *mut MINMAXINFO);
                    info.ptMinTrackSize = POINT {
                        x: (bounds.right - bounds.left)
                            .min(monitor.rcWork.right - monitor.rcWork.left),
                        y: (bounds.bottom - bounds.top)
                            .min(monitor.rcWork.bottom - monitor.rcWork.top),
                    };
                    Some(0)
                }
                WM_SIZE => {
                    if h != a.host {
                        KillTimer(a.host, scroll_ui::TIMER);
                        a.layout_ui();
                        if let Some(u) = a.ui.as_mut() {
                            u.motions[0].snap(u.scroll as f64);
                            u.motions[1].snap(u.list_scroll as f64);
                        }
                    }
                    Some(0)
                }
                WM_NOTIFY => {
                    if l != 0 {
                        let hdr = &*(l as *const windows_sys::Win32::UI::Controls::NMHDR);
                        if hdr.idFrom == 9
                            && hdr.code == windows_sys::Win32::UI::Controls::TCN_SELCHANGE
                        {
                            let page = SendMessageW(
                                hdr.hwndFrom,
                                windows_sys::Win32::UI::Controls::TCM_GETCURSEL,
                                0,
                                0,
                            );
                            if page >= 0 {
                                a.page(page as usize);
                            }
                        }
                    }
                    Some(0)
                }
                WM_VSCROLL if l != 0 && GetDlgCtrlID(l as HWND) == 9004 => {
                    if let Some(u) = &a.ui {
                        let mut si: SCROLLINFO = std::mem::zeroed();
                        si.cbSize = std::mem::size_of_val(&si) as u32;
                        si.fMask = SIF_ALL;
                        GetScrollInfo(l as HWND, SB_CTL, &mut si);
                        let next = match w as i32 & 0xffff {
                            SB_LINEUP => u.list_scroll - 64,
                            SB_LINEDOWN => u.list_scroll + 64,
                            SB_PAGEUP => u.list_scroll - si.nPage as i32,
                            SB_PAGEDOWN => u.list_scroll + si.nPage as i32,
                            SB_THUMBTRACK | SB_THUMBPOSITION => si.nTrackPos,
                            _ => u.list_scroll,
                        };
                        let next = next.clamp(0, (si.nMax - si.nPage as i32 + 1).max(0));
                        if next == u.list_scroll {
                            return Some(0);
                        }
                        let _ = u;
                        if let Some(u) = a.ui.as_mut() {
                            u.list_scroll = next;
                            u.motions[1].snap(next as f64);
                        }
                        a.layout_scrolled_ui();
                    }
                    Some(0)
                }
                WM_VSCROLL | WM_MOUSEWHEEL => {
                    if let Some(u) = a.ui.as_mut() {
                        let delta = if msg == WM_MOUSEWHEEL {
                            -((w >> 16) as i16 as i32) / 120 * 48
                        } else {
                            match w as i32 & 0xffff {
                                SB_LINEUP => -24,
                                SB_LINEDOWN => 24,
                                SB_PAGEUP => -200,
                                SB_PAGEDOWN => 200,
                                SB_THUMBTRACK | SB_THUMBPOSITION => {
                                    let mut si: SCROLLINFO = std::mem::zeroed();
                                    si.cbSize = std::mem::size_of_val(&si) as u32;
                                    si.fMask = SIF_TRACKPOS;
                                    GetScrollInfo(GetDlgItem(u.window, 9003), SB_CTL, &mut si);
                                    si.nTrackPos - u.scroll
                                }
                                _ => 0,
                            }
                        };
                        let mut si: SCROLLINFO = std::mem::zeroed();
                        si.cbSize = std::mem::size_of_val(&si) as u32;
                        si.fMask = SIF_ALL;
                        GetScrollInfo(GetDlgItem(u.window, 9003), SB_CTL, &mut si);
                        let next =
                            (u.scroll + delta).clamp(0, (si.nMax - si.nPage as i32 + 1).max(0));
                        if next == u.scroll {
                            return Some(0);
                        }
                        u.scroll = next;
                        u.motions[0].snap(u.scroll as f64);
                    }
                    a.layout_scrolled_ui();
                    Some(0)
                }
                WM_DPICHANGED => {
                    if h != a.host && l != 0 {
                        a.scale_ui((w & 0xffff) as u32, Some(*(l as *const RECT)));
                    }
                    Some(0)
                }
                WM_HSCROLL => {
                    if let Some(u) = a.ui.as_mut()
                        && l == u.opacity as isize
                    {
                        let value = SendMessageW(u.opacity, WM_USER, 0, 0);
                        ui::set_text(u.window, 405, &format!("{value}%"));
                        if let Some(token) = u.target.as_ref().filter(|t| t.valid()) {
                            u.pending_opacity = Some((token.clone(), value.clamp(10, 100) as u8));
                            SetTimer(a.host, 900, 60, None);
                        }
                    }
                    Some(0)
                }
                LAUNCHED => {
                    let results = std::mem::take(&mut *a.launch_results.lock().unwrap());
                    for (i, r) in results {
                        if let Err(e) = r {
                            a.pending.retain(|p| p.rule != i);
                            error(&format!("启动失败：{e}"));
                        }
                    }
                    Some(0)
                }
                DESTROYED => {
                    let before = a.records.len();
                    a.records.retain(|r| r.token.hwnd != w || r.token.valid());
                    for slot in &mut a.slots {
                        if slot.as_ref().is_some_and(|t| !t.valid()) {
                            *slot = None;
                        }
                    }
                    a.targets.retain(|_, t| t.valid());
                    if before != a.records.len() {
                        let _ = a.journal();
                    }
                    a.refresh_state();
                    Some(0)
                }
                WM_COMMAND => {
                    let id = (w & 0xffff) as i32;
                    if h == a.host {
                        match id {
                            1 => a.open_ui(),
                            24 => {
                                a.open_ui();
                                a.page(3);
                            }
                            20..=23 => a.rest.action(w, None, a.host),
                            2 => a.restore_all(),
                            3 => {
                                a.restore_all();
                                PostQuitMessage(0);
                            }
                            10000.. => {
                                let i = id as usize - 10000;
                                if i < a.records.len() {
                                    a.restore(i);
                                    let _ = a.journal();
                                }
                            }
                            _ => {}
                        }
                    } else if id == 100 && (w >> 16) == LBN_SELCHANGE as usize {
                        let n = SendMessageW(a.ui.as_ref().unwrap().list, LB_GETCURSEL, 0, 0);
                        if n >= 0 {
                            if a.ui.as_ref().unwrap().selected != Some(n as usize) && a.leave_rule()
                            {
                                a.fill_rule(Some(n as usize));
                            } else {
                                let u = a.ui.as_ref().unwrap();
                                SendMessageW(
                                    u.list,
                                    LB_SETCURSEL,
                                    u.selected.unwrap_or(usize::MAX),
                                    0,
                                );
                            }
                        }
                    } else if (w >> 16) == BN_CLICKED as usize {
                        if let Err(e) = a.ui_action(id) {
                            a.status_error(&e);
                        }
                    }
                    Some(0)
                }
                _ => None,
            }
        })
    });
    match handled {
        Ok(Some(n)) => n,
        Ok(None) => DefWindowProcW(h, msg, w, l),
        Err(_) => {
            PostQuitMessage(1);
            DefWindowProcW(h, msg, w, l)
        }
    }
}
pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        windows_sys::Win32::UI::Controls::InitCommonControls();
        let mutex = CreateMutexW(null(), 0, wide("Local\\DunYu.Singleton.1").as_ptr());
        if mutex.is_null() {
            error("无法创建单实例锁");
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let h = FindWindowW(wide(CLASS).as_ptr(), null());
            if !h.is_null() {
                PostMessageW(h, OPEN, 0, 0);
            }
            CloseHandle(mutex);
            return;
        }
        let instance = GetModuleHandleW(null());
        for name in [CLASS, "DunYu.Settings.1", "DunYu.Content.1"] {
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = if name == "DunYu.Content.1" {
                Some(ui::content_proc)
            } else {
                Some(procedure)
            };
            wc.hInstance = instance;
            let n = wide(name);
            wc.lpszClassName = n.as_ptr();
            wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
            wc.hIcon = LoadIconW(GetModuleHandleW(null()), 101usize as _);
            wc.hbrBackground = (COLOR_BTNFACE + 1) as HBRUSH;
            if RegisterClassW(&wc) == 0 {
                error("创建窗口类失败");
                CloseHandle(mutex);
                return;
            }
        }
        let host = CreateWindowExW(
            0,
            wide(CLASS).as_ptr(),
            wide("DunYu").as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if host.is_null() {
            error("创建宿主窗口失败");
            CloseHandle(mutex);
            return;
        }
        let dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("data");
        let first = !dir.join("config.json").exists();
        let mut config_error = false;
        let config = if first {
            AppConfig::default()
        } else {
            match load(&dir.join("config.json")) {
                Ok(c) => c,
                Err(e) => {
                    error(&format!("配置无法读取，原文件已保留：{e}"));
                    config_error = true;
                    AppConfig::default()
                }
            }
        };
        let (records, can_write) = match std::fs::read(dir.join("recovery.json")) {
            Ok(b) => match serde_json::from_slice::<Vec<Record>>(&b) {
                Ok(r) => (r, true),
                Err(e) => {
                    error(&format!("恢复记录损坏，已保留文件并禁用窗口修改：{e}"));
                    (vec![], false)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (vec![], true),
            Err(e) => {
                error(&format!("无法读取恢复记录：{e}"));
                (vec![], false)
            }
        };
        let mut app = App {
            host,
            slots: vec![None; config.slots.len()],
            config: config.clone(),
            dir,
            records,
            targets: HashMap::new(),
            keys: HashMap::new(),
            next_id: 0,
            pending: vec![],
            ui: None,
            hook: null_mut(),
            taskbar: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            can_write,
            launch_results: Default::default(),
            rest: Default::default(),
        };
        app.restore_all();
        if let Err(e) = app.register(&config, first) {
            error(&e);
            config_error = true;
        }
        app.hook = SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_DESTROY,
            null_mut(),
            Some(event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        app.rest.configure(config.rest.clone(), host);
        windows_sys::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(host, 0);
        app.tray();
        APP.with(|a| *a.borrow_mut() = Some(app));
        if first || config_error {
            PostMessageW(host, OPEN, 0, 0);
        }
        let mut msg: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                break;
            }
            let ui = APP.with(|a| {
                a.borrow()
                    .as_ref()
                    .and_then(|a| a.ui.as_ref().map(|u| u.window))
            });
            if scroll_ui::filter(&msg) {
                continue;
            }
            if rest_ui::Runtime::filter(&msg) {
                continue;
            }
            if ui.is_some_and(|h| ui::capture_hotkey(h, &msg) || IsDialogMessageW(h, &msg) != 0) {
                APP.with(|cell| {
                    if let Some(a) = cell.borrow_mut().as_mut() {
                        a.reveal_focus();
                    }
                });
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|cell| {
            let mut guard = cell.borrow_mut();
            let a = guard.as_mut().unwrap();
            for id in a.keys.keys() {
                UnregisterHotKey(host, *id);
            }
            a.rest.close();
            KillTimer(host, rest_ui::TIMER_ID);
            windows_sys::Win32::System::RemoteDesktop::WTSUnRegisterSessionNotification(host);
            UnhookWinEvent(a.hook);
            if let Some(u) = &a.ui {
                DestroyWindow(u.window);
                if !u.font.is_null() {
                    DeleteObject(u.font);
                }
                if !u.value_font.is_null() {
                    DeleteObject(u.value_font);
                }
                if !u.heading_font.is_null() {
                    DeleteObject(u.heading_font);
                }
            }
            for r in &a.records {
                if r.show.is_none() && r.alpha.is_none() && r.token.valid() {
                    RemovePropW(handle(r.token.hwnd), wide(PROP).as_ptr());
                }
            }
            a.records.retain(|r| r.show.is_some() || r.alpha.is_some());
            let _ = a.journal();
            let mut n: NOTIFYICONDATAW = std::mem::zeroed();
            n.cbSize = std::mem::size_of_val(&n) as u32;
            n.hWnd = host;
            n.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &n);
        });
        DestroyWindow(host);
        CloseHandle(mutex);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn pump() {
        let mut m: MSG = std::mem::zeroed();
        while PeekMessageW(&mut m, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
    #[test]
    fn native_window_opacity_and_recovery() {
        unsafe {
            let h = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("DunYu integration fixture").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                20,
                20,
                240,
                120,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!h.is_null());
            let dir = std::env::temp_dir().join(format!("dunyu-test-{}", std::process::id()));
            let mut app = App {
                host: h,
                config: AppConfig::default(),
                dir: dir.clone(),
                records: vec![],
                slots: vec![None; 4],
                targets: HashMap::new(),
                keys: HashMap::new(),
                next_id: 0,
                pending: vec![],
                ui: None,
                hook: null_mut(),
                taskbar: 0,
                can_write: true,
                launch_results: Default::default(),
                rest: Default::default(),
            };
            app.opacity(h, Some(128), 0).unwrap();
            let (mut color, mut alpha, mut flags) = (0, 0, 0);
            assert_ne!(
                GetLayeredWindowAttributes(h, &mut color, &mut alpha, &mut flags),
                0
            );
            assert_eq!(alpha, 128);
            assert!(app.restore(0));
            assert_eq!(GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_LAYERED, 0);
            SetWindowLongPtrW(
                h,
                GWL_EXSTYLE,
                GetWindowLongPtrW(h, GWL_EXSTYLE) | WS_EX_LAYERED as isize,
            );
            SetLayeredWindowAttributes(h, 0, 180, LWA_ALPHA);
            app.opacity(h, Some(90), 0).unwrap();
            assert!(app.restore(0));
            GetLayeredWindowAttributes(h, &mut color, &mut alpha, &mut flags);
            assert_eq!(alpha, 180);
            app.records[0].show = Some(SW_SHOWNORMAL as u32);
            app.journal().unwrap();
            ShowWindow(h, SW_HIDE);
            app.restore(0);
            pump();
            app.restore(0);
            assert_ne!(IsWindowVisible(h), 0);
            // A changed ownership tag simulates a reused handle: never restore it.
            app.opacity(h, Some(80), 0).unwrap();
            SetPropW(h, wide(PROP).as_ptr(), 1usize as HANDLE);
            assert!(!app.records[0].token.valid());
            assert!(app.restore(0));
            GetLayeredWindowAttributes(h, &mut color, &mut alpha, &mut flags);
            assert_eq!(alpha, 80);
            RemovePropW(h, wide(PROP).as_ptr());
            DestroyWindow(h);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn hotkey_text_round_trip() {
        for (k, _) in AppConfig::default().bindings() {
            assert_eq!(ui::parse_key(&ui::hotkey_text(&k)).unwrap(), k);
        }
        assert!(ui::parse_key("Ctrl+A+B").is_err());
    }
}
