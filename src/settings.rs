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
            font: "微软雅黑".into(),
            size: 22.,
            weight: 1,
            line_spacing: 1.6,
            paragraph_spacing: 14.,
            margin: 35.,
            max_width: 760.,
            theme: 1,
            background: [248, 242, 226],
            foreground: [47, 43, 37],
            toc: true,
            immersive: false,
            toc_width: 235.,
            collapsed_toc: Vec::new(),
            window_size: [1100., 800.],
            window_pos: None,
            window_opacity: default_window_opacity(),
            last_book: String::new(),
        }
    }
}
impl Settings {
    pub fn set_theme(&mut self, theme: usize) {
        self.theme = theme;
        (self.background, self.foreground) = match theme {
            0 => ([255, 255, 255], [35, 35, 35]),
            2 => ([28, 30, 34], [213, 216, 221]),
            _ => ([248, 242, 226], [47, 43, 37]),
        };
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
        bounded(&mut self.line_spacing, 1., 3., 1.6);
        bounded(&mut self.paragraph_spacing, 0., 60., 14.);
        bounded(&mut self.margin, 0., 160., 35.);
        bounded(&mut self.max_width, 300., 1600., 760.);
        bounded(&mut self.window_size[0], 500., 8000., 1100.);
        bounded(&mut self.window_size[1], 350., 8000., 800.);
        self.weight = self.weight.min(2);
        self.window_opacity = self.window_opacity.clamp(20, 100);
        bounded(&mut self.toc_width, 150., 600., 235.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
