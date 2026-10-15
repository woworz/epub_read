#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod diagnostics;
mod epub;
mod fonts;
mod hotkey;
mod model;
mod native;
mod picker;
mod render;
mod settings;
mod storage;
mod ui;

// Exercise the exact vendored scheduling helper through the normal offline test suite.
#[cfg(test)]
#[path = "../vendor/eframe/src/native/repaint_schedule.rs"]
mod repaint_schedule_tests;

fn main() {
    diagnostics::init();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|s| s == "--selftest" || s == "--selftest-store")
    {
        let result = if args[0] == "--selftest-store" {
            storage::data_dir()
                .and_then(|d| storage::selftest(&d))
                .map(|_| "存储自检通过：写入阅读位置 → 重新读取 → 校验 → 清理\n".to_string())
        } else {
            args.get(1)
                .ok_or_else(|| "用法：epub-reader.exe --selftest <epub路径>".into())
                .and_then(|p| epub::selftest(std::path::Path::new(p)))
        };
        match result {
            Ok(s) => {
                native::console(&s);
            }
            Err(e) => {
                native::console(&format!("自检失败：{e}\n"));
                std::process::exit(1);
            }
        }
        return;
    }
    let data_dir = storage::data_dir();
    let mut settings = data_dir
        .as_ref()
        .ok()
        .and_then(|d| storage::read::<settings::Settings>(&d.join("settings.json")).ok())
        .unwrap_or_default();
    settings.sanitize();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("便携 EPUB 阅读器")
        .with_inner_size(settings.window_size)
        .with_min_inner_size([500., 350.]);
    if let Some(p) = settings.window_pos {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let path = args.first().map(std::path::PathBuf::from);
    if let Err(e) = eframe::run_native(
        "便携 EPUB 阅读器",
        options,
        Box::new(move |cc| Ok(Box::new(ui::Reader::new(cc, settings, data_dir, path)))),
    ) {
        native::console(&format!("窗口启动失败：{e}\n"));
        // 启动期错误也写在 exe 旁，便于无控制台的双击场景诊断。
        if let Ok(d) = storage::data_dir() {
            let _ = std::fs::write(d.join("startup-error.txt"), e.to_string());
        }
    }
}
