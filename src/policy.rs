pub fn opacity_step(current: u8, direction: i8, step: u8) -> u8 {
    (current as i32 + direction as i32 * (255 * step as i32 + 50) / 100).clamp(26, 255) as u8
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleAction {
    Hide,
    Minimize,
    Activate,
}
pub fn toggle_action(
    foreground: bool,
    visible: bool,
    iconic: bool,
    minimize: bool,
) -> ToggleAction {
    if foreground && visible && !iconic {
        if minimize {
            ToggleAction::Minimize
        } else {
            ToggleAction::Hide
        }
    } else {
        ToggleAction::Activate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreground_hides_only_when_visible_and_restored() {
        for foreground in [false, true] {
            for visible in [false, true] {
                for iconic in [false, true] {
                    let expected = if foreground && visible && !iconic {
                        ToggleAction::Hide
                    } else {
                        ToggleAction::Activate
                    };
                    assert_eq!(toggle_action(foreground, visible, iconic, false), expected);
                }
            }
        }
    }
    #[test]
    fn minimize_is_optional() {
        assert_eq!(
            toggle_action(true, true, false, true),
            ToggleAction::Minimize
        );
        assert_eq!(
            toggle_action(false, true, false, true),
            ToggleAction::Activate
        );
    }
    #[test]
    fn opacity_stays_in_safe_range() {
        for alpha in 0..=255 {
            for step in 1..=50 {
                for d in [-1, 1] {
                    assert!((26..=255).contains(&opacity_step(alpha, d, step)));
                }
            }
        }
    }
    #[test]
    fn opacity_direction_and_rounding() {
        assert_eq!(opacity_step(255, -1, 10), 229);
        assert_eq!(opacity_step(100, 1, 10), 126);
        assert_eq!(opacity_step(26, -1, 50), 26);
        assert_eq!(opacity_step(250, 1, 10), 255);
    }
}
