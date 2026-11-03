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
