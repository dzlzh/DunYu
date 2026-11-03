use super::*;
use dunyu::rest::{Phase, RestConfig, RestScheduler};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
pub(super) const ACTION: u32 = WM_APP + 6;
pub(super) const TIMER_ID: usize = 901;
const ANIMATION_TIMER: usize = 902;
const REST_CLASS: &str = "DunYu.Rest.1";
pub(super) struct Runtime {
    pub scheduler: RestScheduler,
    windows: Vec<HWND>,
    generation: usize,
    shown: Option<(Phase, bool, bool)>,
    pub locked: bool,
    pub suspended: bool,
    preview_until: u64,
    motion_start: u64,
    natural: bool,
    display_failed: bool,
    dismissed: Option<bool>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            scheduler: RestScheduler::new(RestConfig::default(), unsafe { GetTickCount64() }),
            windows: vec![],
            generation: 0,
            shown: None,
            locked: false,
            suspended: false,
            preview_until: 0,
            motion_start: 0,
            natural: false,
            display_failed: false,
            dismissed: None,
        }
    }
}
unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => 1,
        WM_TIMER if w == ANIMATION_TIMER => {
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(h, &mut r);
            let layout = super::swim::Layout::new(r.right as f32, r.bottom as f32);
            let area = layout.rect(586., 186., 500., 500.);
            let dirty = RECT {
                left: area[0].floor() as i32,
                top: area[1].floor() as i32,
                right: (area[0] + area[2]).ceil() as i32,
                bottom: (area[1] + area[3]).ceil() as i32,
            };
            InvalidateRect(h, &dirty, 0);
            0
        }
        WM_SETTINGCHANGE => {
            update_animation(h);
            InvalidateRect(h, null(), 0);
            0
        }
        WM_PAINT | WM_PRINTCLIENT => {
            paint(h, m, w);
            0
        }
        WM_CTLCOLORSTATIC => {
            theme::canvas(h);
            let dc = w as HDC;
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(
                dc,
                pond_color(
                    if GetDlgCtrlID(l as HWND) == 14 {
                        theme::MUTED
                    } else {
                        theme::INK
                    },
                    COLOR_WINDOWTEXT,
                ),
            );
            SetBkColor(dc, pond_color(theme::BACKGROUND, COLOR_WINDOW));
            SetDCBrushColor(dc, pond_color(theme::BACKGROUND, COLOR_WINDOW));
            let mut origin = POINT { x: 0, y: 0 };
            MapWindowPoints(l as HWND, h, &mut origin, 1);
            SetBrushOrgEx(dc, -origin.x, -origin.y, null_mut());
            theme::background_brush(dc) as isize
        }

        WM_COMMAND | WM_CLOSE => {
            let action = if m == WM_CLOSE { 4 } else { w & 0xffff };
            PostMessageW(
                GetPropW(h, wide("DunYu.RestHost").as_ptr()),
                ACTION,
                action,
                GetWindowLongPtrW(h, GWLP_USERDATA),
            );
            0
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            PostMessageW(
                GetPropW(h, wide("DunYu.RestHost").as_ptr()),
                WM_DISPLAYCHANGE,
                0,
                0,
            );
            0
        }
        WM_NCDESTROY => {
            KillTimer(h, ANIMATION_TIMER);
            let fonts: Vec<_> = ["DunYu.RestFont", "DunYu.RestTitle", "DunYu.RestTimer"]
                .iter()
                .map(|name| RemovePropW(h, wide(name).as_ptr()) as HFONT)
                .collect();
            let result = DefWindowProcW(h, m, w, l);
            for font in fonts {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            result
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
pub(super) unsafe fn motion_enabled() -> bool {
    let mut enabled: BOOL = 1;
    SystemParametersInfoW(
        SPI_GETCLIENTAREAANIMATION,
        0,
        &mut enabled as *mut _ as _,
        0,
    );
    let mut hc: HIGHCONTRASTW = std::mem::zeroed();
    hc.cbSize = std::mem::size_of_val(&hc) as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0);
    enabled != 0 && hc.dwFlags & HCF_HIGHCONTRASTON == 0
}
unsafe fn update_animation(h: HWND) {
    KillTimer(h, ANIMATION_TIMER);
    if !GetPropW(h, wide("DunYu.Full").as_ptr()).is_null() && motion_enabled() {
        SetTimer(h, ANIMATION_TIMER, 33, None);
    }
}
unsafe fn pond_color(value: u32, system: i32) -> u32 {
    let mut hc: HIGHCONTRASTW = std::mem::zeroed();
    hc.cbSize = std::mem::size_of_val(&hc) as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0);
    if hc.dwFlags & HCF_HIGHCONTRASTON != 0 {
        GetSysColor(system)
    } else {
        (value & 255) << 16 | value & 0xff00 | value >> 16
    }
}
unsafe fn paint(h: HWND, message: u32, w: WPARAM) {
    use windows_sys::Win32::Graphics::GdiPlus::*;
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = if message == WM_PAINT {
        BeginPaint(h, &mut ps)
    } else {
        w as HDC
    };
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    let dirty = if message == WM_PAINT { ps.rcPaint } else { r };
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(
        dc,
        (dirty.right - dirty.left).max(1),
        (dirty.bottom - dirty.top).max(1),
    );
    if memory.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !memory.is_null() {
            DeleteDC(memory);
        }
        if message == WM_PAINT {
            EndPaint(h, &ps);
        }
        return;
    }
    let old = SelectObject(memory, bitmap);
    let brush = CreateSolidBrush(pond_color(theme::BACKGROUND, COLOR_WINDOW));
    let buffer = RECT {
        left: 0,
        top: 0,
        right: dirty.right - dirty.left,
        bottom: dirty.bottom - dirty.top,
    };
    SetBrushOrgEx(memory, -dirty.left, -dirty.top, null_mut());
    theme::canvas(h);
    theme::paper(memory, buffer);
    DeleteObject(brush);
    let full = !GetPropW(h, wide("DunYu.Full").as_ptr()).is_null();
    let scale = GetPropW(h, wide("DunYu.LayoutDpi").as_ptr()) as usize as f32 / 96.;
    let mut g = null_mut();
    if GdipCreateFromHDC(memory, &mut g) == 0 {
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipTranslateWorldTransform(g, -dirty.left as f32, -dirty.top as f32, MatrixOrderPrepend);
        let argb = |hex, sys| {
            let c = pond_color(hex, sys);
            0xff000000 | (c & 255) << 16 | c & 0xff00 | c >> 16
        };
        let mut pen = null_mut();
        GdipCreatePen1(
            argb(theme::POND_RIPPLE, COLOR_WINDOWTEXT),
            2. * scale,
            UnitPixel,
            &mut pen,
        );
        let duration = GetPropW(h, wide("DunYu.Duration").as_ptr()) as usize as u64;
        let left = GetPropW(h, wide("DunYu.Left").as_ptr()) as usize as u64;
        let stamp = GetPropW(h, wide("DunYu.Stamp").as_ptr()) as usize as u64;
        let moving = motion_enabled();
        let elapsed = if full && moving {
            GetTickCount64().saturating_sub(stamp).min(1000)
        } else {
            0
        };
        let progress = super::swim::progress(duration, left, elapsed);
        let percent = (progress * 100.).clamp(0., 100.);
        let layout = super::swim::Layout::new(r.right as f32, r.bottom as f32);
        if full {
            let [x, y, d, _] = layout.rect(661., 261., 350., 350.);
            GdipSetPenColor(pen, argb(theme::INK, COLOR_WINDOWTEXT));
            GdipSetPenWidth(pen, 2. * scale);
            if theme::high_contrast() {
                GdipDrawEllipse(g, pen, x, y, d, d);
            } else {
                super::assets::ring(g, [x, y, d, d]);
            }
            GdipSetPenColor(pen, argb(theme::ACCENT, COLOR_HIGHLIGHT));
            GdipSetPenWidth(pen, 6. * layout.scale);
            if percent > 0. {
                GdipDrawArc(g, pen, x, y, d, d, -90., percent * 3.6);
            }
        } else {
            let x = r.right as f32 * 109. / 1561.;
            let y = r.bottom as f32 * 640. / 1008.;
            GdipDrawLine(g, pen, x, y, r.right as f32 - x, y);
            GdipSetPenColor(pen, argb(theme::ACCENT, COLOR_HIGHLIGHT));
            GdipDrawLine(
                g,
                pen,
                x,
                y,
                x + (r.right as f32 - 2. * x) * percent / 100.,
                y,
            );
        }
        if full {
            rest_art::koi(
                g,
                (r.right as f32 / 2., layout.y + 436. * layout.scale),
                layout.scale,
                progress,
                GetTickCount64().saturating_sub(GetPropW(h, wide("DunYu.MotionStart").as_ptr()) as usize as u64),
                moving,
            );
        }
        GdipDeletePen(pen);
        GdipDeleteGraphics(g);
    }
    if full && !theme::high_contrast() {
        let l = super::swim::Layout::new(r.right as f32, r.bottom as f32);
        let [x, y, w, h] = l.rect(699., 220., 140., 10.);
        super::assets::stroke(
            memory,
            RECT {
                left: x as i32 - dirty.left,
                top: y as i32 - dirty.top,
                right: (x + w) as i32 - dirty.left,
                bottom: (y + h) as i32 - dirty.top,
            },
            false,
        );
    }
    let logo_scale = if full {
        layout_scale(r.right, r.bottom)
    } else {
        r.right as f32 / 1561.
    };
    let (logo_x, logo_y, logo_size) = if full {
        (30., 27., 50.)
    } else {
        (80., 60., 100.)
    };
    let logo = LoadImageW(
        GetModuleHandleW(null()),
        101usize as *const u16,
        IMAGE_ICON,
        (logo_size * logo_scale) as i32,
        (logo_size * logo_scale) as i32,
        LR_SHARED,
    ) as HICON;
    if !logo.is_null() {
        DrawIconEx(
            memory,
            (logo_x * logo_scale) as i32 - dirty.left,
            (logo_y * logo_scale) as i32 - dirty.top,
            logo,
            (logo_size * logo_scale) as i32,
            (logo_size * logo_scale) as i32,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }
    BitBlt(
        dc,
        dirty.left,
        dirty.top,
        buffer.right,
        buffer.bottom,
        memory,
        0,
        0,
        SRCCOPY,
    );
    SelectObject(memory, old);
    DeleteObject(bitmap);
    DeleteDC(memory);
    if message == WM_PAINT {
        EndPaint(h, &ps);
    }
}
unsafe extern "system" fn monitor(_: HMONITOR, _: HDC, r: *mut RECT, data: LPARAM) -> BOOL {
    (*(data as *mut Vec<RECT>)).push(*r);
    1
}
impl Runtime {
    pub unsafe fn close(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        for h in self.windows.drain(..) {
            DestroyWindow(h);
        }
        self.shown = None;
    }
    pub unsafe fn configure(&mut self, c: RestConfig, host: HWND) {
        if self.scheduler.config == c {
            return;
        }
        self.close();
        self.preview_until = 0;
        self.display_failed = false;
        self.dismissed = None;
        self.natural = false;
        self.scheduler = RestScheduler::new(c, GetTickCount64());
        if self.scheduler.config.active() {
            SetTimer(host, TIMER_ID, 1000, None);
        } else {
            KillTimer(host, TIMER_ID);
        }
    }
    pub unsafe fn action(&mut self, action: usize, generation: Option<usize>, host: HWND) {
        if generation.is_some_and(|g| g != self.generation) {
            return;
        }
        let now = GetTickCount64();
        if !self.scheduler.config.active() && action != 23 && self.preview_until == 0 {
            return;
        }
        if self.preview_until != 0 && matches!(action, 1..=4) {
            self.preview_until = 0;
            self.close();
            if !self.scheduler.config.active() {
                KillTimer(host, TIMER_ID);
            }
            return;
        }
        if action == 4 {
            self.dismissed = Some(self.scheduler.phase == Phase::Resting);
            self.close();
            return;
        }
        self.dismissed = None;
        match action {
            1 => self.scheduler.start(now),
            2 => self.scheduler.snooze(now),
            3 => self.scheduler.skip(now),
            20 => self.scheduler.pause(now, 30),
            21 => self.scheduler.pause(now, 60),
            22 => self.scheduler.restart(now),
            23 => {
                self.preview_until = now + 5000;
                SetTimer(host, TIMER_ID, 1000, None);
            }
            _ => return,
        }
        self.close();
        self.tick(host);
        if matches!(action, 1 | 23) {
            let mut cursor: POINT = std::mem::zeroed();
            GetCursorPos(&mut cursor);
            let selected = self
                .windows
                .iter()
                .find(|h| {
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(**h, &mut r);
                    cursor.x >= r.left
                        && cursor.x < r.right
                        && cursor.y >= r.top
                        && cursor.y < r.bottom
                })
                .or(self.windows.first());
            if let Some(h) = selected {
                SetForegroundWindow(*h);
                SetFocus(GetDlgItem(*h, 3));
            }
        }
    }
    pub unsafe fn status(&self) -> String {
        let s = &self.scheduler;
        let kind = if s.long() { "舒展" } else { "小憩" };
        let seconds = s.remaining_ms.div_ceil(1000);
        if self.display_failed {
            return "提醒窗口未能显示，正在重试".into();
        }
        if s.blocked() && matches!(s.phase, Phase::Due | Phase::Working | Phase::Snoozed) {
            return "全屏应用或系统暂缓中，恢复后再预告".into();
        }
        match s.phase {
            Phase::Disabled => "休憩未启用".into(),
            Phase::Paused => "提醒已暂停，到期后重新计时".into(),
            Phase::Due => format!("即将{kind}，等待显示预告"),
            Phase::Warning => format!(
                "{seconds}秒后{}",
                if s.auto_start() {
                    format!("自动开始{kind}")
                } else {
                    format!("提醒{kind}")
                }
            ),
            Phase::Awaiting => format!("{kind}时间到了 · 等待你开始"),
            Phase::Resting => format!("正在{kind} · {:02}:{:02}", seconds / 60, seconds % 60),
            _ => {
                let times = s.next_times().map(|time| {
                    time.map(|ms| {
                        format!(
                            "{:02}:{:02}",
                            ms.div_ceil(1000) / 60,
                            ms.div_ceil(1000) % 60
                        )
                    })
                    .unwrap_or("已关闭".into())
                });
                format!("小憩 {} · 舒展 {}", times[0], times[1])
            }
        }
    }
    pub unsafe fn tick(&mut self, host: HWND) {
        let now = GetTickCount64();
        let mut input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        let idle =
            GetLastInputInfo(&mut input) != 0 && (now as u32).wrapping_sub(input.dwTime) >= 300_000;
        let natural =
            idle && self.scheduler.config.natural_breaks && self.scheduler.phase != Phase::Resting;
        if natural && !self.natural {
            self.scheduler.natural_reset(now);
        }
        self.natural = natural;
        let mut state = QUNS_ACCEPTS_NOTIFICATIONS;
        let queried = SHQueryUserNotificationState(&mut state) >= 0;
        let away =
            self.locked || self.suspended || natural || (queried && state == QUNS_NOT_PRESENT);
        let blocked = self.scheduler.config.defer_fullscreen
            && self.scheduler.phase != Phase::Resting
            && queried
            && state != QUNS_ACCEPTS_NOTIFICATIONS;
        self.advance(host, now, away, blocked);
    }
    unsafe fn advance(&mut self, host: HWND, now: u64, away: bool, blocked: bool) {
        let previous = self.scheduler.phase;
        let previous_kind = self.scheduler.long();
        self.scheduler.tick(now, away, blocked);
        if previous_kind != self.scheduler.long() {
            self.dismissed = None;
        }
        if self.scheduler.show_due(now) || self.scheduler.phase == Phase::Resting {
            self.preview_until = 0;
        }
        let preview = self.preview_until > now;
        if self.preview_until != 0 && !preview {
            self.preview_until = 0;
            self.close();
            if !self.scheduler.config.active() {
                KillTimer(host, TIMER_ID);
            }
        }
        let group = match self.scheduler.phase {
            Phase::Due | Phase::Warning | Phase::Awaiting => Some(false),
            Phase::Resting => Some(true),
            _ => None,
        };
        if self.dismissed != group {
            self.dismissed = None;
        }
        if self.dismissed.is_some() && !preview {
            // A previously dismissed warning remains hidden after a short interruption.
            // Its countdown may resume without requiring a second visible card.
            self.scheduler.warning_presented(now);
        }
        let visible = if preview {
            !self.locked && !self.suspended
        } else {
            self.dismissed.is_none()
                && !away
                && (self.scheduler.phase == Phase::Resting || self.scheduler.show_due(now))
        };
        if !visible {
            if !self.windows.is_empty() {
                self.close();
            }
            return;
        }
        let phase = if preview {
            Phase::Resting
        } else {
            if self.scheduler.phase == Phase::Due {
                Phase::Warning
            } else {
                self.scheduler.phase
            }
        };
        let fullscreen = self.scheduler.config.fullscreen && phase == Phase::Resting;
        if self.shown != Some((phase, fullscreen, self.scheduler.long())) {
            self.close();
            self.shown = Some((phase, fullscreen, self.scheduler.long()));
            self.motion_start = GetTickCount64();
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.hInstance = GetModuleHandleW(null());
            wc.lpfnWndProc = Some(procedure);
            let name = wide(REST_CLASS);
            wc.lpszClassName = name.as_ptr();
            wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
            RegisterClassW(&wc);
            let mut bounds = vec![];
            if fullscreen {
                EnumDisplayMonitors(
                    null_mut(),
                    null(),
                    Some(monitor),
                    &mut bounds as *mut _ as isize,
                );
            } else {
                let mut point = POINT { x: 0, y: 0 };
                GetCursorPos(&mut point);
                let mut mi: MONITORINFO = std::mem::zeroed();
                mi.cbSize = std::mem::size_of_val(&mi) as u32;
                GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut mi);
                bounds.push(mi.rcWork);
            }
            for r in bounds {
                self.create(r, fullscreen, phase, host);
            }
        }
        self.display_failed = self.windows.is_empty();
        if self.display_failed {
            self.shown = None;
            return;
        }
        if !preview {
            self.scheduler.warning_presented(now);
        }
        if fullscreen && previous == Phase::Warning && self.scheduler.phase == Phase::Resting {
            let mut point: POINT = std::mem::zeroed();
            GetCursorPos(&mut point);
            let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
            if let Some(h) = self
                .windows
                .iter()
                .find(|h| MonitorFromWindow(**h, MONITOR_DEFAULTTONEAREST) == monitor)
            {
                SetForegroundWindow(*h);
                SetFocus(GetDlgItem(*h, 3));
            }
        }
        let seconds = if preview {
            (self.preview_until - now).div_ceil(1000)
        } else {
            self.scheduler.remaining_ms.div_ceil(1000)
        };
        let kind = if self.scheduler.long() {
            "舒展"
        } else {
            "小憩"
        };
        let heading = if preview {
            "遁鱼 · 休憩预览".into()
        } else if phase == Phase::Awaiting {
            format!("{kind}时间到了 · 只提醒")
        } else if phase == Phase::Warning {
            format!(
                "{kind}将至 · {}",
                if self.scheduler.auto_start() {
                    "到点自动休息"
                } else {
                    "由你决定何时开始"
                }
            )
        } else {
            format!("遁鱼 · {kind}")
        };
        let counter = if phase == Phase::Awaiting {
            "歇一会儿".into()
        } else if phase == Phase::Warning {
            format!("{seconds} 秒")
        } else {
            format!("{:02}:{:02}", seconds / 60, seconds % 60)
        };
        let body = if phase == Phase::Awaiting {
            "准备好后，点击现在休息。"
        } else if phase == Phase::Warning {
            "保存手头的工作，准备歇一会儿。"
        } else {
            [
                "看看远处，让视线歇一会儿。",
                "放松肩膀，换个舒服的坐姿。",
                "站起来走几步，喝口水。",
            ][(self.scheduler.round % 3) as usize]
        };
        let stamp = GetTickCount64();
        for h in &self.windows {
            SetWindowTextW(GetDlgItem(*h, 10), wide(&counter).as_ptr());
            SetWindowTextW(GetDlgItem(*h, 12), wide(&heading).as_ptr());
            SetWindowTextW(GetDlgItem(*h, 13), wide(body).as_ptr());
            let duration = if preview {
                5000
            } else if phase == Phase::Warning {
                30_000
            } else {
                self.scheduler.duration()
            };
            let left = if preview {
                self.preview_until - now
            } else {
                self.scheduler.remaining_ms
            };
            SetPropW(*h, wide("DunYu.Duration").as_ptr(), duration as HANDLE);
            SetPropW(*h, wide("DunYu.Left").as_ptr(), left as HANDLE);
            SetPropW(*h, wide("DunYu.Stamp").as_ptr(), stamp as HANDLE);
            InvalidateRect(*h, null(), 0);
        }
    }
    unsafe fn create(&mut self, r: RECT, full: bool, phase: Phase, host: HWND) {
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            wide(REST_CLASS).as_ptr(),
            wide("遁鱼 · 休憩").as_ptr(),
            WS_POPUP | WS_CLIPCHILDREN,
            r.left,
            r.top,
            1,
            1,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if h.is_null() {
            return;
        }
        SetWindowLongPtrW(h, GWLP_USERDATA, self.generation as isize);
        SetPropW(
            h,
            wide("DunYu.MotionStart").as_ptr(),
            self.motion_start as HANDLE,
        );
        SetPropW(h, wide("DunYu.RestHost").as_ptr(), host);
        let dpi = if full {
            GetDpiForWindow(h)
                .max(96)
                .min(((r.bottom - r.top).max(1) * 96 / 560) as u32)
                .min(((r.right - r.left).max(1) * 96 / 360) as u32)
                .max(48)
        } else {
            GetDpiForWindow(h).max(96)
        };
        SetPropW(h, wide("DunYu.LayoutDpi").as_ptr(), dpi as HANDLE);
        let px = |n: i32| n * dpi as i32 / 96;
        let width = if full {
            r.right - r.left
        } else {
            px(480).min(r.right - r.left)
        };
        let height = if full {
            r.bottom - r.top
        } else {
            px(310).min(r.bottom - r.top)
        };
        let x = if full {
            r.left
        } else {
            r.left + (r.right - r.left - width) / 2
        };
        let y = if full {
            r.top
        } else {
            r.top + px(24).min((r.bottom - r.top - height).max(0))
        };
        SetWindowPos(h, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
        SetPropW(h, wide("DunYu.Full").as_ptr(), usize::from(full) as HANDLE);
        if !full {
            let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, px(24), px(24));
            if SetWindowRgn(h, region, 0) == 0 {
                DeleteObject(region);
            }
        }
        let skin = theme::Theme::new();
        let make_font = |size: i32, name: &str| {
            let mut lf: LOGFONTW = std::mem::zeroed();
            lf.lfHeight = -px(size);
            let face = wide(if name == "DunYu.RestTitle" {
                super::fonts::FACE
            } else if name == "DunYu.RestTimer" && full {
                "Segoe UI Light"
            } else {
                super::fonts::FACE
            });
            lf.lfFaceName[..face.len()].copy_from_slice(&face);
            let font = CreateFontIndirectW(&lf);
            SetPropW(h, wide(name).as_ptr(), font);
            font
        };
        let font = make_font(16, "DunYu.RestFont");
        let heading_font = make_font(if full { 22 } else { 20 }, "DunYu.RestTitle");
        let timer_font = make_font(if full { 72 } else { 56 }, "DunYu.RestTimer");
        let top = if full {
            ((height - px(520)) / 2).max(px(16))
        } else {
            px(24)
        };
        let left = ((width - px(420)) / 2).max(px(16));
        let body_width = (width - left * 2).max(1);
        let timer_y = if full { 146 } else { 55 };
        let body_y = if full { 366 } else { 125 };
        for (id, offset, tall, f) in [
            (12, 0, 38, heading_font),
            (10, timer_y, if full { 100 } else { 64 }, timer_font),
            (13, body_y, 52, font),
        ] {
            let label = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | windows_sys::Win32::System::SystemServices::SS_CENTER,
                if id == 12 {
                    px(64)
                } else if full && id == 10 {
                    (width - px(210)) / 2
                } else {
                    left
                },
                top + px(offset),
                if id == 12 {
                    (width - px(128)).max(1)
                } else if full && id == 10 {
                    px(210)
                } else {
                    body_width
                },
                px(tall),
                h,
                id as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(label, WM_SETFONT, f as usize, 0);
        }
        // Reserve a separate header lane for the logo, title and close control.
        let close = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("×").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            width - px(56),
            px(16),
            px(40),
            px(40),
            h,
            4 as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(close, WM_SETFONT, heading_font as usize, 0);
        theme::attach(close, theme::Role::Button, skin.clone());
        theme::scene(close, dpi, vec![], vec![]);
        theme::button_style(close, false, theme::BACKGROUND);
        let buttons: Vec<(usize, &str)> = if matches!(phase, Phase::Warning | Phase::Awaiting) {
            vec![
                (1, "现在休息"),
                (
                    2,
                    if self.scheduler.long() {
                        "稍后5分钟"
                    } else {
                        "稍后2分钟"
                    },
                ),
                (3, "跳过本轮"),
            ]
        } else {
            vec![(3, "结束休息")]
        };
        let bw = ((body_width - px(8) * (buttons.len() as i32 - 1)) / buttons.len() as i32).max(1);
        for (i, (id, text)) in buttons.iter().enumerate() {
            let b = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                if full {
                    (width - px(180)) / 2
                } else {
                    left + i as i32 * (bw + px(8))
                },
                top + px(if full { 430 } else { 210 }),
                if full { px(180) } else { bw },
                px(48),
                h,
                *id as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(b, WM_SETFONT, font as usize, 0);
            theme::attach(b, theme::Role::Button, skin.clone());
            theme::scene(b, dpi, vec![], vec![]);
            theme::button_style(b, *id == 1, theme::BACKGROUND);
        }

        if full {
            let layout = super::swim::Layout::new(width as f32, height as f32);
            let put = |id, area: [f32; 4]| {
                MoveWindow(
                    GetDlgItem(h, id),
                    area[0].round() as i32,
                    area[1].round() as i32,
                    area[2].round() as i32,
                    area[3].round() as i32,
                    0,
                );
            };
            for (id, area) in [
                (12, [510., 165., 652., 64.]),
                (10, [686., 374., 300., 128.]),
                (13, [500., 647., 672., 54.]),
                (3, [699., 737., 274., 64.]),
            ] {
                put(id, layout.rect(area[0], area[1], area[2], area[3]));
            }
            put(
                4,
                [
                    width as f32 - 76. * layout.scale,
                    28. * layout.scale,
                    48. * layout.scale,
                    48. * layout.scale,
                ],
            );
            for (id, size, name) in [
                (12, 36., "DunYu.RestTitle"),
                (10, 96., "DunYu.RestTimer"),
                (13, 28., "DunYu.RestFont"),
            ] {
                let mut lf: LOGFONTW = std::mem::zeroed();
                lf.lfHeight = -(size * layout.scale).round() as i32;
                let face = wide(if id == 10 {
                    "Segoe UI Light"
                } else {
                    super::fonts::FACE
                });
                lf.lfFaceName[..face.len()].copy_from_slice(&face);
                let font = CreateFontIndirectW(&lf);
                let old = GetPropW(h, wide(name).as_ptr());
                SetPropW(h, wide(name).as_ptr(), font);
                SendMessageW(GetDlgItem(h, id), WM_SETFONT, font as usize, 0);
                if id == 13 {
                    SendMessageW(GetDlgItem(h, 3), WM_SETFONT, font as usize, 0);
                }
                if id == 12 {
                    SendMessageW(GetDlgItem(h, 4), WM_SETFONT, font as usize, 0);
                }
                DeleteObject(old);
            }
            theme::outline(GetDlgItem(h, 3));
        }
        if !full && buttons.len() == 1 {
            let sx = width as f32 / 1561.;
            let sy = height as f32 / 1008.;
            for (id, x, y, w, tall) in [
                (12, 390., 60., 780., 100.),
                (10, 450., 235., 660., 190.),
                (13, 180., 455., 1200., 110.),
                (3, 80., 726., 1402., 144.),
                (4, 1406., 53., 102., 102.),
            ] {
                MoveWindow(
                    GetDlgItem(h, id),
                    (x * sx).round() as i32,
                    (y * sy).round() as i32,
                    (w * sx).round() as i32,
                    (tall * sy).round() as i32,
                    0,
                );
            }
            SendMessageW(GetDlgItem(h, 3), WM_SETFONT, heading_font as usize, 0);
            theme::outline(GetDlgItem(h, 3));
        }
        if !full {
            fit_card_counter(h, phase);
        }
        update_animation(h);
        ShowWindow(h, SW_SHOWNOACTIVATE);
        self.windows.push(h);
    }
    pub unsafe fn filter(msg: &MSG) -> bool {
        let root = GetAncestor(msg.hwnd, GA_ROOT);
        if class(root) != REST_CLASS {
            return false;
        }
        if msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE as usize {
            SendMessageW(root, WM_CLOSE, 0, 0);
            return true;
        }
        IsDialogMessageW(root, msg) != 0
    }
}

// Fit the complete GDI font line, including the CJK font's leading and descent.
// The timer lane is bounded by the title and body so a larger line cannot overlap them.
unsafe fn fit_card_counter(h: HWND, phase: Phase) {
    let label = GetDlgItem(h, 10);
    let dpi = GetPropW(h, wide("DunYu.LayoutDpi").as_ptr()) as usize as i32;
    let gap = (6 * dpi / 96).max(2);
    let bounds = |id| {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(GetDlgItem(h, id), &mut r);
        MapWindowPoints(null_mut(), h, &mut r as *mut _ as *mut POINT, 2);
        r
    };
    let title = bounds(12);
    let body = bounds(13);
    let timer = bounds(10);
    let top = title.bottom + gap;
    let bottom = (body.top - gap).max(top + 1);
    let available_height = (bottom - top - 2).max(1);
    let available_width = (timer.right - timer.left - gap * 2).max(1);
    let sample = wide(match phase {
        Phase::Awaiting => "歇一会儿",
        Phase::Warning => "30 秒",
        _ => "000:00",
    });
    let key = wide("DunYu.RestTimer");
    let original = GetPropW(h, key.as_ptr()) as HFONT;
    let mut font = original;
    let mut lf: LOGFONTW = std::mem::zeroed();
    GetObjectW(
        font,
        std::mem::size_of_val(&lf) as i32,
        &mut lf as *mut _ as _,
    );
    let dc = GetDC(label);
    let line = loop {
        let old = SelectObject(dc, font);
        let mut metrics: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut metrics);
        let mut text: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(dc, sample.as_ptr(), (sample.len() - 1) as i32, &mut text);
        SelectObject(dc, old);
        if (metrics.tmHeight <= available_height && text.cx <= available_width)
            || lf.lfHeight.abs() <= 1
        {
            break metrics.tmHeight;
        }
        lf.lfHeight = -(lf.lfHeight.abs() - 1);
        let next = CreateFontIndirectW(&lf);
        if next.is_null() {
            break metrics.tmHeight;
        }
        if font != original {
            DeleteObject(font);
        }
        font = next;
    };
    ReleaseDC(label, dc);
    if font != original {
        SendMessageW(label, WM_SETFONT, font as usize, 0);
        SetPropW(h, key.as_ptr(), font);
        DeleteObject(original);
    }
    let tall = (line + 2).min(bottom - top);
    SetWindowLongPtrW(
        label,
        GWL_STYLE,
        GetWindowLongPtrW(label, GWL_STYLE)
            | windows_sys::Win32::System::SystemServices::SS_CENTERIMAGE as isize,
    );
    MoveWindow(
        label,
        timer.left,
        top + (bottom - top - tall) / 2,
        timer.right - timer.left,
        tall,
        0,
    );
}

fn layout_scale(width: i32, height: i32) -> f32 {
    (width as f32 / 1672.).min(height as f32 / 941.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_isolated_and_fullscreen_uses_each_monitor() {
        unsafe {
            windows_sys::Win32::UI::Controls::InitCommonControls();
            let host = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("rest-test").as_ptr(),
                0,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!host.is_null());
            let mut runtime = Runtime::default();
            runtime.action(23, None, host);
            assert_eq!(runtime.windows.len(), 1);
            assert_eq!(runtime.scheduler.phase, Phase::Disabled);
            let card = runtime.windows[0];
            let dpi = GetPropW(card, wide("DunYu.LayoutDpi").as_ptr()) as usize as i32;
            let mut title: RECT = std::mem::zeroed();
            let mut close: RECT = std::mem::zeroed();
            GetWindowRect(GetDlgItem(card, 12), &mut title);
            GetWindowRect(GetDlgItem(card, 4), &mut close);
            let mut origin = POINT { x: 0, y: 0 };
            ClientToScreen(card, &mut origin);
            assert!(title.left >= origin.x + 64 * dpi / 96);
            assert!(title.right <= close.left);
            let counter = GetDlgItem(card, 10);
            let mut counter_bounds: RECT = std::mem::zeroed();
            GetClientRect(counter, &mut counter_bounds);
            assert!(
                counter_bounds.bottom >= theme::font_line_height(counter),
                "card countdown line clipped"
            );
            assert!(
                counter_bounds.right >= theme::label_size(counter).cx,
                "card countdown width clipped"
            );
            let mut counter_screen: RECT = std::mem::zeroed();
            let mut body_screen: RECT = std::mem::zeroed();
            GetWindowRect(counter, &mut counter_screen);
            GetWindowRect(GetDlgItem(card, 13), &mut body_screen);
            assert!(counter_screen.top >= title.bottom && counter_screen.bottom <= body_screen.top);

            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                std::fs::create_dir_all("target/ui-preview").unwrap();
                theme::capture(runtime.windows[0], "target/ui-preview/rest-preview.bmp").unwrap();
            }
            for phase in [Phase::Warning, Phase::Awaiting] {
                let mut sample = Runtime::default();
                sample.create(
                    RECT {
                        left: 0,
                        top: 0,
                        right: 1920,
                        bottom: 1080,
                    },
                    false,
                    phase,
                    host,
                );
                let popup = sample.windows[0];
                let label = GetDlgItem(popup, 10);
                SetWindowTextW(
                    label,
                    wide(if phase == Phase::Warning {
                        "30 秒"
                    } else {
                        "歇一会儿"
                    })
                    .as_ptr(),
                );
                let mut bounds: RECT = std::mem::zeroed();
                GetClientRect(label, &mut bounds);
                assert!(bounds.bottom >= theme::font_line_height(label));
                assert!(bounds.right >= theme::label_size(label).cx);
                sample.close();
            }
            let old = runtime.generation;
            runtime.close();
            runtime.action(1, Some(old), host);
            assert!(runtime.windows.is_empty());
            runtime.configure(
                RestConfig {
                    fullscreen: true,
                    ..Default::default()
                },
                host,
            );
            runtime.action(23, None, host);
            let mut expected: Vec<RECT> = vec![];
            EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(monitor),
                &mut expected as *mut _ as isize,
            );
            assert_eq!(runtime.windows.len(), expected.len());
            for (h, expected) in runtime.windows.iter().zip(&expected) {
                let mut actual: RECT = std::mem::zeroed();
                GetWindowRect(*h, &mut actual);
                if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() && *h == runtime.windows[0] {
                    KillTimer(*h, ANIMATION_TIMER);
                    SetPropW(
                        *h,
                        wide("DunYu.Stamp").as_ptr(),
                        (GetTickCount64() + 1000) as HANDLE,
                    );
                    theme::capture(*h, "target/ui-preview/rest-fullscreen.bmp").unwrap();
                }
                assert_eq!(
                    (actual.left, actual.top, actual.right, actual.bottom),
                    (expected.left, expected.top, expected.right, expected.bottom)
                );
            }
            runtime.action(3, Some(runtime.generation), host);
            assert!(runtime.windows.is_empty());
            assert_eq!(runtime.scheduler.phase, Phase::Disabled);
            runtime.configure(
                RestConfig {
                    enabled: true,
                    fullscreen: true,
                    natural_breaks: false,
                    defer_fullscreen: false,
                    ..Default::default()
                },
                host,
            );
            let now = GetTickCount64() + 1_200_000;
            runtime.advance(host, now, false, false);
            assert_eq!(runtime.scheduler.phase, Phase::Warning);
            assert_eq!(runtime.windows.len(), 1);
            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                theme::capture(runtime.windows[0], "target/ui-preview/rest-warning.bmp").unwrap();
            }
            let remaining = runtime.scheduler.remaining_ms;
            let escape = MSG {
                hwnd: runtime.windows[0],
                message: WM_KEYDOWN,
                wParam: VK_ESCAPE as usize,
                ..std::mem::zeroed()
            };
            assert!(Runtime::filter(&escape));
            let mut dismiss: MSG = std::mem::zeroed();
            assert_ne!(
                PeekMessageW(&mut dismiss, host, ACTION, ACTION, PM_REMOVE),
                0
            );
            assert_eq!(dismiss.wParam, 4);
            runtime.action(dismiss.wParam, Some(dismiss.lParam as usize), host);
            assert!(runtime.windows.is_empty());
            assert_eq!(runtime.scheduler.phase, Phase::Warning);
            assert_eq!(runtime.scheduler.remaining_ms, remaining);
            assert_eq!(runtime.scheduler.round, 0);
            runtime.advance(host, now + 1000, false, false);
            assert!(runtime.windows.is_empty());
            runtime.advance(host, now + 30_000, false, false);
            assert_eq!(runtime.scheduler.phase, Phase::Resting);
            assert_eq!(runtime.windows.len(), expected.len());
            if std::env::var_os("DUNYU_UI_SCREENSHOTS").is_some() {
                theme::capture(runtime.windows[0], "target/ui-preview/rest-fullscreen.bmp")
                    .unwrap();
            }
            let prior_phase = runtime.scheduler.phase;
            let prior_left = runtime.scheduler.remaining_ms;
            let prior_round = runtime.scheduler.round;
            SendMessageW(runtime.windows[0], WM_TIMER, ANIMATION_TIMER, 0);
            UpdateWindow(runtime.windows[0]);
            assert_eq!(runtime.scheduler.phase, prior_phase);
            assert_eq!(runtime.scheduler.remaining_ms, prior_left);
            assert_eq!(runtime.scheduler.round, prior_round);
            let close = GetDlgItem(runtime.windows[0], 4);
            assert!(!close.is_null());
            SendMessageW(close, BM_CLICK, 0, 0);
            let mut command: MSG = std::mem::zeroed();
            assert_ne!(
                PeekMessageW(&mut command, host, ACTION, ACTION, PM_REMOVE),
                0
            );
            assert_eq!(command.wParam, 4);
            runtime.action(command.wParam, Some(command.lParam as usize), host);
            assert!(runtime.windows.is_empty());
            assert_eq!(runtime.scheduler.phase, Phase::Resting);
            assert_eq!(runtime.scheduler.round, 0);
            runtime.advance(host, now + 31_000, false, false);
            assert!(runtime.windows.is_empty());
            runtime.advance(host, now + 50_000, false, false);
            assert_eq!(runtime.scheduler.phase, Phase::Working);
            assert_eq!(runtime.scheduler.round, 1);
            runtime.configure(
                RestConfig {
                    enabled: true,
                    short_auto_start: false,
                    natural_breaks: false,
                    defer_fullscreen: false,
                    ..Default::default()
                },
                host,
            );
            let later = GetTickCount64() + 1_200_000;
            runtime.advance(host, later, false, false);
            runtime.action(4, Some(runtime.generation), host);
            runtime.advance(host, later + 30_000, false, false);
            assert_eq!(runtime.scheduler.phase, Phase::Awaiting);
            assert_eq!(runtime.scheduler.round, 0);
            assert!(runtime.windows.is_empty());
            runtime.advance(host, later + 60_000, false, false);
            assert!(runtime.windows.is_empty());
            runtime.action(3, None, host);
            assert_eq!(runtime.scheduler.round, 1);
            runtime.close();
            KillTimer(host, TIMER_ID);
            DestroyWindow(host);
        }
    }
}
