use super::*;
pub(super) const PAGES: [&str; 6] = ["召唤", "绑定", "隐现", "休憩", "传讯", "设置"];
use windows_sys::Win32::System::SystemServices::SS_PATHELLIPSIS;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::*;

pub(super) unsafe fn item(window: HWND, id: i32) -> HWND {
    if matches!(id, 100 | 118) {
        return GetDlgItem(GetDlgItem(GetDlgItem(window, 9000), 119), id);
    }
    let direct = GetDlgItem(window, id);
    if !direct.is_null() {
        direct
    } else {
        GetDlgItem(GetDlgItem(window, 9000), id)
    }
}

pub(super) unsafe extern "system" fn content_proc(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match m {
        WM_COMMAND | WM_HSCROLL | WM_VSCROLL | WM_MOUSEWHEEL => SendMessageW(GetParent(h), m, w, l),
        _ => DefWindowProcW(h, m, w, l),
    }
}

pub(super) fn is_hotkey_field(id: i32) -> bool {
    matches!(id, 207 | 300..=307 | 500..=503)
}

// Run before IsDialogMessage/TranslateMessage so arrows, spaces and Alt
// combinations are recorded instead of navigating or activating controls.
pub(super) unsafe fn capture_hotkey(window: HWND, msg: &MSG) -> bool {
    let field = GetFocus();
    if GetForegroundWindow() != window
        || field.is_null()
        || GetAncestor(field, GA_ROOT) != window
        || IsWindowVisible(field) == 0
        || !is_hotkey_field(GetDlgCtrlID(field))
    {
        return false;
    }
    capture_key_message(field, msg)
}

unsafe fn capture_key_message(field: HWND, msg: &MSG) -> bool {
    let (modifiers, key) = match msg.message {
        // Registered combinations arrive here instead of as ordinary keydowns.
        // Consume them so recording an existing binding never executes it.
        WM_HOTKEY => (msg.lParam as u32 & 15, (msg.lParam as u32 >> 16) & 0xffff),
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let mut modifiers = 0;
            for (vk, flag) in [
                (VK_CONTROL, MOD_CONTROL),
                (VK_MENU, MOD_ALT),
                (VK_SHIFT, MOD_SHIFT),
                (VK_LWIN, MOD_WIN),
                (VK_RWIN, MOD_WIN),
            ] {
                if GetKeyState(vk as i32) < 0 {
                    modifiers |= flag;
                }
            }
            let key = msg.wParam as u32;
            if key == VK_TAB as u32 && modifiers & !MOD_SHIFT == 0 {
                return false;
            }
            if modifiers == 0 && matches!(key, 0x0d | 0x1b) {
                SetFocus(GetNextDlgTabItem(GetParent(field), field, 0));
                return true;
            }
            if msg.lParam & (1 << 30) != 0 || matches!(key, 0x10..=0x12 | 0x5b..=0x5c | 0xa0..=0xa5)
            {
                return true;
            }
            (modifiers, key)
        }
        WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR => {
            return true;
        }
        _ => return false,
    };
    if modifiers != 0
        && matches!(key, 8 | 0x20 | 0x25 | 0x27 | 0x30..=0x39 | 0x41..=0x5a | 0x70..=0x7a)
    {
        SetWindowTextW(
            field,
            wide(&hotkey_text(&Hotkey::new(modifiers, key))).as_ptr(),
        );
    } else {
        windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(MB_ICONWARNING);
    }
    true
}

pub(super) fn hotkey_text(k: &Hotkey) -> String {
    let mut parts = vec![];
    for (m, s) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
        if k.modifiers & m != 0 {
            parts.push(s.to_string());
        }
    }
    parts.push(match k.key {
        0x25 => "Left".into(),
        0x27 => "Right".into(),
        8 => "Backspace".into(),
        0x20 => "Space".into(),
        0x70..=0x7a => format!("F{}", k.key - 0x6f),
        k => (char::from_u32(k).unwrap_or('?')).to_string(),
    });
    parts.join("+")
}
pub(super) fn parse_key(s: &str) -> Result<Hotkey, String> {
    let mut mods = 0;
    let mut key = None;
    for p in s.split('+') {
        let p = p.trim().to_uppercase();
        match p.as_str() {
            "CTRL" => mods |= 2,
            "ALT" => mods |= 1,
            "SHIFT" => mods |= 4,
            "WIN" => mods |= 8,
            _ => {
                if key.is_some() {
                    return Err("快捷键只能包含一个主键".into());
                }
                key = Some(match p.as_str() {
                    "LEFT" => 0x25,
                    "RIGHT" => 0x27,
                    "BACKSPACE" => 8,
                    "SPACE" => 32,
                    _ => {
                        if p.starts_with('F') && p.len() > 1 {
                            {
                                let n = p[1..].parse::<u32>().map_err(|_| "无效功能键")?;
                                if !(1..=11).contains(&n) {
                                    return Err("支持 F1～F11；F12 为系统保留键".into());
                                }
                                0x6f + n
                            }
                        } else if p.len() == 1 {
                            p.as_bytes()[0] as u32
                        } else {
                            return Err("快捷键示例：Ctrl+Alt+1、Ctrl+Alt+Left".into());
                        }
                    }
                });
            }
        }
    }
    Ok(Hotkey::new(mods, key.ok_or("缺少主键")?))
}
pub(super) unsafe fn text_at(h: HWND, id: i32) -> String {
    let e = item(h, id);
    let n = GetWindowTextLengthW(e);
    let mut b = vec![0; n as usize + 1];
    GetWindowTextW(e, b.as_mut_ptr(), b.len() as i32);
    String::from_utf16_lossy(&b[..n as usize])
}
pub(super) unsafe fn set_text(h: HWND, id: i32, s: &str) {
    let child = item(h, id);
    if title(child) != s {
        SetWindowTextW(child, wide(s).as_ptr());
    }
}
pub(super) unsafe fn checked(h: HWND, id: i32) -> bool {
    SendMessageW(item(h, id), BM_GETCHECK, 0, 0) == 1
}
pub(super) unsafe fn check(h: HWND, id: i32, on: bool) {
    SendMessageW(item(h, id), BM_SETCHECK, usize::from(on), 0);
}
pub(super) unsafe fn control(
    u: &mut Ui,
    page: usize,
    kind: &str,
    label: &str,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    style: u32,
) -> HWND {
    let page = if page == 3 { 5 } else { page };
    let id = if id == 0 {
        match label {
            "程序召唤" => 7000,
            "窗口绑定 · 仅在当前会话有效" => 7001,
            "窗口不透明度" => 7002,
            "全局快捷键" => 7003,
            "调整步长" => 7040,
            "启动与后台运行" => 7050,
            "点击后切换到需要调整的窗口" => 7010,
            "增减快捷键支持长按连续调整；步长可在设置页修改。" => 7011,
            "点击快捷键框后按组合键，Tab 离开。修改后需要保存。" => 7051,
            _ => id,
        }
    } else {
        id
    };
    let x = if page == 5 {
        match x {
            244 | 245 => 20,
            380 => 200,
            _ => x,
        }
    } else {
        x
    };
    let style = if kind == "EDIT" && is_hotkey_field(id) {
        style | ES_READONLY as u32
    } else {
        style
    };
    let c = CreateWindowExW(
        0,
        wide(kind).as_ptr(),
        wide(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        w,
        h,
        if page == 99 || matches!(id, 113 | 330 | 506) {
            u.window
        } else {
            u.content
        },
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(c, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
    let role = match kind {
        "STATIC" if id == 9002 => theme::Role::Brand,
        "STATIC" if matches!(id, 7000..=7003 | 7012 | 7040 | 7050 | 820..=823 | 350..=353 | 114) => {
            theme::Role::Heading
        }
        "msctls_trackbar32" => theme::Role::Slider,
        "STATIC" if id == 120 => theme::Role::Gutter,
        "SCROLLBAR" => theme::Role::Scrollbar,
        "DunYu.Content.1" => theme::Role::Surface,
        "BUTTON" if style & 15 == BS_AUTOCHECKBOX as u32 => theme::Role::Checkbox,
        "EDIT" => theme::Role::Edit,
        "SysTabControl32" => theme::Role::Tab,
        "BUTTON" if style & 15 != BS_AUTOCHECKBOX as u32 => theme::Role::Button,
        _ => theme::Role::Native,
    };
    theme::attach(c, role, u.theme.clone());
    if kind == "EDIT" {
        SendMessageW(
            c,
            EM_SETMARGINS,
            (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
            (8 << 16) | 8,
        );
    }
    u.controls.push((page, c));
    c
}
pub(super) unsafe fn field(u: &mut Ui, page: usize, label: &str, id: i32, y: i32, value: &str) {
    control(u, page, "STATIC", label, 6000 + id, 245, y + 4, 130, 24, 0);
    control(
        u,
        page,
        "EDIT",
        value,
        id,
        380,
        y,
        370,
        26,
        WS_TABSTOP | ES_AUTOHSCROLL as u32,
    );
}
impl App {
    pub(super) unsafe fn open_ui(&mut self) {
        if let Some(window) = self.ui.as_ref().map(|u| u.window) {
            {
                let _redraw = super::buffering::RedrawBatch::new(window, window);
                self.refresh_state();
            }
            ShowWindow(window, SW_SHOW);
            RedrawWindow(
                window,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
            SetForegroundWindow(window);
            return;
        }
        let target = GetForegroundWindow();
        let h = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_COMPOSITED,
            wide("DunYu.Settings.1").as_ptr(),
            wide("遁鱼 DunYu").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            920,
            640,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if h.is_null() {
            error("无法创建设置窗口");
            return;
        }
        let dark: i32 = 1;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(h, 20, &dark as *const _ as _, 4);
        for (attribute, value) in [
            (35, theme::BACKGROUND),
            (36, theme::INK),
            (34, theme::POND_RIPPLE),
        ] {
            let color = (value & 255) << 16 | value & 0xff00 | value >> 16;
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                h,
                attribute,
                &color as *const _ as _,
                4,
            );
        }
        let content = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            wide("DunYu.Content.1").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            0,
            50,
            900,
            500,
            h,
            9000 as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if content.is_null() {
            DestroyWindow(h);
            error("无法创建设置内容区域");
            return;
        }
        let skin = theme::Theme::new();
        theme::attach(h, theme::Role::Surface, skin.clone());
        theme::attach(content, theme::Role::Surface, skin.clone());
        let mut u = Ui {
            window: h,
            theme: skin,
            content,
            page: 0,
            controls: vec![],
            list: null_mut(),
            selected: None,
            target: self
                .track(target)
                .ok()
                .map(|n| self.records[n].token.clone()),
            opacity: null_mut(),
            dpi: 96,
            font: null_mut(),
            heading_font: null_mut(),
            value_font: null_mut(),
            pending_opacity: None,
            advanced: false,
            scroll: 0,
            list_scroll: 0,
            motions: Default::default(),
            split_ratio: std::fs::read(self.dir.join("ui-state.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<f64>(&b).ok())
                .filter(|v| v.is_finite() && (0.1..=0.8).contains(v))
                .unwrap_or(super::design::SPLIT),
            dragging: false,
            baseline: vec![],
        };
        let tab = control(
            &mut u,
            99,
            "SysTabControl32",
            "",
            9,
            20,
            12,
            860,
            32,
            WS_TABSTOP | TCS_FIXEDWIDTH,
        );
        control(&mut u, 99, "STATIC", "遁鱼", 9002, 24, 22, 100, 32, 0);
        SetWindowPos(
            tab,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        for (i, name) in PAGES.iter().enumerate() {
            let mut label = wide(name);
            let mut entry: TCITEMW = std::mem::zeroed();
            entry.mask = TCIF_TEXT;
            entry.pszText = label.as_mut_ptr();
            SendMessageW(tab, TCM_INSERTITEMW, i, &entry as *const _ as isize);
        }
        control(
            &mut u,
            99,
            "STATIC",
            "就绪 · 关闭窗口后继续在托盘运行",
            9001,
            20,
            560,
            620,
            30,
            0,
        );
        let clip = control(
            &mut u,
            0,
            "DunYu.Content.1",
            "",
            119,
            36,
            76,
            180,
            268,
            WS_CLIPCHILDREN,
        );
        SetWindowLongPtrW(clip, GWL_EXSTYLE, WS_EX_CONTROLPARENT as isize);
        control(
            &mut u,
            0,
            "STATIC",
            "",
            120,
            244,
            12,
            16,
            400,
            windows_sys::Win32::System::SystemServices::SS_NOTIFY,
        );
        u.list = control(
            &mut u,
            0,
            "LISTBOX",
            "",
            100,
            20,
            52,
            200,
            310,
            LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | WS_TABSTOP,
        );
        SetParent(u.list, clip);
        control(
            &mut u,
            99,
            "SCROLLBAR",
            "",
            9003,
            0,
            0,
            14,
            100,
            SBS_VERT as u32,
        );
        control(
            &mut u,
            0,
            "SCROLLBAR",
            "",
            9004,
            0,
            0,
            14,
            100,
            SBS_VERT as u32,
        );
        let empty = control(
            &mut u,
            0,
            "STATIC",
            "还没有规则",
            118,
            0,
            0,
            180,
            28,
            windows_sys::Win32::System::SystemServices::SS_CENTER
                | windows_sys::Win32::System::SystemServices::SS_CENTERIMAGE,
        );
        SetParent(empty, clip);
        control(&mut u, 0, "STATIC", "程序召唤", 0, 20, 12, 200, 28, 0);
        control(
            &mut u,
            0,
            "STATIC",
            "暂无规则：选择程序并录入快捷键后保存。",
            114,
            244,
            12,
            620,
            28,
            0,
        );
        control(
            &mut u, 0, "BUTTON", "新建", 110, 20, 378, 95, 32, WS_TABSTOP,
        );
        control(
            &mut u, 0, "BUTTON", "删除", 111, 125, 378, 95, 32, WS_TABSTOP,
        );
        for (id, label, y) in [
            (200, "名称", 52),
            (201, "程序路径", 96),
            (207, "快捷键", 140),
            (202, "启动参数", 280),
            (203, "工作目录", 324),
            (204, "匹配进程路径", 368),
            (205, "窗口类名", 412),
            (206, "标题包含", 456),
        ] {
            field(&mut u, 0, label, id, y, "");
        }
        control(
            &mut u,
            0,
            "BUTTON",
            "浏览…",
            115,
            766,
            96,
            90,
            28,
            WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "启用规则",
            208,
            244,
            184,
            180,
            28,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "高级选项 ▸",
            116,
            244,
            228,
            180,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "最小化代替隐藏",
            209,
            244,
            500,
            260,
            28,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "捕获窗口（3秒）",
            112,
            440,
            228,
            260,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "STATIC",
            "点击捕获后切换到目标窗口。快捷键框直接接收组合键。",
            117,
            244,
            544,
            620,
            28,
            0,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "保存规则",
            113,
            700,
            560,
            180,
            32,
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        );
        control(
            &mut u,
            1,
            "STATIC",
            "窗口绑定 · 仅在当前会话有效",
            0,
            20,
            12,
            800,
            28,
            0,
        );
        for (label, x) in [("槽位", 20), ("绑定快捷键", 130), ("显示／隐藏快捷键", 370)]
        {
            control(&mut u, 1, "STATIC", label, 0, x, 52, 220, 24, 0);
        }
        for i in 0..4 {
            let y = 88 + i * 94;
            control(
                &mut u,
                1,
                "STATIC",
                &format!("槽位 {}", i + 1),
                350 + i,
                20,
                y,
                100,
                28,
                0,
            );
            control(
                &mut u,
                1,
                "STATIC",
                "绑定快捷键",
                360 + i * 2,
                0,
                0,
                200,
                24,
                0,
            );
            control(
                &mut u,
                1,
                "STATIC",
                "显示／隐藏快捷键",
                361 + i * 2,
                0,
                0,
                200,
                24,
                0,
            );
            let slot = &self.config.slots[i as usize];
            for (id, x, key) in [
                (300 + i * 2, 130, &slot.bind),
                (301 + i * 2, 370, &slot.toggle),
            ] {
                control(
                    &mut u,
                    1,
                    "EDIT",
                    &hotkey_text(key),
                    id,
                    x,
                    y,
                    220,
                    30,
                    WS_TABSTOP,
                );
            }
            control(
                &mut u,
                1,
                "BUTTON",
                "3秒后绑定",
                320 + i,
                620,
                y,
                220,
                32,
                WS_TABSTOP,
            );
            control(
                &mut u,
                1,
                "STATIC",
                "尚未绑定",
                340 + i,
                130,
                y + 38,
                710,
                28,
                SS_PATHELLIPSIS,
            );
        }
        control(
            &mut u,
            1,
            "BUTTON",
            "保存快捷键",
            330,
            700,
            560,
            180,
            32,
            WS_TABSTOP,
        );
        control(&mut u, 2, "STATIC", "窗口不透明度", 0, 20, 12, 800, 28, 0);
        control(
            &mut u,
            2,
            "STATIC",
            "尚未选择窗口",
            400,
            20,
            60,
            820,
            30,
            SS_PATHELLIPSIS,
        );
        control(
            &mut u,
            2,
            "BUTTON",
            "捕获窗口（3秒）",
            401,
            20,
            108,
            260,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            2,
            "STATIC",
            "点击后切换到需要调整的窗口",
            0,
            300,
            112,
            500,
            28,
            0,
        );
        control(
            &mut u,
            2,
            "STATIC",
            "当前不透明度：—",
            405,
            20,
            172,
            800,
            32,
            0,
        );
        u.opacity = control(
            &mut u,
            2,
            "msctls_trackbar32",
            "",
            402,
            20,
            220,
            820,
            48,
            WS_TABSTOP,
        );
        SendMessageW(u.opacity, TBM_SETRANGE, 1, (100 << 16) | 10);
        control(
            &mut u,
            2,
            "STATIC",
            "10%（更透明）",
            406,
            20,
            280,
            820,
            28,
            0,
        );
        control(
            &mut u,
            2,
            "STATIC",
            "100%（完全不透明）",
            407,
            620,
            280,
            220,
            28,
            0,
        );
        control(
            &mut u,
            2,
            "BUTTON",
            "恢复 100%",
            403,
            20,
            340,
            300,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            2,
            "BUTTON",
            "恢复全部窗口",
            404,
            20,
            410,
            300,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            2,
            "STATIC",
            "增减快捷键支持长按连续调整；步长可在设置页修改。",
            0,
            20,
            466,
            820,
            28,
            0,
        );
        control(&mut u, 3, "STATIC", "全局快捷键", 0, 244, 12, 600, 28, 0);
        for (id, label, y, value) in [
            (
                500,
                "降低不透明度",
                60,
                hotkey_text(&self.config.opacity_down),
            ),
            (
                501,
                "提高不透明度",
                104,
                hotkey_text(&self.config.opacity_up),
            ),
            (
                502,
                "恢复 100%",
                148,
                hotkey_text(&self.config.opacity_reset),
            ),
            (503, "恢复全部", 192, hotkey_text(&self.config.restore_all)),
            (504, "步长（1～50）", 280, self.config.step.to_string()),
        ] {
            field(&mut u, 3, label, id, y, &value);
        }
        control(&mut u, 3, "STATIC", "调整步长", 0, 244, 240, 600, 28, 0);
        control(
            &mut u,
            3,
            "STATIC",
            "启动与后台运行",
            0,
            244,
            340,
            600,
            28,
            0,
        );
        control(
            &mut u,
            3,
            "BUTTON",
            "登录后自动启动",
            505,
            244,
            384,
            500,
            30,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        check(h, 505, self.config.autostart);
        control(
            &mut u,
            3,
            "STATIC",
            "每次改变的不透明度百分点",
            7051,
            244,
            440,
            610,
            48,
            0,
        );
        control(
            &mut u,
            3,
            "BUTTON",
            "保存设置",
            506,
            700,
            560,
            180,
            32,
            WS_TABSTOP,
        );
        let start = u.controls.len();
        control(
            &mut u,
            5,
            "STATIC",
            "提前30秒预告，小憩与舒展分别设置。",
            811,
            0,
            0,
            0,
            0,
            0,
        );
        control(&mut u, 5, "STATIC", "", 810, 0, 0, 0, 0, 0);
        for (id, label, on) in [
            (
                800,
                "启用小憩",
                self.config.rest.enabled && self.config.rest.short_enabled,
            ),
            (
                817,
                "启用舒展",
                self.config.rest.enabled && self.config.rest.long_enabled,
            ),
            (815, "小憩到点自动开始", self.config.rest.short_auto_start),
            (816, "舒展到点自动开始", self.config.rest.long_auto_start),
            (
                805,
                "休息时全屏显示（所有显示器）",
                self.config.rest.fullscreen,
            ),
            (
                806,
                "离开电脑5分钟后重新计时",
                self.config.rest.natural_breaks,
            ),
            (
                807,
                "全屏应用或演示时暂缓提醒",
                self.config.rest.defer_fullscreen,
            ),
        ] {
            let c = control(
                &mut u,
                5,
                "BUTTON",
                label,
                id,
                0,
                0,
                0,
                0,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            );
            SendMessageW(c, BM_SETCHECK, usize::from(on), 0);
        }
        for (id, label, value) in [
            (801, "小憩间隔（分钟）", self.config.rest.interval_minutes),
            (802, "小憩时长（秒）", self.config.rest.short_seconds),
            (803, "舒展时长（分钟）", self.config.rest.long_minutes),
            (804, "舒展间隔（分钟）", self.config.rest.long_interval()),
        ] {
            control(&mut u, 5, "STATIC", label, id + 6000, 0, 0, 0, 0, 0);
            control(
                &mut u,
                5,
                "EDIT",
                &value.to_string(),
                id,
                0,
                0,
                0,
                0,
                WS_TABSTOP | ES_NUMBER as u32,
            );
        }
        for (id, label) in [
            (808, "保存休憩"),
            (809, "预览提醒"),
            (812, "现在休息"),
            (813, "暂停30分钟"),
            (814, "恢复提醒"),
        ] {
            control(&mut u, 5, "BUTTON", label, id, 0, 0, 0, 0, WS_TABSTOP);
        }
        for (page, _) in &mut u.controls[start..] {
            *page = 3;
        }
        control(&mut u, 3, "STATIC", "设置", 7012, 0, 0, 0, 0, 0);
        for (id, label) in [
            (820, "休憩提醒"),
            (821, "小憩"),
            (822, "舒展"),
            (823, "通用选项"),
        ] {
            control(&mut u, 6, "STATIC", label, id, 0, 0, 0, 0, 0);
            u.controls.last_mut().unwrap().0 = 3;
        }
        self.ui = Some(u);
        self.scale_ui(GetDpiForWindow(h).max(96), None);
        self.refresh_list();
        self.fill_rule(if self.config.rules.is_empty() {
            None
        } else {
            Some(0)
        });
        self.page(0);
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
    }

    pub(super) unsafe fn scale_ui(&mut self, dpi: u32, suggested: Option<RECT>) {
        KillTimer(self.host, super::scroll_ui::TIMER);
        let Some(u) = self.ui.as_mut() else { return };
        u.motions[0].snap(u.scroll as f64);
        u.motions[1].snap(u.list_scroll as f64);
        u.dpi = dpi;
        for (kind, logical) in [(ICON_SMALL, 16), (ICON_BIG, 32)] {
            let size = logical * dpi as i32 / 96;
            let icon = LoadImageW(
                GetModuleHandleW(null()),
                101usize as _,
                IMAGE_ICON,
                size,
                size,
                LR_SHARED,
            );
            SendMessageW(u.window, WM_SETICON, kind as usize, icon as isize);
        }

        let mut font: LOGFONTW = std::mem::zeroed();
        font.lfHeight = -(16 * dpi as i32 / 96);
        font.lfCharSet = DEFAULT_CHARSET;
        let face = wide(super::fonts::FACE);
        font.lfFaceName[..face.len()].copy_from_slice(&face);
        let new_font = CreateFontIndirectW(&font);
        font.lfHeight = -(27 * dpi as i32 / 96);
        font.lfWeight = 600;
        font.lfFaceName = [0; 32];
        let heading_face = wide(super::fonts::FACE);
        font.lfFaceName[..heading_face.len()].copy_from_slice(&heading_face);
        let heading = CreateFontIndirectW(&font);
        if !new_font.is_null() {
            for (_, c) in &u.controls {
                SendMessageW(*c, WM_SETFONT, new_font as usize, 1);
            }
            if !u.font.is_null() {
                DeleteObject(u.font);
            }
            u.font = new_font;
        }
        if !heading.is_null() {
            for (_, c) in &u.controls {
                if matches!(GetDlgCtrlID(*c), 9 | 7000..=7003 | 7012 | 7040 | 7050 | 820..=823 | 350..=353 | 114 | 9002)
                {
                    SendMessageW(*c, WM_SETFONT, heading as usize, 1);
                }
            }
            if !u.heading_font.is_null() {
                DeleteObject(u.heading_font);
            }
            u.heading_font = heading;
        }
        font.lfHeight = -(40 * dpi as i32 / 96);
        let value_font = CreateFontIndirectW(&font);
        if !value_font.is_null() {
            SendMessageW(item(u.window, 405), WM_SETFONT, value_font as usize, 1);
            if !u.value_font.is_null() {
                DeleteObject(u.value_font);
            }
            u.value_font = value_font;
        }
        let bounds = suggested.unwrap_or_else(|| {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(u.window, &mut r);
            let mut size = RECT {
                left: 0,
                top: 0,
                right: 1024 * dpi as i32 / 96,
                bottom: 651 * dpi as i32 / 96,
            };
            AdjustWindowRectExForDpi(&mut size, WS_OVERLAPPEDWINDOW, 0, WS_EX_CONTROLPARENT, dpi);
            RECT {
                left: r.left,
                top: r.top,
                right: r.left + size.right - size.left,
                bottom: r.top + size.bottom - size.top,
            }
        });
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        GetMonitorInfoW(
            MonitorFromWindow(u.window, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        let width = (bounds.right - bounds.left).min(info.rcWork.right - info.rcWork.left);
        let height = (bounds.bottom - bounds.top).min(info.rcWork.bottom - info.rcWork.top);
        let x = bounds
            .left
            .clamp(info.rcWork.left, info.rcWork.right - width);
        let y = bounds
            .top
            .clamp(info.rcWork.top, info.rcWork.bottom - height);
        SetWindowPos(
            u.window,
            null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        self.layout_ui();
    }

    // Scroll the focused control into view after dialog keyboard navigation.
    pub(super) unsafe fn reveal_focus(&mut self) {
        let Some(u) = self.ui.as_mut() else { return };
        let focus = GetFocus();
        if focus == u.list {
            let selected = SendMessageW(u.list, LB_GETCURSEL, 0, 0) as i32;
            if selected >= 0 {
                let mut bounds: RECT = std::mem::zeroed();
                GetClientRect(item(u.window, 119), &mut bounds);
                let height = bounds.bottom * 96 / u.dpi as i32;
                if selected * 64 < u.list_scroll {
                    u.list_scroll = selected * 64;
                }
                if (selected + 1) * 64 > u.list_scroll + height {
                    u.list_scroll = (selected + 1) * 64 - height;
                }
                u.motions[1].snap(u.list_scroll as f64);
                self.layout_ui();
            }
            return;
        }
        if GetParent(focus) != u.content {
            return;
        }
        let mut r: RECT = std::mem::zeroed();
        let mut client: RECT = std::mem::zeroed();
        GetWindowRect(focus, &mut r);
        GetClientRect(u.content, &mut client);
        let mut p = POINT {
            x: r.left,
            y: r.top,
        };
        ScreenToClient(u.content, &mut p);
        let adjustment = if p.y < 0 {
            p.y
        } else {
            (p.y + r.bottom - r.top - client.bottom).max(0)
        };
        if adjustment != 0 {
            u.scroll += adjustment * 96 / u.dpi as i32;
            u.motions[0].snap(u.scroll as f64);
            self.layout_ui();
        }
    }

    pub(super) unsafe fn layout_ui(&mut self) {
        self.layout(false);
    }
    pub(super) unsafe fn layout_scrolled_ui(&mut self) {
        self.layout(true);
    }
    unsafe fn layout(&mut self, scrolling: bool) {
        #[cfg(test)]
        let _measurement = diagnostics::measure("layout");
        let Some(u) = self.ui.as_mut() else { return };
        let _redraw = super::buffering::RedrawBatch::new(
            u.window,
            if scrolling { u.content } else { u.window },
        );
        if !scrolling {
            for (toggle, ids) in [(800, [801, 802, 815]), (817, [804, 803, 816])] {
                for id in ids {
                    let child = item(u.window, id);
                    let enabled = checked(u.window, toggle);
                    if (IsWindowEnabled(child) != 0) != enabled {
                        EnableWindow(child, i32::from(enabled));
                    }
                }
            }
        }
        let scale = |v: i32| v * u.dpi as i32 / 96;
        let rect = |x, y, w, h| RECT {
            left: scale(x),
            top: scale(y),
            right: scale(x + w),
            bottom: scale(y + h),
        };
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(u.window, &mut client);
        let button_width = |id, minimum: i32| {
            let measured = theme::label_size(item(u.window, id));
            minimum.max((measured.cx * 96 + u.dpi as i32 - 1) / u.dpi as i32 + 24)
        };
        let save_width = [113, 330, 506]
            .into_iter()
            .map(|id| button_width(id, 156))
            .max()
            .unwrap();
        let browse_width = button_width(115, 76);
        let slot_extra = (320..=323)
            .map(|id| {
                let height = theme::label_size(item(u.window, id)).cy;
                ((height * 96 + u.dpi as i32 - 1) / u.dpi as i32 + 12 - 40).max(0)
            })
            .max()
            .unwrap_or(0);

        let width = client.right * 96 / u.dpi as i32;
        let height = client.bottom * 96 / u.dpi as i32;
        let columns = if width >= 960 { 2 } else { 1 };
        let action_width = [808, 809, 812, 813, 814]
            .into_iter()
            .map(|id| button_width(id, 148))
            .max()
            .unwrap();
        let action_columns = ((width - 104 + 12) / (action_width + 12)).clamp(1, 5);
        let action_rows = (5 + action_columns - 1) / action_columns;
        let mut extent = match u.page {
            0 => {
                if u.advanced {
                    900
                } else {
                    440
                }
            }
            1 => 64 + (4 / columns) * (192 + slot_extra),
            2 => 520,
            3 => {
                if columns == 2 {
                    490 + action_rows * 56 + 12
                } else {
                    738 + action_rows * 56 + 12
                }
            }
            4 => 0,
            _ => {
                if columns == 2 {
                    390
                } else {
                    660
                }
            }
        };
        let design = super::design::Canvas {
            width,
            height,
            split: u.split_ratio,
        };
        let reference = super::design::reference_layout(columns, u.page, u.advanced);
        // The content host must stop above every fixed footer control. The save
        // button can grow with font metrics, so reserve its painted bounds too.
        let mut footer_top = if reference {
            scale(design.control(9001).unwrap().1)
        } else {
            scale(height - 38)
        };
        let save = match u.page {
            0 => Some(113),
            1 => Some(330),
            5 => Some(506),
            _ => None,
        };
        if let Some(id) = save {
            let (_, y, _, h) = if reference {
                design.control(id).unwrap()
            } else {
                (0, height - 54, 0, 44)
            };
            let outer_height = scale(h);
            let button_height = outer_height
                .max(theme::label_size(item(u.window, id)).cy + scale(12))
                .max(scale(28));
            footer_top = footer_top.min(scale(y) - (button_height - outer_height) / 2);
        }
        let viewport =
            super::design::content_height(height, u.page, footer_top * 96 / u.dpi as i32);
        if reference {
            extent = if u.page == 0 && u.advanced {
                900
            } else if u.page == 3 {
                514
            } else {
                480
            };
        }
        u.scroll = u.scroll.clamp(0, (extent - viewport).max(0));
        // Suppress intermediate paints; child geometry and paint snapshots form one frame.

        if !scrolling {
            MoveWindow(u.content, 0, scale(76), client.right, scale(viewport), 0);
        }
        let si = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: extent - 1,
            nPage: viewport as u32,
            nPos: u.scroll,
            nTrackPos: 0,
        };
        SetScrollInfo(item(u.window, 9003), SB_CTL, &si, 0);
        let mut content: RECT = std::mem::zeroed();
        GetClientRect(u.content, &mut content);
        let bw = content.right * 96 / u.dpi as i32;
        let right = bw - 24;
        let left_width = ((bw - 64) as f64 * u.split_ratio)
            .clamp(240., (bw - 64 - 420).clamp(240, 480) as f64) as i32;
        let form = 24 + left_width + 16 + 20;
        let fw = (right - form - 20).max(160);
        let card_width = (bw - 48 - 16 * (columns - 1)) / columns;
        let slot = |i: i32| {
            (
                24 + (i % columns) * (card_width + 16),
                64 + (i / columns) * (192 + slot_extra),
            )
        };
        let mut cards = vec![];
        match u.page {
            0 => {
                cards.push(rect(24, 12, left_width, (viewport - 24).max(180)));
                cards.push(rect(
                    24 + left_width + 16,
                    12 - u.scroll,
                    bw - left_width - 64,
                    (if u.advanced { 864 } else { 404 }).max(viewport - 24),
                ));
            }
            1 => {
                for i in 0..4 {
                    let (x, y) = slot(i);
                    cards.push(rect(x, y - u.scroll, card_width, 176 + slot_extra));
                }
            }
            2 => {
                cards.push(rect(24, 64 - u.scroll, bw - 48, 124));
                cards.push(rect(24, 204 - u.scroll, bw - 48, 204));
                cards.push(rect(24, 424 - u.scroll, bw - 48, 72));
            }
            3 => {
                let group_width = (bw - 88 - 16 * (columns - 1)) / columns;
                let common_y = if columns == 2 { 318 } else { 566 };
                cards.push(rect(24, 12 - u.scroll, bw - 48, extent - 24));
                for i in 0..2 {
                    cards.push(rect(
                        44 + (i % columns) * (group_width + 16),
                        112 + (i / columns) * 248 - u.scroll,
                        group_width,
                        232,
                    ));
                }
                cards.push(rect(44, common_y + 42 - u.scroll, bw - 88, 128));
            }
            4 => {}
            _ => {
                if columns == 2 {
                    let left = (bw - 64) * 3 / 5;
                    cards.push(rect(24, 64 - u.scroll, left, 296));
                    cards.push(rect(40 + left, 64 - u.scroll, bw - left - 64, 156));
                    cards.push(rect(40 + left, 236 - u.scroll, bw - left - 64, 124));
                } else {
                    cards.push(rect(24, 64 - u.scroll, bw - 48, 296));
                    cards.push(rect(24, 376 - u.scroll, bw - 48, 140));
                    cards.push(rect(24, 532 - u.scroll, bw - 48, 116));
                }
            }
        }
        if reference {
            cards = design
                .cards(u.page)
                .into_iter()
                .enumerate()
                .map(|(index, (x, y, w, h))| {
                    let h = if u.page == 0 && index == 1 && u.advanced {
                        (extent - y - 16).max(h)
                    } else {
                        h
                    };
                    rect(
                        x,
                        if u.page == 0 && index == 0 {
                            y
                        } else {
                            y - u.scroll
                        },
                        w,
                        h,
                    )
                })
                .collect();
        }
        let list_area = if reference {
            design.control(119).unwrap()
        } else {
            (36, 76, left_width - 40, (viewport - 160).max(64))
        };
        let list_height = list_area.3;
        let count = SendMessageW(u.list, LB_GETCOUNT, 0, 0).max(0) as i32;
        u.list_scroll = u.list_scroll.clamp(0, (count * 64 - list_height).max(0));
        let list_info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: (count * 64 - 1).max(0),
            nPage: list_height as u32,
            nPos: u.list_scroll,
            nTrackPos: 0,
        };
        SetScrollInfo(item(u.window, 9004), SB_CTL, &list_info, 0);
        SendMessageW(u.list, LB_SETTOPINDEX, (u.list_scroll / 64) as usize, 0);
        let mut fields = vec![];
        let mut placements = vec![];
        for (page, c) in &u.controls {
            let id = GetDlgCtrlID(*c);
            let root = *page == 99 || matches!(id, 113 | 330 | 506);
            if scrolling && root {
                continue;
            }
            let area: Option<(i32, i32, i32, i32)> = match id {
                9 => Some((206, 14, width - 230, 52)),
                9002 => Some((24, 10, 174, 56)),
                9003 if extent > viewport => Some((width - 16, 76, 14, viewport)),
                9003 => None,
                9004 if count * 64 > list_height => {
                    Some((24 + left_width - 24, 76, 12, list_height))
                }
                9004 => None,
                119 => Some((36, 76, left_width - 40, (viewport - 160).max(64))),
                120 => Some((24 + left_width, 12, 16, (viewport - 24).max(180))),
                9001 => Some((24, height - 38, width - save_width - 64, 30)),
                113 | 330 | 506 => Some((width - save_width - 24, height - 54, save_width, 44)),
                7000 => Some((44, 32, left_width - 40, 28)),
                100 => Some((
                    0,
                    -(u.list_scroll % 64),
                    left_width - 40,
                    (viewport - 160).max(64) + u.list_scroll % 64,
                )),
                110 => Some((40, viewport - 68, (left_width - 40) / 2 - 4, 36)),
                111 => Some((
                    40 + (left_width - 40) / 2 + 4,
                    viewport - 68,
                    (left_width - 40) / 2 - 4,
                    36,
                )),
                118 if self.config.rules.is_empty() => Some((
                    44,
                    76 + (viewport - 160).max(64) / 2 + 28,
                    left_width - 40,
                    28,
                )),
                114 => Some((form, 32, fw, 28)),
                112 => Some((form, 76, 220, 36)),
                200 | 201 | 207 | 6200 | 6201 | 6207 => {
                    let field = if id >= 6000 { id - 6000 } else { id };
                    let row = match field {
                        200 => 0,
                        201 => 1,
                        _ => 2,
                    };
                    let y = 132 + row * 78;
                    let w = if field == 201 {
                        fw - browse_width - 8
                    } else if field == 207 {
                        fw - 130
                    } else {
                        fw
                    };
                    Some((
                        form,
                        if id >= 6000 { y } else { y + 26 },
                        w,
                        if id >= 6000 { 22 } else { 36 },
                    ))
                }
                115 => Some((right - 20 - browse_width, 236, browse_width, 40)),
                208 => Some((right - 140, 314, 120, 30)),
                116 => Some((form, 372, fw, 36)),
                202..=206 | 6202..=6206 if u.advanced => {
                    let field = if id >= 6000 { id - 6000 } else { id };
                    let y = 432 + (field - 202) * 76;
                    Some((
                        form,
                        if id >= 6000 { y } else { y + 26 },
                        fw,
                        if id >= 6000 { 22 } else { 36 },
                    ))
                }
                209 if u.advanced => Some((form, 820, fw, 30)),
                117 => None,
                7001 => Some((24, 12, bw - 48, 32)),
                350..=353 => {
                    let (x, y) = slot(id - 350);
                    Some((
                        x + 20,
                        y + 16,
                        card_width - button_width(320 + id - 350, 112) - 44,
                        28,
                    ))
                }
                340..=343 => {
                    let (x, y) = slot(id - 340);
                    Some((x + 20, y + 54 + slot_extra, card_width - 40, 24))
                }
                320..=323 => {
                    let (x, y) = slot(id - 320);
                    let w = button_width(id, 112);
                    Some((x + card_width - w - 20, y + 14, w, 40))
                }
                300..=307 | 360..=367 => {
                    let base = if id >= 360 { 360 } else { 300 };
                    let (x, y) = slot((id - base) / 2);
                    let second = (id - base) % 2;
                    let field_width = (card_width - 56) / 2;
                    Some((
                        x + 20 + second * (field_width + 16),
                        y + 92 + slot_extra + if base == 300 { 24 } else { 0 },
                        field_width,
                        if base == 300 { 36 } else { 22 },
                    ))
                }
                7002 => Some((24, 12, bw - 48, 32)),
                400 => Some((44, 84, bw - 88, 28)),
                401 => Some((44, 132, 220, 36)),
                7010 => Some((280, 136, bw - 324, 24)),
                405 => Some((44, 228, bw - 88, 48)),
                402 => Some((44, 284, bw - 88, 40)),
                406 => Some((44, 328, 180, 24)),
                407 => Some((bw - 224, 328, 180, 24)),
                403 => Some((44, 360, 220, 36)),
                404 => Some((44, 442, 240, 36)),
                7011 => None,
                7012 => Some((24, 12, bw - 48, 32)),
                7003 => Some((
                    44,
                    80,
                    if columns == 2 {
                        (bw - 64) * 3 / 5 - 40
                    } else {
                        bw - 88
                    },
                    32,
                )),
                500..=503 | 6500..=6503 => {
                    let field = if id >= 6000 { id - 6000 } else { id };
                    let left = if columns == 2 {
                        (bw - 64) * 3 / 5
                    } else {
                        bw - 48
                    };
                    Some((
                        if id >= 6000 { 44 } else { 184 },
                        138 + (field - 500) * 50,
                        if id >= 6000 { 132 } else { left - 180 },
                        36,
                    ))
                }
                7040 | 6504 | 504 | 7050 | 505 => {
                    let x = if columns == 2 {
                        60 + (bw - 64) * 3 / 5
                    } else {
                        44
                    };
                    let w = bw - x - 44;
                    let y = if columns == 2 { 80 } else { 392 };
                    match id {
                        7040 => Some((x, y, w, 32)),
                        6504 => Some((x, y + 48, w, 24)),
                        504 => Some((x, y + 80, w.min(160), 36)),
                        7050 => Some((x, if columns == 2 { 252 } else { 548 }, w, 32)),
                        _ => Some((x, if columns == 2 { 300 } else { 594 }, w, 32)),
                    }
                }
                820 => Some((44, 24, bw - 88, 32)),
                811 => Some((44, 62, bw - 88, 24)),
                810 => Some((44, 86, bw - 88, 24)),
                800..=804 | 815..=817 | 6801..=6804 | 821..=822 => {
                    let long = matches!(id, 803 | 804 | 816 | 817 | 6803 | 6804 | 822);
                    let group = i32::from(long);
                    let gw = (bw - 88 - 16 * (columns - 1)) / columns;
                    let x = 60 + (group % columns) * (gw + 16);
                    let y = 124 + (group / columns) * 248;
                    let w = gw - 32;
                    match id {
                        821 | 822 => Some((x, y, w, 32)),
                        800 | 817 => Some((x, y + 40, w, 28)),
                        815 | 816 => Some((x, y + 178, w, 30)),
                        _ => {
                            let field = if id >= 6000 { id - 6000 } else { id };
                            let row = i32::from(matches!(field, 802 | 803));
                            Some((
                                if id >= 6000 { x } else { x + 164 },
                                y + 78 + row * 46,
                                if id >= 6000 { 156 } else { w - 164 },
                                34,
                            ))
                        }
                    }
                }
                823 => Some((60, if columns == 2 { 366 } else { 614 }, bw - 120, 28)),
                805..=807 => Some((
                    60,
                    (if columns == 2 { 400 } else { 648 }) + (id - 805) * 28,
                    bw - 120,
                    28,
                )),
                808 | 809 | 812..=814 => {
                    let i = match id {
                        812 => 0,
                        813 => 1,
                        814 => 2,
                        809 => 3,
                        _ => 4,
                    };
                    let lanes = action_columns;
                    let w = (bw - 88 - (lanes - 1) * 12) / lanes;
                    Some((
                        44 + (i % lanes) * (w + 12),
                        (if columns == 2 { 490 } else { 738 }) + (i / lanes) * 56,
                        w,
                        40,
                    ))
                }
                7051 => None,
                _ => None,
            };
            let area =
                if !super::design::state_allows(id, !self.config.rules.is_empty(), u.advanced) {
                    None
                } else if reference {
                    design
                        .control(id)
                        .map(|mut r| {
                            if id == 100 {
                                r.1 = -(u.list_scroll % 64);
                                r.3 += u.list_scroll % 64;
                            }
                            r
                        })
                        .or(area)
                } else {
                    area
                };
            let visible = (*page == 99 || *page == u.page) && area.is_some();
            if !visible && GetWindowLongPtrW(*c, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                placements.push((
                    *c,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                ));
            }
            if let Some((x, y, w, h)) = area {
                let root = *page == 99 || matches!(id, 113 | 330 | 506);
                let mut r = rect(
                    x,
                    if root
                        || (u.page == 0
                            && matches!(id, 100 | 110 | 111 | 118 | 119 | 120 | 7000 | 9004))
                    {
                        y
                    } else {
                        y - u.scroll
                    },
                    w,
                    h,
                );
                if id == 118 {
                    let (_, (x, y, w, h)) = super::design::empty_state(
                        scale(list_area.2),
                        scale(list_area.3),
                        theme::font_line_height(*c),
                        u.dpi,
                    );
                    r = RECT {
                        left: x,
                        top: y,
                        right: x + w,
                        bottom: y + h,
                    };
                }
                if class(*c).eq_ignore_ascii_case("button")
                    && GetWindowLongPtrW(*c, GWL_STYLE) as u32 & 15 != BS_AUTOCHECKBOX as u32
                {
                    let measured = theme::label_size(*c);
                    r.right = r.right.max(r.left + measured.cx + scale(24));
                    // Add padding around the font's full line height, including descenders.
                    let desired_height = (r.bottom - r.top)
                        .max(measured.cy + scale(12))
                        .max(scale(28));
                    let extra = desired_height - (r.bottom - r.top);
                    r.top -= extra / 2;
                    r.bottom = r.top + desired_height;
                }
                if class(*c) == "Edit" || class(*c) == "EDIT" {
                    if visible {
                        fields.push((*c, r));
                    }
                    r.left += scale(4);
                    r.right -= scale(4);
                    let (top, bottom) = super::design::edit_inset(
                        r.bottom - r.top,
                        theme::font_line_height(*c),
                        u.dpi,
                    );
                    r.top += top;
                    r.bottom -= bottom;
                }
                if visible {
                    let mut current: RECT = std::mem::zeroed();
                    GetWindowRect(*c, &mut current);
                    MapWindowPoints(
                        null_mut(),
                        GetParent(*c),
                        &mut current as *mut _ as *mut POINT,
                        2,
                    );
                    if GetWindowLongPtrW(*c, GWL_STYLE) as u32 & WS_VISIBLE == 0
                        || (current.left, current.top, current.right, current.bottom)
                            != (r.left, r.top, r.right, r.bottom)
                    {
                        placements.push((*c, r));
                    }
                }
                theme::scene(*c, u.dpi, vec![], vec![]);
            }
        }
        if !scrolling {
            set_text(
                u.window,
                116,
                if u.advanced {
                    "高级选项  −"
                } else {
                    "高级选项  +"
                },
            );
            SendMessageW(u.list, LB_SETITEMHEIGHT, 0, scale(64) as isize);
        }
        theme::sync_list_scroll(u.list);
        theme::scene(u.content, u.dpi, cards, fields);
        if !scrolling {
            theme::scene(u.window, u.dpi, vec![], vec![]);
        }
        for parent in [u.window, u.content, item(u.window, 119)] {
            let group: Vec<_> = placements
                .iter()
                .filter(|(h, _)| GetParent(*h) == parent)
                .collect();
            if group.is_empty() {
                continue;
            }
            let flags =
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW | SWP_NOCOPYBITS | SWP_SHOWWINDOW;
            let mut batch = BeginDeferWindowPos(group.len() as i32);
            for (child, r) in &group {
                if batch.is_null() {
                    break;
                }
                batch = DeferWindowPos(
                    batch,
                    *child,
                    null_mut(),
                    r.left,
                    r.top,
                    (r.right - r.left).max(1),
                    (r.bottom - r.top).max(1),
                    if r.right == 0 && r.bottom == 0 {
                        (flags & !SWP_SHOWWINDOW) | SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE
                    } else {
                        flags
                    },
                );
            }
            if batch.is_null() || EndDeferWindowPos(batch) == 0 {
                for (child, r) in group {
                    SetWindowPos(
                        *child,
                        null_mut(),
                        r.left,
                        r.top,
                        (r.right - r.left).max(1),
                        (r.bottom - r.top).max(1),
                        if r.right == 0 && r.bottom == 0 {
                            (flags & !SWP_SHOWWINDOW) | SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE
                        } else {
                            flags
                        },
                    );
                }
            }
        }
        // Size items after positioning the host: its width differs from the main window.
        if !scrolling {
            let tab = item(u.window, 9);
            let mut tab_rect: RECT = std::mem::zeroed();
            GetClientRect(tab, &mut tab_rect);
            let item_width =
                super::design::navigation_item_width(tab_rect.right, u.dpi, PAGES.len() as i32);
            SendMessageW(tab, TCM_SETMINTABWIDTH, 0, 0);
            SendMessageW(
                tab,
                TCM_SETITEMSIZE,
                0,
                ((scale(51) as isize) << 16) | item_width as isize,
            );
        }
        SendMessageW(u.list, LB_SETTOPINDEX, (u.list_scroll / 64) as usize, 0);
        theme::sync_list_scroll(u.list);

        if scrolling {
            for id in [9003, 9004] {
                InvalidateRect(item(u.window, id), null(), 0);
            }
        }
    }

    pub(super) unsafe fn status(&self, message: &str) {
        self.status_color(message, theme::MUTED);
    }
    pub(super) unsafe fn status_error(&self, message: &str) {
        self.status_color(message, theme::DANGER);
    }
    unsafe fn status_success(&self, message: &str) {
        self.status_color(message, theme::SUCCESS);
    }
    unsafe fn status_color(&self, message: &str, value: u32) {
        if let Some(u) = &self.ui {
            set_text(u.window, 9001, message);
            theme::tint(item(u.window, 9001), value);
        }
    }

    unsafe fn rule_snapshot(&self) -> Vec<String> {
        let h = self.ui.as_ref().unwrap().window;
        let mut values = (200..=207).map(|id| text_at(h, id)).collect::<Vec<_>>();
        values.push(checked(h, 208).to_string());
        values.push(checked(h, 209).to_string());
        values
    }

    pub(super) unsafe fn leave_rule(&mut self) -> bool {
        if self.rule_snapshot() == self.ui.as_ref().unwrap().baseline {
            return true;
        }
        let h = self.ui.as_ref().unwrap().window;
        match MessageBoxW(
            h,
            wide("当前规则尚未保存。是否保存？\n选择“否”放弃修改，选择“取消”继续编辑。").as_ptr(),
            wide("未保存的修改").as_ptr(),
            MB_YESNOCANCEL | MB_ICONQUESTION,
        ) {
            IDYES => match self.ui_action(113) {
                Ok(()) => true,
                Err(e) => {
                    self.status_error(&e);
                    false
                }
            },
            IDNO => true,
            _ => false,
        }
    }

    pub(super) unsafe fn cancel_opacity(&mut self) {
        KillTimer(self.host, 900);
        if let Some(u) = self.ui.as_mut() {
            u.pending_opacity = None;
        }
    }

    pub(super) unsafe fn refresh_state(&mut self) {
        let Some(u) = &self.ui else { return };
        for i in 0..4 {
            let label = self
                .slots
                .get(i)
                .and_then(|t| t.as_ref())
                .filter(|t| t.valid())
                .map(|t| format!("已绑定 · {}", title(handle(t.hwnd))))
                .unwrap_or("尚未绑定".into());
            set_text(u.window, 340 + i as i32, &label);
        }
        let target = u
            .target
            .as_ref()
            .filter(|t| t.valid())
            .map(|t| handle(t.hwnd));
        let value = target.and_then(|h| {
            let (mut color, mut alpha, mut flags) = (0, 255, 0);
            if GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0
                && GetLayeredWindowAttributes(h, &mut color, &mut alpha, &mut flags) == 0
            {
                return None;
            }
            Some(if flags & LWA_ALPHA == 0 {
                100
            } else {
                (alpha as u32 * 100 + 127) / 255
            })
        });
        set_text(
            u.window,
            400,
            &target
                .map(|h| title(h))
                .unwrap_or("目标窗口不可用，请重新捕获".into()),
        );
        for id in [402, 403] {
            let child = item(u.window, id);
            if (IsWindowEnabled(child) != 0) != value.is_some() {
                EnableWindow(child, i32::from(value.is_some()));
            }
        }
        if value.is_some() && u.pending_opacity.is_some() {
            return;
        }
        if let Some(percent) = value {
            set_text(u.window, 405, &format!("{percent}%"));
            SendMessageW(u.opacity, TBM_SETPOS, 1, percent.clamp(10, 100) as isize);
        } else {
            set_text(u.window, 405, "不可用");
            self.cancel_opacity();
        }
    }

    pub(super) unsafe fn page(&mut self, page: usize) {
        let Some(window) = self.ui.as_ref().map(|u| u.window) else {
            return;
        };
        let _redraw = super::buffering::RedrawBatch::new(window, window);
        super::recorder::stop();
        if let Some(u) = &mut self.ui {
            u.page = page;
            u.scroll = 0;
            u.motions[0].snap(0.);
            u.motions[1].snap(u.list_scroll as f64);
            SendMessageW(item(u.window, 9), TCM_SETCURSEL, page, 0);
        }
        if let Some(u) = &self.ui {
            set_text(u.window, 810, &self.rest.status());
        }
        self.refresh_state();
        self.layout_ui();
    }
    pub(super) unsafe fn refresh_list(&mut self) {
        if let Some(u) = &self.ui {
            SendMessageW(u.list, LB_RESETCONTENT, 0, 0);
            for r in &self.config.rules {
                SendMessageW(
                    u.list,
                    LB_ADDSTRING,
                    0,
                    wide(&format!(
                        "{}{}\n{}",
                        if r.enabled { "" } else { "[停用] " },
                        r.name,
                        hotkey_text(&r.hotkey)
                    ))
                    .as_ptr() as isize,
                );
            }
            SendMessageW(u.list, LB_SETCURSEL, u.selected.unwrap_or(usize::MAX), 0);
        }
    }
    pub(super) unsafe fn fill_rule(&mut self, index: Option<usize>) {
        let window = self.ui.as_ref().unwrap().window;
        let _redraw = super::buffering::RedrawBatch::new(window, window);
        let u = self.ui.as_mut().unwrap();
        u.selected = index;
        let default = SummonRule {
            name: String::new(),
            enabled: true,
            executable: String::new(),
            arguments: String::new(),
            working_directory: String::new(),
            process_path: String::new(),
            class: String::new(),
            title: String::new(),
            hotkey: Hotkey::new(3, 65),
            minimize: false,
        };
        let r = index.map(|i| &self.config.rules[i]).unwrap_or(&default);
        for (id, s) in [
            (200, r.name.clone()),
            (201, r.executable.clone()),
            (202, r.arguments.clone()),
            (203, r.working_directory.clone()),
            (204, r.process_path.clone()),
            (205, r.class.clone()),
            (206, r.title.clone()),
            (207, hotkey_text(&r.hotkey)),
        ] {
            set_text(u.window, id, &s);
        }
        check(u.window, 208, r.enabled);
        check(u.window, 209, r.minimize);
        u.advanced = !r.arguments.is_empty()
            || !r.working_directory.is_empty()
            || !r.class.is_empty()
            || !r.title.is_empty()
            || r.minimize
            || r.executable.to_lowercase().ends_with(".lnk")
            || (!r.process_path.is_empty() && r.process_path != r.executable);
        SendMessageW(u.list, LB_SETCURSEL, index.unwrap_or(usize::MAX), 0);
        EnableWindow(item(u.window, 111), i32::from(index.is_some()));
        set_text(
            u.window,
            114,
            if index.is_some() {
                "编辑召唤规则"
            } else {
                "新建召唤规则"
            },
        );
        let snapshot = self.rule_snapshot();
        self.ui.as_mut().unwrap().baseline = snapshot;
        self.layout_ui();
    }
    pub(super) unsafe fn apply_config(&mut self, c: AppConfig) -> Result<(), String> {
        if !self.pending.is_empty() {
            return Err("程序正在启动，请稍后保存".into());
        }
        self.register(&c, true)?;
        self.config = c;
        self.slots.resize(self.config.slots.len(), None);
        self.targets.clear();
        self.refresh_list();
        Ok(())
    }
    pub(super) unsafe fn ui_action(&mut self, id: i32) -> Result<(), String> {
        let h = self.ui.as_ref().unwrap().window;
        match id {
            800 | 817 => {
                self.layout_ui();
            }
            808 => {
                let value = |id| {
                    text_at(h, id)
                        .parse::<u32>()
                        .map_err(|_| "请填写正整数".to_string())
                };
                let rest = dunyu::rest::RestConfig {
                    enabled: checked(h, 800) || checked(h, 817),
                    short_enabled: checked(h, 800),
                    long_enabled: checked(h, 817),
                    long_interval_minutes: Some(value(804)?),
                    interval_minutes: value(801)?,
                    short_seconds: value(802)?,
                    long_minutes: value(803)?,
                    long_every: self.config.rest.long_every,
                    fullscreen: checked(h, 805),
                    short_auto_start: checked(h, 815),
                    long_auto_start: checked(h, 816),
                    natural_breaks: checked(h, 806),
                    defer_fullscreen: checked(h, 807),
                };
                rest.validate()?;
                let mut next = self.config.clone();
                next.rest = rest.clone();
                atomic_json(&self.dir.join("config.json"), &next).map_err(|e| e.to_string())?;
                self.config = next;
                self.rest.configure(rest, self.host);
                self.status_success("休憩设置已保存");
            }
            809 | 812..=814 => {
                self.rest.action(
                    match id {
                        809 => 23,
                        812 => 1,
                        813 => 20,
                        _ => 22,
                    },
                    None,
                    self.host,
                );
            }
            110 => {
                if self.leave_rule() {
                    self.fill_rule(None);
                }
            }
            116 => {
                let u = self.ui.as_mut().unwrap();
                u.advanced = !u.advanced;
                self.layout_ui();
            }
            115 => {
                let mut buffer = vec![0u16; 32768];
                let filter = wide("程序或快捷方式\0*.exe;*.lnk\0所有文件\0*.*\0");
                let mut dialog: OPENFILENAMEW = std::mem::zeroed();
                dialog.lStructSize = std::mem::size_of_val(&dialog) as u32;
                dialog.hwndOwner = h;
                dialog.lpstrFilter = filter.as_ptr();
                dialog.lpstrFile = buffer.as_mut_ptr();
                dialog.nMaxFile = buffer.len() as u32;
                dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
                if GetOpenFileNameW(&mut dialog) != 0 {
                    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
                    let path = String::from_utf16_lossy(&buffer[..end]);
                    set_text(h, 201, &path);
                    if path.to_lowercase().ends_with(".exe") && text_at(h, 204).is_empty() {
                        set_text(h, 204, &path);
                    }
                    if path.to_lowercase().ends_with(".lnk") {
                        self.ui.as_mut().unwrap().advanced = true;
                        self.status("快捷方式需要填写目标程序的实际进程路径");
                    }
                    self.layout_ui();
                } else {
                    let code = CommDlgExtendedError();
                    if code != 0 {
                        return Err(format!("无法选择程序文件：{code}"));
                    }
                }
            }
            111 => {
                if let Some(i) = self.ui.as_ref().unwrap().selected {
                    if MessageBoxW(
                        h,
                        wide("删除当前规则？该规则的未保存修改也会丢弃。").as_ptr(),
                        wide("删除规则").as_ptr(),
                        MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
                    ) != IDYES
                    {
                        return Ok(());
                    }
                    let mut c = self.config.clone();
                    c.rules.remove(i);
                    self.apply_config(c)?;
                    self.fill_rule(None);
                }
            }
            112 | 401 | 320..=323 => {
                self.cancel_opacity();
                self.status("捕获完成后请核对目标与填写的信息");
                ShowWindow(h, SW_HIDE);
                SetTimer(self.host, id as usize, 3000, None);
            }
            113 => {
                let exe = text_at(h, 201);
                let path = text_at(h, 204);
                if path.trim().is_empty() && !exe.to_lowercase().ends_with(".exe") {
                    self.ui.as_mut().unwrap().advanced = true;
                    self.layout_ui();
                    SetFocus(item(h, 204));
                    self.reveal_focus();
                    return Err("请填写实际进程路径；快捷方式不能用于匹配进程".into());
                }
                let r = SummonRule {
                    name: text_at(h, 200),
                    enabled: checked(h, 208),
                    executable: exe.clone(),
                    arguments: text_at(h, 202),
                    working_directory: text_at(h, 203),
                    process_path: if path.is_empty() && exe.to_lowercase().ends_with(".exe") {
                        exe
                    } else {
                        path
                    },
                    class: text_at(h, 205),
                    title: text_at(h, 206),
                    hotkey: parse_key(&text_at(h, 207))?,
                    minimize: checked(h, 209),
                };
                let mut c = self.config.clone();
                if let Some(i) = self.ui.as_ref().unwrap().selected {
                    c.rules[i] = r;
                } else {
                    c.rules.push(r);
                }
                let selected = self
                    .ui
                    .as_ref()
                    .unwrap()
                    .selected
                    .unwrap_or(c.rules.len() - 1);
                self.apply_config(c)?;
                self.fill_rule(Some(selected));
                self.status_success("规则已保存");
            }
            330 => {
                let mut c = self.config.clone();
                c.slots = (0..4)
                    .map(|i| {
                        Ok(Slot {
                            bind: parse_key(&text_at(h, 300 + i * 2))?,
                            toggle: parse_key(&text_at(h, 301 + i * 2))?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                self.apply_config(c)?;
            }
            403 => {
                self.cancel_opacity();
                let target = self
                    .ui
                    .as_ref()
                    .unwrap()
                    .target
                    .as_ref()
                    .filter(|t| t.valid())
                    .map(|t| handle(t.hwnd))
                    .ok_or("目标窗口已关闭，请重新捕获")?;
                self.opacity(target, Some(255), 0)?;
                SendMessageW(self.ui.as_ref().unwrap().opacity, WM_USER + 5, 1, 100);
            }
            404 => {
                self.cancel_opacity();
                self.restore_all();
            }
            506 => {
                let mut c = self.config.clone();
                c.opacity_down = parse_key(&text_at(h, 500))?;
                c.opacity_up = parse_key(&text_at(h, 501))?;
                c.opacity_reset = parse_key(&text_at(h, 502))?;
                c.restore_all = parse_key(&text_at(h, 503))?;
                c.step = text_at(h, 504).parse().map_err(|_| "步长应为整数")?;
                c.autostart = checked(h, 505);
                let old = self.config.clone();
                self.apply_config(c.clone())?;
                if let Err(e) = autostart(c.autostart) {
                    let _ = self.apply_config(old);
                    return Err(e);
                }
            }
            _ => {}
        }
        if (808..=814).contains(&id) {
            set_text(h, 810, &self.rest.status());
        }
        if matches!(id, 330 | 506) {
            self.status_success("修改已保存");
        }
        if matches!(id, 403 | 404) {
            self.refresh_state();
        }
        Ok(())
    }
    pub(super) unsafe fn capture(&mut self, id: usize) {
        KillTimer(self.host, id);
        self.cancel_opacity();
        let target = GetForegroundWindow();
        let Some(u) = self.ui.as_ref() else {
            return;
        };
        let h = u.window;
        if !eligible(target) {
            self.status_error("未选中可操作的窗口，请重新捕获");
        } else if id == 112 {
            if let Some((_, _, p)) = process(target) {
                self.status_success("窗口信息已捕获，请核对后保存");
                set_text(h, 204, &p);
                if text_at(h, 201).is_empty() {
                    set_text(h, 201, &p);
                }
                set_text(h, 205, &class(target));
                set_text(h, 206, &title(target));
                if text_at(h, 200).is_empty() {
                    set_text(h, 200, &title(target));
                }
            } else {
                self.status_error("无法读取目标进程，请检查权限并重新捕获");
            }
        } else if id == 401 {
            match self.track(target) {
                Ok(n) => {
                    self.ui.as_mut().unwrap().target = Some(self.records[n].token.clone());
                    self.status_success("目标窗口已捕获");
                }
                Err(e) => {
                    error(&e);
                    self.ui.as_mut().unwrap().target = None;
                }
            }
            set_text(h, 400, &title(target));
        } else {
            let i = id - 320;
            if let Some(old) = self.slots[i].clone() {
                if let Some(n) = self.records.iter().position(|r| r.token.tag == old.tag) {
                    if !self.restore(n) {
                        error("旧窗口尚未恢复，请稍后重试");
                        ShowWindow(h, SW_SHOW);
                        return;
                    }
                }
            }
            match self.track(target) {
                Ok(n) => {
                    self.slots[i] = Some(self.records[n].token.clone());
                    self.status_success("窗口已绑定到槽位");
                    if let Err(e) = self.journal() {
                        error(&e);
                    }
                }
                Err(e) => error(&e),
            }
        }
        if id == 112 {
            self.ui.as_mut().unwrap().advanced = true;
        }
        let page = self.ui.as_ref().unwrap().page;
        self.page(page);
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
    }
}
pub(super) unsafe fn autostart(enabled: bool) -> Result<(), String> {
    let mut key = null_mut();
    let result = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
        0,
        null(),
        0,
        KEY_SET_VALUE,
        null(),
        &mut key,
        null_mut(),
    );
    if result != 0 {
        return Err(format!("无法修改启动项：{result}"));
    }
    let name = wide("DunYu");
    let status = if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = wide(&format!("\"{}\" --background", exe.display()));
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * 2) as u32,
        )
    } else {
        RegDeleteValueW(key, name.as_ptr())
    };
    RegCloseKey(key);
    if status == 0 || (!enabled && status == 2) {
        Ok(())
    } else {
        Err(format!("启动项设置失败：{status}"))
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    pub(super) struct Fixture {
        pub(super) app: App,
        external: HWND,
        _graphics: theme::TestGraphicsScope,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                self.app.rest.close();
                self.app.restore_all();
                for record in &self.app.records {
                    if record.token.valid() {
                        RemovePropW(handle(record.token.hwnd), wide(PROP).as_ptr());
                    }
                }
                if let Some(u) = &self.app.ui {
                    DestroyWindow(u.window);
                    DeleteObject(u.font);
                    DeleteObject(u.heading_font);
                    DeleteObject(u.value_font);
                }
                DestroyWindow(self.app.host);
                if IsWindow(self.external) != 0 {
                    DestroyWindow(self.external);
                }
                let _ = std::fs::remove_dir_all(&self.app.dir);
            }
        }
    }
    pub(super) unsafe fn fixture() -> Fixture {
        let graphics = theme::TestGraphicsScope::new();
        let classes: [(&str, WNDPROC); 2] = [
            ("DunYu.Settings.1", Some(procedure)),
            ("DunYu.Content.1", Some(content_proc)),
        ];
        for (name, proc) in classes {
            let mut wc: WNDCLASSW = std::mem::zeroed();
            let name = wide(name);
            wc.lpszClassName = name.as_ptr();
            wc.lpfnWndProc = proc;
            wc.hInstance = GetModuleHandleW(null());
            wc.hbrBackground = (COLOR_BTNFACE + 1) as HBRUSH;
            assert_ne!(RegisterClassW(&wc), 0);
        }
        InitCommonControls();
        let create = |label: &str| {
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide(label).as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                200,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            )
        };
        let host = create("UI test host");
        let external = create("UI test target");
        let dir = std::env::temp_dir().join(format!("dunyu-ui-test-{}", std::process::id()));
        let mut f = Fixture {
            app: App {
                host,
                config: AppConfig::default(),
                dir,
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
            },
            external,
            _graphics: graphics,
        };
        let app = &mut f.app;
        // Pretend unchanged default keys are already registered: this test covers
        // editor persistence, not registration, and must not reserve global keys.
        for (i, (key, cmd)) in app.config.bindings().into_iter().enumerate() {
            app.keys.insert(i as i32 + 1, (key, cmd));
        }
        app.open_ui();

        app.ui.as_mut().unwrap().target = None;
        app.refresh_state();
        f
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn real_navigation_uses_cache() {
        unsafe {
            let mut f = fixture();
            let h = f.app.ui.as_ref().unwrap().window;
            let dpi = f.app.ui.as_ref().unwrap().dpi as i32;
            for width in [900, 1024] {
                let mut outer: RECT = std::mem::zeroed();
                let mut client: RECT = std::mem::zeroed();
                GetWindowRect(h, &mut outer);
                GetClientRect(h, &mut client);
                SetWindowPos(
                    h,
                    null_mut(),
                    0,
                    0,
                    width * dpi / 96 + outer.right - outer.left - client.right,
                    651 * dpi / 96 + outer.bottom - outer.top - client.bottom,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
                f.app.layout_ui();
                for _ in 0..2 {
                    for page in 0..PAGES.len() {
                        f.app.page(page);
                        let tab = item(h, 9);
                        for focus in [h, tab] {
                            SetFocus(focus);
                            assert_eq!(GetFocus(), focus);
                            theme::verify_navigation_for_test(tab);
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn static_controls_match_original() {
        unsafe {
            let mut f = fixture();
            let h = f.app.ui.as_ref().unwrap().window;
            for page in [0, 1, 2, 0] {
                f.app.page(page);
                for id in [9002, 114, 7001] {
                    let control = item(h, id);
                    theme::verify_static_for_test(control);
                    SendMessageW(control, WM_THEMECHANGED, 0, 0);
                    theme::verify_static_for_test(control);
                }
            }
            set_text(h, 114, "测试标题变化");
            theme::verify_static_for_test(item(h, 114));
        }
    }
    #[test]
    fn editor_drafts_selection_layout_and_target_state() {
        unsafe {
            let mut f = fixture();
            let external = f.external;
            let app = &mut f.app;
            let h = app.ui.as_ref().unwrap().window;
            assert!(!item(h, 201).is_null());
            assert_eq!(
                IsWindowVisible(item(h, 9004)),
                0,
                "empty list must not show scrollbar"
            );
            assert_eq!(GetAncestor(item(h, 201), GA_ROOT), h);
            assert_eq!(app.rule_snapshot(), app.ui.as_ref().unwrap().baseline);
            set_text(h, 200, "测试程序");
            set_text(h, 201, "C:\\测试\\demo.exe");
            check(h, 208, false);
            assert_ne!(app.rule_snapshot(), app.ui.as_ref().unwrap().baseline);
            // Switching through the reserved page must not expose settings controls.
            app.page(4);
            for (_, child) in &app.ui.as_ref().unwrap().controls {
                if GetParent(*child) == app.ui.as_ref().unwrap().content {
                    assert_eq!(
                        IsWindowVisible(*child),
                        0,
                        "content leaked into message page"
                    );
                }
            }
            app.page(5);
            assert_ne!(IsWindowVisible(item(h, 500)), 0);
            assert_ne!(IsWindowVisible(item(h, 506)), 0);
            app.page(3);
            let draft = text_at(h, 801);
            check(h, 800, false);
            app.ui_action(800).unwrap();
            for id in [801, 802, 815] {
                assert_eq!(IsWindowEnabled(item(h, id)), 0);
            }
            check(h, 800, true);
            app.ui_action(800).unwrap();
            assert_ne!(IsWindowEnabled(item(h, 801)), 0);
            assert_eq!(text_at(h, 801), draft);
            app.page(0);
            let saved_rest = app.config.rest.clone();
            set_text(h, 801, "0");
            assert!(app.ui_action(808).is_err());
            assert_eq!(app.config.rest, saved_rest);
            set_text(h, 801, "30");
            check(h, 815, false);
            check(h, 816, true);
            app.ui_action(808).unwrap();
            assert_eq!(app.config.rest.interval_minutes, 30);
            assert!(!app.config.rest.short_auto_start);
            assert!(app.config.rest.long_auto_start);
            check(h, 800, false);
            check(h, 817, true);
            set_text(h, 804, "45");
            app.ui_action(808).unwrap();
            assert!(!app.config.rest.short_enabled);
            assert!(app.config.rest.long_enabled);
            assert_eq!(app.config.rest.long_interval(), 45);
            assert!(app.rest.scheduler.long());
            check(h, 817, false);
            app.ui_action(808).unwrap();
            assert_eq!(app.rest.scheduler.phase, dunyu::rest::Phase::Disabled);
            let saved_interval = app.rest.scheduler.remaining_ms;
            app.refresh_state();
            assert_eq!(app.rest.scheduler.remaining_ms, saved_interval);

            app.page(1);
            app.page(0);
            assert_eq!(text_at(h, 200), "测试程序");
            app.ui_action(113).unwrap();
            assert_eq!(app.ui.as_ref().unwrap().selected, Some(0));
            assert_eq!(
                SendMessageW(app.ui.as_ref().unwrap().list, LB_GETCURSEL, 0, 0),
                0
            );
            assert_eq!(text_at(h, 200), "测试程序");
            assert_eq!(app.rule_snapshot(), app.ui.as_ref().unwrap().baseline);
            // Adding the first rule must suppress the empty caption in both layouts;
            // clearing the list must restore it, and reloading must suppress it again.
            let saved_rules = app.config.rules.clone();
            app.scale_ui(96, None);
            for width in [800, 1100] {
                SetWindowPos(
                    h,
                    null_mut(),
                    0,
                    0,
                    width,
                    700,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
                app.layout_ui();
                assert!(SendMessageW(app.ui.as_ref().unwrap().list, LB_GETCOUNT, 0, 0) > 0);
                assert_eq!(IsWindowVisible(item(h, 118)), 0);
                app.config.rules.clear();
                app.refresh_list();
                app.layout_ui();
                let caption = item(h, 118);
                let clip = item(h, 119);
                assert_ne!(IsWindowVisible(caption), 0);
                assert_eq!(IsWindowVisible(app.ui.as_ref().unwrap().list), 0);
                assert_eq!(GetParent(caption), clip);
                assert_ne!(
                    GetWindowLongPtrW(caption, GWL_STYLE) as u32
                        & windows_sys::Win32::System::SystemServices::SS_CENTER,
                    0
                );
                let mut bounds: RECT = std::mem::zeroed();
                GetClientRect(clip, &mut bounds);
                let mut actual: RECT = std::mem::zeroed();
                GetWindowRect(caption, &mut actual);
                MapWindowPoints(null_mut(), clip, &mut actual as *mut _ as *mut POINT, 2);
                let (_, expected) = super::super::design::empty_state(
                    bounds.right,
                    bounds.bottom,
                    theme::font_line_height(caption),
                    96,
                );
                assert_eq!(
                    (
                        actual.left,
                        actual.top,
                        actual.right - actual.left,
                        actual.bottom - actual.top
                    ),
                    expected
                );
                app.config.rules = saved_rules.clone();
                app.refresh_list();
                app.layout_ui();
                assert_eq!(IsWindowVisible(item(h, 118)), 0);
            }
            app.scale_ui(96, None);
            // Scrolling must not dirty the stationary save button or status line.
            // Exercise both an idle frame and an actual change of scroll position.
            for (page, dpi) in [0, 5]
                .into_iter()
                .flat_map(|page| [96, 120, 144, 192].map(|dpi| (page, dpi)))
            {
                app.scale_ui(dpi, None);
                app.page(page);
                if page == 0 {
                    app.ui.as_mut().unwrap().advanced = true;
                    app.layout_ui();
                }
                let save = if page == 0 { 113 } else { 506 };
                for position in [0., 24., 24., 0.] {
                    RedrawWindow(
                        h,
                        null(),
                        null_mut(),
                        RDW_VALIDATE | RDW_NOERASE | RDW_ALLCHILDREN | RDW_NOFRAME,
                    );
                    app.ui.as_mut().unwrap().motions[0].snap(position);
                    app.scroll_frame();
                    let mut content_bounds: RECT = std::mem::zeroed();
                    let mut save_bounds: RECT = std::mem::zeroed();
                    GetWindowRect(app.ui.as_ref().unwrap().content, &mut content_bounds);
                    GetWindowRect(item(h, save), &mut save_bounds);
                    assert!(
                        content_bounds.bottom <= save_bounds.top,
                        "editor overlaps save button on page {page}, DPI {dpi}, position {position}"
                    );
                    let mut status_bounds: RECT = std::mem::zeroed();
                    GetWindowRect(item(h, 9001), &mut status_bounds);
                    assert!(
                        content_bounds.bottom <= status_bounds.top,
                        "editor overlaps status on page {page}, DPI {dpi}"
                    );
                    for child in [item(h, save), item(h, 9001)] {
                        assert_eq!(
                            GetUpdateRect(child, null_mut(), 0),
                            0,
                            "scroll invalidated footer on page {page}"
                        );
                    }
                    // Root redraws would paint over the footer before its child paint.
                    let mut footer = RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    GetClientRect(h, &mut footer);
                    footer.top = footer.bottom - 60;
                    let update = CreateRectRgn(0, 0, 0, 0);
                    GetUpdateRgn(h, update, 0);
                    let band = CreateRectRgn(footer.left, footer.top, footer.right, footer.bottom);
                    CombineRgn(update, update, band, RGN_AND);
                    let mut dirty: RECT = std::mem::zeroed();
                    assert_eq!(
                        GetRgnBox(update, &mut dirty),
                        NULLREGION,
                        "scroll dirtied root footer"
                    );
                    DeleteObject(update);
                    DeleteObject(band);
                }
            }
            app.page(0);
            app.ui.as_mut().unwrap().advanced = false;
            app.layout_ui();
            set_text(h, 200, "未保存草稿");
            app.pending.push(Pending {
                rule: 0,
                started: Instant::now(),
            });
            assert!(app.ui_action(113).is_err());
            app.pending.clear();
            assert_eq!(text_at(h, 200), "未保存草稿");
            assert_eq!(app.config.rules[0].name, "测试程序");
            assert_ne!(app.rule_snapshot(), app.ui.as_ref().unwrap().baseline);
            // Native regressions: overflow arrows, clipped edit lines, and layout
            // movement when expanding/collapsing advanced summon fields.
            for dpi in [96, 120, 144, 192] {
                app.scale_ui(dpi, None);
                app.page(0);
                app.ui.as_mut().unwrap().advanced = false;
                app.layout_ui();
                let bounds = |id| {
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(item(h, id), &mut r);
                    (r.left, r.top, r.right, r.bottom)
                };
                let ids = [9, 9002, 119, 120, 114, 112, 200, 201, 207, 208, 116];
                let before: Vec<_> = ids.iter().map(|id| bounds(*id)).collect();
                app.ui_action(116).unwrap();
                assert_ne!(IsWindowVisible(item(h, 202)), 0);
                for (id, expected) in ids.iter().zip(&before) {
                    assert_eq!(
                        bounds(*id),
                        *expected,
                        "advanced moved control {id} at DPI {dpi}"
                    );
                }
                app.ui_action(116).unwrap();
                assert_eq!(IsWindowVisible(item(h, 202)), 0);
                for (id, expected) in ids.iter().zip(&before) {
                    assert_eq!(bounds(*id), *expected);
                }
                let tab = item(h, 9);
                UpdateWindow(tab);
                let mut client: RECT = std::mem::zeroed();
                GetClientRect(tab, &mut client);
                for index in 0..PAGES.len() {
                    let mut r: RECT = std::mem::zeroed();
                    SendMessageW(tab, TCM_GETITEMRECT, index, &mut r as *mut _ as isize);
                    assert!(
                        r.left >= 0 && r.right <= client.right,
                        "navigation overflow at DPI {dpi}"
                    );
                }
                let arrows =
                    FindWindowExW(tab, null_mut(), wide("msctls_updown32").as_ptr(), null());
                assert!(
                    arrows.is_null() || IsWindowVisible(arrows) == 0,
                    "navigation scroll arrows at DPI {dpi}"
                );
                app.page(3);
                for id in [801, 802, 803, 804] {
                    let edit = item(h, id);
                    let mut r: RECT = std::mem::zeroed();
                    SendMessageW(edit, EM_GETRECT, 0, &mut r as *mut _ as isize);
                    assert!(
                        r.bottom - r.top >= theme::font_line_height(edit),
                        "clipped numeric edit {id} at DPI {dpi}"
                    );
                }
            }
            app.scale_ui(96, None);
            app.page(0);
            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                app.ui.as_mut().unwrap().split_ratio = super::super::design::SPLIT;
                app.scale_ui(144, None);
                std::fs::create_dir_all("target/ui-preview").unwrap();
                for (page, name) in [
                    (0, "summon"),
                    (1, "bind"),
                    (2, "opacity"),
                    (3, "rest"),
                    (4, "message"),
                    (5, "settings"),
                ] {
                    app.page(page);
                    let fixtures = [
                        (810, "小憩 06:11 · 舒展 已关闭"),
                        (400, "目标窗口不可用，请重新捕获"),
                        (405, "不可用"),
                        (801, "20"),
                        (802, "20"),
                        (804, "60"),
                        (803, "5"),
                    ];
                    let original: Vec<_> = fixtures
                        .iter()
                        .map(|(id, _)| (*id, text_at(h, *id)))
                        .collect();
                    for (id, value) in fixtures {
                        set_text(h, id, value);
                    }
                    theme::capture(h, &format!("target/ui-preview/{name}.bmp")).unwrap();
                    for (id, value) in original {
                        set_text(h, id, &value);
                    }
                }
                app.page(4);
                assert_eq!(IsWindowVisible(item(h, 506)), 0);
                assert_eq!(IsWindowVisible(item(h, 500)), 0);
                app.page(5);
                assert_ne!(IsWindowVisible(item(h, 500)), 0);
                app.page(0);
                let saved = app.config.rules.clone();
                app.config.rules.clear();
                app.refresh_list();
                app.layout_ui();
                theme::capture(h, "target/ui-preview/empty.bmp").unwrap();
                app.config.rules = saved;
                app.refresh_list();
                app.layout_ui();
                EnableWindow(item(h, 113), 0);
                theme::capture(h, "target/ui-preview/disabled.bmp").unwrap();
                EnableWindow(item(h, 113), 1);
                SetFocus(item(h, 201));
                SendMessageW(item(h, 113), WM_MOUSEMOVE, 0, 0);
                theme::capture(h, "target/ui-preview/focus-hover.bmp").unwrap();
                SendMessageW(item(h, 113), WM_MOUSELEAVE, 0, 0);
            }
            app.ui_action(116).unwrap();
            assert!(IsWindowVisible(item(h, 204)) != 0);
            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                theme::capture(h, "target/ui-preview/advanced.bmp").unwrap();
            }
            if std::env::var_os("DUNYU_UI_TRANSITION_CHECK").is_some() {
                std::fs::create_dir_all("target/ui-preview").unwrap();
                let address = h as usize;
                let recorder = std::thread::spawn(move || {
                    for frame in 0..40 {
                        let path = format!("target/ui-preview/transition-{frame:02}.bmp");
                        theme::capture_screen(address as HWND, &path).unwrap();
                        let bytes = std::fs::read(&path).unwrap();
                        let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
                        let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap())
                            .unsigned_abs() as usize;
                        // A broad bright rectangle in the dark content area is not text.
                        let mut rows = 0;
                        for y in 100..height.saturating_sub(40) {
                            let mut run = 0;
                            let mut broad = false;
                            for x in 20..width.saturating_sub(20) {
                                let offset = 54 + (y * width + x) * 4;
                                if bytes[offset..offset + 3].iter().all(|c| *c > 235) {
                                    run += 1;
                                } else {
                                    run = 0;
                                }
                                if run >= 160 {
                                    broad = true;
                                }
                            }
                            rows = if broad { rows + 1 } else { 0 };
                            assert!(rows < 20, "white flash in {path}");
                        }
                        std::thread::sleep(std::time::Duration::from_millis(16));
                    }
                });
                for page in [0, 2, 1, 2, 3, 4, 5, 2].into_iter().cycle().take(32) {
                    app.page(page);
                    let mut message: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                recorder.join().unwrap();
                app.page(0);
            }
            // Opt in on an interactive Windows desktop: compare the actual displayed
            // pixels after scrolling with a complete repaint at identical geometry.
            if std::env::var_os("DUNYU_UI_SCROLL_CHECK").is_some() {
                std::fs::create_dir_all("target/ui-preview").unwrap();
                SetFocus(h);
                for offset in [0, 120, 300, 120, 0, 400, 0] {
                    app.ui.as_mut().unwrap().scroll = offset;
                    app.layout_ui();
                    RedrawWindow(h, null(), null_mut(), RDW_UPDATENOW | RDW_ALLCHILDREN);
                    windows_sys::Win32::Graphics::Dwm::DwmFlush();
                    theme::capture_screen(h, "target/ui-preview/scroll-actual.bmp").unwrap();
                    RedrawWindow(
                        h,
                        null(),
                        null_mut(),
                        RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN,
                    );
                    windows_sys::Win32::Graphics::Dwm::DwmFlush();
                    theme::capture_screen(h, "target/ui-preview/scroll-redrawn.bmp").unwrap();
                    assert_eq!(
                        std::fs::read("target/ui-preview/scroll-actual.bmp").unwrap(),
                        std::fs::read("target/ui-preview/scroll-redrawn.bmp").unwrap(),
                        "scroll residue at {offset}"
                    );
                }
            }
            app.ui_action(116).unwrap();
            assert_eq!(IsWindowVisible(item(h, 204)), 0);
            let rect = |h: HWND| {
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(h, &mut r);
                (r.right - r.left, r.bottom - r.top)
            };
            app.scale_ui(96, None);
            let normal_list = rect(item(h, 119));
            SetWindowPos(
                h,
                null_mut(),
                0,
                0,
                1400,
                900,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            app.layout_ui();
            assert!(rect(item(h, 119)).0 > normal_list.0);
            assert!(rect(item(h, 119)).1 > normal_list.1);
            app.ui.as_mut().unwrap().split_ratio = 0.5;
            app.layout_ui();
            assert!(rect(item(h, 119)).0 > normal_list.0);
            app.ui.as_mut().unwrap().split_ratio = 0.36;
            app.scale_ui(96, None);
            let before = rect(item(h, 201));
            for dpi in [144, 192, 120, 96] {
                app.scale_ui(dpi, None);
            }
            assert_eq!(rect(item(h, 201)), before);
            // Exercise the actual GDI font metrics and paint rectangle at larger
            // text sizes: the previous 12px top/bottom inset clipped even 14px text.
            for dpi in [96, 120, 144, 192] {
                for logical_height in [14, 20, 28] {
                    app.scale_ui(dpi, None);
                    let mut font: LOGFONTW = std::mem::zeroed();
                    font.lfHeight = -(logical_height * dpi as i32 / 96);
                    font.lfCharSet = DEFAULT_CHARSET;
                    let face = wide("Microsoft YaHei UI");
                    font.lfFaceName[..face.len()].copy_from_slice(&face);
                    let large_font = CreateFontIndirectW(&font);
                    assert!(!large_font.is_null());
                    for (_, child) in &app.ui.as_ref().unwrap().controls {
                        if class(*child).eq_ignore_ascii_case("button")
                            && GetWindowLongPtrW(*child, GWL_STYLE) as u32 & 15
                                != BS_AUTOCHECKBOX as u32
                        {
                            SendMessageW(*child, WM_SETFONT, large_font as usize, 0);
                        }
                    }
                    for page in 0..PAGES.len() {
                        app.page(page);
                        for (_, child) in &app.ui.as_ref().unwrap().controls {
                            if IsWindowVisible(*child) == 0
                                || !class(*child).eq_ignore_ascii_case("button")
                                || GetWindowLongPtrW(*child, GWL_STYLE) as u32 & 15
                                    == BS_AUTOCHECKBOX as u32
                            {
                                continue;
                            }
                            let mut bounds: RECT = std::mem::zeroed();
                            GetClientRect(*child, &mut bounds);
                            let label = theme::button_text_rect(bounds, dpi);
                            let size = theme::label_size(*child);
                            assert!(
                                label.bottom - label.top >= size.cy,
                                "clipped button {} at {dpi} DPI, font {logical_height}",
                                title(*child)
                            );
                            assert!(
                                label.right - label.left >= size.cx,
                                "truncated button {} at {dpi} DPI, font {logical_height}",
                                title(*child)
                            );
                        }
                        if dpi == 144
                            && logical_height == 28
                            && std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some()
                        {
                            theme::capture(h, &format!("target/ui-preview/large-text-{page}.bmp"))
                                .unwrap();
                        }
                    }
                    app.scale_ui(dpi, None);
                    DeleteObject(large_font);
                }
            }
            app.scale_ui(96, None);
            app.page(2);
            let n = app.track(external).unwrap();
            app.ui.as_mut().unwrap().target = Some(app.records[n].token.clone());
            app.opacity(external, Some(128), 0).unwrap();
            app.refresh_state();
            assert_eq!(text_at(h, 405), "50%");
            assert_eq!(
                SendMessageW(app.ui.as_ref().unwrap().opacity, WM_USER, 0, 0),
                50
            );
            if std::env::var_os("DUNYU_UI_OPACITY_CHECK").is_some() {
                std::fs::create_dir_all("target/ui-preview").unwrap();
                let slider = app.ui.as_ref().unwrap().opacity;
                SetFocus(slider);
                let mut track: RECT = std::mem::zeroed();
                SendMessageW(slider, TBM_GETCHANNELRECT, 0, &mut track as *mut _ as isize);
                for value in [10, 90, 20, 100, 40, 10] {
                    let x = track.left + (track.right - track.left) * (value - 10) / 90;
                    let y = (track.top + track.bottom) / 2;
                    let point = ((y as isize) << 16) | x as isize;
                    let mut thumb: RECT = std::mem::zeroed();
                    SendMessageW(slider, TBM_GETTHUMBRECT, 0, &mut thumb as *mut _ as isize);
                    let start = (((thumb.top + thumb.bottom) / 2) as isize) << 16
                        | ((thumb.left + thumb.right) / 2) as isize;
                    SendMessageW(slider, WM_LBUTTONDOWN, 1, start);
                    SendMessageW(slider, WM_MOUSEMOVE, 1, point);
                    SendMessageW(slider, WM_LBUTTONUP, 0, point);
                    UpdateWindow(slider);
                    windows_sys::Win32::Graphics::Dwm::DwmFlush();
                    theme::capture_screen(slider, "target/ui-preview/opacity-actual.bmp").unwrap();
                    RedrawWindow(
                        slider,
                        null(),
                        null_mut(),
                        RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
                    );
                    windows_sys::Win32::Graphics::Dwm::DwmFlush();
                    theme::capture_screen(slider, "target/ui-preview/opacity-redrawn.bmp").unwrap();
                    assert_eq!(
                        std::fs::read("target/ui-preview/opacity-actual.bmp").unwrap(),
                        std::fs::read("target/ui-preview/opacity-redrawn.bmp").unwrap(),
                        "opacity residue at {value}"
                    );
                }
            }
            app.ui.as_mut().unwrap().pending_opacity = Some((app.records[n].token.clone(), 30));
            app.ui_action(403).unwrap();
            assert!(app.ui.as_ref().unwrap().pending_opacity.is_none());
            assert_eq!(text_at(h, 405), "100%");
            DestroyWindow(external);
            app.refresh_state();
            assert_eq!(IsWindowEnabled(item(h, 402)), 0);
        }
    }

    #[test]
    fn records_keys_and_consumes_existing_bindings() {
        unsafe {
            let field = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("Ctrl+Alt+A").as_ptr(),
                ES_READONLY as u32,
                0,
                0,
                100,
                30,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!field.is_null());
            struct Cleanup(HWND, [u8; 256]);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    unsafe {
                        SetKeyboardState(self.1.as_ptr());
                        DestroyWindow(self.0);
                    }
                }
            }
            let mut cleanup = Cleanup(field, [0; 256]);
            assert_ne!(GetKeyboardState(cleanup.1.as_mut_ptr()), 0);
            let mut state = [0u8; 256];
            assert_ne!(SetKeyboardState(state.as_ptr()), 0);
            let mut msg: MSG = std::mem::zeroed();
            msg.hwnd = field;
            msg.message = WM_KEYDOWN;
            // Bare keys and incomplete modifier combinations preserve the value.
            for key in [VK_CONTROL, VK_LWIN, VK_F12, 0x42] {
                msg.wParam = key as usize;
                assert!(capture_key_message(field, &msg));
                assert_eq!(title(field), "Ctrl+Alt+A");
            }
            msg.wParam = VK_TAB as usize;
            assert!(!capture_key_message(field, &msg));
            state[VK_SHIFT as usize] = 0x80;
            assert_ne!(SetKeyboardState(state.as_ptr()), 0);
            assert!(!capture_key_message(field, &msg));

            for (modifier, message, key, expected) in [
                (VK_CONTROL, WM_KEYDOWN, 0x42, "Ctrl+B"),
                (VK_MENU, WM_SYSKEYDOWN, VK_LEFT, "Alt+Left"),
                (VK_SHIFT, WM_KEYDOWN, VK_SPACE, "Shift+Space"),
                (VK_RWIN, WM_KEYDOWN, VK_F11, "Win+F11"),
            ] {
                state = [0; 256];
                state[modifier as usize] = 0x80;
                assert_ne!(SetKeyboardState(state.as_ptr()), 0);
                msg.message = message;
                msg.wParam = key as usize;
                assert!(capture_key_message(field, &msg));
                assert_eq!(title(field), expected);
                assert_eq!(hotkey_text(&parse_key(&title(field)).unwrap()), expected);
            }
            msg.message = WM_KEYDOWN;
            msg.wParam = VK_F12 as usize;
            assert!(capture_key_message(field, &msg));
            assert_eq!(title(field), "Win+F11");

            // WM_HOTKEY carries its own modifiers, independent of current state.
            state = [0; 256];
            assert_ne!(SetKeyboardState(state.as_ptr()), 0);
            msg.message = WM_HOTKEY;
            msg.lParam = ((VK_BACK as isize) << 16) | 3;
            assert!(capture_key_message(field, &msg));
            assert_eq!(title(field), "Ctrl+Alt+Backspace");
            for message in [WM_KEYUP, WM_SYSKEYUP, WM_CHAR, WM_SYSCHAR] {
                msg.message = message;
                assert!(capture_key_message(field, &msg));
                assert_eq!(title(field), "Ctrl+Alt+Backspace");
            }
            msg.message = WM_LBUTTONDOWN;
            assert!(!capture_key_message(field, &msg));
        }
    }
}

#[cfg(test)]
#[path = "diagnostics.rs"]
pub(crate) mod diagnostics;
