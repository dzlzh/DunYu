//! A recording session exists only while a hotkey field owns foreground focus.
use super::hotkey_diagnostics::{self as diagnostics, Event};
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
#[cfg(test)]
thread_local! { static HOOK_KEYS: RefCell<[usize; 256]> = const { RefCell::new([0; 256]) }; }
#[cfg(test)]
pub(super) fn hook_count(key: u32) -> usize {
    HOOK_KEYS.with(|keys| keys.borrow()[key as usize])
}
#[cfg(test)]
pub(super) fn generation_for_test() -> usize {
    SESSION.with(|s| s.borrow().generation)
}
// Keep the real recording session alive to exercise the message fallback in UI tests.
#[cfg(test)]
pub(super) unsafe fn disable_hook_for_test() -> usize {
    SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if !s.hook.is_null() {
            UnhookWindowsHookEx(s.hook);
            s.hook = null_mut();
        }
        s.generation
    })
}
fn display_value(s: &mut Session, key: u32, mods: u32) -> String {
    if key == 0 && mods != 0 {
        let text = ui::hotkey_text(&Hotkey::new(mods, 65));
        return format!("{}…", text.trim_end_matches('A'));
    }
    if mods != 0 && matches!(key, 8 | 0x20 | 0x25 | 0x27 | 0x30..=0x39 | 0x41..=0x5a | 0x70..=0x7a)
    {
        s.committed = ui::hotkey_text(&Hotkey::new(mods, key));
    }
    s.committed.clone()
}
// Both ordinary keyboard messages and WM_HOTKEY must commit before focus loss.
pub(super) unsafe fn fallback(field: HWND, key: u32, mods: u32) {
    let text = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.field == field {
            Some(display_value(&mut s, key, mods))
        } else if s.field.is_null() && key != 0 {
            // A field can still receive messages when the recorder could not start.
            let mut temporary = Session {
                committed: title(field),
                ..Default::default()
            };
            Some(display_value(&mut temporary, key, mods))
        } else {
            None
        }
    });
    if let Some(text) = text {
        SetWindowTextW(field, wide(&text).as_ptr());
    }
}
// Lifecycle events contain handles and IDs only; key events retain the selected-key filter.
unsafe fn trace(s: &Session, kind: &'static str, reason: &'static str, error: u32) {
    diagnostics::emit(Event {
        source: "recorder",
        kind,
        reason,
        error,
        field: s.field as usize,
        focus: GetFocus() as usize,
        control_id: GetDlgCtrlID(s.field),
        recording_session: s.generation,
        recording: !s.field.is_null(),
        count: u32::from(!s.hook.is_null()),
        ..Default::default()
    });
}
unsafe fn post(s: &Session, key: u32, mods: u32) {
    let result = PostMessageW(
        s.field,
        RECORDED,
        s.generation,
        ((key << 16) | mods) as isize,
    );
    let error = if result == 0 { GetLastError() } else { 0 };
    if key == 0 || diagnostics::watches_key(key) {
        diagnostics::emit(Event {
            source: "recorder",
            kind: "recording_post",
            key,
            modifiers: mods,
            recording_session: s.generation,
            field: s.field as usize,
            count: u32::from(result != 0),
            error,
            ..Default::default()
        });
    }
}
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
    stop_with_reason("start_replaces_session");
    if !ui::is_hotkey_field(GetDlgCtrlID(field)) {
        diagnostics::emit(Event {
            source: "recorder",
            kind: "recording_start_rejected",
            reason: "not_hotkey_field",
            field: field as usize,
            focus: GetFocus() as usize,
            control_id: GetDlgCtrlID(field),
            ..Default::default()
        });
        return;
    }
    super::priority_hotkey::RECORDING.store(true, std::sync::atomic::Ordering::SeqCst);
    super::priority_hotkey::RECORDING_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        s.field = field;
        s.committed = title(field);
        for vk in [0xa2, 0xa3, 0xa4, 0xa5, 0xa0, 0xa1, 0x5b, 0x5c] {
            s.down[modifier(vk).unwrap()] = GetAsyncKeyState(vk as i32) < 0;
        }
        trace(&s, "recording_start", "focus", 0);
        s.hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), GetModuleHandleW(null()), 0);
        let error = if s.hook.is_null() { GetLastError() } else { 0 };
        trace(
            &s,
            "recording_hook_install",
            if s.hook.is_null() {
                "failed"
            } else {
                "installed"
            },
            error,
        );
    });
}
pub(super) unsafe fn stop_for(field: HWND) {
    if SESSION.with(|s| s.borrow().field == field) {
        stop_with_reason("field_destroyed");
    }
}
pub(super) unsafe fn stop() {
    stop_with_reason("explicit_stop");
}
pub(super) unsafe fn stop_with_reason(reason: &'static str) {
    super::priority_hotkey::RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
    super::priority_hotkey::RECORDING_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let (hook, field, committed) = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        trace(&s, "recording_stop", reason, 0);
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
    #[cfg(test)]
    if matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN) && event.vkCode < 256 {
        HOOK_KEYS.with(|keys| keys.borrow_mut()[event.vkCode as usize] += 1);
    }
    let watched = diagnostics::watches_key(event.vkCode);
    let mut decision = Event {
        source: "recorder",
        kind: "recording_decision",
        callback_id: if watched {
            diagnostics::callback_id()
        } else {
            0
        },
        key: event.vkCode,
        scan_code: event.scanCode,
        input_time: event.time,
        message: w as u32,
        flags: event.flags,
        code,
        ..Default::default()
    };
    if watched {
        diagnostics::snapshot(&mut decision);
        diagnostics::emit(Event {
            kind: "recording_hook_entry",
            ..decision
        });
    }
    let handled = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if watched {
            decision.field = s.field as usize;
            decision.focus = GetFocus() as usize;
            decision.control_id = GetDlgCtrlID(s.field);
            decision.recording_session = s.generation;
            decision.recording = !s.field.is_null();
            decision.reason = if s.field.is_null() {
                "no_session"
            } else if GetFocus() != s.field {
                "focus_mismatch"
            } else if IsWindowVisible(s.field) == 0 {
                "field_hidden"
            } else if GetForegroundWindow() != GetAncestor(s.field, GA_ROOT) {
                "foreground_mismatch"
            } else {
                "accepted"
            };
        }
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
            decision.reason = "owned_repeat";
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
            decision.modifiers = modifiers(&s.down);
            post(&s, 0, decision.modifiers);
            return pressed || owned;
        }
        let mods = modifiers(&s.down);
        decision.modifiers = mods;
        if (event.vkCode == VK_TAB as u32 && mods & !MOD_SHIFT == 0)
            || (mods == 0 && matches!(event.vkCode, 0x0d | 0x1b))
        {
            decision.reason = "navigation_passthrough";
            return false;
        }
        if pressed {
            s.pressed.push(event.vkCode);
            post(&s, event.vkCode, mods);
        }
        pressed || owned
    });
    if watched {
        decision.consumed = handled;
        decision.duration_us = diagnostics::elapsed_since(decision.micros);
        diagnostics::emit(decision);
    }
    if handled {
        1
    } else {
        CallNextHookEx(null_mut(), code, w, l)
    }
}
pub(super) unsafe fn deliver(field: HWND, generation: usize, value: LPARAM) {
    let key = (value as u32) >> 16;
    let watched = key == 0 || diagnostics::watches_key(key);
    let mut delivery = Event {
        source: "recorder",
        kind: "recording_delivery",
        field: field as usize,
        focus: GetFocus() as usize,
        control_id: GetDlgCtrlID(field),
        recording_session: generation,
        key,
        modifiers: value as u32 & 15,
        reason: "accepted",
        ..Default::default()
    };
    let display = SESSION.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.field != field
            || s.generation != generation
            || GetFocus() != field
            || GetForegroundWindow() != GetAncestor(field, GA_ROOT)
        {
            delivery.reason = if s.field != field {
                "field_mismatch"
            } else if s.generation != generation {
                "stale_generation"
            } else if GetFocus() != field {
                "focus_mismatch"
            } else {
                "foreground_mismatch"
            };
            return None;
        }
        let mods = value as u32 & 15;
        let before = s.committed.clone();
        let text = display_value(&mut s, key, mods);
        if s.committed != before {
            delivery.reason = "committed";
        }
        Some(text)
    });
    if watched {
        diagnostics::emit(delivery);
    }
    if let Some(text) = display {
        let result = SetWindowTextW(field, wide(&text).as_ptr());
        let error = if result == 0 { GetLastError() } else { 0 };
        if watched {
            diagnostics::emit(Event {
                kind: "recording_display",
                count: u32::from(result != 0),
                error,
                ..delivery
            });
        }
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
