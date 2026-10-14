use crate::{
    epub::Book,
    fonts,
    model::*,
    render::{Images, Layout, HIGHLIGHTS},
    settings::{toggle_opacity, Settings},
    storage,
};
use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Sense, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
    search: String,
    focus_search: bool,
    fullscreen: bool,
    hwnd: Option<isize>,
    boss_key: Option<crate::native::BossKey>,
    boss_key_error: Option<String>,
    opacity_available: bool,
    applied_opacity: Option<u8>,
    last_low_opacity: u8,
    opacity_mode: (bool, bool),
    opacity_reapply: bool,
    scroll: f32,
    scroll_request: Option<f32>,
    jump: Option<Location>,
    active: usize,
    dirty: bool,
    last_save: Instant,
    first_frame: bool,
    load_path: Option<PathBuf>,
    picker: Option<crate::picker::Picker>,
    #[cfg(test)]
    body_geometry: Option<(egui::Id, Vec2, Rect)>,
    #[cfg(test)]
    progress_rect: Option<Rect>,
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
        let (boss_key, boss_key_error) = match hwnd {
            Some(hwnd) => match crate::native::BossKey::register(hwnd, &cc.egui_ctx) {
                Ok(key) => (Some(key), None),
                Err(e) => (None, Some(format!("老板键 Ctrl+Alt+H 注册失败：{e}。正常阅读仍可使用，本实例不会隐藏窗口。"))),
            },
            None => (None, Some("老板键 Ctrl+Alt+H 注册失败：无法获取有效窗口句柄。正常阅读仍可使用，本实例不会隐藏窗口。".into())),
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
            search: String::new(),
            focus_search: false,
            fullscreen: false,
            hwnd,
            boss_key,
            boss_key_error,
            opacity_available: hwnd.is_some(),
            applied_opacity: None,
            last_low_opacity,
            opacity_mode,
            opacity_reapply: false,
            scroll: 0.,
            scroll_request: None,
            jump: None,
            active: 0,
            dirty: false,
            last_save: Instant::now(),
            first_frame: true,
            load_path,
            picker: None,
            #[cfg(test)]
            body_geometry: None,
            #[cfg(test)]
            progress_rect: None,
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
            self.error = Some("保存失败：exe 旁的 data 目录不可用，请将程序放在可写目录".into());
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
            self.error = Some(format!("保存失败：{e}"));
        } else {
            self.dirty = false;
        }
        self.last_save = Instant::now();
        saved
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
                self.active = self
                    .data
                    .location
                    .paragraph
                    .min(self.paragraphs.len().saturating_sub(1));
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
            self.active = self.data.location.paragraph;
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
    fn mark_index(&self, paragraph: usize) -> Option<usize> {
        let book = self.book.as_ref()?;
        let p = self.paragraphs.get(paragraph)?;
        self.data.marks.iter().position(|m| {
            m.chapter_id == book.chapters[self.data.location.chapter].id
                && m.paragraph == paragraph
                && m.text_hash == p.hash()
        })
    }
    fn toggle_mark(&mut self, paragraph: usize) {
        if let Some(i) = self.mark_index(paragraph) {
            self.data.marks.remove(i);
            self.dirty = true;
            return;
        }
        if let (Some(book), Some(p)) = (&self.book, self.paragraphs.get(paragraph)) {
            if p.text().trim().is_empty() {
                return;
            }
            self.data.marks.push(Mark {
                chapter_id: book.chapters[self.data.location.chapter].id.clone(),
                paragraph,
                text_hash: p.hash(),
                text: p.text(),
                color: 0,
                note: String::new(),
            });
            self.dirty = true;
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
            match (ctrl, key) {
                (true, Key::O) => {
                    self.choose_file(false, false);
                }
                (true, Key::G) | (false, Key::F2) => {
                    self.settings.toc = true;
                    self.focus_search = true;
                    self.dirty = true;
                }
                (true, Key::Comma) => {
                    self.settings_open = !self.settings_open;
                }
                (_, Key::F4) => {
                    self.settings.toc = !self.settings.toc;
                    self.dirty = true;
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
                        (true, Key::D) => self.toggle_mark(self.active),
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
                            self.scroll = self.layout.height;
                            self.scroll_request = Some(self.scroll);
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
    fn toolbar(&mut self, ctx: &egui::Context) {
        if self.settings.immersive {
            return;
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("文件", |ui| {
                    if ui.button("打开 EPUB…   Ctrl+O").clicked() {
                        ui.close_menu();
                        self.choose_file(false, false);
                    }
                    if ui.button("导出标记为 Markdown…").clicked() {
                        ui.close_menu();
                        self.export(false);
                    }
                    if ui.button("导出标记为 TXT…").clicked() {
                        ui.close_menu();
                        self.export(true);
                    }
                    if ui.button("退出").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                self.dirty |= ui.toggle_value(&mut self.settings.toc, "目录 F4").changed();
                self.dirty |= ui
                    .toggle_value(&mut self.settings.marks, "我的标记")
                    .changed();
                if ui.button("排版设置").clicked() {
                    self.settings_open = !self.settings_open;
                }
                if ui.button("沉浸 F10").clicked() {
                    self.settings.immersive = true;
                    self.dirty = true;
                }
                ui.separator();
                if let Some(book) = &self.book {
                    ui.label(RichText::new(&book.title).strong());
                }
            });
        });
    }
    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("progress").show(ctx, |ui| {
            let mut target: Option<(usize, Option<f32>)> = None;
            ui.horizontal(|ui| {
                if self.settings.immersive && ui.small_button("工具栏 F10").clicked() {
                    self.settings.immersive = false;
                    self.dirty = true;
                }
                if let Some(book) = &self.book {
                    if ui.small_button("上一章").clicked() {
                        target = Some((self.data.location.chapter.saturating_sub(1), None));
                    }
                    if ui.small_button("下一章").clicked() {
                        target = Some((self.data.location.chapter + 1, None));
                    }
                    let local = (self.data.location.paragraph as f32 + self.data.location.offset)
                        / self.paragraphs.len().max(1) as f32;
                    let mut percent = (self.data.location.chapter as f32 + local)
                        / book.chapters.len() as f32
                        * 100.;
                    let r = ui.add(
                        egui::Slider::new(&mut percent, 0.0..=100.0)
                            // Always 会在无输入时也舍入数值并报告 changed，造成每帧跳回。
                            .clamping(egui::SliderClamping::Edits)
                            .suffix("%")
                            .fixed_decimals(1)
                            .show_value(true),
                    );
                    #[cfg(test)]
                    {
                        self.progress_rect = Some(r.rect);
                    }
                    if r.changed() {
                        let x = percent / 100. * book.chapters.len() as f32;
                        let c = (x.floor() as usize).min(book.chapters.len() - 1);
                        let fraction = if percent >= 100. { 1. } else { x.fract() };
                        target = Some((c, Some(fraction)));
                    }
                    // 章节名长度变化不能移动正在拖动的进度滑轨。
                    ui.label(&book.chapters[self.data.location.chapter].title);
                } else {
                    ui.label("打开 EPUB，或把文件拖入窗口 · Ctrl+O");
                }
            });
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
    fn toc_panel(&mut self, ctx: &egui::Context) {
        if !self.settings.toc || self.settings.immersive {
            return;
        }
        let toc = self
            .book
            .as_ref()
            .map(|b| b.toc.clone())
            .unwrap_or_default();
        let mut dest = None;
        let panel = egui::SidePanel::left("toc")
            .default_width(self.settings.toc_width)
            .width_range(150.0..=600.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("章节目录");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("搜索章节 · Ctrl+G / F2"),
                );
                if self.focus_search {
                    r.request_focus();
                    self.focus_search = false;
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("toc-scroll")
                    .show(ui, |ui| {
                        self.dirty |= toc_ui(
                            ui,
                            &toc,
                            &self.search.to_lowercase(),
                            self.data.location.chapter,
                            &mut dest,
                            &mut self.settings.collapsed_toc,
                        );
                    });
            });
        let width = panel.response.rect.width();
        if (self.settings.toc_width - width).abs() > 0.1 {
            self.settings.toc_width = width;
            self.dirty = true;
        }
        if let Some((c, a)) = dest {
            self.go_anchor(c, &a);
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::NULL));
        }
    }
    fn marks_panel(&mut self, ctx: &egui::Context) {
        if !self.settings.marks || self.settings.immersive {
            return;
        }
        let mut target = None;
        let mut delete = None;
        let mut changed = false;
        let panel = egui::SidePanel::right("marks")
            .default_width(self.settings.marks_width)
            .width_range(180.0..=600.0)
            .show(ctx, |ui| {
                ui.heading("我的标记");
                ui.horizontal(|ui| {
                    if ui.small_button("导出 MD").clicked() {
                        self.export(false);
                    }
                    if ui.small_button("导出 TXT").clicked() {
                        self.export(true);
                    }
                });
                ui.separator();
                if let Some(book) = &self.book {
                    egui::ScrollArea::vertical()
                        .id_salt("marks-scroll")
                        .show(ui, |ui| {
                            for (ci, c) in book.chapters.iter().enumerate() {
                                if !self.data.marks.iter().any(|m| m.chapter_id == c.id) {
                                    continue;
                                }
                                egui::CollapsingHeader::new(&c.title)
                                    .id_salt(&c.id)
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        for (i, m) in self
                                            .data
                                            .marks
                                            .iter_mut()
                                            .enumerate()
                                            .filter(|(_, m)| m.chapter_id == c.id)
                                        {
                                            let summary: String = m.text.chars().take(65).collect();
                                            if ui.add(egui::Button::new(summary).wrap()).clicked() {
                                                target =
                                                    Some((ci, m.paragraph, m.text_hash.clone()));
                                            }
                                            changed |= ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut m.note)
                                                        .hint_text("一行备注"),
                                                )
                                                .changed();
                                            ui.horizontal(|ui| {
                                                for (color, c) in HIGHLIGHTS.iter().enumerate() {
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                if m.color == color {
                                                                    "●"
                                                                } else {
                                                                    "  "
                                                                },
                                                            )
                                                            .fill(*c),
                                                        )
                                                        .clicked()
                                                    {
                                                        m.color = color;
                                                        changed = true;
                                                    }
                                                }
                                                if ui.small_button("删除").clicked() {
                                                    delete = Some(i);
                                                }
                                            });
                                            ui.separator();
                                        }
                                    });
                            }
                            if self.data.marks.is_empty() {
                                ui.label("右键段落或 Ctrl+D 添加喜欢的段落。");
                            }
                        });
                }
            });
        let width = panel.response.rect.width();
        if (self.settings.marks_width - width).abs() > 0.1 {
            self.settings.marks_width = width;
            self.dirty = true;
        }
        if let Some(i) = delete {
            self.data.marks.remove(i);
            changed = true;
        }
        self.dirty |= changed;
        if let Some((c, p, h)) = target {
            self.go(c, p, 0.);
            if self.paragraphs.get(p).is_none_or(|p| p.hash() != h) {
                if let Some(i) = self.paragraphs.iter().position(|p| p.hash() == h) {
                    self.go(c, i, 0.);
                } else {
                    self.error = Some("标记的原文在当前章节中无法定位".into());
                }
            }
        }
    }
    fn settings_panel(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let before = self.settings.clone();
        let mut open = true;
        egui::Window::new("排版设置 · Ctrl+,")
            .open(&mut open)
            .default_width(380.)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("settings-grid")
                    .num_columns(2)
                    .spacing([15., 12.])
                    .show(ui, |ui| {
                        ui.label("字体");
                        egui::ComboBox::from_id_salt("font")
                            .selected_text(&self.settings.font)
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
                                .step_by(1.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("字重");
                        ui.horizontal(|ui| {
                            for (i, name) in ["细", "常规", "粗"].iter().enumerate() {
                                ui.selectable_value(&mut self.settings.weight, i, *name);
                            }
                        });
                        ui.end_row();
                        ui.label("行间距");
                        ui.add(
                            egui::Slider::new(&mut self.settings.line_spacing, 1.0..=3.0)
                                .step_by(0.1),
                        );
                        ui.end_row();
                        ui.label("段间距");
                        ui.add(
                            egui::Slider::new(&mut self.settings.paragraph_spacing, 0.0..=60.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("左右边距");
                        ui.add(
                            egui::Slider::new(&mut self.settings.margin, 0.0..=160.0).suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("正文最大宽度");
                        ui.add(
                            egui::Slider::new(&mut self.settings.max_width, 300.0..=1600.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("主题");
                        ui.horizontal(|ui| {
                            for (i, name) in ["白天", "护眼", "夜间"].iter().enumerate() {
                                if ui
                                    .selectable_label(self.settings.theme == i, *name)
                                    .clicked()
                                {
                                    self.settings.set_theme(i);
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("背景色");
                        ui.color_edit_button_srgb(&mut self.settings.background);
                        ui.end_row();
                        ui.label("文字色");
                        ui.color_edit_button_srgb(&mut self.settings.foreground);
                        ui.end_row();
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
                    });
                ui.separator();
                if ui.button("恢复默认排版").clicked() {
                    let d = Settings::default();
                    self.settings.font = d.font;
                    self.settings.size = d.size;
                    self.settings.weight = d.weight;
                    self.settings.line_spacing = d.line_spacing;
                    self.settings.paragraph_spacing = d.paragraph_spacing;
                    self.settings.margin = d.margin;
                    self.settings.max_width = d.max_width;
                    self.settings.set_theme(d.theme);
                }
                ui.small("Ctrl+滚轮 / Ctrl+加减号调字号 · F11 全屏 · F10 沉浸");
                ui.small("Ctrl+Shift+↑ / ↓ 调透明度 · F9 切换 · 文字也会变淡");
                ui.separator();
                ui.label("老板键：Ctrl+Alt+H（全局）");
                ui.small("按一次隐藏整个窗口，再按一次恢复；失去焦点时仍有效。");
                ui.small("隐藏后任务栏及 Alt+Tab 均不可见，请用同一快捷键恢复。");
                if let Some(error) = &self.boss_key_error {
                    ui.colored_label(Color32::from_rgb(190, 65, 65), error);
                } else if self.boss_key.as_ref().is_some_and(|key| key.available()) {
                    ui.small("状态：已注册；松开后再次按下才切换，隐藏状态不会保存。");
                }
            });
        self.settings_open = open;
        self.dirty |= before != self.settings;
    }
    fn export(&mut self, txt: bool) {
        if self.book.is_none() {
            self.info = Some("请先打开一本书".into());
            return;
        }
        self.choose_file(true, txt);
    }
    fn choose_file(&mut self, save: bool, txt: bool) {
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
        self.picker = Some(crate::picker::Picker::new(save, txt, directory));
    }
    fn export_to(&mut self, path: PathBuf, txt: bool) {
        let Some(book) = &self.book else {
            self.info = Some("请先打开一本书".into());
            return;
        };
        let mut text = if txt {
            format!("{}\n作者：{}\n\n", book.title, book.author)
        } else {
            format!("# {}\n\n作者：{}\n\n", book.title, book.author)
        };
        for c in &book.chapters {
            let marks: Vec<_> = self
                .data
                .marks
                .iter()
                .filter(|m| m.chapter_id == c.id)
                .collect();
            if marks.is_empty() {
                continue;
            }
            text.push_str(&format!("{}{}\n\n", if txt { "" } else { "## " }, c.title));
            for m in marks {
                if txt {
                    text.push_str(&m.text);
                } else {
                    text.push_str(
                        &m.text
                            .lines()
                            .map(|s| format!("> {s}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                text.push_str("\n\n");
                if !m.note.is_empty() {
                    text.push_str(&format!("备注：{}\n\n", m.note));
                }
            }
        }
        match std::fs::write(&path, text) {
            Ok(()) => self.info = Some(format!("标记已导出至 {}", path.display())),
            Err(e) => self.error = Some(format!("导出失败：{e}")),
        }
    }
    fn load_visible_images(&mut self, ctx: &egui::Context, visible: Rect) {
        let Some(book) = &mut self.book else {
            return;
        };
        let mut loaded = false;
        for (p, row) in self.paragraphs.iter().zip(&self.layout.rows) {
            if row.y > visible.max.y + 100. || row.y + row.height < visible.min.y - 100. {
                continue;
            }
            if let Kind::Image(path) = &p.kind {
                if !self.images.contains_key(path) {
                    let result = (|| {
                        let bytes = book.image(path)?;
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
                        Ok(ctx.load_texture(path, color, egui::TextureOptions::LINEAR))
                    })();
                    self.images.insert(path.clone(), result);
                    loaded = true;
                }
            }
        }
        if loaded {
            self.jump = Some(self.data.location.clone());
            self.layout.clear();
            ctx.request_repaint();
        }
    }
    fn reader(&mut self, ctx: &egui::Context) {
        let bg = Color32::from_rgb(
            self.settings.background[0],
            self.settings.background[1],
            self.settings.background[2],
        );
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg))
            .show(ctx, |ui| {
                if self.book.is_none() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.25);
                        ui.heading("便携 EPUB 阅读器");
                        ui.label("离线阅读 · 段落收藏 · 数据随程序迁移");
                        ui.add_space(20.);
                        if ui.button("打开 EPUB…").clicked() {
                            self.choose_file(false, false);
                        }
                        ui.label("也可以拖入 EPUB 文件");
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
                );
                if rebuilt && self.jump.is_none() {
                    self.jump = Some(self.data.location.clone());
                }
                if let Some(location) = self.jump.take() {
                    ctx.request_repaint();
                    if let Some(row) = self.layout.rows.get(location.paragraph) {
                        self.scroll = row.y + row.height * location.offset;
                        self.scroll_request = Some(self.scroll);
                    }
                }
                let before_scroll = self.scroll;
                let body_hovered = ui.rect_contains_pointer(ui.available_rect_before_wrap());
                let wheel = if body_hovered && self.picker.is_none() && !self.settings_open {
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
                let mut action = None;
                let mut visible = Rect::NOTHING;
                let mut area = egui::ScrollArea::vertical()
                    .id_salt("body-scroll")
                    .auto_shrink([false, false]);
                // 普通帧由 egui 保持滚轮/拖动状态，只有主动跳转才覆盖偏移。
                if let Some(offset) = self.scroll_request.take() {
                    area = area.vertical_scroll_offset(offset.max(0.));
                }
                let output = area.show_viewport(ui, |ui, viewport| {
                    visible = viewport;
                    let total_width = ui.available_width();
                    let (_, rect) = ui.allocate_space(Vec2::new(total_width, self.layout.height));
                    let left = rect.left() + (total_width - width) * 0.5;
                    let first = self.layout.paragraph_at(viewport.min.y - 60.);
                    let last = (self.layout.paragraph_at(viewport.max.y + 60.) + 1)
                        .min(self.paragraphs.len());
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
                        let mark = self.mark_index(i);
                        if let Some(m) = mark {
                            ui.painter().rect_filled(
                                r.expand(4.),
                                3.,
                                HIGHLIGHTS[self.data.marks[m].color.min(3)],
                            );
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
                        let response = ui.interact(
                            r,
                            egui::Id::new(("paragraph", self.data.location.chapter, i)),
                            Sense::click(),
                        );
                        if response.clicked() || response.secondary_clicked() {
                            self.active = i;
                        }
                        response.context_menu(|ui| {
                            if ui
                                .button(if mark.is_some() {
                                    "取消喜欢的段落"
                                } else {
                                    "标记为喜欢的段落"
                                })
                                .clicked()
                            {
                                action = Some(i);
                                ui.close_menu();
                            }
                            if let Some(m) = mark {
                                for (color, c) in HIGHLIGHTS.iter().enumerate() {
                                    if ui
                                        .add(
                                            egui::Button::new(format!("高亮颜色 {}", color + 1))
                                                .fill(*c),
                                        )
                                        .clicked()
                                    {
                                        self.data.marks[m].color = color;
                                        self.dirty = true;
                                    }
                                }
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.data.marks[m].note)
                                            .hint_text("一行备注"),
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            }
                        });
                    }
                });
                self.scroll = output.state.offset.y;
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
                        self.active = index;
                        self.dirty = true;
                        // 底栏先于正文绘制，需要紧接一帧显示本帧得到的位置。
                        ctx.request_repaint();
                    }
                }
                if let Some(i) = action {
                    self.toggle_mark(i);
                }
                self.load_visible_images(ctx, visible);
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
    }
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
    fn matches(n: &Toc, s: &str) -> bool {
        s.is_empty()
            || n.title.to_lowercase().contains(s)
            || n.children.iter().any(|n| matches(n, s))
    }
    for n in nodes {
        if !matches(n, filter) {
            continue;
        }
        let selected = n.chapter == current;
        let label = if selected {
            RichText::new(&n.title)
                .color(ui.visuals().selection.stroke.color)
                .strong()
        } else {
            RichText::new(&n.title)
        };
        if n.children.is_empty() {
            if ui.selectable_label(selected, label).clicked() {
                *dest = Some((n.chapter, n.anchor.clone()));
            }
        } else {
            let id = format!("{}/{}/{}", n.chapter, n.anchor, n.title);
            let mut open = !collapsed.contains(&id) || !filter.is_empty();
            ui.horizontal(|ui| {
                if ui.small_button(if open { "▼" } else { "▶" }).clicked() {
                    open = !open;
                    changed = true;
                    if open {
                        collapsed.retain(|s| s != &id);
                    } else {
                        collapsed.push(id.clone());
                    }
                }
                if ui.selectable_label(selected, label).clicked() {
                    *dest = Some((n.chapter, n.anchor.clone()));
                }
            });
            if open {
                ui.indent(id, |ui| {
                    changed |= toc_ui(ui, &n.children, filter, current, dest, collapsed);
                });
            }
        }
    }
    changed
}
impl eframe::App for Reader {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let restored = self.boss_key.as_ref().is_some_and(|key| key.take_restored());
        if let Some(error) = self.boss_key.as_ref().and_then(|key| key.take_error()) {
            self.error = Some(error.clone());
            self.boss_key_error = Some(error);
        }
        if self.boss_key.as_ref().is_some_and(|key| key.hidden()) {
            // 系统可能送来一次残留重绘。冻结阅读/UI 状态，不安排定时重绘。
            // 恢复完全由 WM_HOTKEY 中的 ShowWindow 唤醒，不依赖此分支运行。
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
        ctx.set_visuals(if self.settings.theme == 2 {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
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
        self.marks_panel(ctx);
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
                crate::picker::Choice::File(path, save, txt) => {
                    if save {
                        self.export_to(path, txt);
                    } else {
                        self.load_path = Some(path);
                        ctx.request_repaint();
                    }
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
            && self.hwnd.is_some_and(crate::native::window_geometry_available)
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
        if self.boss_key.as_ref().is_some_and(|key| key.hide_pending()) {
            // 先捕获完整的本帧阅读位置和有效几何，再落盘；不能等待隐藏后的帧。
            if self.save() {
                if let Some(key) = &self.boss_key {
                    key.hide_after_save();
                }
            } else {
                if let Some(key) = &self.boss_key {
                    key.cancel_hide();
                }
                if let Some(error) = &mut self.error {
                    error.push_str("\n本次未隐藏窗口，待保存成功后可再次使用老板键。");
                }
                ctx.request_repaint();
            }
            if self.boss_key.as_ref().is_some_and(|key| key.hidden()) {
                return;
            }
        } else if self.dirty && self.last_save.elapsed() > Duration::from_millis(800) {
            self.save();
        }
        if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(850));
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
        if let Some(key) = self.boss_key.take() {
            key.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            ..Default::default()
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
            search: String::new(),
            focus_search: false,
            fullscreen: false,
            hwnd: None,
            boss_key: None,
            boss_key_error: None,
            opacity_available: false,
            applied_opacity: None,
            last_low_opacity: 80,
            opacity_mode: (false, false),
            opacity_reapply: false,
            scroll: 0.,
            scroll_request: None,
            jump: None,
            active: 0,
            dirty: false,
            last_save: Instant::now(),
            first_frame: false,
            load_path: None,
            picker: None,
            body_geometry: None,
            progress_rect: None,
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
                r.marks_panel(ctx);
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
        r.paragraphs[5].kind = Kind::Image("test-image".into());
        r.images.insert("test-image".into(), Err("占位".into()));
        r.layout.clear();
        full_frame(&ctx, &mut r, &mut time, vec![]);
        let location = r.data.location.clone();
        let old_scroll = r.scroll;
        let texture = ctx.load_texture(
            "test-image",
            egui::ColorImage::new([8, 800], Color32::WHITE),
            Default::default(),
        );
        r.images.insert("test-image".into(), Ok(texture));
        r.layout.clear();
        full_frame(&ctx, &mut r, &mut time, vec![]);
        assert_eq!(r.data.location.paragraph, location.paragraph);
        assert!((r.data.location.offset - location.offset).abs() < 0.001);
        assert!(
            (r.scroll - old_scroll - 740.).abs() < 0.1,
            "上方图片修正高度必须补偿偏移"
        );
        let expected = 40.
            + r.layout
                .rows
                .iter()
                .map(|row| row.height + r.settings.paragraph_spacing)
                .sum::<f32>();
        assert!((r.layout.height - expected).abs() < 0.1);
        let (_, content, viewport) = r.body_geometry.unwrap();
        assert!(
            (content.y - expected).abs() < 0.1,
            "ScrollArea 必须包含未绘制段落的高度"
        );
        assert!(content.y > viewport.height() * 50.);
        for rows in r.layout.rows.windows(2) {
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
            let to = Pos2::new(bar.center().x, y(max * fraction));
            drag(&ctx, &mut r, &mut time, from, to);
            assert!(
                (r.scroll - max * fraction).abs() < 1.,
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
        // 目录锚点、标记使用的 go、字体/行距/宽度重排均只消费一次请求。
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
            assert!(r.scroll > scroll + 50.);
            for _ in 0..8 {
                full_frame(&ctx, &mut r, &mut time, vec![]);
            }
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
            let x = |p: f32| {
                rect.left() + radius + p / 100. * (ctx.style().spacing.slider_width - 2. * radius)
            };
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
