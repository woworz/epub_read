//! Opt-in scheduler diagnostics; normal launches install no logger and write no files.
use std::{fs::File, io::Write, sync::Mutex, time::Instant};

struct SchedulerLog {
    file: Mutex<File>,
    start: Instant,
}
impl log::Log for SchedulerLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "eframe::native::run"
            || metadata.target() == "epub_reader::native::window_visibility"
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(mut file) = self.file.lock() {
                let _ = writeln!(
                    file,
                    "{:.6} {} {}",
                    self.start.elapsed().as_secs_f64(),
                    record.target(),
                    record.args()
                );
            }
        }
    }
    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

pub fn init() {
    let Some(path) = std::env::var_os("EPUB_READER_SCHEDULER_TRACE") else {
        return;
    };
    if let Ok(file) = File::create(path) {
        if log::set_boxed_logger(Box::new(SchedulerLog {
            file: Mutex::new(file),
            start: Instant::now(),
        }))
        .is_ok()
        {
            log::set_max_level(log::LevelFilter::Trace);
        }
    }
}
