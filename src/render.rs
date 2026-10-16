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
    pub book_label: Option<Arc<egui::Galley>>,
    pub fallback_heading: Option<Arc<egui::Galley>>,
    pub heading_y: f32,
    pub divider_y: f32,
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
        book_title: &str,
        chapter_title: &str,
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
        let key = format!("{key}/{book_title}/{chapter_title}");
        if self.key == key {
            return false;
        }
        self.key = key;
        self.width = width;
        self.rows.clear();
        let color = Color32::from_rgb(
            settings.foreground[0],
            settings.foreground[1],
            settings.foreground[2],
        );
        let make_header = |text: &str, size: f32, family: FontFamily, color: Color32| {
            let mut job =
                LayoutJob::simple(text.to_owned(), FontId::new(size, family), color, width);
            job.wrap.max_width = width;
            ui.fonts(|f| f.layout_job(job))
        };
        self.book_label = Some(make_header(
            book_title,
            14.,
            FontFamily::Proportional,
            color.gamma_multiply(0.7),
        ));
        self.heading_y = 40. + self.book_label.as_ref().unwrap().size().y + 16.;
        let starts_with_heading = paragraphs
            .first()
            .is_some_and(|p| matches!(p.kind, Kind::Heading(_)));
        self.fallback_heading = if starts_with_heading {
            None
        } else {
            Some(make_header(
                chapter_title,
                settings.size * 2.2,
                FontFamily::Name("body".into()),
                color,
            ))
        };
        let mut y = self.heading_y;
        if let Some(g) = &self.fallback_heading {
            self.divider_y = y + g.size().y + 20.;
            y = self.divider_y + 28.;
        }
        for (index, p) in paragraphs.iter().enumerate() {
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
                        Kind::Heading(_) if index == 0 => settings.size * 2.2,
                        Kind::Heading(level) => settings.size * (1.5 - level.min(5) as f32 * 0.07),
                        _ => settings.size,
                    };
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = width;
                    // Long unspaced CJK runs and URLs must stay in the reading column.
                    job.wrap.break_anywhere = true;
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
                                    if matches!(p.kind, Kind::Heading(_)) {
                                        FontFamily::Name("body".into())
                                    } else if bold {
                                        FontFamily::Name("bold".into())
                                    } else {
                                        FontFamily::Name("body".into())
                                    },
                                ),
                                color,
                                italics: run.italic,
                                line_height: Some(
                                    size * if index == 0 && starts_with_heading {
                                        1.2
                                    } else {
                                        settings.line_spacing
                                    },
                                ),
                                ..Default::default()
                            },
                        );
                    }
                    let galley = ui.fonts(|f| f.layout_job(job));
                    let h = galley.size().y;
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
            if index == 0 && starts_with_heading {
                self.divider_y = y + 14.;
                y = self.divider_y + 28.;
            }
        }
        self.height = y + 24.;
        true
    }
    pub fn paragraph_at(&self, y: f32) -> usize {
        self.rows
            .partition_point(|r| r.y + r.height < y)
            .min(self.rows.len().saturating_sub(1))
    }
}
