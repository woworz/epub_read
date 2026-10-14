//! 全局老板键由 winit 所在的阻塞式 Win32 消息循环接收，与 egui 焦点/绘制无关。
use std::{cell::{Cell, RefCell}, ffi::c_void, marker::PhantomData, rc::Rc};
use eframe::egui::{Context, ViewportCommand};

type Hwnd = *mut c_void;
type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterHotKey(hwnd: Hwnd, id: i32, modifiers: u32, key: u32) -> i32;
    fn UnregisterHotKey(hwnd: Hwnd, id: i32) -> i32;
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
    fn SetForegroundWindow(hwnd: Hwnd) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" { fn GetCurrentThreadId() -> u32; }
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize) -> i32;
    fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
}
const HOTKEY_ID: i32 = 0x4550;
const SUBCLASS_ID: usize = 0x45505542;
const WM_HOTKEY: u32 = 0x0312;
const WM_NCDESTROY: u32 = 0x0082;
const MODIFIERS: u32 = 0x0001 | 0x0002 | 0x4000; // ALT | CONTROL | NOREPEAT
const KEY: u32 = b'H' as u32;

#[derive(Clone, Copy, PartialEq)]
enum State { Visible, HidePending, Hidden, Disabled, Destroyed }
struct Hook {
    ctx: Context,
    state: Cell<State>,
    restored: Cell<bool>,
    error: RefCell<Option<String>>,
}
impl Hook {
    fn restore(&self, hwnd: Hwnd) {
        if unsafe { IsWindow(hwnd) } == 0 {
            self.state.set(State::Destroyed);
            *self.error.borrow_mut() = Some("窗口句柄已失效，老板键不可用".into());
            return;
        }
        self.state.set(State::Visible);
        // ShowWindow 会同步发送窗口消息，先标记恢复，避免重入帧保存过渡几何。
        self.restored.set(true);
        // SW_SHOW 保留正常/最大化/全屏位置，不使用 SW_SHOWNORMAL。
        // 必须先原生显示：隐藏窗口可能没有任何 egui 帧，不能只排队 viewport 命令。
        unsafe { ShowWindow(hwnd, if IsIconic(hwnd) != 0 { 9 } else { 5 }); }
        if self.state.get() == State::Destroyed { return; }
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            // 保留注册，允许用户再次尝试恢复，不能禁用仍隐藏窗口的唯一恢复入口。
            self.state.set(State::Hidden);
            *self.error.borrow_mut() = Some("无法显示窗口，请再次按 Ctrl+Alt+H 恢复".into());
            return;
        }
        self.ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        self.restored.set(true);
        self.ctx.request_repaint();
        // Windows 可能拒绝切换前台；不绕过系统限制。
        unsafe { SetForegroundWindow(hwnd); }
    }
    fn fail(&self, hwnd: Hwnd, error: String) {
        self.restore(hwnd);
        if self.state.get() == State::Visible {
            self.state.set(State::Disabled);
            unsafe { UnregisterHotKey(hwnd, HOTKEY_ID); }
        }
        *self.error.borrow_mut() = Some(error);
    }
}
unsafe extern "system" fn window_proc(
    hwnd: Hwnd, message: u32, wparam: usize, lparam: isize, id: usize, data: usize,
) -> isize {
    // 安装回调及本次调用各持有一个引用；同步窗口消息导致重入/销毁时也有效。
    // 所有引用计数操作均在 HWND 所属线程。
    let pointer = data as *const Hook;
    unsafe { Rc::increment_strong_count(pointer); }
    let hook = unsafe { Rc::from_raw(pointer) };
    if message == WM_NCDESTROY {
        hook.state.set(State::Destroyed);
        unsafe {
            UnregisterHotKey(hwnd, HOTKEY_ID);
            RemoveWindowSubclass(hwnd, window_proc, id);
        }
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        // 窗口销毁后不再有回调，释放安装回调时保留的引用。
        unsafe { drop(Rc::from_raw(data as *const Hook)); }
        return result;
    } else if message == WM_HOTKEY && wparam == HOTKEY_ID as usize {
        match hook.state.get() {
            State::Visible => {
                // 下一帧先保存进度再真正隐藏。只唤醒一次，不轮询。
                hook.state.set(State::HidePending);
                hook.ctx.request_repaint();
            }
            State::HidePending | State::Hidden => hook.restore(hwnd),
            State::Disabled | State::Destroyed => {}
        }
        return 0;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// 只能在窗口所属线程创建/释放；Rc 也使其无法 Send/Sync。
pub struct BossKey {
    hwnd: Hwnd,
    hook: Rc<Hook>,
    restore_on_drop: bool,
    _thread_affinity: PhantomData<Rc<()>>,
}
impl BossKey {
    pub fn register(hwnd: isize, ctx: &Context) -> Result<Self, String> {
        let hwnd = hwnd as Hwnd;
        if unsafe { IsWindow(hwnd) } == 0 {
            return Err("无效窗口句柄".into());
        }
        if unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) != GetCurrentThreadId() } {
            return Err("必须在窗口所属线程注册老板键".into());
        }
        // 多实例/其他软件占用会失败，不覆盖其他实例的注册。
        if unsafe { RegisterHotKey(hwnd, HOTKEY_ID, MODIFIERS, KEY) } == 0 {
            return Err(format!("快捷键可能被其他程序或另一阅读器实例占用：{}", std::io::Error::last_os_error()));
        }
        let hook = Rc::new(Hook {
            ctx: ctx.clone(), state: Cell::new(State::Visible),
            restored: Cell::new(false), error: RefCell::new(None),
        });
        let callback_ref = Rc::into_raw(hook.clone());
        if unsafe { SetWindowSubclass(hwnd, window_proc, SUBCLASS_ID, callback_ref as usize) } == 0 {
            unsafe {
                UnregisterHotKey(hwnd, HOTKEY_ID);
                drop(Rc::from_raw(callback_ref));
            }
            return Err("无法安装老板键窗口消息回调".into());
        }
        Ok(Self { hwnd, hook, restore_on_drop: true, _thread_affinity: PhantomData })
    }
    pub fn hide_pending(&self) -> bool { self.hook.state.get() == State::HidePending }
    pub fn hidden(&self) -> bool { self.hook.state.get() == State::Hidden }
    pub fn available(&self) -> bool {
        matches!(self.hook.state.get(), State::Visible | State::HidePending | State::Hidden)
    }
    pub fn take_restored(&self) -> bool { self.hook.restored.replace(false) }
    pub fn take_error(&self) -> Option<String> { self.hook.error.borrow_mut().take() }
    pub fn cancel_hide(&self) {
        if self.hide_pending() { self.hook.state.set(State::Visible); }
    }
    /// Reader 只在保存成功后调用；不修改书籍/排版/全屏状态。
    pub fn hide_after_save(&self) {
        if !self.hide_pending() { return; }
        if unsafe { IsWindow(self.hwnd) } == 0 {
            self.hook.state.set(State::Disabled);
            *self.hook.error.borrow_mut() = Some("窗口句柄已失效，老板键已禁用".into());
            return;
        }
        self.hook.state.set(State::Hidden);
        unsafe { ShowWindow(self.hwnd, 0); }
        if self.hook.state.get() == State::Destroyed { return; }
        // ShowWindow 返回的是先前可见性，不是成功标志。
        if unsafe { IsWindowVisible(self.hwnd) } != 0 {
            self.hook.fail(self.hwnd, "无法隐藏窗口，老板键已禁用".into());
        } else {
            // 同步 winit 的 VISIBLE 标志，避免全屏/样式刷新把窗口重新显示。
            self.hook.ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }
    /// 正常退出时不短暂显示窗口；随 self 的 Drop 注销并移除回调。
    pub fn shutdown(mut self) { self.restore_on_drop = false; }
}
impl Drop for BossKey {
    fn drop(&mut self) {
        if self.hook.state.get() != State::Destroyed {
            // 显式释放而窗口仍存活时，不能留下无法恢复的隐藏窗口。
            if self.restore_on_drop && self.hidden() { self.hook.restore(self.hwnd); }
            if self.hook.state.get() == State::Destroyed { return; }
            // 即便移除失败，残留的已排队 WM_HOTKEY 也不能再隐藏窗口。
            self.hook.state.set(State::Disabled);
            unsafe {
                UnregisterHotKey(self.hwnd, HOTKEY_ID);
                if RemoveWindowSubclass(self.hwnd, window_proc, SUBCLASS_ID) != 0 {
                    drop(Rc::from_raw(Rc::as_ptr(&self.hook)));
                }
                // 移除失败时由 WM_NCDESTROY 释放回调的引用，避免悬空指针。
            }
        }
    }
}

pub fn window_geometry_available(hwnd: isize) -> bool {
    let hwnd = hwnd as Hwnd;
    unsafe { IsWindow(hwnd) != 0 && IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
}
