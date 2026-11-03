use super::*;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
pub(super) const TIMER: usize = 903;
pub(super) unsafe fn filter(msg: &MSG) -> bool {
    APP.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(a) = guard.as_mut() else {
            return false;
        };
        let Some(u) = a.ui.as_mut() else { return false };
        if GetAncestor(msg.hwnd, GA_ROOT) != u.window {
            return false;
        }
        if u.dragging && GetCapture() != ui::item(u.window, 120) {
            u.dragging = false;
        }
        let divider = GetDlgCtrlID(msg.hwnd) == 120;
        if divider && msg.message == WM_MOUSEMOVE {
            SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
        }
        if divider && matches!(msg.message, WM_LBUTTONDOWN | WM_LBUTTONDBLCLK) {
            u.dragging = true;
            SetCapture(msg.hwnd);
            if msg.message == WM_LBUTTONDBLCLK {
                u.split_ratio = super::design::SPLIT;
                u.dragging = false;
                ReleaseCapture();
                if let Err(e) = atomic_json(&a.dir.join("ui-state.json"), &u.split_ratio) {
                    a.status_error(&format!("布局偏好未保存：{e}"));
                }
                a.layout_ui();
            }
            return true;
        }
        if u.dragging && matches!(msg.message, WM_MOUSEMOVE | WM_LBUTTONUP) {
            let mut point: POINT = std::mem::zeroed();
            GetCursorPos(&mut point);
            ScreenToClient(u.content, &mut point);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(u.content, &mut r);
            let width = r.right * 96 / u.dpi as i32;
            u.split_ratio = ((point.x * 96 / u.dpi as i32 - 24) as f64
                / (width - 68).max(1) as f64)
                .clamp(0.1, 0.8);
            if msg.message == WM_LBUTTONUP {
                u.dragging = false;
                ReleaseCapture();
                if let Err(e) = atomic_json(&a.dir.join("ui-state.json"), &u.split_ratio) {
                    a.status_error(&format!("布局偏好未保存：{e}"));
                }
            }
            a.layout_ui();
            return true;
        }
        if msg.message != WM_MOUSEWHEEL {
            return false;
        }
        let p = POINT {
            x: msg.lParam as i16 as i32,
            y: (msg.lParam >> 16) as i16 as i32,
        };
        let mut content: RECT = std::mem::zeroed();
        GetWindowRect(u.content, &mut content);
        if p.x < content.left || p.x >= content.right || p.y < content.top || p.y >= content.bottom
        {
            return false;
        }
        let keywords = ui::item(u.window, 911);
        let mut keyword_rect: RECT = std::mem::zeroed();
        GetWindowRect(keywords, &mut keyword_rect);
        if u.page == 4
            && p.x >= keyword_rect.left
            && p.x < keyword_rect.right
            && p.y >= keyword_rect.top
            && p.y < keyword_rect.bottom
        {
            let mut lines = 3u32;
            SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &mut lines as *mut _ as _, 0);
            let lines = if lines == u32::MAX { 3 } else { lines };
            u.keyword_wheel -= ((msg.wParam >> 16) as i16 as f64) / 120. * lines as f64;
            let count = u.keyword_wheel.trunc() as i32;
            u.keyword_wheel -= count as f64;
            SendMessageW(
                keywords,
                windows_sys::Win32::UI::Controls::EM_LINESCROLL,
                0,
                count as isize,
            );
            return true;
        }
        let mut clip: RECT = std::mem::zeroed();
        GetWindowRect(ui::item(u.window, 119), &mut clip);
        let list = u.page == 0
            && p.x >= clip.left
            && p.x < clip.right + 16 * u.dpi as i32 / 96
            && p.y >= clip.top
            && p.y < clip.bottom;
        let bar = ui::item(u.window, if list { 9004 } else { 9003 });
        let mut info: SCROLLINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        info.fMask = SIF_ALL;
        GetScrollInfo(bar, SB_CTL, &mut info);
        let max = (info.nMax - info.nPage as i32 + 1).max(0) as f64;
        let mut lines = 3u32;
        SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &mut lines as *mut _ as _, 0);
        let unit = if lines == u32::MAX {
            info.nPage as f64 * 0.9
        } else {
            lines as f64 * if list { 64. } else { 24. }
        };
        let delta = -((msg.wParam >> 16) as i16 as f64) / 120. * unit;
        let motion = &mut u.motions[usize::from(list)];
        let actual = if list { u.list_scroll } else { u.scroll } as f64;
        if (motion.current - actual).abs() > 1. {
            motion.snap(actual);
        }
        motion.add(delta, max, GetTickCount64(), rest_ui::motion_enabled());
        if motion.current == actual && motion.target == actual {
            return true;
        }
        SetTimer(a.host, TIMER, 16, None);
        a.scroll_frame();
        true
    })
}
impl App {
    pub(super) unsafe fn scroll_frame(&mut self) {
        let Some(u) = self.ui.as_mut() else {
            KillTimer(self.host, TIMER);
            return;
        };
        let now = GetTickCount64();
        let mut active = false;
        for motion in &mut u.motions {
            active |= motion.tick(now);
        }
        let next = (
            u.motions[0].current.round() as i32,
            u.motions[1].current.round() as i32,
        );
        let changed = next != (u.scroll, u.list_scroll);
        (u.scroll, u.list_scroll) = next;
        if !active {
            KillTimer(self.host, TIMER);
        }
        if changed {
            self.layout_scrolled_ui();
        }
    }
}
