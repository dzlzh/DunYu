//! Native control identities and input handling stay with Windows. Painting owns
//! its own snapshots so synchronous paint messages never borrow the business App.
use super::*;
use std::rc::Rc;
use windows_sys::Win32::Graphics::GdiPlus::*;
use windows_sys::Win32::UI::Controls::*;

pub(super) const BACKGROUND: u32 = 0x0f1115;
const SURFACE: u32 = 0x171b22;
const WHITE: u32 = 0xffffff;
const INK: u32 = 0xe5e7eb;
const MUTED: u32 = 0xa1aab8;
const BLUE: u32 = 0x2563eb;
const ACCENT_TEXT: u32 = 0x79b8ff;
const LINE: u32 = 0x343d4b;
const PALE: u32 = 0x203658;
fn rgb(hex: u32) -> u32 {
    (hex & 255) << 16 | hex & 0xff00 | hex >> 16
}
unsafe fn high_contrast() -> bool {
    let mut hc: HIGHCONTRASTW = std::mem::zeroed();
    hc.cbSize = std::mem::size_of_val(&hc) as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0);
    hc.dwFlags & HCF_HIGHCONTRASTON != 0
}
unsafe fn color(value: u32) -> u32 {
    if high_contrast() {
        GetSysColor(match value {
            INK | MUTED => COLOR_WINDOWTEXT,
            BLUE => COLOR_HIGHLIGHT,
            ACCENT_TEXT => COLOR_WINDOWTEXT,
            SURFACE => COLOR_WINDOW,
            WHITE => COLOR_HIGHLIGHTTEXT,
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
}
struct PaintState {
    _theme: Rc<Theme>,
    role: Role,
    hot: bool,
    tint: Option<u32>,
    dpi: u32,
    cards: Vec<RECT>,
    fields: Vec<(HWND, RECT)>,
}
const SUBCLASS: usize = 0x44554e59;
pub(super) unsafe fn attach(h: HWND, role: Role, theme: Rc<Theme>) {
    let state = Box::new(PaintState {
        _theme: theme,
        role,
        hot: false,
        tint: None,
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
unsafe fn fill(dc: HDC, r: RECT, value: u32) {
    let b = CreateSolidBrush(color(value));
    FillRect(dc, &r, b);
    DeleteObject(b);
}
unsafe fn rounded(dc: HDC, r: RECT, radius: i32, value: u32) {
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
unsafe fn text(dc: HDC, h: HWND, s: &str, mut r: RECT, value: u32, flags: u32) {
    let old = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color(value));
    let text = wide(s);
    DrawTextW(
        dc,
        text.as_ptr(),
        -1,
        &mut r,
        flags | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );
    if !old.is_null() {
        SelectObject(dc, old);
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
    let paths: &[&[(f32, f32)]] = match index {
        0 => &[
            &[(13., 4.), (4., 4.), (4., 20.), (13., 20.)],
            &[(10., 12.), (21., 12.), (17., 8.)],
            &[(21., 12.), (17., 16.)],
        ],
        1 => &[
            &[
                (8., 3.),
                (16., 3.),
                (15., 10.),
                (19., 16.),
                (5., 16.),
                (9., 10.),
                (8., 3.),
            ],
            &[(12., 16.), (12., 22.)],
        ],
        2 => &[&[
            (12., 2.),
            (19., 5.),
            (22., 12.),
            (19., 19.),
            (12., 22.),
            (5., 19.),
            (2., 12.),
            (5., 5.),
            (12., 2.),
            (12., 22.),
        ]],
        _ => &[
            &[(3., 6.), (21., 6.)],
            &[(3., 12.), (21., 12.)],
            &[(3., 18.), (21., 18.)],
            &[(8., 3.), (8., 9.)],
            &[(16., 9.), (16., 15.)],
            &[(8., 15.), (8., 21.)],
        ],
    };
    for path in paths {
        for segment in path.windows(2) {
            GdipDrawLine(
                graphics,
                pen,
                r.left as f32 + segment[0].0 * unit,
                r.top as f32 + segment[0].1 * unit,
                r.left as f32 + segment[1].0 * unit,
                r.top as f32 + segment[1].1 * unit,
            );
        }
    }
    GdipDeletePen(pen);
    GdipDeleteGraphics(graphics);
}
unsafe fn paint(h: HWND, dc: HDC, s: &PaintState) {
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(h, &mut r);
    let px = |v: i32| v * s.dpi as i32 / 96;
    match s.role {
        Role::Brand => {
            fill(dc, r, BACKGROUND);
            let logo = LoadImageW(
                GetModuleHandleW(null()),
                101usize as _,
                IMAGE_ICON,
                px(32),
                px(32),
                LR_SHARED,
            );
            DrawIconEx(
                dc,
                0,
                (r.bottom - px(32)) / 2,
                logo,
                px(32),
                px(32),
                0,
                null_mut(),
                DI_NORMAL,
            );
            let mut label = r;
            label.left += px(38);
            text(dc, h, "遁鱼", label, INK, DT_LEFT);
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
            rounded(dc, box_rect, px(4), if checked { BLUE } else { MUTED });
            if checked {
                text(dc, h, "✓", box_rect, WHITE, DT_CENTER);
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
            fill(dc, r, BACKGROUND);
            for card in &s.cards {
                rounded(dc, *card, px(12), LINE);
                rounded(dc, inset(*card, px(1).max(1)), px(11), SURFACE);
            }
            for (field, rect) in &s.fields {
                let focused = GetFocus() == *field;
                rounded(dc, *rect, px(8), if focused { BLUE } else { LINE });
                rounded(
                    dc,
                    inset(*rect, px(if focused { 2 } else { 1 }).max(1)),
                    px(7),
                    SURFACE,
                );
            }
        }
        Role::Button => {
            fill(
                dc,
                r,
                if matches!(GetDlgCtrlID(h), 113 | 330 | 506) {
                    BACKGROUND
                } else {
                    SURFACE
                },
            );
            let primary = matches!(GetDlgCtrlID(h), 113 | 330 | 506);
            let down = SendMessageW(h, BM_GETSTATE, 0, 0) as u32 & BST_PUSHED != 0;
            let enabled = IsWindowEnabled(h) != 0;
            let bg = if !enabled {
                BACKGROUND
            } else if primary {
                if down {
                    0x1e40af
                } else if s.hot {
                    0x1d4ed8
                } else {
                    BLUE
                }
            } else if down {
                LINE
            } else if s.hot {
                PALE
            } else {
                SURFACE
            };
            let outer = inset(r, px(2));
            rounded(
                dc,
                outer,
                px(8),
                if GetFocus() == h {
                    BLUE
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
                inset(r, px(12)),
                if !enabled {
                    MUTED
                } else if primary {
                    WHITE
                } else if GetDlgCtrlID(h) == 111 {
                    0xfb7185
                } else {
                    INK
                },
                DT_CENTER,
            );
        }
        Role::Tab => {
            fill(dc, r, BACKGROUND);
            let selected = SendMessageW(h, TCM_GETCURSEL, 0, 0);
            for i in 0..4 {
                let mut item: RECT = std::mem::zeroed();
                SendMessageW(h, TCM_GETITEMRECT, i, &mut item as *mut _ as isize);
                if selected == i as isize {
                    rounded(dc, inset(item, px(2)), px(8), PALE);
                }
                if selected == i as isize && GetFocus() == h {
                    let mut line = item;
                    line.top = line.bottom - px(3);
                    fill(dc, line, BLUE);
                }
                let fg = if selected == i as isize {
                    ACCENT_TEXT
                } else {
                    MUTED
                };
                let label = ["召唤", "绑定", "隐现", "设置"][i];
                let font = SelectObject(dc, SendMessageW(h, WM_GETFONT, 0, 0) as _);
                let mut size: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc, wide(label).as_ptr(), 2, &mut size);
                SelectObject(dc, font);
                let left = (item.left + item.right - px(24) - size.cx) / 2;
                let top = (item.top + item.bottom - px(16)) / 2;
                icon(
                    dc,
                    RECT {
                        left,
                        top,
                        right: left + px(16),
                        bottom: top + px(16),
                    },
                    i,
                    fg,
                );
                item.left = left + px(24);
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
        RemoveWindowSubclass(h, Some(procedure), SUBCLASS);
        drop(Box::from_raw(ptr));
        return DefSubclassProc(h, m, w, l);
    }
    let role = (*ptr).role;
    match m {
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
                color(tint.unwrap_or(if id == 9001 { MUTED } else { INK })),
            );
            let backdrop = if matches!(id, 9001 | 9002 | 7001 | 7002) {
                BACKGROUND
            } else {
                SURFACE
            };
            SetBkColor(dc, color(backdrop));
            SetDCBrushColor(dc, color(backdrop));
            return GetStockObject(DC_BRUSH) as isize;
        }
        WM_DRAWITEM if l != 0 => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            if d.CtlID == 100 && d.itemID != u32::MAX {
                let selected = d.itemState & ODS_SELECTED != 0;
                let px = |v: i32| v * (*ptr).dpi as i32 / 96;
                fill(d.hDC, d.rcItem, SURFACE);
                if selected {
                    rounded(d.hDC, inset(d.rcItem, px(4)), px(8), PALE);
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
                        if selected { ACCENT_TEXT } else { INK },
                        DT_LEFT,
                    );
                    r.top += px(24);
                    r.bottom += px(20);
                    text(
                        d.hDC,
                        d.hwndItem,
                        lines.next().unwrap_or(""),
                        r,
                        MUTED,
                        DT_LEFT,
                    );
                }
                if d.itemState & ODS_FOCUS != 0 {
                    DrawFocusRect(d.hDC, &inset(d.rcItem, px(5)));
                }
                return 1;
            }
        }
        WM_NOTIFY if l != 0 => {
            let hdr = &*(l as *const NMHDR);
            if hdr.idFrom == 402 && hdr.code == NM_CUSTOMDRAW {
                let draw = &*(l as *const NMCUSTOMDRAW);
                if draw.dwDrawStage == CDDS_PREPAINT {
                    return CDRF_NOTIFYITEMDRAW as isize;
                }
                if draw.dwDrawStage == CDDS_ITEMPREPAINT {
                    let px = |v: i32| v * (*ptr).dpi as i32 / 96;
                    if draw.dwItemSpec == TBCD_CHANNEL as usize {
                        let mut r = draw.rc;
                        let mid = (r.top + r.bottom) / 2;
                        r.top = mid - px(3);
                        r.bottom = mid + px(3);
                        rounded(draw.hdc, r, px(3), LINE);
                        let value = SendMessageW(hdr.hwndFrom, WM_USER, 0, 0).clamp(10, 100);
                        r.right = r.left + ((r.right - r.left) as isize * (value - 10) / 90) as i32;
                        if r.right > r.left {
                            rounded(draw.hdc, r, px(3), BLUE);
                        }
                        return CDRF_SKIPDEFAULT as isize;
                    }
                    if draw.dwItemSpec == TBCD_THUMB as usize {
                        rounded(
                            draw.hdc,
                            draw.rc,
                            px(8),
                            if IsWindowEnabled(hdr.hwndFrom) != 0 {
                                BLUE
                            } else {
                                MUTED
                            },
                        );
                        rounded(draw.hdc, inset(draw.rc, px(3)), px(5), WHITE);
                        return CDRF_SKIPDEFAULT as isize;
                    }
                }
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
    capture_impl(h, path, false)
}

#[cfg(test)]
pub(super) unsafe fn capture_screen(h: HWND, path: &str) -> std::io::Result<()> {
    capture_impl(h, path, true)
}

#[cfg(test)]
unsafe fn capture_impl(h: HWND, path: &str, actual: bool) -> std::io::Result<()> {
    use std::io::Write;
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(h, &mut r);
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    let screen = GetWindowDC(h);
    let dc = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    let old = SelectObject(dc, bitmap);
    let result = (|| {
        let captured = if actual {
            BitBlt(dc, 0, 0, width, height, screen, 0, 0, SRCCOPY)
        } else {
            windows_sys::Win32::Storage::Xps::PrintWindow(h, dc, 0)
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
