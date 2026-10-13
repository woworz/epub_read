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
    pub marks: bool,
    pub immersive: bool,
    pub toc_width: f32,
    pub marks_width: f32,
    pub collapsed_toc: Vec<String>,
    pub window_size: [f32; 2],
    pub window_pos: Option<[f32; 2]>,
    pub last_book: String,
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
            marks: false,
            immersive: false,
            toc_width: 235.,
            marks_width: 260.,
            collapsed_toc: Vec::new(),
            window_size: [1100., 800.],
            window_pos: None,
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
        bounded(&mut self.toc_width, 150., 600., 235.);
        bounded(&mut self.marks_width, 180., 600., 260.);
    }
}
