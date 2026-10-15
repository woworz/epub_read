mod visibility_state;
mod window_visibility;
use std::{ffi::c_void, path::Path};
pub use window_visibility::{window_geometry_available, WindowVisibility};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetLastError(error: u32);
    fn GetLastError() -> u32;
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    fn AttachConsole(pid: u32) -> i32;
    fn GetStdHandle(which: u32) -> *mut c_void;
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
    fn WriteConsoleW(
        handle: *mut c_void,
        data: *const u16,
        len: u32,
        written: *mut u32,
        reserved: *mut c_void,
    ) -> i32;
    fn WriteFile(
        handle: *mut c_void,
        data: *const u8,
        len: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, value: isize) -> isize;
    fn SetLayeredWindowAttributes(hwnd: *mut c_void, key: u32, alpha: u8, flags: u32) -> i32;
}
const GWL_EXSTYLE: i32 = -20;
const WS_EX_LAYERED: isize = 0x80000;
const LWA_ALPHA: u32 = 2;

pub fn opacity_alpha(percent: u8) -> u8 {
    // 整数四舍五入，先扩大类型避免乘法溢出。
    ((u16::from(percent.clamp(20, 100)) * 255 + 50) / 100) as u8
}

fn set_extended_style(hwnd: *mut c_void, style: isize) -> Result<(), String> {
    // 返回 0 也可能成功，必须先清除线程错误码再判断。
    unsafe { SetLastError(0) };
    if unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style) } == 0 && unsafe { GetLastError() } != 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

pub fn set_window_opacity(hwnd: isize, percent: u8) -> Result<(), String> {
    if hwnd == 0 {
        return Err("无效窗口句柄".into());
    }
    let hwnd = hwnd as *mut c_void;
    unsafe { SetLastError(0) };
    let old_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    if old_style == 0 && unsafe { GetLastError() } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let percent = percent.clamp(20, 100);
    let style = if percent == 100 {
        old_style & !WS_EX_LAYERED
    } else {
        old_style | WS_EX_LAYERED
    };
    if style != old_style {
        set_extended_style(hwnd, style)?;
    }
    if percent < 100
        && unsafe { SetLayeredWindowAttributes(hwnd, 0, opacity_alpha(percent), LWA_ALPHA) } == 0
    {
        let error = std::io::Error::last_os_error().to_string();
        // 首次设置 alpha 失败时撤回新增样式，避免留下不可见窗口。
        if style != old_style {
            let _ = set_extended_style(hwnd, old_style);
        }
        return Err(error);
    }
    Ok(())
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn local_drives() -> Vec<char> {
    let mask = unsafe { GetLogicalDrives() };
    ('A'..='Z')
        .filter(|c| {
            let bit = *c as u32 - 'A' as u32;
            if mask & (1 << bit) == 0 {
                return false;
            }
            let path = wide(&format!("{c}:\\"));
            matches!(unsafe { GetDriveTypeW(path.as_ptr()) }, 2 | 3)
        })
        .collect()
}
pub fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    let from = wide(&from.to_string_lossy());
    let to = wide(&to.to_string_lossy());
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
pub fn console(text: &str) {
    // GUI 子系统在自检时连接父控制台，也支持 PowerShell 的重定向句柄。
    let mut handle = unsafe { GetStdHandle((-11i32) as u32) };
    if handle.is_null() || handle as isize == -1 {
        unsafe {
            AttachConsole(u32::MAX);
        }
        handle = unsafe { GetStdHandle((-11i32) as u32) };
    }
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } != 0 {
        let wide: Vec<_> = text.encode_utf16().collect();
        unsafe {
            WriteConsoleW(
                handle,
                wide.as_ptr(),
                wide.len() as u32,
                &mut count,
                std::ptr::null_mut(),
            );
        }
    } else {
        unsafe {
            WriteFile(
                handle,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut count,
                std::ptr::null_mut(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> *mut c_void;
        fn DestroyWindow(hwnd: *mut c_void) -> i32;
        fn GetLayeredWindowAttributes(
            hwnd: *mut c_void,
            key: *mut u32,
            alpha: *mut u8,
            flags: *mut u32,
        ) -> i32;
    }

    #[test]
    fn layered_window_preserves_styles_and_restores_opaque() {
        // 系统 STATIC 类的隐藏窗口，只验证 FFI；不代表 glow 驱动兼容性。
        let class = wide("STATIC");
        let title = wide("opacity test");
        let null = std::ptr::null_mut();
        let hwnd = unsafe {
            CreateWindowExW(
                0x80,
                class.as_ptr(),
                title.as_ptr(),
                0,
                0,
                0,
                1,
                1,
                null,
                null,
                null,
                null,
            )
        };
        assert!(!hwnd.is_null(), "{}", std::io::Error::last_os_error());
        struct Window(*mut c_void);
        impl Drop for Window {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let window = Window(hwnd);
        let original = unsafe { GetWindowLongPtrW(window.0, GWL_EXSTYLE) };
        for percent in [20, 55, 95] {
            set_window_opacity(window.0 as isize, percent).unwrap();
            assert_eq!(
                unsafe { GetWindowLongPtrW(window.0, GWL_EXSTYLE) },
                original | WS_EX_LAYERED
            );
            let (mut key, mut alpha, mut flags) = (0, 0, 0);
            assert_ne!(
                unsafe { GetLayeredWindowAttributes(window.0, &mut key, &mut alpha, &mut flags) },
                0
            );
            assert_eq!(alpha, opacity_alpha(percent));
            assert_eq!(flags, LWA_ALPHA);
        }
        set_window_opacity(window.0 as isize, 100).unwrap();
        assert_eq!(
            unsafe { GetWindowLongPtrW(window.0, GWL_EXSTYLE) },
            original
        );
        set_window_opacity(window.0 as isize, 100).unwrap();
        assert!(set_window_opacity(0, 50).is_err());
        assert!(set_window_opacity(-1, 50).is_err());
    }

    #[test]
    fn percent_to_alpha_rounds_and_clamps() {
        for (percent, alpha) in [
            (0, 51),
            (20, 51),
            (25, 64),
            (50, 128),
            (95, 242),
            (100, 255),
            (150, 255),
        ] {
            assert_eq!(opacity_alpha(percent), alpha);
        }
    }
}
