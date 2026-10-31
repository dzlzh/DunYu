use super::*;

fn is_hotkey_field(id: i32) -> bool {
    matches!(id, 207 | 300..=307 | 500..=503)
}

// Run before IsDialogMessage/TranslateMessage so arrows, spaces and Alt
// combinations are recorded instead of navigating or activating controls.
pub(super) unsafe fn capture_hotkey(window: HWND, msg: &MSG) -> bool {
    let field = GetFocus();
    if GetForegroundWindow() != window
        || field.is_null()
        || GetParent(field) != window
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
    let e = GetDlgItem(h, id);
    let n = GetWindowTextLengthW(e);
    let mut b = vec![0; n as usize + 1];
    GetWindowTextW(e, b.as_mut_ptr(), b.len() as i32);
    String::from_utf16_lossy(&b[..n as usize])
}
pub(super) unsafe fn set_text(h: HWND, id: i32, s: &str) {
    SetWindowTextW(GetDlgItem(h, id), wide(s).as_ptr());
}
pub(super) unsafe fn checked(h: HWND, id: i32) -> bool {
    SendMessageW(GetDlgItem(h, id), BM_GETCHECK, 0, 0) == 1
}
pub(super) unsafe fn check(h: HWND, id: i32, on: bool) {
    SendMessageW(GetDlgItem(h, id), BM_SETCHECK, usize::from(on), 0);
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
    let style = if kind == "EDIT" && is_hotkey_field(id) {
        style | ES_READONLY as u32
    } else {
        style
    };
    let c = CreateWindowExW(
        if kind == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
        wide(kind).as_ptr(),
        wide(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        w,
        h,
        u.window,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(c, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
    u.controls.push((page, c));
    c
}
pub(super) unsafe fn field(u: &mut Ui, page: usize, label: &str, id: i32, y: i32, value: &str) {
    control(u, page, "STATIC", label, 0, 245, y + 4, 130, 24, 0);
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
        if let Some(u) = &self.ui {
            ShowWindow(u.window, SW_SHOW);
            SetForegroundWindow(u.window);
            return;
        }
        let target = GetForegroundWindow();
        let h = CreateWindowExW(
            0,
            wide("DunYu.Settings.1").as_ptr(),
            wide("遁鱼 DunYu · 摸鱼有道，隐现自如").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            660,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        let mut u = Ui {
            window: h,
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
        };
        for (i, label) in ["召唤", "绑定", "隐现", "设置"].iter().enumerate() {
            control(
                &mut u,
                99,
                "BUTTON",
                label,
                10 + i as i32,
                15 + i as i32 * 190,
                12,
                180,
                32,
                WS_TABSTOP,
            );
        }
        u.list = control(
            &mut u,
            0,
            "LISTBOX",
            "",
            100,
            15,
            60,
            210,
            420,
            WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32 | WS_TABSTOP,
        );
        for (id, label, y) in [(110, "新建规则", 490), (111, "删除规则", 530)] {
            control(&mut u, 0, "BUTTON", label, id, 15, y, 210, 30, WS_TABSTOP);
        }
        for (id, label, y) in [
            (200, "名称", 60),
            (201, "启动路径 (.exe/.lnk)", 98),
            (202, "启动参数", 136),
            (203, "工作目录", 174),
            (204, "匹配进程完整路径", 212),
            (205, "窗口类名（可空）", 250),
            (206, "标题包含（可空）", 288),
            (207, "快捷键（按键）", 326),
        ] {
            field(&mut u, 0, label, id, y, "");
        }
        control(
            &mut u,
            0,
            "BUTTON",
            "启用",
            208,
            245,
            370,
            130,
            28,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "最小化代替隐藏",
            209,
            390,
            370,
            250,
            28,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "捕获窗口信息（3秒后）",
            112,
            245,
            415,
            250,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "BUTTON",
            "保存规则",
            113,
            520,
            415,
            230,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            0,
            "STATIC",
            "启动路径请填写完整路径。快捷方式需匹配实际程序路径。\n点击快捷键框后按组合键，如 Ctrl+Alt+A，再保存。",
            0,
            245,
            465,
            510,
            60,
            0,
        );
        for i in 0..4 {
            let y = 80 + i * 100;
            control(
                &mut u,
                1,
                "STATIC",
                &format!("槽位 {}", i + 1),
                0,
                20,
                y,
                100,
                25,
                0,
            );
            control(
                &mut u,
                1,
                "EDIT",
                &hotkey_text(
                    &self
                        .config
                        .slots
                        .get(i as usize)
                        .map(|s| s.bind.clone())
                        .unwrap_or(Hotkey::new(7, 49 + i as u32)),
                ),
                300 + i * 2,
                130,
                y,
                220,
                28,
                WS_TABSTOP,
            );
            control(
                &mut u,
                1,
                "EDIT",
                &hotkey_text(
                    &self
                        .config
                        .slots
                        .get(i as usize)
                        .map(|s| s.toggle.clone())
                        .unwrap_or(Hotkey::new(3, 49 + i as u32)),
                ),
                301 + i * 2,
                370,
                y,
                220,
                28,
                WS_TABSTOP,
            );
            control(
                &mut u,
                1,
                "BUTTON",
                "3秒后绑定",
                320 + i,
                610,
                y,
                140,
                28,
                WS_TABSTOP,
            );
            control(
                &mut u,
                1,
                "STATIC",
                "尚未绑定",
                340 + i,
                130,
                y + 35,
                610,
                25,
                0,
            );
        }
        control(
            &mut u,
            1,
            "STATIC",
            "左侧为绑定快捷键，右侧为显示／隐藏快捷键；点击框后直接按组合键。",
            0,
            20,
            50,
            740,
            24,
            0,
        );
        control(
            &mut u,
            1,
            "BUTTON",
            "保存槽位快捷键",
            330,
            20,
            510,
            220,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            2,
            "STATIC",
            "选择目标后调整不透明度（10%～100%），增减热键支持长按连调",
            0,
            25,
            70,
            700,
            28,
            0,
        );
        control(
            &mut u,
            2,
            "STATIC",
            &title(target),
            400,
            25,
            115,
            700,
            30,
            0,
        );
        control(
            &mut u,
            2,
            "BUTTON",
            "3秒后捕获当前窗口",
            401,
            25,
            165,
            300,
            32,
            WS_TABSTOP,
        );
        u.opacity = control(
            &mut u,
            2,
            "msctls_trackbar32",
            "",
            402,
            25,
            230,
            710,
            55,
            WS_TABSTOP,
        );
        SendMessageW(u.opacity, WM_USER + 6, 1, (100 << 16) | 10);
        SendMessageW(u.opacity, WM_USER + 5, 1, 100);
        control(
            &mut u,
            2,
            "BUTTON",
            "恢复为 100%",
            403,
            25,
            310,
            300,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            2,
            "BUTTON",
            "恢复全部窗口与透明度",
            404,
            350,
            310,
            385,
            32,
            WS_TABSTOP,
        );
        for (id, label, y, value) in [
            (
                500,
                "降低不透明度",
                80,
                hotkey_text(&self.config.opacity_down),
            ),
            (
                501,
                "提高不透明度",
                130,
                hotkey_text(&self.config.opacity_up),
            ),
            (
                502,
                "恢复 100%",
                180,
                hotkey_text(&self.config.opacity_reset),
            ),
            (503, "恢复全部", 230, hotkey_text(&self.config.restore_all)),
            (504, "步长（1～50）", 280, self.config.step.to_string()),
        ] {
            field(&mut u, 3, label, id, y, &value);
        }
        control(
            &mut u,
            3,
            "BUTTON",
            "登录后自动启动",
            505,
            245,
            335,
            400,
            30,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        check(h, 505, self.config.autostart);
        control(
            &mut u,
            3,
            "BUTTON",
            "保存设置",
            506,
            245,
            390,
            500,
            32,
            WS_TABSTOP,
        );
        control(
            &mut u,
            3,
            "STATIC",
            "点击快捷键框后按组合键，Tab 离开，再点击保存。\n增减不透明度支持长按连调；步长越小，调整越细。\n关闭窗口后仍在托盘运行，退出请使用托盘菜单。",
            0,
            245,
            450,
            500,
            90,
            0,
        );
        self.ui = Some(u);
        self.scale_ui(GetDpiForWindow(h).max(96), None);
        self.refresh_list();
        self.page(0);
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
    }
    pub(super) unsafe fn scale_ui(&mut self, dpi: u32, suggested: Option<RECT>) {
        let Some(u) = self.ui.as_mut() else {
            return;
        };
        let previous = u.dpi;
        if previous == dpi && !u.font.is_null() {
            return;
        }
        let mut font: LOGFONTW = std::mem::zeroed();
        font.lfHeight = -(16 * dpi as i32 / 96);
        font.lfCharSet = DEFAULT_CHARSET;
        let face = wide("Microsoft YaHei UI");
        font.lfFaceName[..face.len()].copy_from_slice(&face);
        let new_font = CreateFontIndirectW(&font);
        for (_, child) in &u.controls {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(*child, &mut r);
            let mut p = POINT {
                x: r.left,
                y: r.top,
            };
            ScreenToClient(u.window, &mut p);
            let scale = |n: i32| ((n as i64 * dpi as i64) / previous as i64) as i32;
            SetWindowPos(
                *child,
                null_mut(),
                scale(p.x),
                scale(p.y),
                scale(r.right - r.left),
                scale(r.bottom - r.top),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(u.window, &mut r);
        let bounds = suggested.unwrap_or(RECT {
            left: r.left,
            top: r.top,
            right: r.left + 800 * dpi as i32 / 96,
            bottom: r.top + 660 * dpi as i32 / 96,
        });
        SetWindowPos(
            u.window,
            null_mut(),
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        if !new_font.is_null() {
            for (_, child) in &u.controls {
                SendMessageW(*child, WM_SETFONT, new_font as usize, 1);
            }
            if !u.font.is_null() {
                DeleteObject(u.font);
            }
            u.font = new_font;
        }
        u.dpi = dpi;
    }
    pub(super) unsafe fn page(&mut self, page: usize) {
        if let Some(u) = &mut self.ui {
            u.page = page;
            for (p, h) in &u.controls {
                ShowWindow(
                    *h,
                    if *p == 99 || *p == page {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for i in 0..4 {
                let label = self
                    .slots
                    .get(i)
                    .and_then(|t| t.as_ref())
                    .filter(|t| t.valid())
                    .map(|t| title(handle(t.hwnd)))
                    .unwrap_or("尚未绑定".into());
                set_text(u.window, 340 + i as i32, &label);
            }
        }
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
                        "{}{}",
                        if r.enabled { "" } else { "[停用] " },
                        r.name
                    ))
                    .as_ptr() as isize,
                );
            }
        }
    }
    pub(super) unsafe fn fill_rule(&mut self, index: Option<usize>) {
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
            10..=13 => self.page((id - 10) as usize),
            110 => self.fill_rule(None),
            111 => {
                if let Some(i) = self.ui.as_ref().unwrap().selected {
                    let mut c = self.config.clone();
                    c.rules.remove(i);
                    self.apply_config(c)?;
                    self.fill_rule(None);
                }
            }
            112 | 401 | 320..=323 => {
                ShowWindow(h, SW_HIDE);
                SetTimer(self.host, id as usize, 3000, None);
            }
            113 => {
                let exe = text_at(h, 201);
                let path = text_at(h, 204);
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
                self.apply_config(c)?;
                self.fill_rule(None);
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
            404 => self.restore_all(),
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
        Ok(())
    }
    pub(super) unsafe fn capture(&mut self, id: usize) {
        KillTimer(self.host, id);
        let target = GetForegroundWindow();
        let Some(u) = self.ui.as_ref() else {
            return;
        };
        let h = u.window;
        if !eligible(target) {
            error("未选中可操作的窗口，请重新捕获");
        } else if id == 112 {
            if let Some((_, _, p)) = process(target) {
                set_text(h, 204, &p);
                if text_at(h, 201).is_empty() {
                    set_text(h, 201, &p);
                }
                set_text(h, 205, &class(target));
                set_text(h, 206, &title(target));
                if text_at(h, 200).is_empty() {
                    set_text(h, 200, &title(target));
                }
            }
        } else if id == 401 {
            match self.track(target) {
                Ok(n) => self.ui.as_mut().unwrap().target = Some(self.records[n].token.clone()),
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
                    if let Err(e) = self.journal() {
                        error(&e);
                    }
                }
                Err(e) => error(&e),
            }
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
