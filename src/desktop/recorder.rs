//! A recording session exists only while a hotkey field owns foreground focus.
use super::*;
pub(super) const RECORDED: u32 = WM_APP + 80;
#[derive(Default)]
struct Session {
    hook: HHOOK,
    field: HWND,
    generation: usize,
    down: [bool; 8],
    committed: String,
    pressed: Vec<u32>,
}
thread_local! { static SESSION: RefCell<Session> = RefCell::new(Session::default()); }
fn modifier(vk: u32) -> Option<usize> {
    match vk {
        0xa2 => Some(0),
        0xa3 => Some(1),
        0xa4 => Some(2),
        0xa5 => Some(3),
        0xa0 => Some(4),
        0xa1 => Some(5),
        0x5b => Some(6),
        0x5c => Some(7),
        _ => None,
    }
}
fn modifiers(down: &[bool; 8]) -> u32 {
    [MOD_CONTROL, MOD_ALT, MOD_SHIFT, MOD_WIN]
        .iter()
        .enumerate()
        .filter(|(i, _)| down[i * 2] || down[i * 2 + 1])
        .fold(0, |v, (_, flag)| v | flag)
}
pub(super) unsafe fn start(field: HWND) {
    stop();
    if !ui::is_hotkey_field(GetDlgCtrlID(field)) {
        return;
    }
    SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        s.field = field;
        s.committed = title(field);
        for vk in [0xa2, 0xa3, 0xa4, 0xa5, 0xa0, 0xa1, 0x5b, 0x5c] {
            s.down[modifier(vk).unwrap()] = GetAsyncKeyState(vk as i32) < 0;
        }
        s.hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), GetModuleHandleW(null()), 0);
    });
}
pub(super) unsafe fn stop_for(field: HWND) {
    if SESSION.with(|s| s.borrow().field == field) {
        stop();
    }
}
pub(super) unsafe fn stop() {
    let (hook, field, committed) = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        let old = (s.hook, s.field, std::mem::take(&mut s.committed));
        s.hook = null_mut();
        s.field = null_mut();
        s.down = [false; 8];
        s.pressed.clear();
        s.generation = s.generation.wrapping_add(1);
        old
    });
    if !hook.is_null() {
        UnhookWindowsHookEx(hook);
    }
    if !field.is_null() && IsWindow(field) != 0 {
        SetWindowTextW(field, wide(&committed).as_ptr());
    }
}
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(null_mut(), code, w, l);
    }
    let event = &*(l as *const KBDLLHOOKSTRUCT);
    let handled = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.field.is_null()
            || GetFocus() != s.field
            || IsWindowVisible(s.field) == 0
            || GetForegroundWindow() != GetAncestor(s.field, GA_ROOT)
        {
            return false;
        }
        let pressed = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let owned = s.pressed.contains(&event.vkCode);
        if pressed && owned {
            return true;
        }
        if !pressed {
            s.pressed.retain(|key| *key != event.vkCode);
        }
        if let Some(i) = modifier(event.vkCode) {
            if pressed {
                s.pressed.push(event.vkCode);
            }
            s.down[i] = pressed;
            PostMessageW(s.field, RECORDED, s.generation, modifiers(&s.down) as isize);
            return pressed || owned;
        }
        let mods = modifiers(&s.down);
        if (event.vkCode == VK_TAB as u32 && mods & !MOD_SHIFT == 0)
            || (mods == 0 && matches!(event.vkCode, 0x0d | 0x1b))
        {
            return false;
        }
        if pressed {
            s.pressed.push(event.vkCode);
            PostMessageW(
                s.field,
                RECORDED,
                s.generation,
                ((event.vkCode << 16) | mods) as isize,
            );
        }
        pressed || owned
    });
    if handled {
        1
    } else {
        CallNextHookEx(null_mut(), code, w, l)
    }
}
pub(super) unsafe fn deliver(field: HWND, generation: usize, value: LPARAM) {
    let display = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.field != field
            || s.generation != generation
            || GetFocus() != field
            || GetForegroundWindow() != GetAncestor(field, GA_ROOT)
        {
            return None;
        }
        let key = (value as u32) >> 16;
        let mods = value as u32 & 15;
        if key == 0 {
            if mods == 0 {
                return Some(s.committed.clone());
            }
            let text = ui::hotkey_text(&Hotkey::new(mods, 65));
            return Some(format!("{}…", text.trim_end_matches('A')));
        }
        if mods != 0
            && matches!(key, 8 | 0x20 | 0x25 | 0x27 | 0x30..=0x39 | 0x41..=0x5a | 0x70..=0x7a)
        {
            s.committed = ui::hotkey_text(&Hotkey::new(mods, key));
            Some(s.committed.clone())
        } else {
            Some(s.committed.clone())
        }
    });
    if let Some(text) = display {
        SetWindowTextW(field, wide(&text).as_ptr());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn releasing_one_windows_key_keeps_other_modifier() {
        let mut down = [false; 8];
        down[6] = true;
        down[7] = true;
        assert_eq!(modifiers(&down), MOD_WIN);
        down[6] = false;
        assert_eq!(modifiers(&down), MOD_WIN);
        down[0] = true;
        assert_eq!(modifiers(&down), MOD_WIN | MOD_CONTROL);
        down[7] = false;
        assert_eq!(modifiers(&down), MOD_CONTROL);
    }
}
