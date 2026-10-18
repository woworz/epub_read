use crate::{
    epub::Book,
    fonts,
    hotkey::BossShortcut,
    model::*,
    render::{Images, Layout},
    settings::{toggle_opacity, Settings},
    storage,
    theme::{self, Icon, Palette},
};
use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const IMAGE_CACHE_LIMIT: usize = 64;
const PANEL_SIDE: i8 = 24;
const TOOLBAR_HEIGHT: f32 = 34.;
const TOOLBAR_LEFT_W: f32 = 270.;
const TOOLBAR_LEFT_W_COMPACT: f32 = 220.;
const TOOLBAR_RIGHT_W: f32 = 260.;
const TOOLBAR_RIGHT_W_COMPACT: f32 = 164.;
const TOOLBAR_TITLE_MIN_W: f32 = 160.;
const COMPACT_BELOW: f32 = 760.;
const FOOTER_HEIGHT: f32 = 32.;
const FOOTER_NAV_W: f32 = 190.;
const FOOTER_LABEL_PREFIX_MIN_W: f32 = 650.;

fn toolbar_geometry(rect: Rect) -> (bool, Rect, Rect, Option<Rect>) {
    let compact = rect.width() < COMPACT_BELOW;
    let (left_w, right_w) = if compact {
        (TOOLBAR_LEFT_W_COMPACT, TOOLBAR_RIGHT_W_COMPACT)
    } else {
        (TOOLBAR_LEFT_W, TOOLBAR_RIGHT_W)
    };
    let left = Rect::from_min_size(rect.min, egui::vec2(left_w, rect.height()));
    let right = Rect::from_min_max(egui::pos2(rect.right() - right_w, rect.top()), rect.max);
    let title_room = rect.width() - left_w - right_w;
    let middle = (!compact && title_room >= TOOLBAR_TITLE_MIN_W).then(|| {
        Rect::from_min_size(
            egui::pos2(left.right() + 8., rect.top()),
            egui::vec2(title_room - 16., rect.height()),
        )
    });
    (compact, left, right, middle)
}

pub struct Reader {
    settings: Settings,
    data_dir: Option<PathBuf>,
    book: Option<Book>,
    data: BookData,
    paragraphs: Vec<Paragraph>,
    layout: Layout,
    images: Images,
    font_list: Vec<fonts::Font>,
    font_key: String,
    true_bold: bool,
    error: Option<String>,
    info: Option<String>,
    settings_open: bool,
    toc_overlay_open: bool,
    search: String,
    focus_search: bool,
    fullscreen: bool,
    hwnd: Option<isize>,
    visibility: Option<crate::native::WindowVisibility>,
    boss_key_error: Option<String>,
    boss_shortcut_draft: BossShortcut,
    boss_shortcut_error: Option<String>,
    opacity_available: bool,
    applied_opacity: Option<u8>,
    last_low_opacity: u8,
    opacity_mode: (bool, bool),
    opacity_reapply: bool,
    scroll: f32,
    scroll_request: Option<f32>,
    jump: Option<Location>,
    dirty: bool,
    last_save: Instant,
    last_save_error: Option<String>,
    first_frame: bool,
    load_path: Option<PathBuf>,
    picker: Option<crate::picker::Picker>,
    #[cfg(test)]
    body_geometry: Option<(egui::Id, Vec2, Rect)>,
    #[cfg(test)]
    progress_rect: Option<Rect>,
    #[cfg(test)]
    empty_open_rect: Option<Rect>,
}

// 隐藏先于任何存储操作；保存结果只影响待保存状态，不能撤销隐藏。
fn hide_then_save<T>(
    state: &mut T,
    hide: impl FnOnce(&mut T),
    save: impl FnOnce(&mut T) -> bool,
) -> bool {
    hide(state);
    save(state)
}

impl Reader {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        settings: Settings,
        dir: Result<PathBuf, String>,
        path: Option<PathBuf>,
    ) -> Self {
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let font_list = fonts::available();
        let font_result = fonts::install(&cc.egui_ctx, &font_list, &settings.font, settings.weight);
        let error = dir
            .as_ref()
            .err()
            .cloned()
            .or_else(|| font_result.as_ref().err().cloned());
        let load_path = path.or_else(|| {
            if settings.last_book.is_empty() {
                None
            } else {
                Some(settings.last_book.clone().into())
            }
        });
        let font_key = format!("{}/{}", settings.font, settings.weight);
        let hwnd = cc
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            });
        let last_low_opacity = if settings.window_opacity < 100 {
            settings.window_opacity
        } else {
            80
        };
        let opacity_mode = (false, settings.immersive);
        let boss_shortcut_draft = settings.boss_shortcut;
        let shortcut_label = settings.boss_shortcut.label();
        let (visibility, boss_key_error) = match hwnd {
            Some(hwnd) => match crate::native::WindowVisibility::register(
                hwnd,
                &cc.egui_ctx,
                settings.boss_shortcut,
            ) {
                Ok(key) => {
                    let error = key.hotkey_error().map(|e| {
                        format!("老板键 {shortcut_label} 注册失败：{e}。鼠标自动隐藏仍可使用。")
                    });
                    (Some(key), error)
                }
                Err(e) => (
                    None,
                    Some(format!(
                        "老板键 {shortcut_label} 注册失败：{e}。正常阅读仍可使用。"
                    )),
                ),
            },
            None => (
                None,
                Some(format!(
                    "老板键 {shortcut_label} 注册失败：无法获取有效窗口句柄。正常阅读仍可使用。"
                )),
            ),
        };
        let error = match (error, &boss_key_error) {
            (Some(e), Some(boss_error)) => Some(format!("{e}\n{boss_error}")),
            (None, Some(boss_error)) => Some(boss_error.clone()),
            (error, None) => error,
        };
        let mut reader = Self {
            settings,
            data_dir: dir.ok(),
            book: None,
            data: BookData::default(),
            paragraphs: Vec::new(),
            layout: Layout::default(),
            images: Images::new(),
            font_list,
            font_key,
            true_bold: font_result.unwrap_or(false),
            error,
            info: hwnd.is_none().then(|| "当前环境不支持窗口透明度".into()),
            settings_open: false,
            toc_overlay_open: false,
            search: String::new(),
            focus_search: false,
            fullscreen: false,
            hwnd,
            visibility,
            boss_key_error,
            boss_shortcut_draft,
            boss_shortcut_error: None,
            opacity_available: hwnd.is_some(),
            applied_opacity: None,
            last_low_opacity,
            opacity_mode,
            opacity_reapply: false,
            scroll: 0.,
            scroll_request: None,
            jump: None,
            dirty: false,
            last_save: Instant::now(),
            last_save_error: None,
            first_frame: true,
            load_path,
            picker: None,
            #[cfg(test)]
            body_geometry: None,
            #[cfg(test)]
            progress_rect: None,
            #[cfg(test)]
            empty_open_rect: None,
        };
        reader.apply_window_opacity();
        reader
    }
    fn apply_window_opacity(&mut self) {
        if !self.opacity_available {
            return;
        }
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let percent = self.settings.window_opacity.clamp(20, 100);
        if self.applied_opacity == Some(percent) {
            return;
        }
        match crate::native::set_window_opacity(hwnd, percent) {
            Ok(()) => {
                self.applied_opacity = Some(percent);
                if percent < 100 {
                    self.last_low_opacity = percent;
                }
            }
            Err(_) => {
                // 失败后尽量恢复可见状态，并禁用入口，避免每帧重试。
                let _ = crate::native::set_window_opacity(hwnd, 100);
                // HWND 继续供老板键和窗口几何检查使用，只禁用透明度。
                self.opacity_available = false;
                self.applied_opacity = None;
                self.settings.window_opacity = 100;
                self.dirty = true;
                self.info = Some("当前环境不支持窗口透明度".into());
            }
        }
    }
    fn save(&mut self) -> bool {
        let Some(dir) = &self.data_dir else {
            self.last_save = Instant::now();
            self.dirty = true;
            // 启动时已报告目录错误，保留待保存状态，不重复弹窗。
            return false;
        };
        let result = (|| {
            storage::write(&dir.join("settings.json"), &self.settings)?;
            if let Some(book) = &self.book {
                storage::write(&dir.join(format!("{}.json", book.hash)), &self.data)?;
            }
            Ok::<(), String>(())
        })();
        let saved = result.is_ok();
        if let Err(e) = result {
            self.dirty = true;
            let message = format!("保存失败：{e}");
            if self.last_save_error.as_ref() != Some(&message) {
                self.error = Some(message.clone());
                self.last_save_error = Some(message);
            }
        } else {
            self.dirty = false;
            self.last_save_error = None;
        }
        self.last_save = Instant::now();
        saved
    }
    fn autosave(&mut self, ctx: &egui::Context) {
        // 没有可用的目录时，定时重试没有意义；手动保存仍会保留 dirty。
        if !self.dirty || self.data_dir.is_none() {
            return;
        }
        if self.last_save.elapsed() > Duration::from_millis(800) {
            self.save();
        }
        if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(850));
        }
    }
    fn open(&mut self, path: &Path) {
        self.save();
        match Book::open(path) {
            Ok(mut book) => {
                let result = self
                    .data_dir
                    .as_ref()
                    .map(|d| storage::read::<BookData>(&d.join(format!("{}.json", book.hash))))
                    .unwrap_or_else(|| Ok(BookData::default()));
                let mut data = match result {
                    Ok(d) => d,
                    Err(e) => {
                        self.error = Some(e);
                        BookData::default()
                    }
                };
                data.location.chapter = data.location.chapter.min(book.chapters.len() - 1);
                match book.chapter(data.location.chapter) {
                    Ok(p) => self.paragraphs = p.clone(),
                    Err(e) => {
                        self.error = Some(e);
                        return;
                    }
                }
                self.settings.last_book = std::fs::canonicalize(path)
                    .unwrap_or_else(|_| path.into())
                    .to_string_lossy()
                    .into();
                self.jump = Some(data.location.clone());
                self.data = data;
                self.book = Some(book);
                self.images.clear();
                self.layout.clear();
                self.dirty = true;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn go(&mut self, chapter: usize, paragraph: usize, offset: f32) {
        if let Some(book) = &mut self.book {
            let chapter = chapter.min(book.chapters.len() - 1);
            if chapter != self.data.location.chapter {
                match book.chapter(chapter) {
                    Ok(p) => self.paragraphs = p.clone(),
                    Err(e) => {
                        self.error = Some(e);
                        return;
                    }
                }
                self.images.clear();
                self.layout.clear();
            }
            self.data.location = Location {
                chapter,
                paragraph: paragraph.min(self.paragraphs.len().saturating_sub(1)),
                offset: offset.clamp(0., 1.),
            };
            self.jump = Some(self.data.location.clone());
            self.dirty = true;
        }
    }
    fn go_anchor(&mut self, chapter: usize, anchor: &str) {
        self.go(chapter, 0, 0.);
        if !anchor.is_empty() {
            if let Some(i) = self.paragraphs.iter().position(|p| p.anchor == anchor) {
                self.go(chapter, i, 0.);
            }
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let (focused, modifiers, keys, scroll) = ctx.input(|i| {
            (
                i.focused,
                i.modifiers,
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            repeat,
                            ..
                        } => Some((*key, *modifiers, *repeat)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                i.raw_scroll_delta.y,
            )
        });
        if !focused {
            return;
        }
        if modifiers.ctrl && scroll != 0. {
            self.settings.size = (self.settings.size + scroll.signum()).clamp(12., 48.);
            self.dirty = true;
            ctx.input_mut(|i| {
                i.raw_scroll_delta = Vec2::ZERO;
                i.smooth_scroll_delta = Vec2::ZERO;
            });
        }
        for (key, modifiers, repeat) in keys {
            let ctrl = modifiers.ctrl;
            let shift = modifiers.shift;
            if modifiers.alt || modifiers.mac_cmd {
                continue;
            }
            if (ctrl && shift && matches!(key, Key::ArrowUp | Key::ArrowDown))
                || (!ctrl && !shift && key == Key::F9)
            {
                // 消耗快捷键，避免方向键继续影响阅读位置或聚焦的控件。
                ctx.input_mut(|i| {
                    i.consume_key(modifiers, key);
                });
                if self.opacity_available && self.hwnd.is_some() {
                    self.settings.window_opacity = match key {
                        Key::ArrowUp => self.settings.window_opacity.saturating_add(5).min(100),
                        Key::ArrowDown => self.settings.window_opacity.saturating_sub(5).max(20),
                        Key::F9 if !repeat => {
                            toggle_opacity(self.settings.window_opacity, &mut self.last_low_opacity)
                        }
                        _ => self.settings.window_opacity,
                    };
                    self.dirty = true;
                }
                continue;
            }
            // 原有 Ctrl 快捷键不接收额外 Shift，尤其不能误触发翻章。
            if ctrl && shift {
                continue;
            }
            if key == Key::Escape {
                self.toc_overlay_open = false;
                self.settings_open = false;
                self.focus_search = false;
                ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
                if self.fullscreen {
                    self.fullscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                }
                continue;
            }
            match (ctrl, key) {
                (true, Key::O) => {
                    self.choose_file();
                }
                (true, Key::G) | (false, Key::F2) => {
                    self.show_toc(ctx);
                    self.focus_search = true;
                }
                (true, Key::Comma) => {
                    self.settings_open = !self.settings_open;
                }
                (_, Key::F4) => {
                    self.toggle_toc(ctx);
                }
                (_, Key::F11) => {
                    self.fullscreen = !self.fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                }
                (_, Key::F10) => {
                    self.settings.immersive = !self.settings.immersive;
                    self.dirty = true;
                }
                (true, Key::Plus) | (true, Key::Equals) => {
                    self.settings.size = (self.settings.size + 1.).min(48.);
                    self.dirty = true;
                }
                (true, Key::Minus) => {
                    self.settings.size = (self.settings.size - 1.).max(12.);
                    self.dirty = true;
                }
                _ => {
                    if ctx.wants_keyboard_input() {
                        continue;
                    }
                    match (ctrl, key) {
                        (true, Key::ArrowLeft) => {
                            self.go(self.data.location.chapter.saturating_sub(1), 0, 0.)
                        }
                        (true, Key::ArrowRight) => self.go(self.data.location.chapter + 1, 0, 0.),
                        (false, Key::Home) => {
                            self.scroll = 0.;
                            self.jump = Some(Location {
                                chapter: self.data.location.chapter,
                                paragraph: 0,
                                offset: 0.,
                            });
                        }
                        (false, Key::End) => {
                            // Clamp to the new chapter bottom even if an image
                            // finishes loading and changes layout this frame.
                            self.scroll_request = Some(f32::MAX);
                            self.jump = None;
                        }
                        (false, Key::PageDown) | (false, Key::Space) if !shift => {
                            self.scroll += ctx.screen_rect().height() * 0.82;
                            self.scroll_request = Some(self.scroll);
                            self.jump = None;
                        }
                        (false, Key::PageUp) | (false, Key::Space) => {
                            self.scroll -= ctx.screen_rect().height() * 0.82;
                            self.scroll_request = Some(self.scroll.max(0.));
                            self.jump = None;
                        }
                        (false, Key::ArrowDown) => {
                            self.scroll += self.settings.size * 3.;
                            self.scroll_request = Some(self.scroll);
                            self.jump = None;
                        }
                        (false, Key::ArrowUp) => {
                            self.scroll -= self.settings.size * 3.;
                            self.scroll_request = Some(self.scroll.max(0.));
                            self.jump = None;
                        }
                        (_, Key::Escape) => {
                            self.settings_open = false;
                            if self.fullscreen {
                                self.fullscreen = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        self.scroll = self.scroll.max(0.);
    }
    fn show_toc(&mut self, ctx: &egui::Context) {
        self.settings.immersive = false;
        if ctx.screen_rect().width() < COMPACT_BELOW {
            self.toc_overlay_open = true;
        } else {
            self.settings.toc = true;
        }
        self.dirty = true;
    }
    fn toggle_toc(&mut self, ctx: &egui::Context) {
        if ctx.screen_rect().width() < COMPACT_BELOW {
            self.toc_overlay_open = !self.toc_overlay_open;
            if self.toc_overlay_open {
                self.settings.immersive = false;
                self.dirty = true;
            }
        } else {
            self.settings.toc = !self.settings.toc;
            self.settings.immersive = false;
            self.dirty = true;
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        if self.settings.immersive {
            return;
        }
        let palette = Palette::for_settings(&self.settings);
        let panel = egui::TopBottomPanel::top("toolbar")
            .show_separator_line(false)
            .frame(theme::panel_frame(
                palette,
                egui::Margin::symmetric(PANEL_SIDE, 14),
            ))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), TOOLBAR_HEIGHT),
                    egui::Sense::hover(),
                );
                let (compact, left, right, middle) = toolbar_geometry(rect);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(left)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                        ui.menu_button(
                            Icon::BookOpen.label(if compact {
                                "阅读器"
                            } else {
                                "便携阅读器"
                            }),
                            |ui| {
                                if ui.button("打开 EPUB…   Ctrl+O").clicked() {
                                    ui.close_menu();
                                    self.choose_file();
                                }
                                ui.separator();
                                if ui.button("退出").clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            },
                        );
                        ui.separator();
                        if theme::action(ui, Icon::FolderOpen, "打开", false)
                            .on_hover_text("打开 EPUB · Ctrl+O；也可以拖入文件")
                            .clicked()
                        {
                            self.choose_file();
                        }
                    },
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(right)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        if compact {
                            ui.menu_button("更多", |ui| {
                                if ui.button("阅读排版   Ctrl+,").clicked() {
                                    self.settings_open = !self.settings_open;
                                    ui.close_menu();
                                }
                                if ui.button("沉浸阅读   F10").clicked() {
                                    self.settings.immersive = true;
                                    self.dirty = true;
                                    ui.close_menu();
                                }
                            });
                        } else {
                            if theme::action(ui, Icon::Moon, "沉浸", false)
                                .on_hover_text("沉浸阅读 · F10")
                                .clicked()
                            {
                                self.settings.immersive = true;
                                self.dirty = true;
                            }
                            if theme::action(ui, Icon::TextAa, "排版", self.settings_open)
                                .on_hover_text("阅读排版 · Ctrl+,")
                                .clicked()
                            {
                                self.settings_open = !self.settings_open;
                            }
                        }
                        let selected = if compact {
                            self.toc_overlay_open
                        } else {
                            self.settings.toc
                        };
                        if theme::action(ui, Icon::List, "目录", selected)
                            .on_hover_text("章节目录 · F4；搜索章节 · Ctrl+G / F2")
                            .clicked()
                        {
                            self.toggle_toc(ctx);
                        }
                    },
                );
                if let Some(middle) = middle {
                    if let Some(book) = &self.book {
                        let title = if book.author.is_empty() {
                            book.title.clone()
                        } else {
                            format!("{} · {}", book.title, book.author)
                        };
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(middle).layout(
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            ),
                            |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&title).color(palette.muted))
                                        .truncate(),
                                )
                                .on_hover_text(&title);
                            },
                        );
                    }
                }
                rect
            });
        // The content Ui excludes the frame margin; the panel response includes it.
        ctx.layer_painter(panel.response.layer_id).hline(
            panel.inner.left() - f32::from(PANEL_SIDE)
                ..=panel.inner.right() + f32::from(PANEL_SIDE),
            panel.response.rect.bottom() - 0.5,
            egui::Stroke::new(1.0_f32, palette.line),
        );
    }
    fn footer(&mut self, ctx: &egui::Context) {
        let palette = Palette::for_settings(&self.settings);
        egui::TopBottomPanel::bottom("progress")
            .frame(theme::panel_frame(
                palette,
                egui::Margin::symmetric(PANEL_SIDE, 8),
            ))
            .show(ctx, |ui| {
                let mut target: Option<(usize, Option<f32>)> = None;
                if let Some(book) = &self.book {
                    let count = book.chapters.len();
                    let chapter = self.data.location.chapter;
                    let local = (self.data.location.paragraph as f32 + self.data.location.offset)
                        / self.paragraphs.len().max(1) as f32;
                    let mut percent = (chapter as f32 + local) / count as f32 * 100.;
                    // The full-width track owns its row; labels never change drag geometry.
                    ui.scope(|ui| {
                        ui.spacing_mut().slider_width = ui.available_width();
                        ui.spacing_mut().interact_size.y = 18.;
                        let v = ui.visuals_mut();
                        v.slider_trailing_fill = true;
                        v.selection.bg_fill = palette.accent;
                        v.widgets.inactive.bg_fill = palette.line;
                        for w in [
                            &mut v.widgets.inactive,
                            &mut v.widgets.hovered,
                            &mut v.widgets.active,
                        ] {
                            w.bg_fill = palette.line;
                            w.fg_stroke = egui::Stroke::new(1.0_f32, palette.accent);
                            w.bg_stroke = egui::Stroke::NONE;
                        }
                        let r = ui.add(
                            egui::Slider::new(&mut percent, 0.0..=100.0)
                                .clamping(egui::SliderClamping::Edits)
                                .show_value(false),
                        );
                        let radius = r.rect.height() / 2.5;
                        let x = r.rect.left()
                            + radius
                            + percent.clamp(0., 100.) / 100. * (r.rect.width() - 2. * radius);
                        ui.painter().circle_filled(
                            egui::pos2(x, r.rect.center().y),
                            radius,
                            palette.accent,
                        );
                        #[cfg(test)]
                        {
                            self.progress_rect = Some(r.rect);
                        }
                        if r.changed() {
                            let x = percent / 100. * count as f32;
                            target = Some((
                                (x.floor() as usize).min(count - 1),
                                Some(if percent >= 100. { 1. } else { x.fract() }),
                            ));
                        }
                    });
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), FOOTER_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let middle = Rect::from_center_size(
                        rect.center(),
                        egui::vec2(FOOTER_NAV_W, rect.height()),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(middle)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            if ui
                                .add_enabled(
                                    chapter > 0,
                                    egui::Button::new(Icon::CaretLeft.label("上一章")).frame(false),
                                )
                                .on_hover_text("Ctrl+←")
                                .clicked()
                            {
                                target = Some((chapter.saturating_sub(1), None));
                            }
                            if ui
                                .add_enabled(
                                    chapter + 1 < count,
                                    egui::Button::new(Icon::CaretRight.label("下一章"))
                                        .frame(false),
                                )
                                .on_hover_text("Ctrl+→")
                                .clicked()
                            {
                                target = Some((chapter + 1, None));
                            }
                        },
                    );
                    let left =
                        Rect::from_min_max(rect.min, egui::pos2(middle.left() - 8., rect.bottom()));
                    let title = &book.chapters[chapter].title;
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(left)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            if self.settings.immersive {
                                if ui
                                    .button("退出沉浸")
                                    .on_hover_text("恢复工具栏 · F10")
                                    .clicked()
                                {
                                    self.settings.immersive = false;
                                    self.dirty = true;
                                }
                            } else {
                                let label = if rect.width() < FOOTER_LABEL_PREFIX_MIN_W {
                                    title.clone()
                                } else {
                                    format!("当前章节  {title}")
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(label).color(palette.muted).size(13.),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(title);
                            }
                        },
                    );
                    let right =
                        Rect::from_min_max(egui::pos2(middle.right() + 8., rect.top()), rect.max);
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(right)
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                        |ui| {
                            ui.label(
                                RichText::new(format!("{percent:.1}% · 全书"))
                                    .color(palette.muted)
                                    .size(13.),
                            )
                            .on_hover_text("按章节与章内位置估算，并非原书页码");
                        },
                    );
                } else {
                    ui.horizontal(|ui| {
                        if self.settings.immersive && ui.button("退出沉浸").clicked() {
                            self.settings.immersive = false;
                            self.dirty = true;
                        }
                        ui.label(
                            RichText::new("离线阅读 · 拖入 EPUB 或按 Ctrl+O 打开")
                                .color(palette.muted)
                                .size(13.),
                        );
                    });
                }
                if let Some((c, f)) = target {
                    self.go(c, 0, 0.);
                    if let Some(f) = f {
                        let p = f * self.paragraphs.len() as f32;
                        self.go(c, p.floor() as usize, p.fract());
                        if f >= 1. {
                            self.scroll = self.layout.height;
                            self.jump = Some(Location {
                                chapter: c,
                                paragraph: self.paragraphs.len().saturating_sub(1),
                                offset: 1.,
                            });
                        }
                    }
                }
            });
    }
    fn toc_contents(&mut self, ui: &mut egui::Ui, toc: &[Toc], dest: &mut Option<(usize, String)>) {
        let palette = Palette::for_settings(&self.settings);
        if let Some(book) = &self.book {
            ui.add(egui::Label::new(RichText::new(&book.title).size(22.).strong()).wrap());
            if !book.author.is_empty() {
                ui.label(RichText::new(&book.author).color(palette.muted));
            }
            ui.add_space(16.);
        }
        ui.label(RichText::new("目录").size(17.).strong());
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(f32::INFINITY)
                .hint_text("搜索章节"),
        );
        response.clone().on_hover_text("搜索章节 · Ctrl+G / F2");
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        ui.add_space(6.);
        egui::ScrollArea::vertical()
            .id_salt("toc-scroll")
            .max_height(ui.available_height().max(40.))
            .show(ui, |ui| {
                let filter = self.search.to_lowercase();
                self.dirty |= toc_ui(
                    ui,
                    toc,
                    &filter,
                    self.data.location.chapter,
                    dest,
                    &mut self.settings.collapsed_toc,
                );
                if toc.is_empty() {
                    ui.label(RichText::new("打开书籍后显示章节").color(palette.muted));
                } else if !toc.iter().any(|n| toc_matches(n, &filter)) {
                    ui.label(RichText::new("没有匹配的章节").color(palette.muted));
                }
            });
    }
    fn toc_panel(&mut self, ctx: &egui::Context) {
        if self.settings.immersive {
            return;
        }
        let narrow = ctx.screen_rect().width() < COMPACT_BELOW;
        if !narrow {
            self.toc_overlay_open = false;
        }
        if !(if narrow {
            self.toc_overlay_open
        } else {
            self.settings.toc
        }) {
            return;
        }
        let toc = self
            .book
            .as_ref()
            .map(|b| b.toc.clone())
            .unwrap_or_default();
        let mut dest = None;
        let mut palette = Palette::for_settings(&self.settings);
        palette.surface = palette.sidebar;
        if narrow {
            let mut open = true;
            egui::Window::new("章节目录")
                .id(egui::Id::new("compact-toc"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width((ctx.screen_rect().width() - 80.).min(360.))
                .max_height((ctx.screen_rect().height() - 110.).max(120.))
                .frame(theme::panel_frame(palette, egui::Margin::same(18)))
                .show(ctx, |ui| self.toc_contents(ui, &toc, &mut dest));
            self.toc_overlay_open = open && dest.is_none();
        } else {
            let panel = egui::SidePanel::left("toc")
                .default_width(self.settings.toc_width)
                .width_range(150.0..=(ctx.screen_rect().width() - 400.).clamp(150., 600.))
                .resizable(true)
                .frame(theme::panel_frame(palette, egui::Margin::symmetric(18, 24)))
                .show(ctx, |ui| self.toc_contents(ui, &toc, &mut dest));
            let width = panel.response.rect.width();
            if (self.settings.toc_width - width).abs() > 0.1
                && ctx.input(|i| i.pointer.primary_down())
            {
                self.settings.toc_width = width;
                self.dirty = true;
            }
        }
        if let Some((c, a)) = dest {
            self.go_anchor(c, &a);
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
    }
    fn settings_panel(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let before = self.settings.clone();
        let mut open = true;
        let palette = Palette::for_settings(&self.settings);
        egui::Window::new("阅读排版")
            .id(egui::Id::new("reader-settings"))
            .open(&mut open)
            .default_pos(egui::pos2((ctx.screen_rect().right() - 440.).max(16.), 80.))
            .default_width(380.)
            .default_height((ctx.screen_rect().height() - 260.).clamp(180., 500.))
            .max_width((ctx.screen_rect().width() - 48.).min(420.))
            .max_height((ctx.screen_rect().height() - 100.).max(160.))
            .vscroll(true)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("改动即时生效，阅读位置自动保留")
                        .color(palette.muted)
                        .size(13.),
                );
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    for (i, name) in ["白天", "纸张", "夜间"].iter().enumerate() {
                        if ui
                            .add_sized(
                                [96., 34.],
                                egui::Button::new(*name).selected(self.settings.theme == i),
                            )
                            .clicked()
                        {
                            self.settings.set_theme(i);
                        }
                    }
                });
                ui.add_space(10.);
                egui::Grid::new("reading-settings-grid")
                    .num_columns(2)
                    .spacing([16., 16.])
                    .show(ui, |ui| {
                        ui.label("字体");
                        egui::ComboBox::from_id_salt("font")
                            .selected_text(&self.settings.font)
                            .width(210.)
                            .show_ui(ui, |ui| {
                                for f in &self.font_list {
                                    ui.selectable_value(
                                        &mut self.settings.font,
                                        f.name.clone(),
                                        &f.name,
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("字号");
                        ui.add(
                            egui::Slider::new(&mut self.settings.size, 12.0..=48.0)
                                .clamping(egui::SliderClamping::Edits)
                                .step_by(1.)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("行间距");
                        ui.add(
                            egui::Slider::new(&mut self.settings.line_spacing, 1.0..=3.0)
                                .clamping(egui::SliderClamping::Edits)
                                .step_by(0.1),
                        );
                        ui.end_row();
                        ui.label("正文宽度");
                        ui.add(
                            egui::Slider::new(&mut self.settings.max_width, 300.0..=1600.0)
                                .clamping(egui::SliderClamping::Edits)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
                ui.add_space(6.);
                egui::CollapsingHeader::new("更多排版").show(ui, |ui| {
                    egui::Grid::new("extra-reading-settings")
                        .num_columns(2)
                        .spacing([16., 14.])
                        .show(ui, |ui| {
                            ui.label("字重");
                            ui.horizontal(|ui| {
                                for (i, name) in ["细", "常规", "粗"].iter().enumerate() {
                                    ui.selectable_value(&mut self.settings.weight, i, *name);
                                }
                            });
                            ui.end_row();
                            ui.label("段间距");
                            ui.add(
                                egui::Slider::new(&mut self.settings.paragraph_spacing, 0.0..=60.0)
                                    .clamping(egui::SliderClamping::Edits)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                            ui.label("左右边距");
                            ui.add(
                                egui::Slider::new(&mut self.settings.margin, 0.0..=160.0)
                                    .clamping(egui::SliderClamping::Edits)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                            ui.label("背景色");
                            ui.color_edit_button_srgb(&mut self.settings.background);
                            ui.end_row();
                            ui.label("文字色");
                            ui.color_edit_button_srgb(&mut self.settings.foreground);
                            ui.end_row();
                        });
                });
                ui.separator();
                egui::CollapsingHeader::new("窗口与快捷键").show(ui, |ui| self.window_settings(ui));
                ui.separator();
                if ui.button("恢复默认排版").clicked() {
                    self.settings.restore_typography();
                }
                ui.label(
                    RichText::new("Ctrl+滚轮调字号 · F10 沉浸 · F11 全屏")
                        .size(13.)
                        .color(palette.muted),
                );
            });
        self.settings_open = open;
        self.dirty |= before != self.settings;
    }
    fn window_settings(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("window-settings-grid")
            .num_columns(2)
            .spacing([15., 12.])
            .show(ui, |ui| {
                ui.label("窗口透明度");
                ui.add_enabled_ui(self.opacity_available, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut self.settings.window_opacity, 20..=100)
                                .clamping(egui::SliderClamping::Edits)
                                .step_by(5.0)
                                .suffix("%"),
                        );
                        if ui.button("100%").clicked() {
                            self.settings.window_opacity = 100;
                        }
                    });
                });
                ui.end_row();
                ui.label("鼠标自动显隐");
                ui.add_enabled(
                    self.visibility
                        .as_ref()
                        .is_some_and(|v| v.auto_hide_available()),
                    egui::Checkbox::new(
                        &mut self.settings.auto_hide_on_mouse_leave,
                        "移出隐藏，移入显示",
                    ),
                );
                ui.end_row();
            });
        ui.small("开启后，鼠标离开窗口即隐藏；回到原窗口区域自动显示。开关自动保存。");
        ui.small(format!(
            "老板键主动隐藏后，仍需 {} 恢复。",
            self.settings.boss_shortcut.label()
        ));
        if !self
            .visibility
            .as_ref()
            .is_some_and(|v| v.auto_hide_available())
        {
            ui.small("当前环境不支持鼠标自动显隐。");
        }
        ui.small("Ctrl+Shift+↑ / ↓ 调透明度 · F9 切换 · 文字也会变淡");
        ui.separator();
        ui.label(format!(
            "老板键：{}（全局）",
            self.settings.boss_shortcut.label()
        ));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.boss_shortcut_draft.ctrl, "Ctrl");
            ui.checkbox(&mut self.boss_shortcut_draft.alt, "Alt");
            ui.checkbox(&mut self.boss_shortcut_draft.shift, "Shift");
            ui.checkbox(&mut self.boss_shortcut_draft.win, "Win");
            egui::ComboBox::from_id_salt("boss-shortcut-key")
                .selected_text(BossShortcut::key_label(self.boss_shortcut_draft.key))
                .width(85.)
                .show_ui(ui, |ui| {
                    for key in BossShortcut::keys() {
                        ui.selectable_value(
                            &mut self.boss_shortcut_draft.key,
                            key,
                            BossShortcut::key_label(key),
                        );
                    }
                });
        });
        ui.horizontal(|ui| {
            let can_apply = self
                .visibility
                .as_ref()
                .is_some_and(|v| v.auto_hide_available());
            if ui
                .add_enabled(can_apply, egui::Button::new("应用快捷键"))
                .clicked()
            {
                self.apply_boss_shortcut();
            }
            if ui
                .add_enabled(can_apply, egui::Button::new("恢复默认快捷键"))
                .clicked()
            {
                self.boss_shortcut_draft = BossShortcut::default();
                self.apply_boss_shortcut();
            }
        });
        ui.small("选择修饰键和主键后点击应用；生效后自动保存。功能键可单独使用（F12 除外）。");
        if let Some(error) = &self.boss_shortcut_error {
            ui.colored_label(Color32::from_rgb(190, 65, 65), error);
        }
        ui.small("按一次隐藏整个窗口，再按一次恢复；失去焦点时仍有效。");
        ui.small("隐藏后任务栏及 Alt+Tab 均不可见，请用同一快捷键恢复。");
        if let Some(error) = &self.boss_key_error {
            ui.colored_label(Color32::from_rgb(190, 65, 65), error);
        } else if self.visibility.as_ref().is_some_and(|key| key.available()) {
            ui.small("状态：已注册；松开后再次按下才切换，隐藏状态不会保存。");
        }
    }
    fn apply_boss_shortcut(&mut self) {
        let result = self
            .visibility
            .as_mut()
            .ok_or_else(|| "当前环境不支持全局老板键".to_owned())
            .and_then(|visibility| visibility.set_shortcut(self.boss_shortcut_draft));
        match result {
            Ok(()) => {
                self.settings.boss_shortcut = self.boss_shortcut_draft;
                self.boss_key_error = None;
                self.boss_shortcut_error = None;
                self.dirty = true;
            }
            Err(error) => {
                let status = if self.visibility.as_ref().is_some_and(|v| v.available()) {
                    format!("原快捷键 {} 仍有效。", self.settings.boss_shortcut.label())
                } else {
                    "请更换组合后重试。".to_owned()
                };
                self.boss_shortcut_error = Some(format!("{error}。{status}"));
            }
        }
    }
    fn choose_file(&mut self) {
        let directory = Path::new(&self.settings.last_book)
            .parent()
            .filter(|p| p.is_dir())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(PathBuf::from))
            })
            .unwrap_or_else(|| PathBuf::from("C:\\"));
        self.picker = Some(crate::picker::Picker::new(directory));
    }
    fn load_visible_images(&mut self, ctx: &egui::Context, visible: Rect) {
        if self.book.is_none() {
            return;
        }
        let pending: Vec<_> = self
            .paragraphs
            .iter()
            .zip(&self.layout.rows)
            .enumerate()
            .filter_map(|(index, (p, row))| {
                if row.y > visible.max.y + 100. || row.y + row.height < visible.min.y - 100. {
                    return None;
                }
                match &p.kind {
                    Kind::Image(path) => Some((index, path.clone())),
                    _ => None,
                }
            })
            .collect();
        let old_scroll = self.scroll;
        let mut changed = false;
        for (index, path) in pending {
            if !self.images.contains_key(&path) {
                let result = (|| {
                    let book = self.book.as_mut().ok_or_else(|| "未打开书籍".to_owned())?;
                    let bytes = book.image(&path)?;
                    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                        .with_guessed_format()
                        .map_err(|e| e.to_string())?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(12000);
                    limits.max_image_height = Some(12000);
                    limits.max_alloc = Some(128 * 1024 * 1024);
                    reader.limits(limits);
                    let img = reader
                        .decode()
                        .map_err(|e| format!("图片无法解码：{e}"))?
                        .to_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                    Ok(ctx.load_texture(&path, color, egui::TextureOptions::LINEAR))
                })();
                self.images.insert(path.clone(), result);
            }
            let above = self.layout.rows[index].y <= self.scroll;
            if let Some(delta) = self.layout.refresh_image_row(index, &path, &self.images) {
                changed = true;
                if above {
                    self.scroll += delta;
                    self.scroll_request = Some(self.scroll);
                }
            }
        }
        self.prune_image_cache(visible.translate(egui::vec2(0., self.scroll - old_scroll)));
        if changed {
            ctx.request_repaint();
        }
    }
    fn prune_image_cache(&mut self, visible: Rect) {
        if self.images.len() <= IMAGE_CACHE_LIMIT {
            return;
        }
        let margin = visible.height() * 2.;
        let mut nearby = std::collections::HashMap::<&str, f32>::new();
        for (p, row) in self.paragraphs.iter().zip(&self.layout.rows) {
            if let Kind::Image(path) = &p.kind {
                if row.y <= visible.max.y + margin && row.y + row.height >= visible.min.y - margin {
                    let distance = (row.y - visible.max.y)
                        .max(visible.min.y - row.y - row.height)
                        .max(0.);
                    nearby
                        .entry(path)
                        .and_modify(|old| *old = old.min(distance))
                        .or_insert(distance);
                }
            }
        }
        // Normally the five-screen window fits the limit. Prefer the nearest
        // textures even in chapters containing many tiny images.
        let mut keep: Vec<_> = nearby
            .into_iter()
            .filter(|(path, _)| self.images.contains_key(*path))
            .collect();
        keep.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(b.0)));
        keep.truncate(IMAGE_CACHE_LIMIT);
        let keep: std::collections::HashSet<_> = keep.into_iter().map(|(path, _)| path).collect();
        self.images.retain(|path, _| keep.contains(path.as_str()));
        // Row dimensions survive eviction so revisiting an image cannot collapse it.
    }
    fn reader(&mut self, ctx: &egui::Context) {
        let bg = Color32::from_rgb(
            self.settings.background[0],
            self.settings.background[1],
            self.settings.background[2],
        );
        let response = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg))
            .show(ctx, |ui| {
                if self.book.is_none() {
                    ui.vertical_centered(|ui| {
                        let compact = ui.available_height() < 400.;
                        ui.spacing_mut().item_spacing.y = 8.;
                        ui.add_space((ui.available_height() - 250.).max(0.) * 0.35);
                        let palette = Palette::for_settings(&self.settings);
                        ui.label(
                            Icon::BookOpen
                                .text(if compact { 32. } else { 44. })
                                .color(palette.accent),
                        );
                        ui.add_space(if compact { 0. } else { 12. });
                        ui.label(RichText::new("留一点时间，读一本书").size(if compact {
                            20.
                        } else {
                            26.
                        }));
                        ui.label(RichText::new("离线阅读 · 数据随程序迁移").color(palette.muted));
                        ui.add_space(if compact { 4. } else { 24. });
                        let open = ui.add(egui::Button::new(Icon::FolderOpen.label("打开 EPUB")));
                        #[cfg(test)]
                        {
                            self.empty_open_rect = Some(open.rect);
                        }
                        if open.clicked() {
                            self.choose_file();
                        }
                        ui.add_space(6.);
                        ui.label(
                            RichText::new("也可以拖入 EPUB 文件 · Ctrl+O")
                                .color(palette.muted)
                                .size(13.),
                        );
                    });
                    return;
                }
                let width = (ui.available_width() - self.settings.margin * 2. - 16.)
                    .min(self.settings.max_width)
                    .max(100.);
                let rebuilt = self.layout.prepare(
                    ui,
                    &self.paragraphs,
                    &self.settings,
                    width,
                    self.data.location.chapter,
                    &self.images,
                    &self.book.as_ref().unwrap().title,
                    &self.book.as_ref().unwrap().chapters[self.data.location.chapter].title,
                );
                if rebuilt && self.jump.is_none() && self.scroll_request.is_none() {
                    self.jump = Some(self.data.location.clone());
                }
                if let Some(location) = self.jump.take() {
                    ctx.request_repaint();
                    if let Some(row) = self.layout.rows.get(location.paragraph) {
                        self.scroll = if location.paragraph == 0 && location.offset == 0. {
                            0.
                        } else {
                            row.y + row.height * location.offset
                        };
                        self.scroll_request = Some(self.scroll);
                    }
                }
                let before_scroll = self.scroll;
                let body_hovered = ui.rect_contains_pointer(ui.available_rect_before_wrap());
                let wheel = if body_hovered
                    && self.picker.is_none()
                    && !self.settings_open
                    && !self.toc_overlay_open
                {
                    ctx.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|e| {
                                if let egui::Event::MouseWheel {
                                    delta, modifiers, ..
                                } = e
                                {
                                    if !modifiers.ctrl {
                                        Some(delta.y)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .sum::<f32>()
                    })
                } else {
                    0.
                };
                let mut image_scroll_delta = 0.;
                let mut area = egui::ScrollArea::vertical()
                    .id_salt("body-scroll")
                    .auto_shrink([false, false]);
                // 普通帧由 egui 保持滚轮/拖动状态，只有主动跳转才覆盖偏移。
                if let Some(offset) = self.scroll_request.take() {
                    area = area.vertical_scroll_offset(offset.max(0.));
                }
                let output = area.show_viewport(ui, |ui, viewport| {
                    // Decode before painting: a reload with unchanged height must
                    // display this frame without requesting another repaint.
                    self.scroll = viewport.min.y;
                    self.load_visible_images(ctx, viewport);
                    image_scroll_delta = self.scroll - viewport.min.y;
                    let total_width = ui.available_width();
                    let (_, rect) = ui.allocate_space(Vec2::new(total_width, self.layout.height));
                    // Apply the same anchor correction to this frame's drawing;
                    // ScrollArea consumes scroll_request on the following frame.
                    let rect = rect.translate(egui::vec2(0., -image_scroll_delta));
                    let viewport = viewport.translate(egui::vec2(0., image_scroll_delta));
                    let left = rect.left() + (total_width - width) * 0.5;
                    if let Some(g) = &self.layout.book_label {
                        ui.painter().galley(
                            egui::pos2(left, rect.top() + 40.),
                            g.clone(),
                            Color32::WHITE,
                        );
                    }
                    if let Some(g) = &self.layout.fallback_heading {
                        ui.painter().galley(
                            egui::pos2(left, rect.top() + self.layout.heading_y),
                            g.clone(),
                            Color32::WHITE,
                        );
                    }
                    let color = Color32::from_rgb(
                        self.settings.foreground[0],
                        self.settings.foreground[1],
                        self.settings.foreground[2],
                    );
                    if let Some(dy) = self.layout.divider_y {
                        ui.painter().line_segment(
                            [
                                egui::pos2(left, rect.top() + dy),
                                egui::pos2(left + 32., rect.top() + dy),
                            ],
                            egui::Stroke::new(1.0_f32, color.gamma_multiply(0.5)),
                        );
                    }
                    let first = self.layout.paragraph_at(viewport.min.y - 60.);
                    let last = (self.layout.paragraph_at(viewport.max.y + 60.) + 1)
                        .min(self.paragraphs.len())
                        .min(self.layout.rows.len());
                    // 整章只有高度占位；仅为可见段落创建交互区域并提交绘制命令。
                    for i in first..last {
                        let row = &self.layout.rows[i];
                        let p = &self.paragraphs[i];
                        let r = Rect::from_min_size(
                            Pos2::new(left, rect.top() + row.y),
                            Vec2::new(width, row.height),
                        );
                        if !ui.is_rect_visible(r) {
                            continue;
                        }
                        if let Some(g) = &row.galley {
                            ui.painter().galley(r.min, g.clone(), Color32::WHITE);
                            if self.settings.weight == 2 && !self.true_bold {
                                ui.painter().galley(
                                    r.min + Vec2::new(0.45, 0.),
                                    g.clone(),
                                    Color32::WHITE,
                                );
                            }
                        } else if let Kind::Image(path) = &p.kind {
                            if let Some(Ok(texture)) = self.images.get(path) {
                                let size = row.image_size.unwrap_or([width, 60.]);
                                let image_rect = Rect::from_min_size(
                                    Pos2::new(r.center().x - size[0] * 0.5, r.top()),
                                    Vec2::new(size[0], size[1]),
                                );
                                ui.painter().image(
                                    texture.id(),
                                    image_rect,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                    Color32::WHITE,
                                );
                            } else {
                                let message = self
                                    .images
                                    .get(path)
                                    .and_then(|r| r.as_ref().err())
                                    .map(|s| s.as_str())
                                    .unwrap_or("正在加载图片…");
                                ui.painter().text(
                                    r.min,
                                    egui::Align2::LEFT_TOP,
                                    message,
                                    egui::FontId::proportional(14.),
                                    Color32::GRAY,
                                );
                            }
                        } else if matches!(p.kind, Kind::Rule) {
                            ui.painter().line_segment(
                                [r.left_center(), r.right_center()],
                                egui::Stroke::new(1.0_f32, Color32::GRAY),
                            );
                        }
                    }
                });
                self.scroll = output.state.offset.y + image_scroll_delta;
                #[cfg(test)]
                {
                    self.body_geometry = Some((output.id, output.content_size, output.inner_rect));
                }
                let index = self.layout.paragraph_at(self.scroll);
                if let Some(row) = self.layout.rows.get(index) {
                    let offset = ((self.scroll - row.y) / row.height.max(1.)).clamp(0., 1.);
                    if self.data.location.paragraph != index
                        || (self.data.location.offset - offset).abs() > 0.001
                    {
                        self.data.location.paragraph = index;
                        self.data.location.offset = offset;
                        self.dirty = true;
                        // 底栏先于正文绘制，需要紧接一帧显示本帧得到的位置。
                        ctx.request_repaint();
                    }
                }
                let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.);
                // 短章没有滚动范围时，滚轮仍可继续阅读下一章。
                if wheel < 0. && before_scroll >= max_offset - 1. {
                    if self
                        .book
                        .as_ref()
                        .is_some_and(|b| self.data.location.chapter + 1 < b.chapters.len())
                    {
                        self.go(self.data.location.chapter + 1, 0, 0.);
                        ctx.request_repaint();
                    }
                } else if wheel > 0. && before_scroll <= 1. && self.data.location.chapter > 0 {
                    self.go(self.data.location.chapter - 1, 0, 0.);
                    self.go(
                        self.data.location.chapter,
                        self.paragraphs.len().saturating_sub(1),
                        1.,
                    );
                    ctx.request_repaint();
                }
            });
        // 只检测右键，不在阅读区叠加点击/拖拽交互，保留 ScrollArea 的输入处理。
        let menu_id = egui::Id::new("reader-context-menu");
        let mut menu = egui::menu::BarState::load(ctx, menu_id);
        let (pos, secondary_clicked, pressed) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_clicked(egui::PointerButton::Secondary),
                i.pointer.any_pressed(),
            )
        });
        if let Some(pos) = pos {
            let in_menu = menu
                .as_ref()
                .is_some_and(|root| root.menu_state.read().area_contains(pos));
            if !in_menu {
                if secondary_clicked
                    && ctx.rect_contains_pointer(response.response.layer_id, response.response.rect)
                {
                    egui::menu::MenuRoot::handle_menu_response(
                        &mut menu,
                        egui::menu::MenuResponse::Create(pos, response.response.id),
                    );
                } else if pressed {
                    egui::menu::MenuRoot::handle_menu_response(
                        &mut menu,
                        egui::menu::MenuResponse::Close,
                    );
                }
            }
        }
        menu.show(&response.response, |ui| {
            if ui.button("排版设置…   Ctrl+,").clicked() {
                self.settings_open = !self.settings_open;
                ui.close_menu();
            }
        });
        menu.store(ctx, menu_id);
    }
}
fn toc_matches(n: &Toc, filter: &str) -> bool {
    filter.is_empty()
        || n.title.to_lowercase().contains(filter)
        || n.children.iter().any(|n| toc_matches(n, filter))
}
fn toc_ui(
    ui: &mut egui::Ui,
    nodes: &[Toc],
    filter: &str,
    current: usize,
    dest: &mut Option<(usize, String)>,
    collapsed: &mut Vec<String>,
) -> bool {
    let mut changed = false;
    for n in nodes {
        if !toc_matches(n, filter) {
            continue;
        }
        let id = format!("{}/{}/{}", n.chapter, n.anchor, n.title);
        let selected = n.chapter == current;
        let mut open = !collapsed.contains(&id) || !filter.is_empty();
        ui.push_id(&id, |ui| {
            ui.horizontal(|ui| {
                if !n.children.is_empty() {
                    let icon = if open {
                        Icon::CaretDown
                    } else {
                        Icon::CaretRight
                    };
                    if ui
                        .add(
                            egui::Button::new(icon.text(16.))
                                .frame(false)
                                .min_size(egui::vec2(22., 32.)),
                        )
                        .on_hover_text(if open { "折叠章节" } else { "展开章节" })
                        .clicked()
                    {
                        open = !open;
                        changed = true;
                        if open {
                            collapsed.retain(|s| s != &id);
                        } else {
                            collapsed.push(id.clone());
                        }
                    }
                }
                let text = RichText::new(&n.title).color(if selected {
                    ui.visuals().selection.stroke.color
                } else {
                    ui.visuals().text_color()
                });
                let size = egui::vec2(ui.available_width(), 36.);
                let r = ui
                    .allocate_ui_with_layout(
                        size,
                        egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                        |ui| {
                            ui.add(
                                egui::Button::new(text)
                                    .truncate()
                                    .selected(selected)
                                    .frame(selected)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(size),
                            )
                        },
                    )
                    .inner;
                if selected {
                    ui.painter().rect_filled(
                        Rect::from_min_size(r.rect.left_top(), egui::vec2(3., r.rect.height())),
                        1.,
                        ui.visuals().selection.stroke.color,
                    );
                }
                if r.clone().on_hover_text(&n.title).clicked() {
                    *dest = Some((n.chapter, n.anchor.clone()));
                }
            });
            if open && !n.children.is_empty() {
                ui.indent(&id, |ui| {
                    changed |= toc_ui(ui, &n.children, filter, current, dest, collapsed);
                });
            }
        });
    }
    changed
}
impl eframe::App for Reader {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let restored = self
            .visibility
            .as_ref()
            .is_some_and(|key| key.take_restored());
        if let Some(error) = self.visibility.as_ref().and_then(|key| key.take_error()) {
            self.error = Some(error.clone());
            self.boss_key_error = Some(error);
        }
        if self.visibility.as_ref().is_some_and(|key| key.hidden()) {
            // 系统可能送来一次残留重绘。冻结阅读/UI 状态，不安排定时重绘。
            // 恢复由 WM_HOTKEY / WM_TIMER 中的 ShowWindow 唤醒。
            return;
        }
        if restored {
            self.applied_opacity = None;
            if self.jump.is_none() {
                self.scroll_request = Some(self.scroll);
            }
        }
        if self.opacity_reapply {
            self.applied_opacity = None;
            self.opacity_reapply = false;
        }
        theme::apply(ctx, &self.settings);
        let font_key = format!("{}/{}", self.settings.font, self.settings.weight);
        if self.font_key != font_key {
            match fonts::install(
                ctx,
                &self.font_list,
                &self.settings.font,
                self.settings.weight,
            ) {
                Ok(b) => self.true_bold = b,
                Err(e) => self.error = Some(e),
            }
            self.font_key = font_key;
            self.layout.clear();
            self.jump = Some(self.data.location.clone());
        }
        self.keyboard(ctx);
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.load_path = Some(path);
        }
        // 先完成首帧，让窗口能立即出现；书籍在下一帧才读取。
        if self.first_frame {
            self.first_frame = false;
            ctx.request_repaint();
        } else if let Some(path) = self.load_path.take() {
            self.open(&path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.book
                    .as_ref()
                    .map(|b| format!("{} — 便携 EPUB 阅读器", b.title))
                    .unwrap_or_else(|| "便携 EPUB 阅读器".into()),
            ));
        }
        self.toolbar(ctx);
        self.footer(ctx);
        self.toc_panel(ctx);
        self.reader(ctx);
        self.settings_panel(ctx);
        let mode = (self.fullscreen, self.settings.immersive);
        if self.opacity_mode != mode {
            self.opacity_mode = mode;
            // 全屏命令在本帧结束后执行，下一帧再补应用一次透明度。
            self.opacity_reapply = true;
            ctx.request_repaint();
        }
        self.apply_window_opacity();
        if restored {
            // Visible(true) 在本帧结束后交给 winit，会重新设置扩展样式。
            // 再重绘一帧补回分层透明度，不重建 GL 上下文或书籍排版。
            self.opacity_reapply = true;
            ctx.request_repaint();
        }
        if let Some(choice) = self.picker.as_mut().and_then(|p| p.show(ctx)) {
            self.picker = None;
            match choice {
                crate::picker::Choice::Cancel => {}
                crate::picker::Choice::File(path) => {
                    self.load_path = Some(path);
                    ctx.request_repaint();
                }
            }
        }
        if let Some(message) = self.error.clone() {
            let mut open = true;
            egui::Window::new("无法完成操作")
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(message);
                    if ui.button("知道了").clicked() {
                        self.error = None;
                    }
                });
            if !open {
                self.error = None;
            }
        }
        if let Some(message) = self.info.clone() {
            let mut open = true;
            egui::Window::new("提示")
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(message);
                    if ui.button("确定").clicked() {
                        self.info = None;
                    }
                });
            if !open {
                self.info = None;
            }
        }
        if !self.fullscreen
            && !restored
            && self
                .hwnd
                .is_some_and(crate::native::window_geometry_available)
            && !ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
                let size = [rect.width(), rect.height()];
                let pos = Some([rect.min.x, rect.min.y]);
                if size.iter().all(|v| v.is_finite() && *v > 0.)
                    && rect.min.x.is_finite()
                    && rect.min.y.is_finite()
                    && (self.settings.window_size != size || self.settings.window_pos != pos)
                {
                    self.settings.window_size = size;
                    self.settings.window_pos = pos;
                    self.dirty = true;
                }
            }
        }
        if let Some(visibility) = &self.visibility {
            if let Err(error) = visibility.set_auto_hide(self.settings.auto_hide_on_mouse_leave) {
                self.settings.auto_hide_on_mouse_leave = false;
                self.dirty = true;
                self.info = Some(error);
                ctx.request_repaint();
            }
        }
        // 全屏样式命令在帧末才执行，等下一帧再提交鼠标隐藏，避免被重新显示。
        if self.visibility.as_ref().is_some_and(|key| {
            key.hide_pending() && !(key.mouse_hide_pending() && self.opacity_reapply)
        }) {
            // 阅读位置和有效几何已在本帧捕获。先真正隐藏，再同步落盘。
            // 保存失败保留 dirty 和错误，用户恢复窗口后才显示提示。
            hide_then_save(
                self,
                |reader| {
                    if let Some(key) = &reader.visibility {
                        key.hide_now();
                    }
                },
                Reader::save,
            );
            if self.visibility.as_ref().is_some_and(|key| key.hidden()) {
                return;
            }
        }
        self.autosave(ctx);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
        if let Some(key) = self.visibility.take() {
            key.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_title_geometry_never_overlaps_buttons_or_has_negative_width() {
        for width in 0..=1200 {
            let rect = Rect::from_min_size(
                Pos2::new(24., 14.),
                egui::vec2(width as f32, TOOLBAR_HEIGHT),
            );
            let (compact, left, right, middle) = toolbar_geometry(rect);
            assert_eq!(compact, rect.width() < COMPACT_BELOW);
            if let Some(middle) = middle {
                assert!(!compact);
                assert!(middle.width() >= TOOLBAR_TITLE_MIN_W - 16.);
                assert!(middle.left() >= left.right() + 8.);
                assert!(middle.right() <= right.left() - 8.);
            }
            if width >= 500 - 2 * i32::from(PANEL_SIDE) {
                assert!(left.right() <= right.left());
            }
        }
    }

    #[test]
    fn toolbar_divider_tracks_the_outer_panel_bottom() {
        let (ctx, mut r) = reader();
        for (width, height) in [(500., 350.), (760., 600.), (1100., 800.)] {
            let mut bottom = 0.;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
                    ..Default::default()
                },
                |ctx| {
                    theme::apply(ctx, &r.settings);
                    r.toolbar(ctx);
                    bottom = ctx.available_rect().top();
                },
            );
            let dividers: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::LineSegment { points, stroke } = &shape.shape {
                        if stroke.width > 0. && (points[1].x - points[0].x - width).abs() < 0.01 {
                            return Some(points);
                        }
                    }
                    None
                })
                .collect();
            assert_eq!(dividers.len(), 1);
            assert_eq!(dividers[0][0].y, bottom - 0.5);
            assert_eq!(dividers[0][1].y, bottom - 0.5);
        }
    }

    #[test]
    fn visible_image_refresh_compensates_only_rows_starting_above_viewport() {
        for relative_top in [-20., 0., 20., 200.] {
            let (ctx, mut r, _time) = long_reader();
            r.paragraphs[5].kind = Kind::Image("test-image".into());
            r.images
                .insert("test-image".into(), Err("placeholder".into()));
            r.layout.clear();
            sized_frame(&ctx, &mut r, 1100., 800., 0.1, vec![]);
            let row_y = r.layout.rows[5].y;
            r.scroll = row_y - relative_top;
            r.scroll_request = None;
            r.jump = None;
            let old_scroll = r.scroll;
            let height = r.layout.height;
            let galley = r.layout.rows[6].galley.clone().unwrap();
            let texture = ctx.load_texture(
                "test-image",
                egui::ColorImage::new([8, 800], Color32::WHITE),
                Default::default(),
            );
            r.images.insert("test-image".into(), Ok(texture));
            let visible = Rect::from_min_size(egui::pos2(0., r.scroll), egui::vec2(600., 300.));
            r.load_visible_images(&ctx, visible);
            assert!(r.jump.is_none());
            assert_eq!(r.layout.height, height + 740.);
            assert!(std::sync::Arc::ptr_eq(
                &galley,
                r.layout.rows[6].galley.as_ref().unwrap()
            ));
            if relative_top <= 0. {
                assert_eq!(r.scroll, old_scroll + 740.);
                assert_eq!(r.scroll_request, Some(r.scroll));
            } else {
                assert_eq!(r.scroll, old_scroll);
                assert_eq!(r.scroll_request, None);
            }
            r.scroll_request = None;
            r.load_visible_images(&ctx, visible);
            assert_eq!(r.scroll_request, None);
        }
    }

    #[test]
    fn visible_image_decodes_and_reloads_without_losing_retained_dimensions() {
        let (ctx, mut r) = reader();
        r.paragraphs = vec![Paragraph {
            kind: Kind::Image("OPS/image.png".into()),
            runs: vec![],
            anchor: String::new(),
        }];
        r.images
            .insert("OPS/image.png".into(), Err("placeholder".into()));
        sized_frame(&ctx, &mut r, 1100., 800., 0., vec![]);
        let height = r.layout.height;
        for _ in 0..2 {
            r.images.remove("OPS/image.png");
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1100., 800.))),
                    ..Default::default()
                },
                |ctx| r.reader(ctx),
            );
            assert!(r.images["OPS/image.png"].is_ok());
            let texture_id = r.images["OPS/image.png"].as_ref().unwrap().id();
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id)));
            assert_eq!(r.layout.rows[0].image_size, Some([120., 60.]));
            assert_eq!(r.layout.height, height);
            assert!(r.jump.is_none() && r.scroll_request.is_none());
        }
    }

    #[test]
    fn image_cache_evicts_distant_textures_without_changing_row_geometry() {
        let (ctx, mut r) = reader();
        let texture = ctx.load_texture(
            "cache-test",
            egui::ColorImage::new([1, 100], Color32::WHITE),
            Default::default(),
        );
        r.paragraphs.clear();
        r.layout.clear();
        for index in 0..100 {
            let path = format!("image-{index}");
            r.paragraphs.push(Paragraph {
                kind: Kind::Image(path.clone()),
                runs: vec![],
                anchor: String::new(),
            });
            r.layout.rows.push(crate::render::Row {
                y: index as f32 * 100.,
                height: 100.,
                galley: None,
                image_size: Some([1., 100.]),
            });
            r.images.insert(path, Ok(texture.clone()));
        }
        r.layout.height = 10000.;
        let before: Vec<_> = r
            .layout
            .rows
            .iter()
            .map(|row| (row.y, row.height, row.image_size))
            .collect();
        r.prune_image_cache(Rect::from_min_max(
            egui::pos2(0., 3000.),
            egui::pos2(600., 3500.),
        ));
        assert!(r.images.len() <= IMAGE_CACHE_LIMIT);
        for index in 0..100 {
            assert_eq!(
                r.images.contains_key(&format!("image-{index}")),
                (19..=45).contains(&index)
            );
        }
        assert_eq!(r.layout.height, 10000.);
        assert_eq!(
            before,
            r.layout
                .rows
                .iter()
                .map(|row| (row.y, row.height, row.image_size))
                .collect::<Vec<_>>()
        );
        // A dense five-screen window must also respect the hard cap.
        for (index, row) in r.layout.rows.iter_mut().enumerate() {
            row.y = index as f32;
            r.images
                .insert(format!("image-{index}"), Ok(texture.clone()));
        }
        r.prune_image_cache(Rect::from_min_size(Pos2::ZERO, egui::vec2(600., 100.)));
        assert_eq!(r.images.len(), IMAGE_CACHE_LIMIT);
    }

    #[test]
    fn empty_chapter_and_cleared_layout_are_safe_for_footer_and_reader() {
        let (ctx, mut r) = reader();
        r.paragraphs.clear();
        for (width, height) in [(500., 350.), (760., 600.), (1100., 800.)] {
            r.layout.clear();
            sized_frame(&ctx, &mut r, width, height, 0., vec![]);
            assert!(r.layout.rows.is_empty() && r.layout.divider_y.is_none());
        }
    }

    struct SaveDir(PathBuf);
    impl SaveDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let path = parent.join(format!(
                "epub-reader-save-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            // 仅创建并清理本测试独占的绝对路径，不覆盖已有目录。
            assert_eq!(path.parent(), Some(parent.as_path()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sized_frame(
        ctx: &egui::Context,
        r: &mut Reader,
        width: f32,
        height: f32,
        time: f64,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                theme::apply(ctx, &r.settings);
                r.keyboard(ctx);
                r.toolbar(ctx);
                r.footer(ctx);
                r.toc_panel(ctx);
                r.reader(ctx);
                r.settings_panel(ctx);
            },
        );
    }
    #[test]
    fn empty_state_open_button_is_visible_and_works_at_minimum_size() {
        let (ctx, mut r) = reader();
        r.book = None;
        sized_frame(&ctx, &mut r, 500., 350., 0., vec![]);
        sized_frame(&ctx, &mut r, 500., 350., 0.1, vec![]);
        let rect = r.empty_open_rect.unwrap();
        assert!(
            rect.top() >= 62. && rect.bottom() < 300.,
            "打开按钮必须在工具栏和底栏之间：{rect:?}"
        );
        let point = rect.center();
        sized_frame(
            &ctx,
            &mut r,
            500.,
            350.,
            0.2,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        sized_frame(
            &ctx,
            &mut r,
            500.,
            350.,
            0.3,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(r.picker.is_some());
    }
    #[test]
    fn long_body_text_stays_inside_the_configured_column() {
        let (ctx, mut r) = reader();
        r.settings.max_width = 300.;
        r.settings.size = 32.;
        sized_frame(&ctx, &mut r, 1100., 800., 0., vec![]);
        for row in &r.layout.rows {
            if let Some(g) = &row.galley {
                assert!(
                    g.size().x <= r.layout.width + 1.,
                    "正文不能横向溢出：{} > {}",
                    g.size().x,
                    r.layout.width
                );
            }
        }
    }

    #[test]
    fn chapter_header_does_not_duplicate_existing_titles_or_skip_top() {
        let (ctx, mut r) = reader();
        r.go(1, 0, 0.);
        sized_frame(&ctx, &mut r, 1100., 800., 0., vec![]);
        assert!(r.layout.fallback_heading.is_none());
        assert_eq!(r.layout.rows.len(), r.paragraphs.len());
        assert_eq!(r.scroll, 0.);
        r.paragraphs.remove(0);
        r.layout.clear();
        r.go(1, 0, 0.);
        sized_frame(&ctx, &mut r, 1100., 800., 0.1, vec![]);
        assert!(r.layout.fallback_heading.is_some());
        assert_eq!(r.layout.rows.len(), r.paragraphs.len());
        assert!(r.layout.rows[0].y > r.layout.divider_y.unwrap());
        assert_eq!(r.scroll, 0.);
    }

    #[test]
    fn compact_toc_does_not_overwrite_saved_sidebar_preference() {
        let (ctx, mut r) = reader();
        r.settings.toc = true;
        r.settings.toc_width = 285.;
        sized_frame(&ctx, &mut r, 500., 350., 0., vec![]);
        assert!(r.settings.toc);
        assert_eq!(r.settings.toc_width, 285.);
        r.show_toc(&ctx);
        sized_frame(&ctx, &mut r, 500., 350., 0.1, vec![]);
        assert!(r.toc_overlay_open);
        assert!(r.settings.toc);
        sized_frame(&ctx, &mut r, 500., 350., 0.2, key(Key::Escape, false));
        assert!(!r.toc_overlay_open);
        assert!(r.settings.toc);
        assert_eq!(r.settings.toc_width, 285.);
        r.show_toc(&ctx);
        sized_frame(&ctx, &mut r, 1100., 800., 0.3, vec![]);
        assert!(!r.toc_overlay_open);
        assert!(r.settings.toc);
    }
    #[test]
    fn sidebar_reflow_preserves_paragraph_and_progress_track_geometry() {
        let (ctx, mut r, mut time) = long_reader();
        let location = r.data.location.clone();
        let track = r.progress_rect.unwrap();
        r.settings.toc = true;
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.data.location.paragraph, location.paragraph);
        assert!((r.data.location.offset - location.offset).abs() < 0.001);
        assert_eq!(r.progress_rect.unwrap(), track);
        let c = r.data.location.chapter;
        r.book.as_mut().unwrap().chapters[c].title = "很长的章节名称".repeat(30);
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.progress_rect.unwrap(), track);
        r.settings.toc = false;
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.data.location.paragraph, location.paragraph);
    }
    #[test]
    fn minimal_layout_settings_and_themes_fit_small_viewports() {
        for theme in 0..3 {
            let (ctx, mut r) = reader();
            r.settings.set_theme(theme);
            r.settings_open = true;
            sized_frame(&ctx, &mut r, 500., 350., 0., vec![]);
            sized_frame(&ctx, &mut r, 500., 350., 0.1, vec![]);
            let (_, _, body) = r.body_geometry.unwrap();
            assert!(body.width() > 300. && body.height() > 150.);
            assert!(r.progress_rect.unwrap().right() <= 500.);
        }
    }

    #[test]
    fn unavailable_data_dir_does_not_repeat_errors_or_schedule_autosave() {
        let (ctx, mut r) = reader();
        r.dirty = true;
        r.error = Some("启动时已提示 data 目录不可用".into());
        assert!(!r.save());
        assert_eq!(r.error.as_deref(), Some("启动时已提示 data 目录不可用"));
        r.error = None; // 用户关闭启动时的提示。
        let mut frame = eframe::Frame::_new_kittest();
        for tick in 0..6 {
            assert!(!r.save());
            assert!(r.error.is_none());
            let expired = Instant::now() - Duration::from_secs(2);
            r.last_save = expired;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    time: Some(tick as f64),
                    ..Default::default()
                },
                |ctx| eframe::App::update(&mut r, ctx, &mut frame),
            );
            assert!(r.error.is_none());
            assert!(r.dirty);
            assert_eq!(r.last_save, expired, "update 不应重试不可用的目录");
        }
        // 单独检查保存调度，排除界面布局/动画本身请求的重绘。
        let idle_ctx = egui::Context::default();
        for tick in 0..6 {
            let output = idle_ctx.run(
                egui::RawInput {
                    time: Some(tick as f64),
                    ..Default::default()
                },
                |ctx| r.autosave(ctx),
            );
            if tick >= 3 {
                assert_eq!(
                    output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
                    Duration::MAX
                );
            }
        }
    }

    #[test]
    fn save_error_is_reported_once_and_reset_after_success() {
        let temp = SaveDir::new();
        let dir = temp.0.join("data");
        let (_, mut r) = reader();
        r.data_dir = Some(dir.clone());
        r.dirty = false;
        assert!(!r.save()); // 子目录不存在，实际存储操作失败。
        assert!(r.dirty);
        let first_error = r.error.take().unwrap();
        assert_eq!(r.last_save_error.as_ref(), Some(&first_error));
        for _ in 0..3 {
            assert!(!r.save());
            assert!(r.error.is_none());
            assert_eq!(r.last_save_error.as_ref(), Some(&first_error));
        }

        std::fs::create_dir(&dir).unwrap();
        assert!(r.save());
        assert!(!r.dirty);
        assert!(r.last_save_error.is_none());
        let saved: BookData =
            storage::read(&dir.join(format!("{}.json", r.book.as_ref().unwrap().hash))).unwrap();
        assert_eq!(saved.location, r.data.location);

        // 改回同类路径不存在错误，验证成功会重新允许提示。
        r.data_dir = Some(temp.0.join("missing"));
        assert!(!r.save());
        assert_eq!(r.error.as_ref(), Some(&first_error));
        assert_eq!(r.last_save_error.as_ref(), Some(&first_error));
        r.error = None;

        // 错误文案改变时，即使未成功过，也应提示一次新的错误。
        std::fs::create_dir(dir.join("settings.json.tmp")).unwrap();
        r.data_dir = Some(dir);
        assert!(!r.save());
        let different_error = r.error.take().unwrap();
        assert_ne!(different_error, first_error);
        assert_eq!(r.last_save_error.as_ref(), Some(&different_error));
        assert!(!r.save());
        assert!(r.error.is_none());
    }

    #[test]
    fn boss_key_hides_before_saving_and_stays_hidden_on_save_failure() {
        use std::cell::{Cell, RefCell};
        let temp = SaveDir::new();
        let (_, mut r) = reader();
        r.data_dir = Some(temp.0.join("missing"));
        r.settings.window_pos = Some([120., 230.]);
        r.settings.window_size = [900., 600.];
        let location = r.data.location.clone();
        let settings = r.settings.clone();
        let hidden = Cell::new(false);
        let calls = RefCell::new(Vec::new());
        let saved = hide_then_save(
            &mut r,
            |r| {
                assert_eq!(r.data.location, location);
                assert!(r.settings == settings);
                calls.borrow_mut().push("hide");
                hidden.set(true);
            },
            |r| {
                assert!(hidden.get(), "存储操作必须在窗口不可见之后");
                assert_eq!(r.data.location, location);
                assert!(r.settings == settings);
                calls.borrow_mut().push("save");
                r.save()
            },
        );
        assert!(!saved);
        assert_eq!(*calls.borrow(), ["hide", "save"]);
        assert!(hidden.get());
        assert!(r.dirty);
        assert!(r.error.is_some());
        assert!(r.last_save_error.is_some());
    }

    #[test]
    fn opacity_shortcuts_do_not_scroll_or_change_chapter() {
        let (ctx, mut r) = reader();
        // 只测键盘逻辑；不会使用这个哨兵句柄调用 Win32。
        r.hwnd = Some(1);
        r.opacity_available = true;
        r.settings.window_opacity = 50;
        r.scroll = 123.;
        let chapter = r.data.location.chapter;
        let mut time = 0.;
        let mut press =
            |r: &mut Reader, key: Key, modifiers: egui::Modifiers, repeat: bool, focused: bool| {
                time += 0.1;
                let mut events = Vec::new();
                if !repeat {
                    // 独立按键先释放；egui 会自行将持续按下识别为 repeat。
                    events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    });
                }
                events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat,
                    modifiers,
                });
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        focused,
                        modifiers,
                        events,
                        ..Default::default()
                    },
                    |ctx| r.keyboard(ctx),
                );
            };
        let ctrl_shift = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        press(&mut r, Key::ArrowUp, ctrl_shift, false, true);
        assert_eq!(r.settings.window_opacity, 55);
        press(&mut r, Key::ArrowDown, ctrl_shift, false, true);
        assert_eq!(r.settings.window_opacity, 50);
        press(&mut r, Key::ArrowLeft, ctrl_shift, false, true);
        press(&mut r, Key::ArrowRight, ctrl_shift, false, true);
        assert_eq!(r.data.location.chapter, chapter);
        assert_eq!(r.scroll, 123.);
        assert!(r.scroll_request.is_none());
        let plain = egui::Modifiers::default();
        press(&mut r, Key::F9, plain, false, true);
        assert_eq!(r.settings.window_opacity, 100);
        press(&mut r, Key::F9, plain, true, true);
        assert_eq!(r.settings.window_opacity, 100);
        press(&mut r, Key::F9, plain, false, true);
        assert_eq!(r.settings.window_opacity, 50);
        press(&mut r, Key::ArrowDown, ctrl_shift, false, false);
        assert_eq!(r.settings.window_opacity, 50);
        let alt = egui::Modifiers {
            alt: true,
            ..ctrl_shift
        };
        press(&mut r, Key::ArrowDown, alt, false, true);
        assert_eq!(r.settings.window_opacity, 50);
        r.hwnd = None;
        press(&mut r, Key::ArrowDown, ctrl_shift, false, true);
        assert_eq!(r.settings.window_opacity, 50);
    }

    fn reader() -> (egui::Context, Reader) {
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings = Settings::default();
        let font_list = fonts::available();
        let true_bold = fonts::install(&ctx, &font_list, &settings.font, settings.weight).unwrap();
        let mut book = Book::open(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-epub3.epub"),
        )
        .unwrap();
        let paragraphs = book.chapter(1).unwrap().clone();
        let data = BookData {
            location: Location {
                chapter: 1,
                ..Default::default()
            },
        };
        let reader = Reader {
            settings,
            data_dir: None,
            book: Some(book),
            data,
            paragraphs,
            layout: Layout::default(),
            images: Images::new(),
            font_list,
            font_key: String::new(),
            true_bold,
            error: None,
            info: None,
            settings_open: false,
            toc_overlay_open: false,
            search: String::new(),
            focus_search: false,
            fullscreen: false,
            hwnd: None,
            visibility: None,
            boss_key_error: None,
            boss_shortcut_draft: BossShortcut::default(),
            boss_shortcut_error: None,
            opacity_available: false,
            applied_opacity: None,
            last_low_opacity: 80,
            opacity_mode: (false, false),
            opacity_reapply: false,
            scroll: 0.,
            scroll_request: None,
            jump: None,
            dirty: false,
            last_save: Instant::now(),
            last_save_error: None,
            first_frame: false,
            load_path: None,
            picker: None,
            body_geometry: None,
            progress_rect: None,
            #[cfg(test)]
            empty_open_rect: None,
        };
        (ctx, reader)
    }
    fn frame(ctx: &egui::Context, reader: &mut Reader, time: f64, wheel: f32, ctrl: bool) {
        let modifiers = egui::Modifiers {
            ctrl,
            ..Default::default()
        };
        let mut events = vec![egui::Event::PointerMoved(Pos2::new(450., 300.))];
        if wheel != 0. {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., wheel),
                modifiers,
            });
        }
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                time: Some(time),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                reader.keyboard(ctx);
                reader.reader(ctx);
            },
        );
    }
    // 包括底栏和侧栏：只测试正文会漏掉进度控件反向触发的跳转。
    fn full_frame(ctx: &egui::Context, r: &mut Reader, time: &mut f64, events: Vec<egui::Event>) {
        *time += 0.1;
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                time: Some(*time),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                r.keyboard(ctx);
                r.toolbar(ctx);
                r.footer(ctx);
                r.toc_panel(ctx);
                r.reader(ctx);
            },
        );
    }
    fn long_reader() -> (egui::Context, Reader, f64) {
        let (ctx, mut r) = reader();
        r.paragraphs.truncate(241);
        r.go(1, 120, 0.3);
        let mut time = 0.;
        full_frame(&ctx, &mut r, &mut time, vec![]);
        full_frame(&ctx, &mut r, &mut time, vec![]);
        (ctx, r, time)
    }
    fn percent(r: &Reader) -> f32 {
        (r.data.location.chapter as f32
            + (r.data.location.paragraph as f32 + r.data.location.offset)
                / r.paragraphs.len().max(1) as f32)
            / r.book.as_ref().unwrap().chapters.len() as f32
            * 100.
    }
    fn wheel(delta: f32) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(Pos2::new(650., 350.)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., delta),
                modifiers: Default::default(),
            },
        ]
    }
    fn key(key: Key, shift: bool) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }]
    }
    fn drag(ctx: &egui::Context, r: &mut Reader, time: &mut f64, from: Pos2, to: Pos2) {
        full_frame(ctx, r, time, vec![egui::Event::PointerMoved(from)]);
        full_frame(
            ctx,
            r,
            time,
            vec![egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        full_frame(ctx, r, time, vec![egui::Event::PointerMoved(to)]);
        full_frame(
            ctx,
            r,
            time,
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        full_frame(ctx, r, time, vec![]);
    }
    #[test]
    fn restored_middle_accepts_scroll_in_full_ui() {
        let (ctx, mut r, mut time) = long_reader();
        for _ in 0..8 {
            let before = r.scroll;
            let progress = percent(&r);
            full_frame(&ctx, &mut r, &mut time, wheel(-12.));
            assert!(
                r.scroll > before,
                "小幅滚动必须生效：{before} -> {}",
                r.scroll
            );
            for _ in 0..8 {
                let before = r.scroll;
                full_frame(&ctx, &mut r, &mut time, vec![]);
                assert!(r.scroll >= before - 0.01, "空闲帧不得拉回正文");
                assert!(r.jump.is_none() && r.scroll_request.is_none());
            }
            assert!(percent(&r) > progress, "进度必须随小幅滚动增加");
            assert!((r.scroll - before - 12.).abs() < 0.1, "完整滚动量必须保留");
        }
    }
    #[test]
    fn restore_is_consumed_once_and_idle_does_not_round_location() {
        let (ctx, mut r, mut time) = long_reader();
        let row = &r.layout.rows[120];
        assert!((r.scroll - (row.y + row.height * 0.3)).abs() < 0.05);
        assert!(r.jump.is_none() && r.scroll_request.is_none());
        let scroll = r.scroll;
        let location = r.data.location.clone();
        let galley = r.layout.rows[120].galley.clone().unwrap();
        for _ in 0..20 {
            full_frame(&ctx, &mut r, &mut time, vec![]);
            assert_eq!(r.data.location, location);
            assert_eq!(r.scroll, scroll);
            assert!(r.jump.is_none() && r.scroll_request.is_none());
            assert!(
                std::sync::Arc::ptr_eq(&galley, r.layout.rows[120].galley.as_ref().unwrap()),
                "布局不得每帧重建"
            );
        }
    }
    #[test]
    fn total_height_includes_all_rows_and_image_reflow_preserves_anchor() {
        let (ctx, mut r, mut time) = long_reader();
        r.paragraphs[119].kind = Kind::Image("test-image".into());
        r.images.insert("test-image".into(), Err("占位".into()));
        r.layout.clear();
        full_frame(&ctx, &mut r, &mut time, vec![]);
        let location = r.data.location.clone();
        let old_scroll = r.scroll;
        let galley = r.layout.rows[120].galley.clone().unwrap();
        let texture = ctx.load_texture(
            "test-image",
            egui::ColorImage::new([8, 800], Color32::WHITE),
            Default::default(),
        );
        r.images.insert("test-image".into(), Ok(texture));
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert!(std::sync::Arc::ptr_eq(
            &galley,
            r.layout.rows[120].galley.as_ref().unwrap()
        ));
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert!(r.jump.is_none() && r.scroll_request.is_none());
        assert_eq!(r.data.location.paragraph, location.paragraph);
        assert!((r.data.location.offset - location.offset).abs() < 0.001);
        assert!(
            (r.scroll - old_scroll - 740.).abs() < 0.1,
            "上方图片修正高度必须补偿偏移"
        );
        let last = r.layout.rows.last().unwrap();
        let expected = last.y + last.height + r.settings.paragraph_spacing + 24.;
        assert!((r.layout.height - expected).abs() < 0.1);
        let (_, content, viewport) = r.body_geometry.unwrap();
        assert!(
            (content.y - expected).abs() < 0.1,
            "ScrollArea 必须包含未绘制段落的高度"
        );
        assert!(content.y > viewport.height() * 50.);
        for rows in r.layout.rows[1..].windows(2) {
            assert!(
                (rows[1].y - rows[0].y - rows[0].height - r.settings.paragraph_spacing).abs()
                    < 0.01
            );
        }
    }
    #[test]
    fn scrollbar_drag_moves_to_zero_half_and_end_without_snapback() {
        let (ctx, mut r, mut time) = long_reader();
        let mut progress = Vec::new();
        for fraction in [0., 0.5, 1.] {
            let (id, content, viewport) = r.body_geometry.unwrap();
            let bar = ctx.read_response(id.with(1_usize)).unwrap().rect;
            let max = content.y - viewport.height();
            let y = |offset: f32| {
                bar.top() + (offset + viewport.height() * 0.5) / content.y * bar.height()
            };
            let from = Pos2::new(bar.center().x, y(r.scroll));
            // Endpoints drag all the way to the track edge; subpixel thumb
            // rounding must not leave a tiny offset in a very long chapter.
            let target_y = if fraction == 0. {
                bar.top()
            } else if fraction == 1. {
                bar.bottom()
            } else {
                y(max * fraction)
            };
            let to = Pos2::new(bar.center().x, target_y);
            drag(&ctx, &mut r, &mut time, from, to);
            assert!(
                (r.scroll - max * fraction).abs()
                    < if fraction == 0. || fraction == 1. {
                        1.
                    } else {
                        content.y / bar.height()
                    },
                "拖动 {fraction} 后偏移错误：{}",
                r.scroll
            );
            let end = r.scroll;
            for _ in 0..5 {
                full_frame(&ctx, &mut r, &mut time, vec![]);
            }
            assert!((r.scroll - end).abs() < 0.1, "释放拖动后不得复位");
            progress.push(percent(&r));
        }
        assert!(progress[0] < progress[1] && progress[1] < progress[2]);
        let (_, content, viewport) = r.body_geometry.unwrap();
        assert!(
            (r.scroll + viewport.height() - content.y).abs() < 1.,
            "底端必须显示章末"
        );
        full_frame(&ctx, &mut r, &mut time, wheel(-120.));
        assert_eq!(r.data.location.chapter, 1); // 当前已是最后一章。
        r.go(0, 0, 0.);
        full_frame(&ctx, &mut r, &mut time, vec![]);
        full_frame(&ctx, &mut r, &mut time, key(Key::End, false));
        full_frame(&ctx, &mut r, &mut time, wheel(-120.));
        assert_eq!(r.data.location.chapter, 1);
        assert!(r.jump.is_some());
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert!(r.jump.is_none());
    }
    #[test]
    fn keyboard_jumps_and_relayout_remain_scrollable() {
        let (ctx, mut r, mut time) = long_reader();
        for (key_code, shift, direction) in [
            (Key::PageDown, false, 1.),
            (Key::PageUp, false, -1.),
            (Key::Space, false, 1.),
            (Key::Space, true, -1.),
            (Key::ArrowDown, false, 1.),
            (Key::ArrowUp, false, -1.),
            (Key::Home, false, -1.),
            (Key::End, false, 1.),
        ] {
            let before = r.scroll;
            full_frame(&ctx, &mut r, &mut time, key(key_code, shift));
            assert!(
                (r.scroll - before) * direction > 1.,
                "{key_code:?} 必须移动正文"
            );
            let scroll = r.scroll;
            full_frame(&ctx, &mut r, &mut time, vec![]);
            assert!((r.scroll - scroll).abs() < 0.1, "键盘滚动不得在下一帧还原");
        }
        // 目录锚点、go 跳转、字体/行距/宽度重排均只消费一次请求。
        r.go_anchor(1, "p100");
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.data.location.paragraph, 100);
        r.go(1, 120, 0.3);
        full_frame(&ctx, &mut r, &mut time, vec![]);
        for setting in 0..3 {
            let anchor = r.data.location.clone();
            match setting {
                0 => r.settings.size += 3.,
                1 => r.settings.line_spacing += 0.4,
                _ => r.settings.margin += 25.,
            }
            full_frame(&ctx, &mut r, &mut time, vec![]);
            assert_eq!(r.data.location.paragraph, anchor.paragraph);
            assert!((r.data.location.offset - anchor.offset).abs() < 0.001);
            assert!(r.jump.is_none() && r.scroll_request.is_none());
            let scroll = r.scroll;
            full_frame(&ctx, &mut r, &mut time, wheel(-120.));
            assert!(r.scroll > scroll, "重排后滚轮必须立即生效");
            for _ in 0..8 {
                full_frame(&ctx, &mut r, &mut time, vec![]);
            }
            assert!(r.scroll > scroll + 50., "平滑滚动稳定后必须保留滚动量");
        }
    }
    #[test]
    fn progress_slider_drags_are_real_jumps_and_then_allow_wheel() {
        let (ctx, mut r, mut time) = long_reader();
        r.open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/scroll-regression.epub"));
        full_frame(&ctx, &mut r, &mut time, vec![]);
        full_frame(&ctx, &mut r, &mut time, vec![]);
        for target in [25., 75., 100., 0.] {
            let rect = r.progress_rect.unwrap();
            let radius = rect.height() / 2.5;
            let x = |p: f32| rect.left() + radius + p / 100. * (rect.width() - 2. * radius);
            let from = Pos2::new(x(percent(&r)), rect.center().y);
            let to = Pos2::new(x(target), rect.center().y);
            drag(&ctx, &mut r, &mut time, from, to);
            // 末屏保存的是顶部段落，故其显示进度可以略小于 100%。
            assert!(
                (percent(&r) - target).abs() < 1.,
                "进度拖动必须改变位置：{target} -> {}",
                percent(&r)
            );
            assert!(r.jump.is_none() && r.scroll_request.is_none());
        }
        let before = r.scroll;
        full_frame(&ctx, &mut r, &mut time, wheel(-120.));
        assert!(r.scroll > before + 50.);
        for _ in 0..8 {
            full_frame(&ctx, &mut r, &mut time, vec![]);
        }
        assert!(r.scroll > before + 50.);
    }
    #[test]
    fn saved_position_survives_switch_and_exit_restore_then_scrolls() {
        let (ctx, mut r, mut time) = long_reader();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "fixtures/scroll-store-test-{}/data",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        r.data_dir = Some(dir.clone());
        full_frame(&ctx, &mut r, &mut time, wheel(-120.));
        for _ in 0..8 {
            full_frame(&ctx, &mut r, &mut time, vec![]);
        }
        let saved = r.data.location.clone();
        let book3 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-epub3.epub");
        let book2 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-epub2.epub");
        r.open(&book2); // 切书先保存旧书。
        r.open(&book3);
        assert_eq!(r.jump.as_ref(), Some(&saved));
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.data.location.paragraph, saved.paragraph);
        assert!((r.data.location.offset - saved.offset).abs() < 0.001);
        eframe::App::on_exit(&mut r, None);
        let (ctx2, mut reopened) = reader();
        reopened.book = None;
        reopened.data_dir = Some(dir.clone());
        reopened.open(&book3);
        assert_eq!(reopened.jump.as_ref(), Some(&saved));
        let mut time2 = 0.;
        full_frame(&ctx2, &mut reopened, &mut time2, vec![]);
        full_frame(&ctx2, &mut reopened, &mut time2, vec![]);
        let before = reopened.scroll;
        full_frame(&ctx2, &mut reopened, &mut time2, wheel(-120.));
        for _ in 0..8 {
            full_frame(&ctx2, &mut reopened, &mut time2, vec![]);
        }
        assert!(reopened.scroll > before + 50.);
        // 只清理本测试创建的两个书籍 JSON 和 settings。
        for path in std::fs::read_dir(&dir).unwrap() {
            std::fs::remove_file(path.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
        std::fs::remove_dir(dir.parent().unwrap()).unwrap();
    }
    #[test]
    fn wheel_scrolls_long_chapter_and_idle_keeps_position() {
        let (ctx, mut r) = reader();
        frame(&ctx, &mut r, 0., 0., false);
        frame(&ctx, &mut r, 0.1, 0., false);
        let start = r.scroll;
        frame(&ctx, &mut r, 0.2, -240., false);
        for i in 3..20 {
            frame(&ctx, &mut r, i as f64 * 0.1, 0., false);
        }
        assert!(
            r.scroll > start + 100.,
            "滚轮必须移动正文：{} -> {}",
            start,
            r.scroll
        );
        let end = r.scroll;
        frame(&ctx, &mut r, 3., 0., false);
        assert!((r.scroll - end).abs() < 1., "空闲帧不得复位滚动");
        frame(&ctx, &mut r, 3.1, 240., false);
        for i in 32..50 {
            frame(&ctx, &mut r, i as f64 * 0.1, 0., false);
        }
        assert!(r.scroll < end - 100., "向上滚轮必须回退正文");
    }
    #[test]
    fn wheel_on_short_chapter_continues_to_next_chapter() {
        let (ctx, mut r) = reader();
        r.data.location.chapter = 0;
        r.paragraphs = vec![Paragraph {
            kind: Kind::Text,
            runs: vec![Run {
                text: "短章".into(),
                ..Default::default()
            }],
            anchor: String::new(),
        }];
        frame(&ctx, &mut r, 0., 0., false);
        frame(&ctx, &mut r, 0.1, 0., false);
        frame(&ctx, &mut r, 0.2, -120., false);
        assert_eq!(r.data.location.chapter, 1, "短章滚轮必须继续阅读下一章");
    }
    #[test]
    fn ctrl_wheel_changes_font_without_scrolling() {
        let (ctx, mut r) = reader();
        frame(&ctx, &mut r, 0., 0., false);
        frame(&ctx, &mut r, 0.1, 0., false);
        let old_size = r.settings.size;
        let start = r.scroll;
        frame(&ctx, &mut r, 0.2, 120., true);
        assert_eq!(r.settings.size, old_size + 1.);
        assert!((r.scroll - start).abs() < 1.);
    }
}
