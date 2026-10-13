use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;
pub struct Font {
    pub name: String,
    pub files: [&'static str; 3],
}
pub fn available() -> Vec<Font> {
    [
        Font {
            name: "微软雅黑".into(),
            files: ["msyhl.ttc", "msyh.ttc", "msyhbd.ttc"],
        },
        Font {
            name: "宋体".into(),
            files: ["simsun.ttc", "simsun.ttc", "simsun.ttc"],
        },
        Font {
            name: "黑体".into(),
            files: ["simhei.ttf", "simhei.ttf", "simhei.ttf"],
        },
        Font {
            name: "楷体".into(),
            files: ["simkai.ttf", "simkai.ttf", "simkai.ttf"],
        },
        Font {
            name: "仿宋".into(),
            files: ["simfang.ttf", "simfang.ttf", "simfang.ttf"],
        },
        Font {
            name: "Segoe UI".into(),
            files: ["segoeuil.ttf", "segoeui.ttf", "segoeuib.ttf"],
        },
        Font {
            name: "Times New Roman".into(),
            files: ["times.ttf", "times.ttf", "timesbd.ttf"],
        },
    ]
    .into_iter()
    .filter(|f| path(f.files[1]).exists())
    .collect()
}
fn path(name: &str) -> std::path::PathBuf {
    std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("Fonts")
        .join(name)
}
pub fn install(
    ctx: &egui::Context,
    list: &[Font],
    selected: &str,
    weight: usize,
) -> Result<bool, String> {
    let mut defs = FontDefinitions::empty();
    let selected = list
        .iter()
        .find(|f| f.name == selected)
        .or_else(|| list.first())
        .ok_or("未找到系统字体，请检查 Windows Fonts 文件夹")?;
    let chosen = if path(selected.files[weight]).exists() {
        selected.files[weight]
    } else {
        selected.files[1]
    };
    let true_bold = chosen != selected.files[1] && weight == 2;
    let mut family = Vec::new();
    // 正文字体后放入系统中文和符号字体，所有字体仅运行时读取。
    for file in [
        chosen,
        "msyh.ttc",
        "simsun.ttc",
        "segoeui.ttf",
        "seguisym.ttf",
    ] {
        if family.iter().any(|f| f == file) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(path(file)) {
            defs.font_data
                .insert(file.into(), Arc::new(FontData::from_owned(bytes)));
            family.push(file.to_string());
        }
    }
    if family.is_empty() {
        return Err("无法读取系统字体".into());
    }
    defs.families
        .insert(FontFamily::Proportional, family.clone());
    defs.families.insert(FontFamily::Monospace, family.clone());
    // 加粗段落优先使用真实字重文件。
    let mut bold_family = Vec::new();
    let selected_bold = if selected.files[2] != selected.files[1] {
        selected.files[2]
    } else {
        "msyhbd.ttc"
    };
    for file in [selected_bold, "msyhbd.ttc", "segoeuib.ttf"] {
        if !defs.font_data.contains_key(file) {
            if let Ok(bytes) = std::fs::read(path(file)) {
                defs.font_data
                    .insert(file.into(), Arc::new(FontData::from_owned(bytes)));
            }
        }
        if defs.font_data.contains_key(file) && !bold_family.iter().any(|f| f == file) {
            bold_family.push(file.to_string());
        }
    }
    bold_family.extend(family);
    defs.families
        .insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(defs);
    Ok(true_bold)
}
