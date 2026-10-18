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
    pub divider_y: Option<f32>,
}
pub type Images = HashMap<String, Result<egui::TextureHandle, String>>;
impl Layout {
    pub fn clear(&mut self) {
        *self = Self::default();
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
            "{chapter}/{width:.1}/{}/{}/{}/{}/{}/{:?}/{:?}",
            settings.font,
            settings.size,
            settings.weight,
            settings.line_spacing,
            settings.paragraph_spacing,
            settings.foreground,
            settings.background
        );
        let key = format!("{key}/{book_title}/{chapter_title}");
        if self.key == key {
            return false;
        }
        self.clear();
        self.divider_y = None;
        self.key = key;
        self.width = width;
        if paragraphs.is_empty() {
            return true;
        }
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
            let divider = y + g.size().y + 20.;
            self.divider_y = Some(divider);
            y = divider + 28.;
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
                let divider = y + 14.;
                self.divider_y = Some(divider);
                y = divider + 28.;
            }
        }
        self.height = y + 24.;
        true
    }
    /// Update image geometry without rebuilding text galleys or invalidating the key.
    pub fn refresh_image_row(&mut self, index: usize, path: &str, images: &Images) -> Option<f32> {
        let row = self.rows.get_mut(index)?;
        let size = images
            .get(path)
            .and_then(|result| result.as_ref().ok())
            .map(|texture| {
                let size = texture.size_vec2();
                let scale = (self.width / size.x).min(1.);
                [size.x * scale, size.y * scale]
            });
        let height = size.map_or(60., |size| size[1]);
        let delta = height - row.height;
        // Width can change even when the decoded height matches the placeholder.
        row.image_size = size;
        if delta.abs() < 0.01 {
            return None;
        }
        row.height = height;
        for row in &mut self.rows[index + 1..] {
            row.y += delta;
        }
        self.height += delta;
        Some(delta)
    }
    pub fn paragraph_at(&self, y: f32) -> usize {
        self.rows
            .partition_point(|r| r.y + r.height < y)
            .min(self.rows.len().saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Run;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        let family = fonts.families[&FontFamily::Proportional].clone();
        fonts
            .families
            .insert(FontFamily::Name("body".into()), family.clone());
        fonts
            .families
            .insert(FontFamily::Name("bold".into()), family);
        ctx.set_fonts(fonts);
        ctx
    }

    fn paragraph(kind: Kind) -> Paragraph {
        Paragraph {
            kind,
            runs: vec![Run {
                text: "Chapter text".into(),
                ..Default::default()
            }],
            anchor: String::new(),
        }
    }

    fn prepare(
        ctx: &egui::Context,
        layout: &mut Layout,
        paragraphs: &[Paragraph],
        images: &Images,
        chapter: usize,
    ) -> bool {
        let mut rebuilt = false;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rebuilt = layout.prepare(
                    ui,
                    paragraphs,
                    &Settings::default(),
                    300.,
                    chapter,
                    images,
                    "Book",
                    "Chapter",
                );
            });
        });
        rebuilt
    }

    #[test]
    fn image_refresh_preserves_key_galleys_and_moves_only_following_rows() {
        let ctx = context();
        let mut layout = Layout::default();
        let paragraphs = [
            paragraph(Kind::Heading(1)),
            paragraph(Kind::Image("image".into())),
            paragraph(Kind::Text),
        ];
        let mut images = Images::new();
        assert!(prepare(&ctx, &mut layout, &paragraphs, &images, 0));
        let key = layout.key.clone();
        let y: Vec<_> = layout.rows.iter().map(|row| row.y).collect();
        let height = layout.height;
        let galley = layout.rows[2].galley.clone().unwrap();
        images.insert(
            "image".into(),
            Ok(ctx.load_texture(
                "image",
                egui::ColorImage::new([600, 800], Color32::WHITE),
                Default::default(),
            )),
        );
        let delta = layout.refresh_image_row(1, "image", &images).unwrap();
        assert_eq!(delta, 340.);
        assert_eq!(layout.rows.len(), paragraphs.len());
        assert_eq!(layout.rows[1].image_size, Some([300., 400.]));
        assert_eq!(layout.rows[1].height, 400.);
        assert_eq!(layout.rows[0].y, y[0]);
        assert_eq!(layout.rows[1].y, y[1]);
        assert_eq!(layout.rows[2].y, y[2] + delta);
        assert_eq!(layout.height, height + delta);
        assert_eq!(layout.key, key);
        assert!(!prepare(&ctx, &mut layout, &paragraphs, &images, 0));
        assert!(Arc::ptr_eq(
            &galley,
            layout.rows[2].galley.as_ref().unwrap()
        ));
        assert_eq!(layout.refresh_image_row(1, "image", &images), None);
        images.insert("image".into(), Err("decode failed".into()));
        assert_eq!(layout.refresh_image_row(1, "image", &images), Some(-340.));
        assert_eq!(layout.rows[1].height, 60.);
        assert_eq!(layout.rows[1].image_size, None);
        assert_eq!(layout.height, height);
        assert_eq!(layout.refresh_image_row(100, "image", &images), None);
    }

    #[test]
    fn image_with_placeholder_height_still_updates_width_without_upscaling() {
        let ctx = context();
        let mut layout = Layout::default();
        let paragraphs = [paragraph(Kind::Image("image".into()))];
        let mut images = Images::new();
        prepare(&ctx, &mut layout, &paragraphs, &images, 0);
        let height = layout.height;
        images.insert(
            "image".into(),
            Ok(ctx.load_texture(
                "image",
                egui::ColorImage::new([20, 60], Color32::WHITE),
                Default::default(),
            )),
        );
        assert_eq!(layout.refresh_image_row(0, "image", &images), None);
        assert_eq!(layout.rows[0].image_size, Some([20., 60.]));
        assert_eq!(layout.height, height);
    }

    #[test]
    fn chapter_dividers_follow_heading_or_fallback_and_disappear_for_empty_chapters() {
        let ctx = context();
        let mut layout = Layout::default();
        let images = Images::new();
        let settings = Settings::default();
        prepare(
            &ctx,
            &mut layout,
            &[paragraph(Kind::Heading(1)), paragraph(Kind::Text)],
            &images,
            0,
        );
        let row = &layout.rows[0];
        let divider = layout.divider_y.unwrap();
        assert_eq!(
            divider,
            row.y + row.height + settings.paragraph_spacing + 14.
        );
        assert_eq!(layout.rows[1].y, divider + 28.);
        assert!(layout.fallback_heading.is_none());
        prepare(&ctx, &mut layout, &[paragraph(Kind::Text)], &images, 1);
        let divider = layout.divider_y.unwrap();
        assert_eq!(
            divider,
            layout.heading_y + layout.fallback_heading.as_ref().unwrap().size().y + 20.
        );
        assert_eq!(layout.rows[0].y, divider + 28.);
        prepare(&ctx, &mut layout, &[], &images, 2);
        assert!(layout.divider_y.is_none());
        assert!(layout.book_label.is_none() && layout.fallback_heading.is_none());
        assert!(layout.rows.is_empty());
        assert_eq!(layout.height, 0.);
        assert_eq!(layout.heading_y, 0.);
    }

    #[test]
    fn clear_invalidates_rows_height_and_all_header_state() {
        let ctx = context();
        let mut layout = Layout::default();
        for kind in [Kind::Heading(1), Kind::Text] {
            prepare(&ctx, &mut layout, &[paragraph(kind)], &Images::new(), 0);
            assert!(layout.divider_y.is_some());
            layout.clear();
            assert!(layout.key.is_empty() && layout.rows.is_empty());
            assert_eq!(layout.height, 0.);
            assert_eq!(layout.width, 0.);
            assert!(layout.book_label.is_none() && layout.fallback_heading.is_none());
            assert_eq!(layout.heading_y, 0.);
            assert!(layout.divider_y.is_none());
            assert_eq!(layout.paragraph_at(1000.), 0);
        }
    }
}
