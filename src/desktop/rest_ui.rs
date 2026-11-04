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
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            let cache = GetPropW(h, wide("DunYu.BackBuffer").as_ptr()) as *mut RestBuffers;
            if !cache.is_null() {
                (*cache).key = None;
            }
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
            let buffer = RemovePropW(h, wide("DunYu.BackBuffer").as_ptr()) as *mut RestBuffers;
            if !buffer.is_null() {
                drop(Box::from_raw(buffer));
            }
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
#[derive(Default)]
struct RestBuffers {
    frame: Option<super::buffering::BackBuffer>,
    backdrop: Option<super::buffering::BackBuffer>,
    key: Option<(i32, i32)>,
}
fn animation_rect(r: RECT) -> RECT {
    let l = super::swim::Layout::new(r.right as f32, r.bottom as f32);
    let a = l.rect(586., 186., 500., 500.);
    RECT {
        left: a[0].floor() as i32,
        top: a[1].floor() as i32,
        right: (a[0] + a[2]).ceil() as i32,
        bottom: (a[1] + a[3]).ceil() as i32,
    }
}
// Build at the same device coordinates as a full frame, then crop. Rendering
// into a translated small bitmap changes GDI+ sampling at fractional scales.
unsafe fn prepare_backdrop(dc: HDC, r: RECT, cache: &mut RestBuffers) -> Option<HDC> {
    use windows_sys::Win32::Graphics::GdiPlus::*;
    let area = animation_rect(r);
    let memory = super::buffering::BackBuffer::prepare(
        &mut cache.backdrop,
        dc,
        area.right - area.left,
        area.bottom - area.top,
    )?;
    if cache.key != Some((r.right, r.bottom)) {
        #[cfg(test)]
        let _build = super::ui::diagnostics::measure("paint.rest.static_build");
        // dc is the existing full-size offscreen frame, before dirty clipping.
        // Only paper and ring enter the cache; dynamic layers are drawn later.
        theme::paper(dc, r);
        let mut g = null_mut();
        if GdipCreateFromHDC(dc, &mut g) != 0 {
            return None;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        let l = super::swim::Layout::new(r.right as f32, r.bottom as f32);
        super::assets::ring(g, l.rect(661., 261., 350., 350.));
        GdipDeleteGraphics(g);
        if BitBlt(
            memory,
            0,
            0,
            area.right - area.left,
            area.bottom - area.top,
            dc,
            area.left,
            area.top,
            SRCCOPY,
        ) == 0
        {
            return None;
        }
        cache.key = Some((r.right, r.bottom));
    }
    Some(memory)
}
fn clipped_dirty(dirty: RECT, client: RECT) -> Option<RECT> {
    let r = RECT {
        left: dirty.left.max(client.left),
        top: dirty.top.max(client.top),
        right: dirty.right.min(client.right),
        bottom: dirty.bottom.min(client.bottom),
    };
    (r.left < r.right && r.top < r.bottom).then_some(r)
}
#[cfg(test)]
thread_local! {
    static PAINT_CLOCK: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
    static PAINT_DIRTY: std::cell::Cell<Option<RECT>> = const { std::cell::Cell::new(None) };
    static PAINT_MOVING: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
unsafe fn paint_clock() -> u64 {
    #[cfg(test)]
    if let Some(t) = PAINT_CLOCK.get() {
        return t;
    }
    GetTickCount64()
}
unsafe fn paint(h: HWND, message: u32, w: WPARAM) {
    #[cfg(test)]
    let _measurement = super::ui::diagnostics::measure("paint");
    use windows_sys::Win32::Graphics::GdiPlus::*;
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = if message == WM_PAINT {
        BeginPaint(h, &mut ps)
    } else {
        w as HDC
    };
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    #[cfg(test)]
    let _role = super::ui::diagnostics::measure("paint.rest");
    let dirty = if message == WM_PAINT { ps.rcPaint } else { r };
    #[cfg(test)]
    let dirty = PAINT_DIRTY.get().unwrap_or(dirty);
    let Some(dirty) = clipped_dirty(dirty, r) else {
        if message == WM_PAINT {
            EndPaint(h, &ps);
        }
        return;
    };
    let cache = GetPropW(h, wide("DunYu.BackBuffer").as_ptr()) as *mut RestBuffers;
    let memory = if cache.is_null() {
        None
    } else {
        super::buffering::BackBuffer::prepare(&mut (*cache).frame, dc, r.right, r.bottom)
    };
    let buffered = memory.is_some();
    let memory = memory.unwrap_or(dc);
    let saved = SaveDC(memory);
    SetBrushOrgEx(memory, 0, 0, null_mut());
    theme::canvas(h);
    let use_cache = true;
    #[cfg(test)]
    let use_cache = use_cache && !REFERENCE.get();
    let backdrop = if use_cache
        && buffered
        && !cache.is_null()
        && !GetPropW(h, wide("DunYu.Full").as_ptr()).is_null()
        && !theme::high_contrast()
    {
        prepare_backdrop(memory, r, &mut *cache)
    } else {
        None
    };
    // Keep offscreen rasterization identical to a full frame. A small graphics
    // clip changes GDI+ image sampling even when the world coordinates match.
    // Only the final copy is limited to dirty; every subsequently submitted
    // region has its background restored first, so off-region scratch pixels
    // are never used as an authoritative previous frame.
    let render_clip = if buffered { r } else { dirty };
    IntersectClipRect(
        memory,
        render_clip.left,
        render_clip.top,
        render_clip.right,
        render_clip.bottom,
    );
    theme::paper(memory, dirty);
    let static_cached = backdrop.is_some_and(|backdrop| {
        let area = animation_rect(r);
        let Some(clip) = clipped_dirty(dirty, area) else {
            return true;
        };
        #[cfg(test)]
        let _copy = super::ui::diagnostics::measure("paint.rest.static_copy");
        BitBlt(
            memory,
            clip.left,
            clip.top,
            clip.right - clip.left,
            clip.bottom - clip.top,
            backdrop,
            clip.left - area.left,
            clip.top - area.top,
            SRCCOPY,
        ) != 0
    });
    if backdrop.is_some() && !static_cached {
        // A failed cache copy must not leave a partially composited ring behind.
        theme::paper(memory, dirty);
    }
    let full = !GetPropW(h, wide("DunYu.Full").as_ptr()).is_null();
    let scale = GetPropW(h, wide("DunYu.LayoutDpi").as_ptr()) as usize as f32 / 96.;
    let mut g = null_mut();
    if GdipCreateFromHDC(memory, &mut g) == 0 {
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipSetClipRectI(
            g,
            render_clip.left,
            render_clip.top,
            render_clip.right - render_clip.left,
            render_clip.bottom - render_clip.top,
            CombineModeIntersect,
        );
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
        #[cfg(test)]
        let moving = PAINT_MOVING.get().unwrap_or(moving);
        let elapsed = if full && moving {
            paint_clock().saturating_sub(stamp).min(1000)
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
            } else if !static_cached {
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
                paint_clock().saturating_sub(GetPropW(h, wide("DunYu.MotionStart").as_ptr()) as usize as u64),
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
                left: x as i32,
                top: y as i32,
                right: (x + w) as i32,
                bottom: (y + h) as i32,
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
            (logo_x * logo_scale) as i32,
            (logo_y * logo_scale) as i32,
            logo,
            (logo_size * logo_scale) as i32,
            (logo_size * logo_scale) as i32,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }
    if buffered {
        BitBlt(
            dc,
            dirty.left,
            dirty.top,
            dirty.right - dirty.left,
            dirty.bottom - dirty.top,
            memory,
            dirty.left,
            dirty.top,
            SRCCOPY,
        );
    }
    RestoreDC(memory, saved);
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
        #[cfg(test)]
        let _measurement = super::ui::diagnostics::measure("rest.close");
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
            let skin = theme::Theme::new();
            for r in bounds {
                self.create(r, fullscreen, phase, host, skin.clone());
            }
        }
        self.display_failed = self.windows.is_empty();
        if self.display_failed {
            self.shown = None;
            return;
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
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(*h, &mut client);
            let mut damage = if fullscreen {
                animation_rect(client)
            } else {
                client
            };
            for (id, text) in [(10, counter.as_str()), (12, heading.as_str()), (13, body)] {
                let child = GetDlgItem(*h, id);
                if title(child) != text {
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(child, &mut r);
                    MapWindowPoints(null_mut(), *h, &mut r as *mut _ as *mut POINT, 2);
                    damage.left = damage.left.min(r.left);
                    damage.top = damage.top.min(r.top);
                    damage.right = damage.right.max(r.right);
                    damage.bottom = damage.bottom.max(r.bottom);
                }
            }
            // WM_SETREDRAW temporarily clears WS_VISIBLE. Keep fullscreen
            // windows visible throughout each tick so the shell retains their
            // fullscreen coverage instead of exposing the taskbar.
            let _redraw = (!fullscreen).then(|| super::buffering::RedrawBatch::region(*h, damage));
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
            for (id, text) in [(10, counter.as_str()), (12, heading.as_str()), (13, body)] {
                let child = GetDlgItem(*h, id);
                if title(child) != text {
                    SetWindowTextW(child, wide(text).as_ptr());
                }
            }
            if fullscreen {
                RedrawWindow(*h, &damage, null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
            } else {
                InvalidateRect(*h, &damage, 0);
            }
        }
        // Every monitor's initial state is ready before any new window is shown.
        for h in &self.windows {
            if IsWindowVisible(*h) == 0 {
                #[cfg(test)]
                let _show = super::ui::diagnostics::measure("rest.show_and_paint");
                ShowWindow(*h, SW_SHOWNOACTIVATE);
                RedrawWindow(
                    *h,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
            }
        }
        for h in &self.windows {
            if GetPropW(*h, wide("DunYu.AnimationStarted").as_ptr()).is_null() {
                update_animation(*h);
                SetPropW(
                    *h,
                    wide("DunYu.AnimationStarted").as_ptr(),
                    1usize as HANDLE,
                );
            }
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
    }
    unsafe fn create(
        &mut self,
        r: RECT,
        full: bool,
        phase: Phase,
        host: HWND,
        skin: std::rc::Rc<theme::Theme>,
    ) {
        #[cfg(test)]
        let _measurement = super::ui::diagnostics::measure("rest.create");
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_COMPOSITED,
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
        let buffer = Box::into_raw(Box::new(RestBuffers::default()));
        if SetPropW(h, wide("DunYu.BackBuffer").as_ptr(), buffer as HANDLE) == 0 {
            drop(Box::from_raw(buffer));
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
        let layout = super::swim::Layout::new(width as f32, height as f32);
        let area = |id: i32, fallback: [i32; 4]| -> [i32; 4] {
            if !full {
                return fallback;
            }
            let r = match id {
                12 => layout.rect(510., 165., 652., 64.),
                10 => layout.rect(686., 374., 300., 128.),
                13 => layout.rect(500., 647., 672., 54.),
                3 => layout.rect(699., 737., 274., 64.),
                4 => [
                    width as f32 - 76. * layout.scale,
                    28. * layout.scale,
                    48. * layout.scale,
                    48. * layout.scale,
                ],
                _ => return fallback,
            };
            r.map(|v| v.round() as i32)
        };
        let make_font = |size: i32, name: &str| {
            let mut lf: LOGFONTW = std::mem::zeroed();
            lf.lfHeight = if full {
                -(match name {
                    "DunYu.RestTitle" => 36.,
                    "DunYu.RestTimer" => 96.,
                    _ => 28.,
                } * layout.scale)
                    .round() as i32
            } else {
                -px(size)
            };
            let face = wide(if name == "DunYu.RestTitle" {
                super::fonts::FACE
            } else if name == "DunYu.RestTimer" && full {
                "Segoe UI Light"
            } else {
                super::fonts::FACE
            });
            lf.lfFaceName[..face.len()].copy_from_slice(&face);
            let font = {
                #[cfg(test)]
                let _font = super::ui::diagnostics::measure("rest.font");
                CreateFontIndirectW(&lf)
            };
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
            let rect = area(
                id,
                [
                    if id == 12 { px(64) } else { left },
                    top + px(offset),
                    if id == 12 {
                        (width - px(128)).max(1)
                    } else {
                        body_width
                    },
                    px(tall),
                ],
            );
            let label = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | windows_sys::Win32::System::SystemServices::SS_CENTER,
                rect[0],
                rect[1],
                rect[2],
                rect[3],
                h,
                id as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(label, WM_SETFONT, f as usize, 0);
        }
        // Reserve a separate header lane for the logo, title and close control.
        let rect = area(4, [width - px(56), px(16), px(40), px(40)]);
        let close = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("×").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            rect[0],
            rect[1],
            rect[2],
            rect[3],
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
            let rect = area(
                *id as i32,
                [left + i as i32 * (bw + px(8)), top + px(210), bw, px(48)],
            );
            let b = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                rect[0],
                rect[1],
                rect[2],
                rect[3],
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
pub(super) unsafe fn benchmark(runtime: &mut Runtime, host: HWND) -> serde_json::Value {
    use serde_json::json;
    unsafe fn pump(ms: u64) {
        let start = Instant::now();
        while start.elapsed().as_millis() < ms as u128 {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
                if start.elapsed().as_millis() >= ms as u128 {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    unsafe fn cpu() -> u64 {
        let mut a: FILETIME = std::mem::zeroed();
        let mut b = a;
        let mut k = a;
        let mut u = a;
        assert_ne!(
            GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u),
            0
        );
        let value = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        value(k) + value(u)
    }
    let mut runs = Vec::new();
    for kind in ["first_fullscreen", "reopen", "card_to_fullscreen"] {
        for _ in 0..if kind == "first_fullscreen" { 1 } else { 10 } {
            if kind == "card_to_fullscreen" {
                runtime.configure(RestConfig::default(), host);
                runtime.action(23, None, host);
                pump(200);
            } else {
                runtime.close();
                pump(200);
            }
            let mut handler = 0.;
            let measurement = super::ui::diagnostics::sample_action(|| {
                let start = Instant::now();
                runtime.configure(
                    RestConfig {
                        fullscreen: true,
                        ..Default::default()
                    },
                    host,
                );
                runtime.action(23, None, host);
                handler = start.elapsed().as_secs_f64() * 1000.;
                pump(200);
            });
            runs.push(json!({"scenario":kind,"handler_ms":handler,"measurement":measurement}));
        }
    }
    runtime.preview_until = GetTickCount64() + 10000;
    pump(1000);
    let monitors: Vec<_> = runtime
        .windows
        .iter()
        .map(|h| {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(*h, &mut r);
            json!({"rect":[r.left,r.top,r.right,r.bottom],"dpi":GetDpiForWindow(*h)})
        })
        .collect();
    let begin = cpu();
    let start = Instant::now();
    let animation = super::ui::diagnostics::sample_action(|| {
        for _ in 0..5 {
            runtime.tick(host);
            pump(1000);
        }
    });
    let wall_ms = start.elapsed().as_secs_f64() * 1000.;
    let cpu_ms = (cpu() - begin) as f64 / 10000.;
    runtime.close();
    json!({"runs":runs,"animation":animation,"cpu_ms":cpu_ms,"wall_ms":wall_ms,
        "monitors":monitors,"high_contrast":theme::high_contrast(),"motion_enabled":motion_enabled()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fullscreen_countdown_stays_visible_during_text_updates() {
        #[derive(Default)]
        struct Observation {
            updates: std::cell::Cell<usize>,
            hidden: std::cell::Cell<bool>,
        }
        unsafe extern "system" fn observe(
            h: HWND,
            m: u32,
            w: WPARAM,
            l: LPARAM,
            id: usize,
            data: usize,
        ) -> LRESULT {
            let observation = &*(data as *const Observation);
            if m == WM_SETTEXT {
                observation.updates.set(observation.updates.get() + 1);
                // Check inside the update: the old batch restored visibility
                // before advance returned, hiding the transient loss from tests.
                if IsWindowVisible(GetAncestor(h, GA_ROOT)) == 0 {
                    observation.hidden.set(true);
                }
            }
            if m == WM_NCDESTROY {
                RemoveWindowSubclass(h, Some(observe), id);
            }
            DefSubclassProc(h, m, w, l)
        }
        struct Cleanup {
            runtime: Runtime,
            host: theme::TestWindow,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    self.runtime.close();
                    KillTimer(self.host.0, TIMER_ID);
                }
            }
        }
        unsafe {
            let _graphics = theme::TestGraphicsScope::new();
            windows_sys::Win32::UI::Controls::InitCommonControls();
            for preview in [false, true] {
                // Outlive the windows and their subclasses, including on panic.
                let observation = Box::<Observation>::default();
                let host = theme::TestWindow(CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide("rest-visibility-test").as_ptr(),
                    0,
                    0,
                    0,
                    1,
                    1,
                    null_mut(),
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                ));
                assert!(!host.0.is_null());
                let mut cleanup = Cleanup {
                    runtime: Runtime::default(),
                    host,
                };
                let host = cleanup.host.0;
                let runtime = &mut cleanup.runtime;
                let now = GetTickCount64();
                runtime.scheduler = RestScheduler::new(
                    RestConfig {
                        enabled: !preview,
                        fullscreen: true,
                        ..Default::default()
                    },
                    now,
                );
                if preview {
                    runtime.preview_until = now + 5000;
                } else {
                    runtime.scheduler.start(now);
                }
                runtime.advance(host, now, false, false);
                let windows = runtime.windows.clone();
                let generation = runtime.generation;
                assert!(!windows.is_empty());
                for h in &windows {
                    assert_ne!(IsWindowVisible(*h), 0);
                    assert_ne!(
                        SetWindowSubclass(
                            GetDlgItem(*h, 10),
                            Some(observe),
                            0xD175,
                            &*observation as *const Observation as usize,
                        ),
                        0
                    );
                }
                for second in 1..=3 {
                    runtime.advance(host, now + second * 1000, false, false);
                    assert_eq!(runtime.windows, windows);
                    assert_eq!(runtime.generation, generation);
                    assert!(
                        !observation.hidden.get(),
                        "preview={preview}, second={second}"
                    );
                    assert_eq!(observation.updates.get(), second as usize * windows.len());
                    let left = if preview { 5 } else { 20 } - second;
                    for h in &windows {
                        assert_ne!(IsWindowVisible(*h), 0);
                        assert_eq!(title(GetDlgItem(*h, 10)), format!("00:{left:02}"));
                        assert_eq!(
                            GetPropW(*h, wide("DunYu.Left").as_ptr()) as usize as u64,
                            left * 1000
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn cached_rest_matches_original() {
        struct Owner(Runtime);
        impl Drop for Owner {
            fn drop(&mut self) {
                unsafe {
                    self.0.close();
                }
            }
        }
        unsafe {
            let _graphics = theme::TestGraphicsScope::new();
            windows_sys::Win32::UI::Controls::InitCommonControls();
            let host = theme::TestWindow(CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            ));
            let mut owner = Owner(Runtime::default());
            let runtime = &mut owner.0;
            struct PaintReset;
            impl Drop for PaintReset {
                fn drop(&mut self) {
                    PAINT_CLOCK.set(None);
                    PAINT_DIRTY.set(None);
                    PAINT_MOVING.set(None);
                    REFERENCE.set(false);
                }
            }
            let _reset = PaintReset;
            let directory = std::path::PathBuf::from(format!(
                "rest-pixel-diagnostics-{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                std::process::id()
            ));
            std::fs::create_dir_all(&directory).unwrap();
            println!("Rest pixel artifacts: {}", directory.display());
            let mut report = serde_json::json!({"protocol":"rest-pixels-v1",
                "build":option_env!("DUNYU_DIAGNOSTIC_REVISION"),"high_contrast":theme::high_contrast(),
                "clock_base_ms":10000,"motion_enabled_in_test":true,"expected_cases":96,"complete":false,"cases":[]});
            let save = |report: &serde_json::Value| {
                std::fs::write(
                    directory.join("report.json"),
                    serde_json::to_vec_pretty(report).unwrap(),
                )
                .unwrap();
            };
            save(&report);
            let mut probes = Vec::new();
            let mut failed = 0;
            // Register the production class, then use hidden windows with explicit
            // client sizes: no monitor mode changes and no timer-driven test state.
            runtime.configure(
                RestConfig {
                    fullscreen: true,
                    ..Default::default()
                },
                host.0,
            );
            runtime.action(23, None, host.0);
            assert!(
                !runtime.windows.is_empty(),
                "fullscreen preview was not created"
            );
            runtime.close();
            KillTimer(host.0, TIMER_ID);
            for (width, height) in [(1920, 1080), (1672, 941), (3440, 1440), (1080, 1920)] {
                runtime.create(
                    RECT {
                        left: 0,
                        top: 0,
                        right: width,
                        bottom: height,
                    },
                    true,
                    Phase::Resting,
                    host.0,
                    theme::Theme::new(),
                );
                assert_eq!(runtime.windows.len(), 1);
                let h = runtime.windows[0];
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                assert_eq!((r.right, r.bottom), (width, height));
                for (key, value) in [
                    ("DunYu.Duration", 20000usize),
                    ("DunYu.Left", 20000),
                    ("DunYu.Stamp", 10000),
                    ("DunYu.MotionStart", 10000),
                ] {
                    assert_ne!(SetPropW(h, wide(key).as_ptr(), value as HANDLE), 0);
                }
                for offset in [0, 333, 999] {
                    for kind in [
                        "reference_repeat",
                        "cached_full_cold",
                        "cached_full_warm",
                        "uncached_partial",
                        "cached_partial",
                        "clip_small",
                        "clip_overlap",
                        "clip_second_only",
                        "clip_aba",
                        "clip_bab",
                        "clip_empty",
                        "clip_outside",
                        "clip_bounds",
                        "cached_rebuild",
                    ] {
                        if offset != 0 && (kind.starts_with("clip_") || kind == "cached_rebuild") {
                            continue;
                        }
                        let name = format!("{width}x{height}-{offset}-{kind}");
                        let mut measurements = Vec::new();
                        let mut result = theme::diagnose_frames_for_test(
                            width,
                            height,
                            &directory.join(&name),
                            |dc, reference| {
                                let cache = GetPropW(h, wide("DunYu.BackBuffer").as_ptr())
                                    as *mut RestBuffers;
                                assert!(!cache.is_null());
                                // Independent buffers for the two sides and each case.
                                *cache = RestBuffers::default();
                                REFERENCE.set(
                                    reference
                                        || matches!(kind, "reference_repeat" | "uncached_partial"),
                                );
                                PAINT_MOVING.set(Some(true));
                                PAINT_DIRTY.set(None);
                                let partial = !reference
                                    && matches!(kind, "uncached_partial" | "cached_partial");
                                PAINT_CLOCK.set(Some(if partial { 10000 } else { 10000 + offset }));
                                let mut steps = Vec::new();
                                let counters = super::super::ui::diagnostics::capture_counters(
                                    || {
                                        if kind.starts_with("clip_") {
                                            // A black destination makes any writes outside the requested
                                            // damage visible. Reference renders a full frame and clips only
                                            // the destination; the candidate uses the actual dirty path.
                                            PatBlt(dc, 0, 0, width, height, BLACKNESS);
                                            let area = animation_rect(r);
                                            let small = RECT {
                                                left: area.left + 80,
                                                top: area.top + 20,
                                                right: area.left + 210,
                                                bottom: area.top + 150,
                                            };
                                            let second = RECT {
                                                left: small.left + 40,
                                                top: small.top + 30,
                                                right: small.right + 80,
                                                bottom: small.bottom + 70,
                                            };
                                            let rects = match kind {
                                                "clip_small" => vec![small],
                                                "clip_overlap" => vec![small, second],
                                                "clip_second_only" => vec![second],
                                                "clip_aba" => vec![small, second, small],
                                                "clip_bab" => vec![second, small, second],
                                                "clip_empty" => vec![RECT {
                                                    left: 20,
                                                    top: 20,
                                                    right: 20,
                                                    bottom: 40,
                                                }],
                                                "clip_outside" => vec![RECT {
                                                    left: width + 10,
                                                    top: 20,
                                                    right: width + 50,
                                                    bottom: 40,
                                                }],
                                                _ => vec![
                                                    RECT {
                                                        left: -30,
                                                        top: -20,
                                                        right: 100,
                                                        bottom: 100,
                                                    },
                                                    RECT {
                                                        left: width - 30,
                                                        top: height - 20,
                                                        right: width + 100,
                                                        bottom: height + 100,
                                                    },
                                                ],
                                            };
                                            for (index, rect) in rects.into_iter().enumerate() {
                                                let time =
                                                    if matches!(kind, "clip_aba" | "clip_bab") {
                                                        [0, 333, 999][index]
                                                    } else {
                                                        0
                                                    };
                                                PAINT_CLOCK.set(Some(10000 + time));
                                                steps.push(serde_json::json!({"offset_ms":time,
                                                    "rect":[rect.left,rect.top,rect.right,rect.bottom]}));
                                                let saved = SaveDC(dc);
                                                if reference {
                                                    IntersectClipRect(
                                                        dc,
                                                        rect.left,
                                                        rect.top,
                                                        rect.right,
                                                        rect.bottom,
                                                    );
                                                } else {
                                                    PAINT_DIRTY.set(Some(rect));
                                                }
                                                paint(h, WM_PRINTCLIENT, dc as usize);
                                                RestoreDC(dc, saved);
                                            }
                                            return;
                                        }
                                        if !reference && kind == "cached_rebuild" {
                                            paint(h, WM_PRINTCLIENT, dc as usize);
                                            SendMessageW(h, WM_THEMECHANGED, 0, 0);
                                            KillTimer(h, ANIMATION_TIMER);
                                        }
                                        if partial || (!reference && kind == "cached_full_warm") {
                                            paint(h, WM_PRINTCLIENT, dc as usize);
                                        }
                                        PAINT_CLOCK.set(Some(10000 + offset));
                                        if partial {
                                            PAINT_DIRTY.set(Some(animation_rect(r)));
                                        }
                                        paint(h, WM_PRINTCLIENT, dc as usize);
                                    },
                                );
                                if !reference
                                    && !REFERENCE.get()
                                    && !theme::high_contrast()
                                    && !kind.starts_with("clip_")
                                {
                                    assert!(
                                        counters.contains_key("paint.rest.static_copy"),
                                        "cache was not used"
                                    );
                                    assert_eq!(
                                        counters.get("paint.rest.static_build").map(|v| v.0),
                                        Some(if kind == "cached_rebuild" { 2 } else { 1 }),
                                        "unexpected cache build count"
                                    );
                                }
                                measurements.push(
                                    serde_json::json!({"reference":reference,"counters":counters,"steps":steps}),
                                );
                                for buffer in [&mut (*cache).frame, &mut (*cache).backdrop]
                                    .into_iter()
                                    .flatten()
                                {
                                    probes.push(buffer.observe_release());
                                }
                                PAINT_CLOCK.set(None);
                                PAINT_DIRTY.set(None);
                                PAINT_MOVING.set(None);
                                REFERENCE.set(false);
                            },
                        );
                        result["scenario"] = kind.into();
                        result["size"] = serde_json::json!([width, height]);
                        result["offset_ms"] = offset.into();
                        result["dpi"] = GetDpiForWindow(h).into();
                        result["measurements"] = measurements.into();
                        if result["passed"] != true {
                            failed += 1;
                        }
                        println!("{name}: different_pixels={}", result["different_pixels"]);
                        report["cases"].as_array_mut().unwrap().push(result);
                        save(&report);
                    }
                }
                runtime.close();
            }
            runtime.close();
            KillTimer(host.0, TIMER_ID);
            for probe in probes {
                probe.assert_released("rest pixel buffer");
            }
            assert_eq!(
                report["cases"].as_array().unwrap().len(),
                96,
                "incomplete rest pixel coverage"
            );
            report["complete"] = true.into();
            report["failed_cases"] = failed.into();
            save(&report);
            assert_eq!(
                failed,
                0,
                "Rest pixel differences; see {}",
                directory.display()
            );
        }
    }
    #[test]
    fn preview_is_isolated_and_fullscreen_uses_each_monitor() {
        struct Cleanup {
            runtime: Runtime,
            host: theme::TestWindow,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    self.runtime.close();
                    KillTimer(self.host.0, TIMER_ID);
                }
            }
        }
        unsafe {
            let _graphics = theme::TestGraphicsScope::new();
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
            let mut cleanup = Cleanup {
                runtime: Runtime::default(),
                host: theme::TestWindow(host),
            };
            let runtime = &mut cleanup.runtime;
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
                    theme::Theme::new(),
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
