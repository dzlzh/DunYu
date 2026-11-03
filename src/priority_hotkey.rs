//! Key ownership is retained until release, even when bindings change.
use crate::{Command, Hotkey};
#[derive(Default)]
pub struct KeyState {
    owned: std::collections::HashSet<u32>,
}
impl KeyState {
    pub fn event(
        &mut self,
        key: u32,
        down: bool,
        repeated: bool,
        modifiers: u32,
        routes: &[(Hotkey, Command)],
    ) -> (bool, Option<Command>) {
        if !down {
            return (self.owned.remove(&key), None);
        }
        if self.owned.contains(&key) {
            return (true, None);
        }
        if repeated {
            return (false, None);
        }
        if let Some((_, command)) = routes
            .iter()
            .find(|(h, _)| h.key == key && h.modifiers == modifiers)
        {
            self.owned.insert(key);
            (true, Some(*command))
        } else {
            (false, None)
        }
    }
    pub fn clear(&mut self) {
        self.owned.clear();
    }
    pub fn release(&mut self, key: u32) {
        self.owned.remove(&key);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_repeat_and_modifier_release() {
        let routes = vec![(Hotkey::new(8, 69), Command::Summon(2))];
        let mut s = KeyState::default();
        assert_eq!(s.event(69, true, false, 10, &routes), (false, None));
        assert_eq!(
            s.event(69, true, false, 8, &routes),
            (true, Some(Command::Summon(2)))
        );
        assert_eq!(s.event(69, true, true, 0, &[]), (true, None));
        assert_eq!(s.event(69, false, false, 0, &[]), (true, None));
        assert_eq!(s.event(69, true, false, 0, &routes), (false, None));
        assert_eq!(s.event(69, true, true, 8, &routes), (false, None));
    }
    #[test]
    fn recording_reset_does_not_swallow_other_keys() {
        let routes = vec![(Hotkey::new(2, 65), Command::Summon(0))];
        let mut s = KeyState::default();
        s.event(65, true, false, 2, &routes);
        s.clear();
        assert_eq!(s.event(65, false, false, 2, &routes), (false, None));
        assert_eq!(s.event(66, true, false, 2, &routes), (false, None));
    }
}

/// Delivery policy shared by the native callback and fault-injection tests.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Accepted,
    MaskFailed,
    PostFailed,
}
pub fn deliver(
    modifiers: u32,
    mask: impl FnOnce() -> bool,
    post: impl FnOnce() -> bool,
) -> Delivery {
    if modifiers & (8 | 1) != 0 && !mask() {
        return Delivery::MaskFailed;
    }
    if !post() {
        return Delivery::PostFailed;
    }
    Delivery::Accepted
}
#[cfg(test)]
mod delivery_tests {
    use super::*;
    #[test]
    fn failed_mask_does_not_deliver_and_failed_post_is_distinct() {
        assert_eq!(
            deliver(8, || false, || panic!("must not post")),
            Delivery::MaskFailed
        );
        assert_eq!(deliver(8, || true, || false), Delivery::PostFailed);
        assert_eq!(deliver(8, || true, || true), Delivery::Accepted);
        assert_eq!(
            deliver(2, || panic!("Ctrl needs no menu mask"), || true),
            Delivery::Accepted
        );
    }
    #[test]
    fn input_classification_and_delivery_keep_release_ownership() {
        let routes = [(crate::Hotkey::new(8, 81), crate::Command::Summon(0))];
        for failed in [true, false] {
            let mut state = KeyState::default();
            assert_eq!(state.event(81, true, false, 0, &routes), (false, None));
            let (handled, command) = state.event(81, true, false, 8, &routes);
            assert!(handled && command.is_some());
            if deliver(8, || !failed, || true) != Delivery::Accepted {
                state.release(81);
            }
            assert_eq!(state.event(81, false, false, 8, &routes), (!failed, None));
        }
    }
}
