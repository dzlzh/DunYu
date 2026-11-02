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
                GdiplusShutdown(self.token);
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
struct PaintState {
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
        state.dpi = dpi;
        state.cards = cards;
        state.fields = fields;
    }
    InvalidateRect(h, null(), 0);
}
struct Paper(HBRUSH);
impl Drop for Paper {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
thread_local! {
    static PAPER: RefCell<HashMap<(i32,i32,i32), Paper>> = RefCell::new(HashMap::new());
    static CANVAS: std::cell::Cell<(i32,i32,i32)> = const { std::cell::Cell::new((1024,650,0)) };
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
    PAPER.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&size) {
            if cache.len() >= 4 {
                cache.clear();
            }
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, size.0, size.1);
            let old = SelectObject(memory, bitmap);
            super::assets::background(memory, size.0, size.1, size.2);
            let brush = CreatePatternBrush(bitmap);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            cache.insert(size, Paper(brush));
        }
        cache[&size].0
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
    let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
    let mut metrics: TEXTMETRICW = std::mem::zeroed();
    GetTextMetricsW(dc, &mut metrics);
    SelectObject(dc, old);
    ReleaseDC(h, dc);
    metrics.tmHeight
}
pub(super) unsafe fn label_size(h: HWND) -> SIZE {
    let dc = GetDC(h);
    let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
    let label = wide(&title(h));
    let mut size: SIZE = std::mem::zeroed();
    GetTextExtentPoint32W(dc, label.as_ptr(), (label.len() - 1) as i32, &mut size);
    if !old.is_null() {
        SelectObject(dc, old);
    }
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
    let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
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
    if !old.is_null() {
        SelectObject(dc, old);
    }
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
unsafe fn paint(h: HWND, dc: HDC, s: &PaintState) {
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
            let face = wide("Source Han Serif SC");
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
        Role::Tab => {
            fill(dc, r, BACKGROUND);
            let selected = SendMessageW(h, TCM_GETCURSEL, 0, 0);
            for i in 0..super::ui::PAGES.len() {
                let mut item: RECT = std::mem::zeroed();
                SendMessageW(h, TCM_GETITEMRECT, i, &mut item as *mut _ as isize);
                if selected == i as isize {
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
                if selected == i as isize && GetFocus() == h {
                    rounded(dc, inset(item, px(2)), px(8), ACCENT_TEXT);
                    rounded(dc, inset(item, px(4)), px(6), SELECTION);
                }
                let fg = if selected == i as isize {
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
                item.left = left + px(44);
                item.right = item.left + size.cx;
                text(dc, h, label, item, fg, DT_LEFT);
            }
        }
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
                "Source Han Serif SC"
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
                super::recorder::stop();
            } else {
                super::recorder::start(GetFocus());
            }
        }
        WM_SHOWWINDOW if w == 0 && role == Role::Surface => {
            super::recorder::stop();
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
                    super::recorder::stop();
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
            let dc = if m == WM_PAINT {
                BeginPaint(h, &mut ps)
            } else {
                w as HDC
            };
            let saved = SaveDC(dc);
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(h, &mut bounds);
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, bounds.right.max(1), bounds.bottom.max(1));
            if !memory.is_null() && !bitmap.is_null() {
                let original = SelectObject(memory, bitmap);
                paint(h, memory, &*ptr);
                BitBlt(dc, 0, 0, bounds.right, bounds.bottom, memory, 0, 0, SRCCOPY);
                SelectObject(memory, original);
            } else {
                paint(h, dc, &*ptr);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !memory.is_null() {
                DeleteDC(memory);
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
