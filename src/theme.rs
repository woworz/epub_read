use crate::settings::Settings;
use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Stroke, TextStyle};

#[derive(Clone, Copy)]
pub struct Palette {
    pub surface: Color32,
    pub sidebar: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub selected: Color32,
    pub line: Color32,
}
impl Palette {
    pub fn for_settings(settings: &Settings) -> Self {
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        match settings.theme {
            2 => Self {
                surface: rgb(28, 30, 34),
                sidebar: rgb(36, 39, 44),
                text: rgb(213, 216, 221),
                muted: rgb(167, 176, 184),
                accent: rgb(163, 187, 201),
                selected: rgb(49, 65, 76),
                line: rgb(62, 67, 74),
            },
            1 => Self {
                surface: rgb(248, 242, 226),
                sidebar: rgb(239, 234, 220),
                text: rgb(47, 43, 37),
                muted: rgb(109, 103, 92),
                accent: rgb(94, 112, 83),
                selected: rgb(225, 228, 211),
                line: rgb(218, 211, 194),
            },
            _ => Self {
                surface: rgb(250, 250, 248),
                sidebar: rgb(243, 244, 243),
                text: rgb(40, 42, 43),
                muted: rgb(98, 107, 115),
                accent: rgb(83, 108, 123),
                selected: rgb(231, 237, 240),
                line: rgb(226, 229, 231),
            },
        }
    }
}

pub fn apply(ctx: &egui::Context, settings: &Settings) {
    let p = Palette::for_settings(settings);
    ctx.style_mut(|style| {
        style.visuals = if settings.theme == 2 {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let v = &mut style.visuals;
        v.override_text_color = Some(p.text);
        v.panel_fill = p.surface;
        v.window_fill = p.surface;
        v.window_stroke = Stroke::new(1.0_f32, p.line);
        v.window_shadow = egui::epaint::Shadow::NONE;
        v.window_corner_radius = egui::CornerRadius::same(8);
        v.extreme_bg_color = p.sidebar;
        v.faint_bg_color = p.sidebar;
        v.selection.bg_fill = p.selected;
        v.selection.stroke = Stroke::new(1.0_f32, p.accent);
        v.hyperlink_color = p.accent;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = egui::CornerRadius::same(5);
            w.fg_stroke = Stroke::new(1.0_f32, p.text);
            w.bg_stroke = Stroke::new(1.0_f32, p.line);
            w.expansion = 0.;
        }
        v.widgets.noninteractive.bg_fill = p.surface;
        v.widgets.noninteractive.weak_bg_fill = p.surface;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
        v.widgets.inactive.bg_fill = p.sidebar;
        v.widgets.inactive.weak_bg_fill = p.sidebar;
        v.widgets.hovered.bg_fill = p.selected;
        v.widgets.hovered.weak_bg_fill = p.selected;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
        v.widgets.active.bg_fill = p.selected;
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, p.accent);
        v.widgets.open.bg_fill = p.selected;
        v.widgets.open.weak_bg_fill = p.sidebar;
        v.widgets.active.weak_bg_fill = p.selected;
        style.spacing.item_spacing = egui::vec2(10., 10.);
        style.spacing.button_padding = egui::vec2(10., 6.);
        style.spacing.interact_size = egui::vec2(32., 32.);
        style.spacing.window_margin = egui::Margin::same(12);
        style.spacing.slider_width = 160.;
        for (kind, size) in [
            (TextStyle::Body, 15.),
            (TextStyle::Button, 15.),
            (TextStyle::Small, 13.),
            (TextStyle::Heading, 24.),
        ] {
            style.text_styles.insert(kind, FontId::proportional(size));
        }
    });
}

#[derive(Clone, Copy)]
pub enum Icon {
    BookOpen,
    FolderOpen,
    List,
    TextAa,
    Moon,
    CaretLeft,
    CaretRight,
    CaretDown,
}
impl Icon {
    pub fn glyph(self) -> char {
        match self {
            Self::BookOpen => '\u{e0e6}',
            Self::FolderOpen => '\u{e256}',
            Self::List => '\u{e2f0}',
            Self::TextAa => '\u{e6ee}',
            Self::Moon => '\u{e330}',
            Self::CaretLeft => '\u{e138}',
            Self::CaretRight => '\u{e13a}',
            Self::CaretDown => '\u{e136}',
        }
    }
    pub fn text(self, size: f32) -> RichText {
        RichText::new(self.glyph().to_string())
            .font(FontId::new(size, FontFamily::Name("icons".into())))
    }
    pub fn label(self, text: &str) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &self.glyph().to_string(),
            0.,
            egui::TextFormat {
                font_id: FontId::new(20., FontFamily::Name("icons".into())),
                color: Color32::PLACEHOLDER,
                ..Default::default()
            },
        );
        if !text.is_empty() {
            job.append(
                &format!("  {text}"),
                0.,
                egui::TextFormat {
                    font_id: FontId::proportional(15.),
                    color: Color32::PLACEHOLDER,
                    ..Default::default()
                },
            );
        }
        job
    }
}
pub fn action(ui: &mut egui::Ui, icon: Icon, text: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(icon.label(text))
            .frame(selected)
            .selected(selected),
    )
}

pub fn panel_frame(p: Palette, margin: egui::Margin) -> egui::Frame {
    egui::Frame::NONE.fill(p.surface).inner_margin(margin)
}
