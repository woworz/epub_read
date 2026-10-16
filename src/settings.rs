use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub font: String,
    pub size: f32,
    pub weight: usize,
    pub line_spacing: f32,
    pub paragraph_spacing: f32,
    pub margin: f32,
    pub max_width: f32,
    pub theme: usize,
    pub background: [u8; 3],
    pub foreground: [u8; 3],
    pub toc: bool,
    pub immersive: bool,
    pub toc_width: f32,
    pub collapsed_toc: Vec<String>,
    pub window_size: [f32; 2],
    pub window_pos: Option<[f32; 2]>,
    #[serde(default = "default_window_opacity")]
    pub window_opacity: u8,
    pub auto_hide_on_mouse_leave: bool,
    pub boss_shortcut: crate::hotkey::BossShortcut,
    pub last_book: String,
}
fn default_window_opacity() -> u8 {
    100
}

// 100% 时恢复最近的低透明度；首次使用 F9 默认切换到 80%。
pub fn toggle_opacity(current: u8, last_low: &mut u8) -> u8 {
    let current = current.clamp(20, 100);
    if current < 100 {
        *last_low = current;
        100
    } else {
        *last_low = (*last_low).clamp(20, 95);
        *last_low
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            font: "宋体".into(),
            size: 22.,
            weight: 1,
            line_spacing: 1.8,
            paragraph_spacing: 22.,
            margin: 35.,
            max_width: 680.,
            theme: 0,
            background: [250, 250, 248],
            foreground: [40, 42, 43],
            toc: false,
            immersive: false,
            toc_width: 250.,
            collapsed_toc: Vec::new(),
            window_size: [1100., 800.],
            window_pos: None,
            window_opacity: default_window_opacity(),
            auto_hide_on_mouse_leave: false,
            boss_shortcut: Default::default(),
            last_book: String::new(),
        }
    }
}
impl Settings {
    pub fn set_theme(&mut self, theme: usize) {
        self.theme = theme;
        (self.background, self.foreground) = match theme {
            0 => ([250, 250, 248], [40, 42, 43]),
            2 => ([28, 30, 34], [213, 216, 221]),
            _ => ([248, 242, 226], [47, 43, 37]),
        };
    }
    pub fn restore_typography(&mut self) {
        let d = Self::default();
        self.font = d.font;
        self.size = d.size;
        self.weight = d.weight;
        self.line_spacing = d.line_spacing;
        self.paragraph_spacing = d.paragraph_spacing;
        self.margin = d.margin;
        self.max_width = d.max_width;
        self.set_theme(d.theme);
    }
    pub fn sanitize(&mut self) {
        fn bounded(v: &mut f32, low: f32, high: f32, fallback: f32) {
            *v = if v.is_finite() {
                v.clamp(low, high)
            } else {
                fallback
            };
        }
        bounded(&mut self.size, 12., 48., 22.);
        bounded(&mut self.line_spacing, 1., 3., 1.8);
        bounded(&mut self.paragraph_spacing, 0., 60., 22.);
        bounded(&mut self.margin, 0., 160., 35.);
        bounded(&mut self.max_width, 300., 1600., 680.);
        bounded(&mut self.window_size[0], 500., 8000., 1100.);
        bounded(&mut self.window_size[1], 350., 8000., 800.);
        self.weight = self.weight.min(2);
        self.window_opacity = self.window_opacity.clamp(20, 100);
        if self.boss_shortcut.validate().is_err() {
            self.boss_shortcut = Default::default();
        }
        bounded(&mut self.toc_width, 150., 600., 250.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_preferences_survive_new_minimal_defaults() {
        let saved = r#"{"font":"微软雅黑","size":24,"line_spacing":1.6,"paragraph_spacing":14,"max_width":760,"theme":1,"background":[248,242,226],"foreground":[47,43,37],"toc":true,"toc_width":310,"window_opacity":65,"auto_hide_on_mouse_leave":true}"#;
        let mut settings: Settings = serde_json::from_str(saved).unwrap();
        settings.sanitize();
        assert_eq!(settings.font, "微软雅黑");
        assert_eq!(settings.theme, 1);
        assert_eq!(settings.background, [248, 242, 226]);
        assert!(settings.toc);
        assert_eq!(settings.toc_width, 310.);
        assert_eq!(settings.max_width, 760.);
        let old = settings.clone();
        settings.restore_typography();
        assert_eq!(settings.font, "宋体");
        assert_eq!(settings.theme, 0);
        assert_eq!(settings.max_width, 680.);
        assert_eq!(settings.toc, old.toc);
        assert_eq!(settings.toc_width, old.toc_width);
        assert_eq!(settings.window_opacity, old.window_opacity);
        assert_eq!(
            settings.auto_hide_on_mouse_leave,
            old.auto_hide_on_mouse_leave
        );
        assert_eq!(settings.boss_shortcut, old.boss_shortcut);
    }

    #[test]
    fn opacity_is_bounded() {
        for (input, expected) in [(0, 20), (10, 20), (20, 20), (100, 100), (150, 100)] {
            let mut settings = Settings {
                window_opacity: input,
                ..Default::default()
            };
            settings.sanitize();
            assert_eq!(settings.window_opacity, expected);
        }
    }

    #[test]
    fn old_json_defaults_to_opaque() {
        let settings: Settings = serde_json::from_str(r#"{"size":24,"immersive":true}"#).unwrap();
        assert_eq!(settings.window_opacity, 100);
        assert_eq!(settings.size, 24.);
        assert!(settings.immersive);
        let settings: Settings = serde_json::from_str(r#"{"window_opacity":35}"#).unwrap();
        assert_eq!(settings.window_opacity, 35);
        let saved = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&saved)
                .unwrap()
                .window_opacity,
            35
        );
    }

    #[test]
    fn f9_remembers_low_opacity() {
        let mut last = 80;
        assert_eq!(toggle_opacity(100, &mut last), 80);
        assert_eq!(toggle_opacity(35, &mut last), 100);
        assert_eq!(last, 35);
        assert_eq!(toggle_opacity(100, &mut last), 35);
        assert_eq!(toggle_opacity(20, &mut last), 100);
        assert_eq!(toggle_opacity(100, &mut last), 20);
        last = 0;
        assert_eq!(toggle_opacity(100, &mut last), 20);
        last = 150;
        assert_eq!(toggle_opacity(100, &mut last), 95);
    }

    #[test]
    fn auto_hide_defaults_off_and_round_trips() {
        let mut settings: Settings = serde_json::from_str(r#"{"size":24}"#).unwrap();
        assert!(!settings.auto_hide_on_mouse_leave);
        settings.auto_hide_on_mouse_leave = true;
        let saved = serde_json::to_string(&settings).unwrap();
        assert!(
            serde_json::from_str::<Settings>(&saved)
                .unwrap()
                .auto_hide_on_mouse_leave
        );
    }

    #[test]
    fn boss_shortcut_migrates_round_trips_and_recovers_invalid_config() {
        let mut settings: Settings = serde_json::from_str(r#"{"size":24}"#).unwrap();
        assert_eq!(settings.boss_shortcut, Default::default());
        settings.boss_shortcut.key = b'J' as u32;
        settings.boss_shortcut.shift = true;
        let saved = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&saved)
                .unwrap()
                .boss_shortcut,
            settings.boss_shortcut
        );
        settings.sanitize();
        assert_eq!(settings.boss_shortcut.label(), "Ctrl+Alt+Shift+J");
        settings.boss_shortcut.key = 0xFFFF;
        settings.sanitize();
        assert_eq!(settings.boss_shortcut, Default::default());
    }
}
