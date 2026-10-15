//! 显示策略独立于 Win32；老板键隐藏优先于鼠标自动显示。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    BossKey,
    Mouse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Visible,
    HidePending(Reason),
    Hidden(Reason),
    Disabled,
    Destroyed,
}

impl State {
    pub fn boss_toggle(self) -> Self {
        match self {
            Self::Visible | Self::HidePending(Reason::Mouse) => Self::HidePending(Reason::BossKey),
            // 自动隐藏后按老板键，进入主动隐藏，鼠标不再唤回窗口。
            Self::Hidden(Reason::Mouse) => Self::Hidden(Reason::BossKey),
            Self::HidePending(Reason::BossKey) | Self::Hidden(Reason::BossKey) => Self::Visible,
            other => other,
        }
    }

    pub fn mouse(self, inside: bool) -> Self {
        match (self, inside) {
            (Self::Visible, false) => Self::HidePending(Reason::Mouse),
            (Self::HidePending(Reason::Mouse) | Self::Hidden(Reason::Mouse), true) => Self::Visible,
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_leaves_and_returns_to_hidden_area() {
        assert_eq!(State::Visible.mouse(true), State::Visible);
        assert_eq!(
            State::Visible.mouse(false),
            State::HidePending(Reason::Mouse)
        );
        assert_eq!(
            State::HidePending(Reason::Mouse).mouse(true),
            State::Visible
        );
        assert_eq!(
            State::Hidden(Reason::Mouse).mouse(false),
            State::Hidden(Reason::Mouse)
        );
        assert_eq!(State::Hidden(Reason::Mouse).mouse(true), State::Visible);
    }

    #[test]
    fn boss_key_hiding_is_never_reversed_by_mouse() {
        for state in [
            State::HidePending(Reason::BossKey),
            State::Hidden(Reason::BossKey),
        ] {
            assert_eq!(state.mouse(true), state);
            assert_eq!(state.mouse(false), state);
            assert_eq!(state.boss_toggle(), State::Visible);
        }
    }

    #[test]
    fn boss_key_takes_over_automatic_hiding() {
        assert_eq!(
            State::Visible.boss_toggle(),
            State::HidePending(Reason::BossKey)
        );
        assert_eq!(
            State::HidePending(Reason::Mouse).boss_toggle(),
            State::HidePending(Reason::BossKey)
        );
        let state = State::Hidden(Reason::Mouse).boss_toggle();
        assert_eq!(state, State::Hidden(Reason::BossKey));
        assert_eq!(state.mouse(true).boss_toggle(), State::Visible);
    }

    #[test]
    fn disabled_and_destroyed_cannot_hide_or_restore() {
        for state in [State::Disabled, State::Destroyed] {
            assert_eq!(state.mouse(true), state);
            assert_eq!(state.mouse(false), state);
            assert_eq!(state.boss_toggle(), state);
        }
    }
}
