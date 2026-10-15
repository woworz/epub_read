use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BossShortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: u32,
}

impl Default for BossShortcut {
    fn default() -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            key: b'H' as u32,
        }
    }
}

impl BossShortcut {
    pub fn keys() -> impl Iterator<Item = u32> {
        (0x41..=0x5A).chain(0x30..=0x39).chain(0x70..=0x87).chain([
            0x20, 0x2D, 0x2E, 0x24, 0x23, 0x21, 0x22, 0x25, 0x26, 0x27, 0x28,
        ])
    }

    pub fn key_label(key: u32) -> String {
        match key {
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(key).unwrap().to_string(),
            0x70..=0x87 => format!("F{}", key - 0x70 + 1),
            0x20 => "Space".into(),
            0x2D => "Insert".into(),
            0x2E => "Delete".into(),
            0x24 => "Home".into(),
            0x23 => "End".into(),
            0x21 => "PageUp".into(),
            0x22 => "PageDown".into(),
            0x25 => "←".into(),
            0x26 => "↑".into(),
            0x27 => "→".into(),
            0x28 => "↓".into(),
            _ => "未知按键".into(),
        }
    }

    pub fn label(self) -> String {
        let mut parts = Vec::new();
        for (enabled, label) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.win, "Win"),
        ] {
            if enabled {
                parts.push(label.to_owned());
            }
        }
        parts.push(Self::key_label(self.key));
        parts.join("+")
    }

    pub fn modifiers(self) -> u32 {
        0x4000
            | u32::from(self.alt)
            | (u32::from(self.ctrl) << 1)
            | (u32::from(self.shift) << 2)
            | (u32::from(self.win) << 3)
    }

    pub fn validate(self) -> Result<(), String> {
        if !Self::keys().any(|key| key == self.key) {
            return Err("请选择列表中的主键".into());
        }
        if self.key == 0x7B {
            return Err("F12 为系统保留按键，请选择其他按键".into());
        }
        if !(self.ctrl || self.alt || self.shift || self.win || (0x70..=0x87).contains(&self.key)) {
            return Err("字母、数字和导航键至少需要搭配一个修饰键".into());
        }
        Ok(())
    }

    pub fn matches_message(self, lparam: isize) -> bool {
        let data = lparam as u32;
        (data & 0xF) == (self.modifiers() & 0xF) && (data >> 16) == self.key
    }
}

// 先用另一个 ID 注册新组合，成功后才注销旧组合；失败时保留原恢复入口。
pub fn replace_registration(
    active: Option<i32>,
    register: impl FnOnce(i32) -> Result<(), String>,
    unregister: impl FnOnce(i32),
) -> Result<i32, String> {
    let next = if active == Some(0x4550) {
        0x4551
    } else {
        0x4550
    };
    register(next)?;
    if let Some(old) = active {
        unregister(old);
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn shortcut_json_and_windows_modifiers_agree() {
        assert_eq!(BossShortcut::default().label(), "Ctrl+Alt+H");
        assert_eq!(BossShortcut::default().modifiers(), 0x4003);
        let shortcut = BossShortcut {
            ctrl: false,
            alt: false,
            shift: true,
            win: true,
            key: 0x78,
        };
        assert_eq!(shortcut.label(), "Shift+Win+F9");
        assert_eq!(shortcut.modifiers(), 0x400C);
        let json = serde_json::to_string(&shortcut).unwrap();
        assert_eq!(
            serde_json::from_str::<BossShortcut>(&json).unwrap(),
            shortcut
        );
    }

    #[test]
    fn validates_global_shortcuts() {
        let mut shortcut = BossShortcut {
            ctrl: false,
            alt: false,
            ..Default::default()
        };
        assert!(shortcut.validate().is_err());
        shortcut.key = 0x78;
        assert!(shortcut.validate().is_ok());
        shortcut.key = 0x7B;
        assert!(shortcut.validate().is_err());
        shortcut.key = 0xFFFF;
        assert!(shortcut.validate().is_err());
    }

    #[test]
    fn occupied_shortcut_keeps_original_registration() {
        let removed = Cell::new(false);
        let result = replace_registration(
            Some(0x4550),
            |id| {
                assert_eq!(id, 0x4551);
                Err("已占用".into())
            },
            |_| removed.set(true),
        );
        assert!(result.is_err());
        assert!(!removed.get());
    }

    #[test]
    fn old_queued_shortcut_does_not_trigger_after_rebind() {
        let original = BossShortcut::default();
        let changed = BossShortcut {
            key: b'J' as u32,
            ..original
        };
        let message = |key: BossShortcut| ((key.key << 16) | (key.modifiers() & 0xF)) as isize;
        assert!(changed.matches_message(message(changed)));
        assert!(changed.matches_message(message(changed) | 0x4000));
        assert!(!changed.matches_message(message(original)));
        assert!(!changed.matches_message(message(BossShortcut {
            shift: true,
            ..changed
        })));
    }

    #[test]
    fn replacing_shortcut_registers_before_releasing_old_and_alternates_ids() {
        for (old, new) in [(0x4550, 0x4551), (0x4551, 0x4550)] {
            let registered = Cell::new(false);
            let result = replace_registration(
                Some(old),
                |id| {
                    assert_eq!(id, new);
                    registered.set(true);
                    Ok(())
                },
                |id| {
                    assert!(registered.get());
                    assert_eq!(id, old);
                },
            );
            assert_eq!(result.unwrap(), new);
        }
    }
}
