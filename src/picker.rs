use eframe::egui;
use std::path::PathBuf;

// 不调用 Shell/COM 文件对话框，避免系统 MRU / 最近文件记录写到注册表。
pub struct Picker {
    pub save: bool,
    pub txt: bool,
    directory: PathBuf,
    path_input: String,
    name: String,
    entries: Vec<(String, bool)>,
    error: String,
    overwrite: Option<PathBuf>,
    drives: Vec<char>,
}
pub enum Choice {
    Cancel,
    File(PathBuf, bool, bool),
}
impl Picker {
    pub fn new(save: bool, txt: bool, directory: PathBuf) -> Self {
        let mut p = Self {
            save,
            txt,
            path_input: directory.to_string_lossy().into(),
            directory,
            name: if save {
                if txt {
                    "我的标记.txt"
                } else {
                    "我的标记.md"
                }
                .into()
            } else {
                String::new()
            },
            entries: Vec::new(),
            error: String::new(),
            overwrite: None,
            drives: crate::native::local_drives(),
        };
        p.refresh();
        p
    }
    fn refresh(&mut self) {
        self.path_input = self.directory.to_string_lossy().into();
        self.entries.clear();
        self.error.clear();
        match std::fs::read_dir(&self.directory) {
            Ok(entries) => {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if dir || self.save || name.to_lowercase().ends_with(".epub") {
                        self.entries.push((name, dir));
                    }
                }
                self.entries.sort_by(|a, b| {
                    b.1.cmp(&a.1)
                        .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                });
            }
            Err(e) => self.error = format!("无法读取目录：{e}"),
        }
    }
    fn choose(&mut self, path: PathBuf) -> Option<Choice> {
        if path.is_dir() {
            self.directory = path;
            self.refresh();
            return None;
        }
        if self.save {
            let path = if path.extension().is_none() {
                path.with_extension(if self.txt { "txt" } else { "md" })
            } else {
                path
            };
            if !path.parent().is_some_and(|p| p.is_dir()) {
                self.error = "目标文件夹不存在".into();
                return None;
            }
            if path.exists() {
                self.overwrite = Some(path);
                None
            } else {
                Some(Choice::File(path, true, self.txt))
            }
        } else if path.is_file() {
            Some(Choice::File(path, false, false))
        } else {
            self.error = "找不到该文件，请检查路径".into();
            None
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Choice> {
        let mut open = true;
        let mut result = None;
        let mut navigate = None;
        let mut selected = None;
        egui::Window::new(if self.save {
            "导出标记"
        } else {
            "打开 EPUB"
        })
        .id(egui::Id::new("file-picker"))
        .open(&mut open)
        .default_size([620., 440.])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("上级").clicked() {
                    navigate = self.directory.parent().map(PathBuf::from);
                }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .desired_width(ui.available_width() - 55.)
                        .hint_text("文件夹或完整文件路径"),
                );
                if ui.button("转到").clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    let path = PathBuf::from(self.path_input.trim().trim_matches('"'));
                    if path.is_dir() {
                        navigate = Some(path);
                    } else {
                        selected = Some(path);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("磁盘：");
                for c in &self.drives {
                    let drive = PathBuf::from(format!("{c}:\\"));
                    if ui.small_button(format!("{c}:\\")).clicked() {
                        navigate = Some(drive);
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("file-list")
                .max_height(290.)
                .show(ui, |ui| {
                    for (name, dir) in &self.entries {
                        let label = if *dir {
                            format!("▸ {name}\\")
                        } else {
                            name.clone()
                        };
                        let r = ui.selectable_label(!dir && self.name == *name, label);
                        if r.clicked() {
                            if *dir {
                                navigate = Some(self.directory.join(name));
                            } else {
                                self.name = name.clone();
                            }
                        }
                        if r.double_clicked() && !dir {
                            selected = Some(self.directory.join(name));
                        }
                    }
                    if self.entries.is_empty() {
                        ui.label("此目录没有可选择的文件");
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("文件名：");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .desired_width(ui.available_width() - 100.),
                );
                if ui.button(if self.save { "导出" } else { "打开" }).clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    if !self.name.trim().is_empty() {
                        selected = Some(self.directory.join(self.name.trim().trim_matches('"')));
                    }
                }
                if ui.button("取消").clicked() {
                    result = Some(Choice::Cancel);
                }
            });
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(190, 65, 65), &self.error);
            }
            if let Some(path) = self.overwrite.clone() {
                ui.separator();
                ui.label(format!("文件已存在：{}", path.display()));
                ui.horizontal(|ui| {
                    if ui.button("确认覆盖").clicked() {
                        result = Some(Choice::File(path, true, self.txt));
                    }
                    if ui.button("返回").clicked() {
                        self.overwrite = None;
                    }
                });
            }
        });
        if let Some(path) = navigate {
            self.directory = path;
            self.refresh();
        }
        if let Some(path) = selected {
            result = self.choose(path);
        }
        if !open {
            result = Some(Choice::Cancel);
        }
        result
    }
}
