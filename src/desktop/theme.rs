//! Native control identities and input handling stay with Windows. Painting owns
//! its own snapshots so synchronous paint messages never borrow the business App.
use super::*;
use std::rc::Rc;
use windows_sys::Win32::Graphics::GdiPlus::*;
use windows_sys::Win32::UI::Controls::*;

// Design palette: deep teal paper, warm ink and vermilion accents.
pub(super) const BACKGROUND: u32 = 0x061e1d;
const SURFACE: u32 = 0x102826;
const ON_ACCENT: u32 = 0xffe7c5;
pub(super) const INK: u32 = 0xf0ddbd;
pub(super) const MUTED: u32 = 0xa99f89;
pub(super) const ACCENT: u32 = 0xc9432b;
const ACCENT_HOVER: u32 = 0xdd5136;
const ACCENT_PRESSED: u32 = 0xa53423;
const ACCENT_TEXT: u32 = 0xe76b4f;
const LINE: u32 = 0x526b65;
const SELECTION: u32 = 0x263c35;
const SURFACE_HOVER: u32 = 0x1b3531;
const SURFACE_PRESSED: u32 = 0x253f39;
pub(super) const SUCCESS: u32 = 0x6fb897;
pub(super) const DANGER: u32 = 0xee795e;
pub(super) const POND_RIPPLE: u32 = 0x8a9078;
fn rgb(hex: u32) -> u32 {
    (hex & 255) << 16 | hex & 0xff00 | hex >> 16
}
pub(super) unsafe fn high_contrast() -> bool {
    let mut hc: HIGHCONTRASTW = std::mem::zeroed();
    hc.cbSize = std::mem::size_of_val(&hc) as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0);
    hc.dwFlags & HCF_HIGHCONTRASTON != 0
}
unsafe fn color(value: u32) -> u32 {
    if high_contrast() {
        GetSysColor(match value {
            INK | MUTED | SUCCESS | DANGER | ACCENT_TEXT | LINE => COLOR_WINDOWTEXT,
            ACCENT | ACCENT_HOVER | ACCENT_PRESSED | SELECTION => COLOR_HIGHLIGHT,
            BACKGROUND | SURFACE | SURFACE_HOVER | SURFACE_PRESSED => COLOR_WINDOW,
            ON_ACCENT => COLOR_HIGHLIGHTTEXT,
            _ => COLOR_BTNFACE,
        })
    } else {
        rgb(value)
    }
}
pub(super) struct Theme {
    token: usize,
}
impl Theme {
    pub(super) unsafe fn new() -> Rc<Self> {
        super::fonts::ensure();
        let mut token = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        GdiplusStartup(&mut token, &input, null_mut());
        Rc::new(Self { token })
    }
}
impl Drop for Theme {
    fn drop(&mut self) {
        unsafe {
            if self.token != 0 {
                #[cfg(test)]
                eprintln!("[graphics-cleanup] GdiplusShutdown({}) begin", self.token);
                GdiplusShutdown(self.token);
                #[cfg(test)]
                eprintln!("[graphics-cleanup] GdiplusShutdown({}) end", self.token);
            }
        }
    }
}
// Declare before test windows/fixtures so they are destroyed before this scope.
// In particular, never leave GdiplusShutdown to Windows TLS detach callbacks.
#[cfg(test)]
pub(super) struct TestGraphicsScope(Option<Rc<Theme>>);
#[cfg(test)]
impl TestGraphicsScope {
    pub(super) unsafe fn new() -> Self {
        Self(Some(Theme::new()))
    }
}
#[cfg(test)]
impl Drop for TestGraphicsScope {
    fn drop(&mut self) {
        eprintln!("[graphics-cleanup] begin");
        let paper = PAPER.with(|cache| std::mem::take(&mut *cache.borrow_mut()));
        drop(paper);
        super::assets::clear_test_cache();
        eprintln!("[graphics-cleanup] GDI+ shutdown begin");
        drop(self.0.take());
        eprintln!("[graphics-cleanup] GDI+ shutdown end; fonts begin");
        super::fonts::clear_test_cache();
        eprintln!("[graphics-cleanup] end");
    }
}
#[cfg(test)]
pub(super) struct TestWindow(pub(super) HWND);
#[cfg(test)]
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.0) != 0 {
                DestroyWindow(self.0);
            }
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Role {
    Surface,
    Tab,
    Button,
    Edit,
    Native,
    Checkbox,
    Scrollbar,
    Brand,
    Heading,
    Slider,
    Gutter,
}
#[derive(PartialEq)]
struct TabItemKey {
    rect: [i32; 4],
    visible: [i32; 4],
    bounds: (i32, i32),
    origin: (i32, i32),
    canvas: (i32, i32, i32),
    dpi: u32,
    font: usize,
}
struct TabItemCache {
    key: TabItemKey,
    buffer: Option<super::buffering::BackBuffer>,
}
#[derive(PartialEq)]
struct StaticKey {
    label: String,
    font: usize,
    dpi: u32,
    bounds: (i32, i32),
    origin: (i32, i32),
    canvas: (i32, i32, i32),
}
struct PaintState {
    static_key: Option<StaticKey>,
    tabs: RefCell<[Option<TabItemCache>; super::ui::PAGES.len()]>,
    buffer: Option<super::buffering::BackBuffer>,
    _theme: Rc<Theme>,
    role: Role,
    hot: bool,
    tint: Option<u32>,
    button_style: Option<(bool, u32)>,
    outline: bool,
    edit_font: HFONT,
    dpi: u32,
    cards: Vec<RECT>,
    fields: Vec<(HWND, RECT)>,
}
impl Drop for PaintState {
    fn drop(&mut self) {
        unsafe {
            if !self.edit_font.is_null() {
                DeleteObject(self.edit_font);
            }
        }
    }
}
const SUBCLASS: usize = 0x44554e59;
pub(super) unsafe fn attach(h: HWND, role: Role, theme: Rc<Theme>) {
    let state = Box::new(PaintState {
        static_key: None,
        tabs: RefCell::new(std::array::from_fn(|_| None)),
        buffer: None,
        _theme: theme,
        role,
        hot: false,
        tint: None,
        button_style: None,
        outline: false,
        edit_font: null_mut(),
        dpi: 96,
        cards: vec![],
        fields: vec![],
    });
    let ptr = Box::into_raw(state);
    if SetWindowSubclass(h, Some(procedure), SUBCLASS, ptr as usize) == 0 {
        drop(Box::from_raw(ptr));
    }
}
pub(super) unsafe fn tint(h: HWND, value: u32) {
    let mut ptr = 0;
    if GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr) != 0 {
        (*(ptr as *mut PaintState)).tint = Some(value);
    }
    InvalidateRect(h, null(), 0);
}
// Rest controls live directly on the pond canvas rather than a settings card.
pub(super) unsafe fn button_style(h: HWND, primary: bool, backdrop: u32) {
    let mut ptr = 0;
    if GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr) != 0 {
        (*(ptr as *mut PaintState)).button_style = Some((primary, backdrop));
    }
    InvalidateRect(h, null(), 0);
}

pub(super) unsafe fn outline(h: HWND) {
    let mut ptr = 0;
    if GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr) != 0 {
        (*(ptr as *mut PaintState)).outline = true;
    }
}
pub(super) unsafe fn scene(h: HWND, dpi: u32, cards: Vec<RECT>, fields: Vec<(HWND, RECT)>) {
    let mut ptr = 0;
    if GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr) != 0 {
        let state = &mut *(ptr as *mut PaintState);
        let rect_eq = |a: &RECT, b: &RECT| {
            (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
        };
        if state.dpi == dpi
            && state.cards.len() == cards.len()
            && state.fields.len() == fields.len()
            && state.cards.iter().zip(&cards).all(|(a, b)| rect_eq(a, b))
            && state
                .fields
                .iter()
                .zip(&fields)
                .all(|((ha, a), (hb, b))| ha == hb && rect_eq(a, b))
        {
            return;
        }
        state.dpi = dpi;
        state.cards = cards;
        state.fields = fields;
    }
    InvalidateRect(h, null(), 0);
}
type PaperKey = (i32, i32, i32);
struct Paper {
    brush: HBRUSH,
    #[cfg(test)]
    release: Option<Rc<std::cell::Cell<Option<BOOL>>>>,
}
impl Paper {
    unsafe fn build(dc: HDC, key: PaperKey) -> Option<Self> {
        #[cfg(test)]
        let _build = super::ui::diagnostics::measure("paint.paper.build");
        let mut buffer = None;
        let memory = super::buffering::BackBuffer::prepare(&mut buffer, dc, key.0, key.1)?;
        super::assets::background(memory, key.0, key.1, key.2);
        let bitmap = GetCurrentObject(memory, OBJ_BITMAP as u32);
        let brush = CreatePatternBrush(bitmap);
        if brush.is_null() {
            return None;
        }
        Some(Self {
            brush,
            #[cfg(test)]
            release: None,
        })
    }
}
impl Drop for Paper {
    fn drop(&mut self) {
        let _released = unsafe { DeleteObject(self.brush) };
        #[cfg(test)]
        if let Some(probe) = &self.release {
            probe.set(Some(_released));
        }
    }
}
struct PaperCache {
    // Oldest first; the small entry limit keeps lookup and promotion cheap.
    entries: Vec<(PaperKey, Paper)>,
    bytes: u64,
    budget: u64,
    limit: usize,
    // WM_CTLCOLOR callers use the returned brush after the callback returns.
    // Keep an oversized, uncached brush alive until the next successful lookup.
    temporary: Option<Paper>,
}
impl Default for PaperCache {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
            budget: 128 * 1024 * 1024,
            limit: 16,
            temporary: None,
        }
    }
}
impl PaperCache {
    fn cost(key: PaperKey) -> u64 {
        key.0.max(1) as u64 * key.1.max(1) as u64 * 4
    }
    fn record(&self, _key: PaperKey, _event: &str) {
        #[cfg(test)]
        super::ui::diagnostics::paper_cache_event(_key, _event, self.bytes, self.entries.len());
    }
    fn get_or_build(
        &mut self,
        key: PaperKey,
        build: impl FnOnce() -> Option<Paper>,
    ) -> Option<HBRUSH> {
        if let Some(index) = self.entries.iter().position(|(k, _)| *k == key) {
            let entry = self.entries.remove(index);
            let brush = entry.1.brush;
            self.entries.push(entry);
            self.temporary = None;
            self.record(key, "hits");
            return Some(brush);
        }
        self.record(key, "misses");
        let Some(paper) = build() else {
            self.record(key, "failures");
            return None;
        };
        let brush = paper.brush;
        let cost = Self::cost(key);
        if cost > self.budget {
            self.temporary = Some(paper);
            self.record(key, "temporary_builds");
        } else {
            while self.entries.len() >= self.limit || self.bytes + cost > self.budget {
                let (evicted, paper) = self.entries.remove(0);
                self.bytes -= Self::cost(evicted);
                drop(paper);
                self.record(evicted, "evictions");
            }
            self.entries.push((key, paper));
            self.bytes += cost;
            self.temporary = None;
            self.record(key, "builds");
        }
        Some(brush)
    }
}
thread_local! {
    static PAPER: RefCell<PaperCache> = RefCell::new(PaperCache::default());
    static CANVAS: std::cell::Cell<PaperKey> = const { std::cell::Cell::new((1024,650,0)) };
}
pub(super) unsafe fn canvas(h: HWND) {
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(GetAncestor(h, GA_ROOT), &mut r);
    let root = GetAncestor(h, GA_ROOT);
    let mode = if !GetPropW(root, wide("DunYu.Full").as_ptr()).is_null() {
        1
    } else if !GetPropW(root, wide("DunYu.RestHost").as_ptr()).is_null() {
        2
    } else {
        0
    };
    CANVAS.set((r.right.max(1), r.bottom.max(1), mode));
}
unsafe fn paper_brush(dc: HDC) -> HBRUSH {
    let size = CANVAS.get();
    PAPER
        .with(|cache| {
            cache
                .borrow_mut()
                .get_or_build(size, || Paper::build(dc, size))
        })
        .unwrap_or_else(|| {
            // Stock DC brush is borrowed, never owned by the cache. Retry next paint.
            SetDCBrushColor(dc, rgb(if size.2 == 3 { SURFACE } else { BACKGROUND }));
            GetStockObject(DC_BRUSH)
        })
}
pub(super) unsafe fn background_brush(dc: HDC) -> HBRUSH {
    if high_contrast() {
        GetSysColorBrush(COLOR_WINDOW)
    } else {
        paper_brush(dc)
    }
}
pub(super) unsafe fn paper(dc: HDC, r: RECT) {
    if high_contrast() {
        let b = CreateSolidBrush(color(BACKGROUND));
        FillRect(dc, &r, b);
        DeleteObject(b);
    } else {
        FillRect(dc, &r, paper_brush(dc));
    }
}
unsafe fn fill(dc: HDC, r: RECT, value: u32) {
    if value == SURFACE && !high_contrast() {
        let saved = CANVAS.get();
        CANVAS.set((saved.0, saved.1, 3));
        paper(dc, r);
        CANVAS.set(saved);
        return;
    }
    if value == BACKGROUND {
        paper(dc, r);
        return;
    }
    let b = CreateSolidBrush(color(value));
    FillRect(dc, &r, b);
    DeleteObject(b);
}
unsafe fn rounded(dc: HDC, r: RECT, radius: i32, value: u32) {
    if matches!(value, SURFACE | BACKGROUND) && !high_contrast() {
        let saved = SaveDC(dc);
        let region = CreateRoundRectRgn(
            r.left,
            r.top,
            r.right + 1,
            r.bottom + 1,
            radius * 2,
            radius * 2,
        );
        ExtSelectClipRgn(dc, region, RGN_AND);
        fill(dc, r, value);
        RestoreDC(dc, saved);
        DeleteObject(region);
        return;
    }
    let mut g = null_mut();
    let mut path = null_mut();
    let mut brush = null_mut();
    if GdipCreateFromHDC(dc, &mut g) != 0 {
        fill(dc, r, value);
        return;
    }
    GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
    GdipCreatePath(FillModeAlternate, &mut path);
    let c = color(value);
    let argb = 0xff000000 | (c & 255) << 16 | c & 0xff00 | c >> 16;
    GdipCreateSolidFill(argb, &mut brush);
    let d = (radius * 2)
        .min(r.right - r.left)
        .min(r.bottom - r.top)
        .max(1) as f32;
    for (x, y, angle) in [
        (r.left as f32, r.top as f32, 180.),
        (r.right as f32 - d, r.top as f32, 270.),
        (r.right as f32 - d, r.bottom as f32 - d, 0.),
        (r.left as f32, r.bottom as f32 - d, 90.),
    ] {
        GdipAddPathArc(path, x, y, d, d, angle, 90.);
    }
    GdipClosePathFigure(path);
    GdipFillPath(g, brush as _, path);
    GdipDeleteBrush(brush as _);
    GdipDeletePath(path);
    GdipDeleteGraphics(g);
}
fn inset(mut r: RECT, n: i32) -> RECT {
    r.left += n;
    r.top += n;
    r.right -= n;
    r.bottom -= n;
    r
}
// Horizontal breathing room must not consume the text's vertical line box.
pub(super) fn button_text_rect(mut r: RECT, dpi: u32) -> RECT {
    let px = |v: i32| v * dpi as i32 / 96;
    r.left += px(12);
    r.right -= px(12);
    r.top += px(4);
    r.bottom -= px(4);
    r
}

/// A complete line even when the edit is empty (text extent then reports zero).
pub(super) unsafe fn font_line_height(h: HWND) -> i32 {
    let dc = GetDC(h);
    let selected = super::fonts::select(dc, h, &title(h));
    let mut metrics: TEXTMETRICW = std::mem::zeroed();
    GetTextMetricsW(dc, &mut metrics);
    drop(selected);
    ReleaseDC(h, dc);
    metrics.tmHeight
}
pub(super) unsafe fn label_size(h: HWND) -> SIZE {
    let dc = GetDC(h);
    let selected = super::fonts::select(dc, h, &title(h));
    let label = wide(&title(h));
    let mut size: SIZE = std::mem::zeroed();
    GetTextExtentPoint32W(dc, label.as_ptr(), (label.len() - 1) as i32, &mut size);
    drop(selected);
    ReleaseDC(h, dc);
    size
}

unsafe fn text(dc: HDC, h: HWND, s: &str, mut r: RECT, value: u32, flags: u32) {
    let symbol = match s {
        "×" => Some(5),
        "✓" => Some(6),
        "▴" => Some(7),
        "▾" => Some(8),
        _ => None,
    };
    if let Some(index) = symbol {
        let side = (r.right - r.left)
            .min(r.bottom - r.top)
            .min(20 * GetDpiForWindow(h).max(96) as i32 / 96);
        let left = (r.left + r.right - side) / 2;
        let top = (r.top + r.bottom - side) / 2;
        icon(
            dc,
            RECT {
                left,
                top,
                right: left + side,
                bottom: top + side,
            },
            index,
            value,
        );
        return;
    }
    let selected = super::fonts::select(dc, h, s);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color(value));
    let text = wide(s);
    DrawTextW(
        dc,
        text.as_ptr(),
        -1,
        &mut r,
        flags | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    drop(selected);
}
// Hand-drawn animation contour: bold silhouette with restrained pressure changes.
unsafe fn ink_curve(g: *mut GpGraphics, pen: *mut GpPen, p: [(f32, f32); 4]) {
    let mut previous = p[0];
    for i in 1..=24 {
        let t = i as f32 / 24.;
        let u = 1. - t;
        let point = (
            u * u * u * p[0].0
                + 3. * u * u * t * p[1].0
                + 3. * u * t * t * p[2].0
                + t * t * t * p[3].0,
            u * u * u * p[0].1
                + 3. * u * u * t * p[1].1
                + 3. * u * t * t * p[2].1
                + t * t * t * p[3].1,
        );
        GdipSetPenWidth(pen, 1.25 + 1.0 * (std::f32::consts::PI * t).sin());
        GdipDrawLine(g, pen, previous.0, previous.1, point.0, point.1);
        previous = point;
    }
}
unsafe fn ink_line(g: *mut GpGraphics, pen: *mut GpPen, x: f32, y: f32, xx: f32, yy: f32) {
    ink_curve(
        g,
        pen,
        [
            (x, y),
            (x + (xx - x) * 0.3, y + (yy - y) * 0.3),
            (x + (xx - x) * 0.7, y + (yy - y) * 0.7),
            (xx, yy),
        ],
    );
}
unsafe fn ink_arc(g: *mut GpGraphics, pen: *mut GpPen, r: [f32; 4], start: f32, sweep: f32) {
    let mut previous = (0., 0.);
    for i in 0..=40 {
        let t = i as f32 / 40.;
        let angle = (start + sweep * t).to_radians();
        let point = (
            r[0] + r[2] * 0.5 * (1. + angle.cos()),
            r[1] + r[3] * 0.5 * (1. + angle.sin()),
        );
        if i > 0 {
            GdipSetPenWidth(pen, 1.25 + 1.0 * (std::f32::consts::PI * t).sin());
            GdipDrawLine(g, pen, previous.0, previous.1, point.0, point.1);
        }
        previous = point;
    }
}
unsafe fn icon(dc: HDC, r: RECT, index: usize, value: u32) {
    let mut graphics = null_mut();
    let mut pen = null_mut();
    if GdipCreateFromHDC(dc, &mut graphics) != 0 {
        return;
    }
    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
    let c = color(value);
    let argb = 0xff000000 | (c & 255) << 16 | c & 0xff00 | c >> 16;
    let unit = (r.right - r.left) as f32 / 24.;
    GdipCreatePen1(argb, 2. * unit, UnitPixel, &mut pen);
    GdipSetPenStartCap(pen, LineCapRound);
    GdipSetPenEndCap(pen, LineCapRound);
    GdipSetPenLineJoin(pen, LineJoinRound);
    GdipTranslateWorldTransform(graphics, r.left as f32, r.top as f32, MatrixOrderPrepend);
    GdipScaleWorldTransform(graphics, unit, unit, MatrixOrderPrepend);
    GdipSetPenWidth(pen, 1.8);
    match index {
        0 => {
            for (x, y, xx, yy) in [
                (3., 4., 21., 4.),
                (21., 4., 21., 20.),
                (21., 20., 3., 20.),
                (3., 20., 3., 4.),
            ] {
                ink_line(graphics, pen, x, y, xx, yy);
            }
            ink_line(graphics, pen, 3., 9., 21., 9.);
            ink_line(graphics, pen, 10., 15., 16., 15.);
            ink_line(graphics, pen, 13., 12., 16., 15.);
            ink_line(graphics, pen, 13., 18., 16., 15.);
        }
        1 => {
            ink_arc(graphics, pen, [2., 7., 13., 10.], 70., 250.);
            ink_arc(graphics, pen, [9., 7., 13., 10.], 250., 250.);
            ink_line(graphics, pen, 9., 12., 15., 12.);
        }
        2 => {
            ink_curve(graphics, pen, [(2., 12.), (7., 3.), (17., 3.), (22., 12.)]);
            ink_curve(
                graphics,
                pen,
                [(2., 12.), (7., 21.), (17., 21.), (22., 12.)],
            );
            ink_arc(graphics, pen, [9., 9., 6., 6.], 0., 355.);
        }
        3 => {
            ink_curve(graphics, pen, [(5., 19.), (-1., 8.), (12., 4.), (21., 3.)]);
            ink_curve(
                graphics,
                pen,
                [(21., 3.), (21., 15.), (17., 23.), (5., 19.)],
            );
            ink_curve(graphics, pen, [(3., 22.), (7., 16.), (12., 12.), (17., 8.)]);
        }
        5 => {
            ink_line(graphics, pen, 6., 6., 18., 18.);
            ink_line(graphics, pen, 18., 6., 6., 18.);
        }
        6 => {
            ink_line(graphics, pen, 4., 12., 10., 18.);
            ink_line(graphics, pen, 10., 18., 21., 5.);
        }
        7 => {
            ink_line(graphics, pen, 5., 16., 12., 8.);
            ink_line(graphics, pen, 12., 8., 19., 16.);
        }
        8 => {
            ink_line(graphics, pen, 5., 8., 12., 16.);
            ink_line(graphics, pen, 12., 16., 19., 8.);
        }
        _ => {
            ink_arc(graphics, pen, [5., 5., 14., 14.], 0., 355.);
            ink_arc(graphics, pen, [9., 9., 6., 6.], 0., 355.);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                ink_line(
                    graphics,
                    pen,
                    12. + a.cos() * 7.,
                    12. + a.sin() * 7.,
                    12. + a.cos() * 10.,
                    12. + a.sin() * 10.,
                );
            }
        }
    }
    if index < 5 && !high_contrast() {
        let mut pigment = null_mut();
        GdipCreateSolidFill(0xffc9a45a, &mut pigment);
        match index {
            0 => {
                GdipFillRectangle(graphics, pigment as _, 5., 5., 2., 2.);
            }
            1 => {
                GdipFillEllipse(graphics, pigment as _, 10.5, 10.5, 3., 3.);
            }
            2 => {
                GdipFillEllipse(graphics, pigment as _, 10.5, 10.5, 3., 3.);
            }
            3 => {
                GdipSetPenColor(pen, 0xff7faf9b);
                ink_line(graphics, pen, 7., 17., 16., 8.);
            }
            _ => {
                GdipFillEllipse(graphics, pigment as _, 10.5, 10.5, 3., 3.);
            }
        }
        GdipDeleteBrush(pigment as _);
    }
    GdipDeletePen(pen);
    GdipDeleteGraphics(graphics);
}
unsafe fn tab_item(h: HWND, dc: HDC, dpi: u32, i: usize, mut item: RECT, selected: bool) {
    let px = |v: i32| v * dpi as i32 / 96;
    if selected {
        if high_contrast() {
            rounded(dc, inset(item, px(2)), px(8), SELECTION);
        }
        let mark = RECT {
            left: item.left + px(12),
            right: item.right - px(12),
            top: item.bottom - px(6),
            bottom: item.bottom - px(3),
        };
        super::assets::stroke(
            dc,
            RECT {
                top: mark.top - px(2),
                bottom: mark.bottom + px(2),
                ..mark
            },
            true,
        );
    }
    if selected && GetFocus() == h {
        rounded(dc, inset(item, px(2)), px(8), ACCENT_TEXT);
        rounded(dc, inset(item, px(4)), px(6), SELECTION);
    }
    let fg = if selected {
        if high_contrast() { ON_ACCENT } else { INK }
    } else {
        INK
    };
    let label = super::ui::PAGES[i];
    let font = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
    let mut size: SIZE = std::mem::zeroed();
    GetTextExtentPoint32W(dc, wide(label).as_ptr(), 2, &mut size);
    SelectObject(dc, font);
    let left = (item.left + item.right - px(44) - size.cx) / 2;
    let top = (item.top + item.bottom - px(20)) / 2;
    let icon_rect = RECT {
        left,
        top: top - px(4),
        right: left + px(36),
        bottom: top + px(22),
    };
    {
        #[cfg(test)]
        let _icon = super::ui::diagnostics::measure("paint.tab.icon");
        if high_contrast() {
            icon(
                dc,
                icon_rect,
                if i == 5 {
                    4
                } else if i == 4 {
                    0
                } else {
                    i
                },
                fg,
            );
        } else {
            super::assets::navigation(dc, icon_rect, i);
        }
    }
    #[cfg(test)]
    let _text = super::ui::diagnostics::measure("paint.tab.text");
    item.left = left + px(44);
    item.right = item.left + size.cx;
    text(dc, h, label, item, fg, DT_LEFT);
}

unsafe fn paint_tabs(h: HWND, dc: HDC, s: &PaintState, cache_enabled: bool) {
    let mut bounds: RECT = std::mem::zeroed();
    GetClientRect(h, &mut bounds);
    fill(dc, bounds, BACKGROUND);
    let selected = SendMessageW(h, TCM_GETCURSEL, 0, 0);
    let mut origin = POINT { x: 0, y: 0 };
    MapWindowPoints(h, GetAncestor(h, GA_ROOT), &mut origin, 1);
    let font = SendMessageW(h, WM_GETFONT, 0, 0) as usize;
    let contrast = high_contrast();
    let cache_enabled = cache_enabled && !contrast;
    for i in 0..super::ui::PAGES.len() {
        let mut item: RECT = std::mem::zeroed();
        SendMessageW(h, TCM_GETITEMRECT, i, &mut item as *mut _ as isize);
        if selected == i as isize {
            #[cfg(test)]
            let _active = super::ui::diagnostics::measure("paint.tab.active");
            tab_item(h, dc, s.dpi, i, item, true);
            continue;
        }
        // Keep the original coordinates for rasterization, but store only pixels
        // that exist in the host buffer. Native tabs can exceed its height.
        let visible = RECT {
            top: item.top.max(bounds.top),
            bottom: item.bottom.min(bounds.bottom),
            ..item
        };
        let width = visible.right - visible.left;
        let height = visible.bottom - visible.top;
        let fallback = if contrast {
            Some("paint.tab.fallback.high_contrast")
        } else if !cache_enabled {
            Some("paint.tab.fallback.no_buffer")
        } else if width <= 0 || height <= 0 {
            Some("paint.tab.fallback.empty")
        } else if item.left < bounds.left || item.right > bounds.right {
            Some("paint.tab.fallback.horizontal_clip")
        } else {
            None
        };
        if let Some(_reason) = fallback {
            #[cfg(test)]
            let _fallback = super::ui::diagnostics::measure(_reason);
            tab_item(h, dc, s.dpi, i, item, false);
            continue;
        }
        let key = TabItemKey {
            rect: [item.left, item.top, item.right, item.bottom],
            visible: [visible.left, visible.top, visible.right, visible.bottom],
            bounds: (bounds.right, bounds.bottom),
            origin: (origin.x, origin.y),
            canvas: CANVAS.get(),
            dpi: s.dpi,
            font,
        };
        let mut tabs = s.tabs.borrow_mut();
        if let Some(entry) = tabs[i].as_mut().filter(|entry| entry.key == key)
            && let Some(memory) =
                super::buffering::BackBuffer::prepare(&mut entry.buffer, dc, width, height)
        {
            #[cfg(test)]
            let _copy = super::ui::diagnostics::measure("paint.tab.cache_copy");
            if BitBlt(
                dc,
                visible.left,
                visible.top,
                width,
                height,
                memory,
                0,
                0,
                SRCCOPY,
            ) != 0
            {
                continue;
            }
            #[cfg(test)]
            let _failure = super::ui::diagnostics::measure("paint.tab.fallback.copy_failed");
        }
        #[cfg(test)]
        let _build = super::ui::diagnostics::measure("paint.tab.cache_build");
        tab_item(h, dc, s.dpi, i, item, false);
        // Copy from the completed frame to preserve exact text rasterization and
        // paper alignment; no coordinate transforms or transparent text layers.
        let mut buffer = None;
        let memory = super::buffering::BackBuffer::prepare(&mut buffer, dc, width, height);
        tabs[i] = memory
            .filter(|&memory| {
                BitBlt(
                    memory,
                    0,
                    0,
                    width,
                    height,
                    dc,
                    visible.left,
                    visible.top,
                    SRCCOPY,
                ) != 0
            })
            .map(|_| TabItemCache { key, buffer });
        #[cfg(test)]
        if tabs[i].is_none() {
            let _failure = super::ui::diagnostics::measure("paint.tab.fallback.store_failed");
        }
    }
}

unsafe fn paint(h: HWND, dc: HDC, s: &PaintState, cache_enabled: bool) {
    canvas(h);
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    let px = |v: i32| v * s.dpi as i32 / 96;
    let mut origin = POINT { x: 0, y: 0 };
    MapWindowPoints(h, GetAncestor(h, GA_ROOT), &mut origin, 1);
    SetBrushOrgEx(dc, -origin.x, -origin.y, null_mut());
    match s.role {
        Role::Gutter => fill(dc, r, BACKGROUND),
        Role::Slider => {
            fill(dc, r, SURFACE);
            let mut channel: RECT = std::mem::zeroed();
            let mut thumb: RECT = std::mem::zeroed();
            SendMessageW(h, TBM_GETCHANNELRECT, 0, &mut channel as *mut _ as isize);
            SendMessageW(h, TBM_GETTHUMBRECT, 0, &mut thumb as *mut _ as isize);
            let middle = (channel.top + channel.bottom) / 2;
            channel.top = middle - px(3);
            channel.bottom = middle + px(3);
            rounded(dc, channel, px(3), LINE);
            if IsWindowEnabled(h) != 0 {
                let value = SendMessageW(h, WM_USER, 0, 0).clamp(10, 100);
                let mut active = channel;
                active.right = active.left
                    + ((active.right - active.left) as isize * (value - 10) / 90) as i32;
                if active.right > active.left {
                    rounded(dc, active, px(3), ACCENT);
                }
            }
            rounded(
                dc,
                thumb,
                px(8),
                if IsWindowEnabled(h) != 0 {
                    ACCENT
                } else {
                    MUTED
                },
            );
            rounded(dc, inset(thumb, px(3)), px(5), ON_ACCENT);
            if GetFocus() == h {
                DrawFocusRect(dc, &inset(r, px(2)));
            }
        }
        Role::Heading => {
            let background = if matches!(GetDlgCtrlID(h), 7001 | 7002 | 7012) {
                BACKGROUND
            } else {
                SURFACE
            };
            fill(dc, r, background);
            let mut label = r;
            label.bottom -= px(4);
            text(dc, h, &title(h), label, INK, DT_LEFT);
            super::assets::stroke(
                dc,
                RECT {
                    left: 0,
                    top: r.bottom - px(8),
                    right: px(102).min(r.right),
                    bottom: r.bottom,
                },
                false,
            );
        }
        Role::Brand => {
            fill(dc, r, BACKGROUND);
            let logo = LoadImageW(
                GetModuleHandleW(null()),
                101usize as _,
                IMAGE_ICON,
                px(48),
                px(48),
                LR_SHARED,
            );
            DrawIconEx(
                dc,
                0,
                (r.bottom - px(48)) / 2,
                logo,
                px(48),
                px(48),
                0,
                null_mut(),
                DI_NORMAL,
            );
            let mut label = r;
            label.left += px(58);
            label.bottom -= px(18);
            text(dc, h, "遁鱼", label, INK, DT_LEFT);
            label.top = r.bottom - px(22);
            label.bottom = r.bottom;
            let mut lf: LOGFONTW = std::mem::zeroed();
            lf.lfHeight = -px(11);
            let face = wide(super::fonts::FACE);
            lf.lfFaceName[..face.len()].copy_from_slice(&face);
            let small = CreateFontIndirectW(&lf);
            let old = SelectObject(dc, small);
            SetTextColor(dc, color(MUTED));
            SetBkMode(dc, TRANSPARENT as i32);
            DrawTextW(
                dc,
                wide("摸鱼有道 · 隐现自如").as_ptr(),
                -1,
                &mut label,
                DT_SINGLELINE | DT_VCENTER,
            );
            SelectObject(dc, old);
            DeleteObject(small);
        }
        Role::Checkbox => {
            fill(dc, r, SURFACE);
            let top = (r.bottom - px(16)) / 2;
            let box_rect = RECT {
                left: px(2),
                top,
                right: px(18),
                bottom: top + px(16),
            };
            let checked = SendMessageW(h, BM_GETCHECK, 0, 0) != 0;
            rounded(
                dc,
                box_rect,
                px(4),
                if checked && IsWindowEnabled(h) != 0 {
                    ACCENT
                } else {
                    MUTED
                },
            );
            if checked {
                text(
                    dc,
                    h,
                    "✓",
                    box_rect,
                    if IsWindowEnabled(h) != 0 {
                        BACKGROUND
                    } else {
                        MUTED
                    },
                    DT_CENTER,
                );
            } else {
                rounded(dc, inset(box_rect, px(1).max(1)), px(3), SURFACE);
            }
            let mut label = r;
            label.left += px(26);
            text(
                dc,
                h,
                &title(h),
                label,
                if IsWindowEnabled(h) != 0 { INK } else { MUTED },
                DT_LEFT,
            );
            if GetFocus() == h {
                DrawFocusRect(dc, &inset(r, 1));
            }
        }
        Role::Scrollbar => {
            fill(dc, r, BACKGROUND);
            let mut info: SCROLLBARINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of_val(&info) as u32;
            if GetScrollBarInfo(h, OBJID_CLIENT, &mut info) != 0
                && info.xyThumbBottom > info.xyThumbTop
            {
                let thumb = RECT {
                    left: px(3),
                    top: info.xyThumbTop,
                    right: r.right - px(3),
                    bottom: info.xyThumbBottom,
                };
                rounded(dc, thumb, px(4), MUTED);
            }
            text(
                dc,
                h,
                "▴",
                RECT {
                    bottom: info.dxyLineButton,
                    ..r
                },
                MUTED,
                DT_CENTER,
            );
            text(
                dc,
                h,
                "▾",
                RECT {
                    top: r.bottom - info.dxyLineButton,
                    ..r
                },
                MUTED,
                DT_CENTER,
            );
        }
        Role::Surface => {
            fill(
                dc,
                r,
                if GetDlgCtrlID(h) == 119 {
                    SURFACE
                } else {
                    BACKGROUND
                },
            );
            if GetDlgCtrlID(h) == 119
                && SendMessageW(GetDlgItem(h, 100), LB_GETCOUNT, 0, 0) == 0
                && !high_contrast()
            {
                let caption = GetDlgItem(h, 118);
                let ((x, y, w, h), _) =
                    super::design::empty_state(r.right, r.bottom, font_line_height(caption), s.dpi);
                super::assets::navigation(
                    dc,
                    RECT {
                        left: x,
                        top: y,
                        right: x + w,
                        bottom: y + h,
                    },
                    0,
                );
            }
            for card in &s.cards {
                rounded(dc, *card, px(12), LINE);
                rounded(dc, inset(*card, px(1).max(1)), px(11), SURFACE);
            }
            for (field, rect) in &s.fields {
                let focused = GetFocus() == *field;
                rounded(dc, *rect, px(8), if focused { ACCENT_TEXT } else { LINE });
                rounded(
                    dc,
                    inset(*rect, px(if focused { 2 } else { 1 }).max(1)),
                    px(7),
                    SURFACE,
                );
            }
        }
        Role::Button => {
            let id = GetDlgCtrlID(h);
            let default_primary = matches!(id, 113 | 330 | 506 | 808 | 401);
            // Only the fixed footer buttons sit on the window background.
            // Card actions need the card texture around their rounded corners.
            let (primary, backdrop) = s.button_style.unwrap_or((
                default_primary,
                if matches!(id, 113 | 330 | 506) {
                    BACKGROUND
                } else {
                    SURFACE
                },
            ));
            fill(dc, r, backdrop);
            let down = SendMessageW(h, BM_GETSTATE, 0, 0) as u32 & BST_PUSHED != 0;
            let enabled = IsWindowEnabled(h) != 0;
            let bg = if !enabled {
                BACKGROUND
            } else if primary {
                if down {
                    ACCENT_PRESSED
                } else if s.hot {
                    ACCENT_HOVER
                } else {
                    ACCENT
                }
            } else if down {
                SURFACE_PRESSED
            } else if s.hot {
                SURFACE_HOVER
            } else if s.outline {
                backdrop
            } else {
                SURFACE
            };
            // Highlight both sides of the text/background pair in system contrast mode.
            let highlighted = high_contrast() && enabled && (primary || s.hot || down);
            let bg = if highlighted { ACCENT } else { bg };
            let outer = inset(r, px(2));
            rounded(
                dc,
                outer,
                px(8),
                if GetFocus() == h {
                    ACCENT_TEXT
                } else if s.outline {
                    ACCENT
                } else if primary {
                    bg
                } else {
                    LINE
                },
            );
            rounded(dc, inset(outer, px(1).max(1)), px(7), bg);
            text(
                dc,
                h,
                &title(h),
                button_text_rect(r, s.dpi),
                if !enabled {
                    MUTED
                } else if primary || highlighted {
                    ON_ACCENT
                } else if GetDlgCtrlID(h) == 111 && !s.hot && !down {
                    DANGER
                } else {
                    INK
                },
                DT_CENTER,
            );
        }
        Role::Tab => paint_tabs(h, dc, s, cache_enabled),
        _ => {}
    }
}
unsafe extern "system" fn procedure(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let ptr = data as *mut PaintState;
    if m == WM_NCDESTROY {
        super::recorder::stop_for(h);
        RemoveWindowSubclass(h, Some(procedure), SUBCLASS);
        drop(Box::from_raw(ptr));
        return DefSubclassProc(h, m, w, l);
    }
    let role = (*ptr).role;
    if matches!(
        m,
        WM_SETTEXT | WM_SETFONT | WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE
    ) {
        (*ptr).static_key = None;
    }
    if role == Role::Tab
        && matches!(
            m,
            WM_SETFONT | WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE
        )
    {
        *(*ptr).tabs.borrow_mut() = std::array::from_fn(|_| None);
    }
    match m {
        WM_SETFONT if matches!(role, Role::Edit | Role::Tab) && w != 0 => {
            let mut lf: LOGFONTW = std::mem::zeroed();
            GetObjectW(
                w as _,
                std::mem::size_of::<LOGFONTW>() as i32,
                &mut lf as *mut _ as _,
            );
            lf.lfFaceName = [0; 32];
            let face = wide(if role == Role::Tab {
                super::fonts::FACE
            } else {
                "Microsoft YaHei UI"
            });
            if role == Role::Tab {
                lf.lfHeight = lf.lfHeight * 21 / 27;
                lf.lfWeight = 400;
            }
            lf.lfFaceName[..face.len()].copy_from_slice(&face);
            let font = CreateFontIndirectW(&lf);
            if !font.is_null() {
                let old = (*ptr).edit_font;
                (*ptr).edit_font = font;
                let result = DefSubclassProc(h, m, font as usize, l);
                if !old.is_null() {
                    DeleteObject(old);
                }
                return result;
            }
        }
        WM_ACTIVATE if role == Role::Surface && GetAncestor(h, GA_ROOT) == h => {
            if w & 0xffff == WA_INACTIVE as usize {
                super::recorder::stop_with_reason("window_deactivated");
            } else {
                super::recorder::start(GetFocus());
            }
        }
        WM_SHOWWINDOW if w == 0 && role == Role::Surface => {
            super::recorder::stop_with_reason("surface_hidden");
        }
        super::recorder::RECORDED => {
            super::recorder::deliver(h, w, l);
            return 0;
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            InvalidateRect(h, null(), 1);
        }
        WM_MOUSEMOVE if role == Role::Button => {
            if !(*ptr).hot {
                (*ptr).hot = true;
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
                InvalidateRect(h, null(), 0);
            }
        }
        WM_MOUSELEAVE if role == Role::Button => {
            (*ptr).hot = false;
            InvalidateRect(h, null(), 0);
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            if role == Role::Edit {
                if m == WM_SETFOCUS {
                    super::recorder::start(h);
                }
                if m == WM_KILLFOCUS {
                    super::recorder::stop_with_reason("field_lost_focus");
                }
            }
            InvalidateRect(h, null(), 0);
            if role == Role::Edit {
                InvalidateRect(GetParent(h), null(), 0);
            }
        }
        WM_ERASEBKGND
            if matches!(
                role,
                Role::Surface
                    | Role::Button
                    | Role::Tab
                    | Role::Checkbox
                    | Role::Scrollbar
                    | Role::Brand
                    | Role::Heading
                    | Role::Slider
                    | Role::Gutter
            ) =>
        {
            return 1;
        }
        WM_PAINT | WM_PRINTCLIENT
            if matches!(
                role,
                Role::Surface
                    | Role::Button
                    | Role::Tab
                    | Role::Checkbox
                    | Role::Scrollbar
                    | Role::Brand
                    | Role::Heading
                    | Role::Slider
                    | Role::Gutter
            ) =>
        {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            #[cfg(test)]
            let _measurement = super::ui::diagnostics::measure("paint");
            let dc = if m == WM_PAINT {
                BeginPaint(h, &mut ps)
            } else {
                w as HDC
            };
            let saved = SaveDC(dc);
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(h, &mut bounds);
            #[cfg(test)]
            let _role = super::ui::diagnostics::measure(match role {
                Role::Surface => "paint.surface",
                Role::Tab => "paint.tab",
                Role::Brand => "paint.brand",
                Role::Heading => "paint.heading",
                _ => "paint.control",
            });
            let memory = super::buffering::BackBuffer::prepare(
                &mut (*ptr).buffer,
                dc,
                bounds.right,
                bounds.bottom,
            );
            if let Some(memory) = memory {
                let state = SaveDC(memory);
                {
                    #[cfg(test)]
                    let _draw = super::ui::diagnostics::measure("paint.draw");
                    let key = if matches!(role, Role::Heading | Role::Brand) && !high_contrast() {
                        canvas(h);
                        let mut origin = POINT { x: 0, y: 0 };
                        MapWindowPoints(h, GetAncestor(h, GA_ROOT), &mut origin, 1);
                        Some(StaticKey {
                            label: title(h),
                            font: SendMessageW(h, WM_GETFONT, 0, 0) as usize,
                            dpi: (*ptr).dpi,
                            bounds: (bounds.right, bounds.bottom),
                            origin: (origin.x, origin.y),
                            canvas: CANVAS.get(),
                        })
                    } else {
                        None
                    };
                    if key.is_some() && key == (*ptr).static_key {
                        #[cfg(test)]
                        let _hit = super::ui::diagnostics::measure("paint.static.cache_copy");
                    } else {
                        paint(h, memory, &*ptr, true);
                        (*ptr).static_key = key;
                    }
                }
                {
                    #[cfg(test)]
                    let _copy = super::ui::diagnostics::measure("paint.copy");
                    BitBlt(dc, 0, 0, bounds.right, bounds.bottom, memory, 0, 0, SRCCOPY);
                }
                RestoreDC(memory, state);
            } else {
                paint(h, dc, &*ptr, false);
            }
            RestoreDC(dc, saved);
            if m == WM_PAINT {
                EndPaint(h, &ps);
            }
            return 0;
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            canvas(h);
            let dc = w as HDC;
            SetBkMode(dc, TRANSPARENT as i32);
            let child = l as HWND;
            let id = GetDlgCtrlID(child);
            let mut child_ptr = 0;
            let tint = if GetWindowSubclass(child, Some(procedure), SUBCLASS, &mut child_ptr) != 0 {
                (*(child_ptr as *mut PaintState)).tint
            } else {
                None
            };
            SetTextColor(
                dc,
                color(
                    tint.unwrap_or(if id == 9001 || IsWindowEnabled(child) == 0 {
                        MUTED
                    } else {
                        INK
                    }),
                ),
            );
            let backdrop = if matches!(id, 9001 | 9002 | 7001 | 7002 | 7012 | 120) {
                BACKGROUND
            } else {
                SURFACE
            };
            SetBkColor(dc, color(backdrop));
            SetDCBrushColor(dc, color(backdrop));
            if !high_contrast() {
                let mut origin = POINT { x: 0, y: 0 };
                MapWindowPoints(child, GetAncestor(child, GA_ROOT), &mut origin, 1);
                SetBrushOrgEx(dc, -origin.x, -origin.y, null_mut());
                let saved = CANVAS.get();
                if backdrop == SURFACE {
                    CANVAS.set((saved.0, saved.1, 3));
                }
                let brush = paper_brush(dc);
                CANVAS.set(saved);
                return brush as isize;
            }
            return GetStockObject(DC_BRUSH) as isize;
        }
        WM_DRAWITEM if l != 0 => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            if d.CtlID == 100 && d.itemID != u32::MAX {
                let selected = d.itemState & ODS_SELECTED != 0;
                let px = |v: i32| v * (*ptr).dpi as i32 / 96;
                fill(d.hDC, d.rcItem, SURFACE);
                if selected {
                    rounded(d.hDC, inset(d.rcItem, px(4)), px(8), SELECTION);
                }
                let n = SendMessageW(d.hwndItem, LB_GETTEXTLEN, d.itemID as usize, 0);
                if n >= 0 {
                    let mut buf = vec![0u16; n as usize + 1];
                    SendMessageW(
                        d.hwndItem,
                        LB_GETTEXT,
                        d.itemID as usize,
                        buf.as_mut_ptr() as isize,
                    );
                    let label = String::from_utf16_lossy(&buf[..n as usize]);
                    let mut lines = label.lines();
                    let mut r = inset(d.rcItem, px(12));
                    r.bottom = r.top + px(24);
                    text(
                        d.hDC,
                        d.hwndItem,
                        lines.next().unwrap_or(""),
                        r,
                        if selected {
                            if high_contrast() {
                                ON_ACCENT
                            } else {
                                ACCENT_TEXT
                            }
                        } else {
                            INK
                        },
                        DT_LEFT,
                    );
                    r.top += px(24);
                    r.bottom += px(20);
                    text(
                        d.hDC,
                        d.hwndItem,
                        lines.next().unwrap_or(""),
                        r,
                        if selected && high_contrast() {
                            ON_ACCENT
                        } else {
                            MUTED
                        },
                        DT_LEFT,
                    );
                }
                if d.itemState & ODS_FOCUS != 0 {
                    DrawFocusRect(d.hDC, &inset(d.rcItem, px(5)));
                }
                return 1;
            }
        }
        _ => {}
    }
    let result = DefSubclassProc(h, m, w, l);
    if GetDlgCtrlID(h) == 100
        && matches!(
            m,
            WM_MOUSEWHEEL
                | WM_KEYDOWN
                | LB_ADDSTRING
                | LB_DELETESTRING
                | LB_RESETCONTENT
                | LB_SETTOPINDEX
        )
    {
        sync_list_scroll(h);
    }
    if role == Role::Slider
        && matches!(
            m,
            WM_MOUSEMOVE
                | WM_MOUSEWHEEL
                | WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_KEYDOWN
                | WM_KEYUP
                | TBM_SETPOS
                | TBM_SETRANGE
                | WM_SIZE
                | WM_ENABLE
        )
    {
        InvalidateRect(h, null(), 0);
    }
    if role == Role::Scrollbar
        && matches!(
            m,
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | SBM_SETSCROLLINFO
        )
    {
        InvalidateRect(h, null(), 0);
    }

    if matches!(role, Role::Button | Role::Checkbox)
        && matches!(
            m,
            WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_KEYDOWN
                | WM_KEYUP
                | BM_SETSTATE
                | BM_SETCHECK
                | WM_ENABLE
                | WM_SETFOCUS
                | WM_KILLFOCUS
        )
    {
        InvalidateRect(h, null(), 0);
    }
    result
}

/// Captures the actual native window, including child controls, without grabbing
/// any other application's desktop content. Used only by Windows UI tests.
#[cfg(test)]
pub(super) unsafe fn capture(h: HWND, path: &str) -> std::io::Result<()> {
    capture_impl(h, path, false, true)
}

#[cfg(test)]
pub(super) unsafe fn capture_screen(h: HWND, path: &str) -> std::io::Result<()> {
    capture_impl(h, path, true, false)
}

#[cfg(test)]
unsafe fn capture_impl(h: HWND, path: &str, actual: bool, client: bool) -> std::io::Result<()> {
    use std::io::Write;
    let mut r: RECT = std::mem::zeroed();
    if client {
        GetClientRect(h, &mut r);
    } else {
        GetWindowRect(h, &mut r);
    }
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    let screen = if client { GetDC(h) } else { GetWindowDC(h) };
    let dc = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    let old = SelectObject(dc, bitmap);
    let result = (|| {
        let captured = if actual {
            BitBlt(dc, 0, 0, width, height, screen, 0, 0, SRCCOPY)
        } else {
            windows_sys::Win32::Storage::Xps::PrintWindow(h, dc, u32::from(client))
        };
        if captured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        SelectObject(dc, old);
        let stride = width as usize * 4;
        let mut pixels = vec![0u8; stride * height as usize];
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        if GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr() as _,
            &mut info,
            DIB_RGB_COLORS,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if pixels.chunks_exact(4).all(|p| p[..3] == pixels[..3]) {
            return Err(std::io::Error::other("window capture is blank"));
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(b"BM")?;
        file.write_all(&(54 + pixels.len() as u32).to_le_bytes())?;
        file.write_all(&[0; 4])?;
        file.write_all(&54u32.to_le_bytes())?;
        file.write_all(std::slice::from_raw_parts(
            &info.bmiHeader as *const _ as *const u8,
            40,
        ))?;
        file.write_all(&pixels)
    })();
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    ReleaseDC(h, screen);
    result
}

pub(super) unsafe fn sync_list_scroll(list: HWND) {
    let bar = GetDlgItem(GetParent(list), 9004);
    if bar.is_null() {
        return;
    }
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(list, &mut r);
    let row = SendMessageW(list, LB_GETITEMHEIGHT, 0, 0).max(1) as i32;
    let count = SendMessageW(list, LB_GETCOUNT, 0, 0).max(0) as i32;
    let si = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: (count - 1).max(0),
        nPage: (r.bottom / row).max(1) as u32,
        nPos: SendMessageW(list, LB_GETTOPINDEX, 0, 0).max(0) as i32,
        nTrackPos: 0,
    };
    SetScrollInfo(bar, SB_CTL, &si, 1);
}

#[cfg(test)]
pub(super) unsafe fn compare_frames_for_test(width: i32, height: i32, draw: impl Fn(HDC, bool)) {
    navigation_tests::compare_frames(width, height, draw);
}
#[cfg(test)]
pub(super) unsafe fn diagnose_frames_for_test(
    width: i32,
    height: i32,
    path: &std::path::Path,
    draw: impl FnMut(HDC, bool),
) -> serde_json::Value {
    navigation_tests::diagnose_frames(width, height, path, draw)
}
#[cfg(test)]
pub(super) unsafe fn verify_static_for_test(h: HWND) {
    let mut ptr = 0;
    assert_ne!(GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr), 0);
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    compare_frames_for_test(r.right, r.bottom, |dc, reference| {
        if reference {
            paint(h, dc, &*(ptr as *const PaintState), false);
        } else {
            SendMessageW(h, WM_PRINTCLIENT, dc as usize, 0);
            let c = super::ui::diagnostics::capture_counters(|| {
                SendMessageW(h, WM_PRINTCLIENT, dc as usize, 0);
            });
            if !high_contrast() {
                assert!(c.contains_key("paint.static.cache_copy"));
            }
        }
    });
}
#[cfg(test)]
pub(super) unsafe fn verify_navigation_for_test(h: HWND) {
    unsafe { navigation_tests::verify_real_control(h) }
}

#[cfg(test)]
pub(super) unsafe fn navigation_geometry(h: HWND) -> serde_json::Value {
    let mut bounds: RECT = std::mem::zeroed();
    GetClientRect(h, &mut bounds);
    let items: Vec<_> = (0..super::ui::PAGES.len()).map(|i| {
        let mut r: RECT = std::mem::zeroed();
        SendMessageW(h, TCM_GETITEMRECT, i, &mut r as *mut _ as isize);
        serde_json::json!({"index":i,"rect":[r.left,r.top,r.right,r.bottom],
            "visible":[r.left.max(bounds.left),r.top.max(bounds.top),r.right.min(bounds.right),r.bottom.min(bounds.bottom)]})
    }).collect();
    serde_json::json!({"client":[bounds.left,bounds.top,bounds.right,bounds.bottom],
        "high_contrast":high_contrast(),"items":items})
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    struct Pixels {
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
        data: *const u8,
        len: usize,
    }
    impl Pixels {
        unsafe fn new(width: i32, height: i32) -> Self {
            unsafe {
                let dc = CreateCompatibleDC(null_mut());
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth = width;
                info.bmiHeader.biHeight = -height;
                info.bmiHeader.biPlanes = 1;
                info.bmiHeader.biBitCount = 32;
                let mut data = null_mut();
                let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut data, null_mut(), 0);
                assert!(!dc.is_null() && !bitmap.is_null());
                let old = SelectObject(dc, bitmap);
                Self {
                    dc,
                    bitmap,
                    old,
                    data: data as _,
                    len: width as usize * height as usize * 4,
                }
            }
        }
        unsafe fn rgb(&self) -> Vec<u8> {
            unsafe {
                GdiFlush();
                std::slice::from_raw_parts(self.data, self.len)
                    .chunks_exact(4)
                    .flat_map(|p| p[..3].iter().copied())
                    .collect()
            }
        }
    }
    impl Drop for Pixels {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.old);
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
    }
    pub(super) unsafe fn compare_frames(width: i32, height: i32, draw: impl Fn(HDC, bool)) {
        let a = Pixels::new(width, height);
        let b = Pixels::new(width, height);
        draw(a.dc, false);
        draw(b.dc, true);
        assert!(
            a.rgb() == b.rgb(),
            "cached and reference pixels differ ({width}x{height})"
        );
    }
    pub(super) unsafe fn diagnose_frames(
        width: i32,
        height: i32,
        path: &std::path::Path,
        mut draw: impl FnMut(HDC, bool),
    ) -> serde_json::Value {
        let a = Pixels::new(width, height);
        let b = Pixels::new(width, height);
        draw(a.dc, false);
        draw(b.dc, true);
        let actual = a.rgb();
        let reference = b.rgb();
        let mut count = 0usize;
        let mut bounds = [width, height, 0, 0];
        let mut maximum = 0u8;
        let mut first = None;
        let mut diff = vec![0u8; width as usize * height as usize * 3];
        for (i, (a, b)) in actual
            .chunks_exact(3)
            .zip(reference.chunks_exact(3))
            .enumerate()
        {
            if a != b {
                count += 1;
                let (x, y) = (i as i32 % width, i as i32 / width);
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x + 1),
                    bounds[3].max(y + 1),
                ];
                if first.is_none() {
                    first = Some(serde_json::json!({"x":x,"y":y,"actual_bgr":a,"reference_bgr":b}));
                }
                for c in 0..3 {
                    maximum = maximum.max(a[c].abs_diff(b[c]));
                }
                diff[i * 3..i * 3 + 3].copy_from_slice(&[255, 0, 255]);
            }
        }
        if count != 0 {
            std::fs::create_dir_all(path).unwrap();
            for (name, data) in [
                ("actual.bmp", &actual),
                ("reference.bmp", &reference),
                ("diff.bmp", &diff),
            ] {
                let mut bmp = vec![0u8; 54];
                let len = 54 + width as u32 * height as u32 * 4;
                bmp[..2].copy_from_slice(b"BM");
                bmp[2..6].copy_from_slice(&len.to_le_bytes());
                bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
                bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
                bmp[18..22].copy_from_slice(&width.to_le_bytes());
                bmp[22..26].copy_from_slice(&(-height).to_le_bytes());
                bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
                bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
                for pixel in data.chunks_exact(3) {
                    bmp.extend_from_slice(pixel);
                    bmp.push(255);
                }
                std::fs::write(path.join(name), bmp).unwrap();
            }
        }
        serde_json::json!({"passed":count == 0,"different_pixels":count,
            "difference_bounds": if count == 0 { None } else { Some(bounds) },
            "max_channel_difference":maximum,"first_difference":first,
            "artifacts":if count == 0 { None } else { Some(path.to_string_lossy()) }})
    }
    #[test]
    fn pixel_diagnostics_save_exact_difference_and_pass_clean_frames() {
        unsafe {
            let path = std::env::temp_dir().join(format!(
                "dunyu-pixel-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let draw = |dc: HDC, reference: bool| {
                PatBlt(dc, 0, 0, 3, 2, BLACKNESS);
                if !reference {
                    SetPixelV(dc, 2, 1, 0x0000ff);
                }
            };
            let result = diagnose_frames(3, 2, &path, draw);
            assert_eq!(result["different_pixels"], 1);
            assert_eq!(result["difference_bounds"], serde_json::json!([2, 1, 3, 2]));
            assert_eq!(result["max_channel_difference"], 255);
            let bmp = std::fs::read(path.join("actual.bmp")).unwrap();
            assert_eq!(&bmp[..2], b"BM");
            assert_eq!(bmp.len(), 54 + 3 * 2 * 4);
            assert_eq!(&bmp[74..78], &[0, 0, 255, 255]);
            assert!(path.join("reference.bmp").is_file() && path.join("diff.bmp").is_file());
            let clean = path.join("clean");
            let result = diagnose_frames(3, 2, &clean, |dc, _| {
                PatBlt(dc, 0, 0, 3, 2, BLACKNESS);
            });
            assert_eq!(result["passed"], true);
            assert!(result["difference_bounds"].is_null());
            assert!(!clean.exists());
            std::fs::remove_dir_all(path).unwrap();
        }
    }
    pub(super) unsafe fn verify_real_control(h: HWND) {
        unsafe {
            let mut ptr = 0;
            assert_ne!(GetWindowSubclass(h, Some(procedure), SUBCLASS, &mut ptr), 0);
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(h, &mut bounds);
            let actual = Pixels::new(bounds.right, bounds.bottom);
            let expected = Pixels::new(bounds.right, bounds.bottom);
            let state = &*(ptr as *const PaintState);
            paint(h, actual.dc, state, true);
            let counters = super::super::ui::diagnostics::capture_counters(|| {
                paint(h, actual.dc, state, true);
            });
            paint(h, expected.dc, state, false);
            assert!(
                actual.rgb() == expected.rgb(),
                "real navigation pixels differ"
            );
            if !high_contrast() {
                assert_eq!(
                    counters.get("paint.tab.cache_copy").map(|v| v.0),
                    Some(5),
                    "real navigation cache unused: {}; {counters:?}",
                    navigation_geometry(h)
                );
                assert!(
                    !counters.contains_key("buffer.allocate"),
                    "warm navigation allocated"
                );
            }
        }
    }
    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn cached_navigation_matches_original() {
        unsafe {
            let graphics = TestGraphicsScope::new();
            InitCommonControls();
            let root = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Navigation cache verification").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                0,
                0,
                1100,
                250,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let _root_owner = TestWindow(root);
            let tab = CreateWindowExW(
                0,
                wide("SysTabControl32").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | TCS_FIXEDWIDTH,
                20,
                20,
                900,
                70,
                root,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!root.is_null() && !tab.is_null());
            attach(tab, Role::Tab, Theme::new());
            for (i, label) in super::super::ui::PAGES.iter().enumerate() {
                let mut label = wide(label);
                let entry = TCITEMW {
                    mask: TCIF_TEXT,
                    pszText: label.as_mut_ptr(),
                    ..std::mem::zeroed()
                };
                SendMessageW(tab, TCM_INSERTITEMW, i, &entry as *const _ as isize);
            }
            let mut ptr = 0;
            assert_ne!(
                GetWindowSubclass(tab, Some(procedure), SUBCLASS, &mut ptr),
                0
            );
            for dpi in [96, 120, 144, 192] {
                for (base_width, base_height) in [
                    (900, 51),
                    (900, 52),
                    (900, 70),
                    (960, 51),
                    (960, 52),
                    (960, 70),
                ] {
                    let width = base_width * dpi / 96;
                    let height = base_height * dpi / 96;
                    SetWindowPos(
                        root,
                        null_mut(),
                        0,
                        0,
                        width + 80,
                        height + 120,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    SetWindowPos(
                        tab,
                        null_mut(),
                        20,
                        20,
                        width,
                        height,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    let mut lf: LOGFONTW = std::mem::zeroed();
                    lf.lfHeight = -27 * dpi / 96;
                    let font = CreateFontIndirectW(&lf);
                    SendMessageW(tab, WM_SETFONT, font as usize, 0);
                    DeleteObject(font);
                    assert!(
                        (*(ptr as *const PaintState))
                            .tabs
                            .borrow()
                            .iter()
                            .all(Option::is_none)
                    );
                    scene(tab, dpi as u32, vec![], vec![]);
                    let item_width =
                        super::super::design::navigation_item_width(width, dpi as u32, 6);
                    SendMessageW(
                        tab,
                        TCM_SETITEMSIZE,
                        0,
                        ((51 * dpi / 96) << 16 | item_width) as isize,
                    );
                    let actual = Pixels::new(width, height);
                    let expected = Pixels::new(width, height);
                    for _ in 0..2 {
                        for page in 0..6 {
                            SendMessageW(tab, TCM_SETCURSEL, page, 0);
                            for focused in [false, true] {
                                SetFocus(if focused { tab } else { root });
                                assert_eq!(GetFocus() == tab, focused);
                                let state = &*(ptr as *const PaintState);
                                paint(tab, actual.dc, state, true);
                                canvas(tab);
                                SetBrushOrgEx(expected.dc, -20, -20, null_mut());
                                paint_tabs(tab, expected.dc, state, false);
                                let a = actual.rgb();
                                let b = expected.rgb();
                                assert!(
                                    a == b,
                                    "navigation mismatch at DPI {dpi}, width {width}, page {page}, focus {focused}; first byte {:?}",
                                    a.iter().zip(&b).position(|(a, b)| a != b)
                                );
                                let handles: Vec<_> = state
                                    .tabs
                                    .borrow()
                                    .iter()
                                    .map(|e| {
                                        e.as_ref()
                                            .and_then(|e| e.buffer.as_ref())
                                            .map(|b| b.dc_for_test())
                                    })
                                    .collect();
                                let counters =
                                    super::super::ui::diagnostics::capture_counters(|| {
                                        paint(tab, actual.dc, state, true);
                                    });
                                if !high_contrast() {
                                    assert_eq!(
                                        counters.get("paint.tab.cache_copy").map(|v| v.0),
                                        Some(5),
                                        "cache not used at DPI {dpi}, height {height}, page {page}: {counters:?}"
                                    );
                                }
                                assert!(actual.rgb() == b, "cached repaint differs");
                                assert_eq!(
                                    handles,
                                    state
                                        .tabs
                                        .borrow()
                                        .iter()
                                        .map(|e| e
                                            .as_ref()
                                            .and_then(|e| e.buffer.as_ref())
                                            .map(|b| b.dc_for_test()))
                                        .collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                    // Change background geometry and position without a font/theme
                    // message: key comparison alone must reject the old pixels.
                    SetWindowPos(
                        root,
                        null_mut(),
                        0,
                        0,
                        width + 100,
                        height + 160,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    SetWindowPos(
                        tab,
                        null_mut(),
                        30,
                        25,
                        width - 12,
                        height,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    let resized_actual = Pixels::new(width - 12, height);
                    let resized_expected = Pixels::new(width - 12, height);
                    let state = &*(ptr as *const PaintState);
                    paint(tab, resized_actual.dc, state, true);
                    canvas(tab);
                    SetBrushOrgEx(resized_expected.dc, -30, -25, null_mut());
                    paint_tabs(tab, resized_expected.dc, state, false);
                    assert!(
                        resized_actual.rgb() == resized_expected.rgb(),
                        "stale cache after move/resize"
                    );
                    SendMessageW(tab, WM_THEMECHANGED, 0, 0);
                    assert!(
                        (*(ptr as *const PaintState))
                            .tabs
                            .borrow()
                            .iter()
                            .all(Option::is_none)
                    );
                }
            }
            paint(
                tab,
                Pixels::new(1920, 140).dc,
                &*(ptr as *const PaintState),
                true,
            );
            let state = &mut *(ptr as *mut PaintState);
            let mut releases: Vec<_> = state
                .tabs
                .borrow_mut()
                .iter_mut()
                .enumerate()
                .filter_map(|(index, entry)| {
                    entry.as_mut()?.buffer.as_mut().map(|buffer| {
                        (format!("navigation item {index}"), buffer.observe_release())
                    })
                })
                .collect();
            if !high_contrast() {
                assert!(!releases.is_empty(), "no navigation caches to verify");
            }
            if let Some(buffer) = state.buffer.as_mut() {
                releases.push(("navigation paint buffer".into(), buffer.observe_release()));
            }
            assert_ne!(
                DestroyWindow(root),
                0,
                "DestroyWindow failed: {}",
                std::io::Error::last_os_error()
            );
            assert_eq!(IsWindow(root), 0, "root window remains after destruction");
            assert_eq!(IsWindow(tab), 0, "tab window remains after destruction");
            for (context, probe) in &releases {
                probe.assert_released(context);
            }
            println!(
                "Navigation checks completed; {} observed buffers released successfully. Cleanup follows.",
                releases.len()
            );
            drop(_root_owner);
            drop(graphics);
            println!("Navigation cleanup completed; awaiting test runner result.");
        }
    }
}

#[cfg(test)]
mod paper_tests {
    use super::*;

    #[test]
    #[ignore = "requires Windows GDI and an interactive desktop"]
    fn paper_cache_reuses_evicts_and_matches_pixels() {
        unsafe {
            let _graphics = TestGraphicsScope::new();
            let cache = RefCell::new(PaperCache::default());
            let probes = RefCell::new(Vec::new());
            let builds = std::cell::Cell::new(0);
            // The actual six-key working set: settings, fullscreen, then card.
            let keys = [
                (1024, 651, 0),
                (1024, 651, 3),
                (1920, 1080, 1),
                (1920, 1080, 3),
                (480, 310, 2),
                (480, 310, 3),
            ];
            for _ in 0..2 {
                for key in keys {
                    compare_frames_for_test(key.0, key.1, |dc, reference| {
                        let fresh;
                        let brush = if reference {
                            fresh = Paper::build(dc, key).expect("reference brush");
                            fresh.brush
                        } else {
                            cache
                                .borrow_mut()
                                .get_or_build(key, || {
                                    builds.set(builds.get() + 1);
                                    let mut paper = Paper::build(dc, key)?;
                                    let probe = Rc::new(std::cell::Cell::new(None));
                                    paper.release = Some(probe.clone());
                                    probes.borrow_mut().push(probe);
                                    Some(paper)
                                })
                                .expect("cached brush")
                        };
                        // Include the brush origin used by child controls.
                        SetBrushOrgEx(dc, -13, -27, null_mut());
                        assert_ne!(
                            FillRect(
                                dc,
                                &RECT {
                                    left: 0,
                                    top: 0,
                                    right: key.0,
                                    bottom: key.1
                                },
                                brush
                            ),
                            0
                        );
                    });
                }
            }
            assert_eq!(
                builds.get(),
                6,
                "warm scene switches must not rebuild paper"
            );
            assert_eq!(cache.borrow().entries.len(), 6);
            assert!(cache.borrow().bytes <= cache.borrow().budget);
            drop(cache);
            for probe in probes.into_inner() {
                assert_ne!(
                    probe.get().expect("paper destructor ran"),
                    0,
                    "DeleteObject failed"
                );
            }

            // Small real brushes with declared dimensions isolate accounting and
            // ownership tests from large allocations. Pixel rendering is above.
            let releases = RefCell::new(Vec::new());
            let make = || {
                let brush = CreateSolidBrush(rgb(BACKGROUND));
                assert!(!brush.is_null());
                let probe = Rc::new(std::cell::Cell::new(None));
                releases.borrow_mut().push(probe.clone());
                Some(Paper {
                    brush,
                    release: Some(probe),
                })
            };
            let mut cache = PaperCache::default();
            for key in keys.into_iter().chain([
                (3840, 2160, 1),
                (3840, 2160, 3),
                (2560, 1440, 1),
                (2560, 1440, 3),
            ]) {
                cache.get_or_build(key, &make).unwrap();
            }
            assert!(cache.bytes <= cache.budget);
            // 1080p + 4K + 1440p and the settings/card backgrounds all fit.
            assert_eq!(cache.entries.len(), 10);
            for key in keys {
                cache
                    .get_or_build(key, || panic!("unexpected eviction"))
                    .unwrap();
            }
            drop(cache);

            let mut cache = PaperCache {
                budget: 48,
                limit: 2,
                ..Default::default()
            };
            let a = (2, 2, 0);
            let b = (2, 2, 1);
            let c = (2, 2, 2);
            let original = cache.get_or_build(a, &make).unwrap();
            cache.get_or_build(b, &make).unwrap();
            assert_eq!(cache.get_or_build(a, || panic!("miss")), Some(original));
            cache.get_or_build(c, &make).unwrap();
            assert_ne!(
                releases.borrow()[11]
                    .get()
                    .expect("LRU victim released immediately"),
                0
            );
            assert_eq!(
                cache.entries.iter().map(|e| e.0).collect::<Vec<_>>(),
                vec![a, c]
            );
            assert_eq!(cache.get_or_build(b, || None), None);
            assert_eq!(
                cache.entries.len(),
                2,
                "failed creation must retain working set"
            );
            cache.get_or_build(b, &make).unwrap();
            cache.get_or_build((3, 3, 0), &make).unwrap();
            assert_eq!(cache.entries.len(), 1, "byte budget eviction");
            assert_eq!(cache.bytes, 36);
            cache.get_or_build((4, 4, 0), &make).unwrap();
            assert!(cache.temporary.is_some());
            assert_eq!(
                cache.bytes, 36,
                "oversized brush must not enter reusable cache"
            );
            cache.get_or_build((3, 3, 0), || panic!("miss")).unwrap();
            assert!(cache.temporary.is_none());
            drop(cache);
            for probe in releases.into_inner() {
                assert_ne!(probe.get().expect("all owned brushes released"), 0);
            }
            let unavailable = super::super::ui::diagnostics::sample_action(|| {});
            assert_eq!(unavailable["paper_cache"]["available"], false);
            let mut cache = PaperCache::default();
            let report = super::super::ui::diagnostics::sample_action(|| {
                cache
                    .get_or_build(a, || {
                        let brush = CreateSolidBrush(rgb(BACKGROUND));
                        assert!(!brush.is_null());
                        Some(Paper {
                            brush,
                            release: None,
                        })
                    })
                    .unwrap();
                cache
                    .get_or_build(a, || panic!("diagnostic warm miss"))
                    .unwrap();
            });
            assert_eq!(report["paper_cache"]["available"], true);
            assert_eq!(report["paper_cache"]["retained_bytes"], 16);
            assert_eq!(report["paper_cache"]["keys"]["2x2:scene0"]["builds"], 1);
            assert_eq!(report["paper_cache"]["keys"]["2x2:scene0"]["hits"], 1);
            println!("Paper cache reuse, pixel equality, budget, retry and release checks passed.");
        }
    }
}
