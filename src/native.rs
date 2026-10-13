use std::{ffi::c_void, path::Path};
#[link(name = "kernel32")]
unsafe extern "system" {
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
