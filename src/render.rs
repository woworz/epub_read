use crate::{
    model::{Kind, Paragraph},
    settings::Settings,
};
use eframe::egui::{
    self,
    text::{LayoutJob, TextFormat},
    Color32, FontFamily, FontId,
};
use std::{collections::HashMap, sync::Arc};
pub struct Row {
    pub y: f32,
    pub height: f32,
    pub galley: Option<Arc<egui::Galley>>,
    pub image_size: Option<[f32; 2]>,
}
#[derive(Default)]
pub struct Layout {
    pub rows: Vec<Row>,
    pub height: f32,
    pub width: f32,
    key: String,
}
pub type Images = HashMap<String, Result<egui::TextureHandle, String>>;
impl Layout {
    pub fn clear(&mut self) {
        self.key.clear();
    }
    pub fn prepare(
        &mut self,
        ui: &egui::Ui,
        paragraphs: &[Paragraph],
        settings: &Settings,
        width: f32,
        chapter: usize,
        images: &Images,
    ) -> bool {
        let key = format!(
            "{chapter}/{width:.1}/{}/{}/{}/{}/{}/{}/{:?}/{:?}",
            settings.font,
            settings.size,
            settings.weight,
            settings.line_spacing,
            settings.paragraph_spacing,
            images.len(),
            settings.foreground,
            settings.background
        );
        if self.key == key {
            return false;
        }
        self.key = key;
        self.width = width;
        self.rows.clear();
        let mut y = 20.;
        for p in paragraphs {
            let (galley, height, image_size) = match &p.kind {
                Kind::Image(path) => {
                    let size = images
                        .get(path)
                        .and_then(|r| r.as_ref().ok())
                        .map(|t| t.size_vec2());
                    let size = size.map(|s| {
                        let scale = (width / s.x).min(1.);
                        [s.x * scale, s.y * scale]
                    });
                    (None, size.map(|s| s[1]).unwrap_or(60.), size)
                }
                Kind::Rule => (None, 20., None),
                _ => {
                    let size = match p.kind {
                        Kind::Heading(level) => settings.size * (1.5 - level.min(5) as f32 * 0.07),
                        _ => settings.size,
                    };
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = width;
                    let color = Color32::from_rgb(
                        settings.foreground[0],
                        settings.foreground[1],
                        settings.foreground[2],
                    );
                    for run in &p.runs {
                        let bold = run.bold || matches!(p.kind, Kind::Heading(_));
                        job.append(
                            &run.text,
                            0.,
                            TextFormat {
                                font_id: FontId::new(
                                    size,
                                    if bold {
                                        FontFamily::Name("bold".into())
                                    } else {
                                        FontFamily::Proportional
                                    },
                                ),
                                color,
                                italics: run.italic,
                                line_height: Some(size * settings.line_spacing),
                                ..Default::default()
                            },
                        );
                    }
                    let galley = ui.fonts(|f| f.layout_job(job));
                    let h = galley.size().y.max(size * settings.line_spacing);
                    (Some(galley), h, None)
                }
            };
            self.rows.push(Row {
                y,
                height,
                galley,
                image_size,
            });
            y += height + settings.paragraph_spacing;
        }
        self.height = y + 20.;
        true
    }
    pub fn paragraph_at(&self, y: f32) -> usize {
        self.rows
            .partition_point(|r| r.y + r.height < y)
            .min(self.rows.len().saturating_sub(1))
    }
}
