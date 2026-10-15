//! 老板键和鼠标自动隐藏均由 Win32 消息循环驱动，隐藏后无需渲染帧。
use super::visibility_state::{Reason, State};
use crate::hotkey::{replace_registration, BossShortcut};
use eframe::egui::{Context, ViewportCommand};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
    rc::Rc,
};

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
    fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn SetForegroundWindow(hwnd: Hwnd) -> i32;
    fn SetTimer(hwnd: Hwnd, id: usize, interval: u32, callback: usize) -> usize;
    fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut ScreenRect) -> i32;
    fn GetCapture() -> Hwnd;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
}
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize) -> i32;
    fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
}
const SUBCLASS_ID: usize = 0x45505542;
const WM_HOTKEY: u32 = 0x0312;
const WM_NCDESTROY: u32 = 0x0082;

const TIMER_ID: usize = 0x4550554D;
const WM_TIMER: u32 = 0x0113;
const WM_ENTERSIZEMOVE: u32 = 0x0231;
const WM_EXITSIZEMOVE: u32 = 0x0232;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ScreenRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl ScreenRect {
    fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }
}
struct Hook {
    ctx: Context,
    state: Cell<State>,
    hotkey_registered: Cell<bool>,
    hotkey_id: Cell<i32>,
    shortcut: Cell<BossShortcut>,
    timer: Cell<usize>,
    auto_hide_enabled: Cell<bool>,
    moving: Cell<bool>,
    mouse_area: Cell<Option<ScreenRect>>,
    winit_hidden: Cell<bool>,
    restored: Cell<bool>,
    error: RefCell<Option<String>>,
}
impl Hook {
    fn restore(&self, hwnd: Hwnd, activate: bool) {
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
        unsafe {
            ShowWindow(
                hwnd,
                if activate {
                    if IsIconic(hwnd) != 0 {
                        9
                    } else {
                        5
                    }
                } else {
                    8
                },
            );
        }
        if self.state.get() == State::Destroyed {
            return;
        }
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            // 保留注册，允许用户再次尝试恢复，不能禁用仍隐藏窗口的唯一恢复入口。
            self.state.set(State::Hidden(if activate {
                Reason::BossKey
            } else {
                Reason::Mouse
            }));
            *self.error.borrow_mut() =
                Some("无法显示窗口，请将鼠标移回原窗口区域或再次按当前老板键恢复".into());
            return;
        }
        if self.winit_hidden.replace(false) {
            self.ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        }
        if !activate {
            // 显示到其他普通窗口上方，同时保留原前台窗口的键盘焦点。
            unsafe {
                SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, 0x0013);
            }
        }
        self.restored.set(true);
        if let Err(error) = self.sync_timer(hwnd) {
            *self.error.borrow_mut() = Some(error);
        }
        self.ctx.request_repaint();
        // Windows 可能拒绝切换前台；不绕过系统限制。
        if activate {
            unsafe {
                SetForegroundWindow(hwnd);
            }
        }
    }
    fn stop_timer(&self, hwnd: Hwnd) {
        let timer = self.timer.replace(0);
        if timer != 0 {
            log::trace!("mouse timer stopped; state={:?}", self.state.get());
            unsafe {
                KillTimer(hwnd, timer);
            }
        }
    }
    fn sync_timer(&self, hwnd: Hwnd) -> Result<(), String> {
        if !self
            .state
            .get()
            .needs_mouse_timer(self.auto_hide_enabled.get())
        {
            self.stop_timer(hwnd);
        } else if self.timer.get() == 0 {
            let timer = unsafe { SetTimer(hwnd, TIMER_ID, 120, 0) };
            if timer == 0 {
                return Err(format!(
                    "无法启动鼠标检测：{}",
                    std::io::Error::last_os_error()
                ));
            }
            self.timer.set(timer);
            log::trace!("mouse timer started; state={:?}", self.state.get());
        }
        Ok(())
    }
    fn mouse_tick(&self, hwnd: Hwnd) {
        if self.timer.get() == 0
            || self.moving.get()
            || unsafe { IsIconic(hwnd) != 0 || !GetCapture().is_null() }
        {
            return;
        }
        let state = self.state.get();
        if !matches!(
            state,
            State::Visible | State::HidePending(Reason::Mouse) | State::Hidden(Reason::Mouse)
        ) {
            return;
        }
        let mut point = Point::default();
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return;
        }
        let area = if matches!(state, State::Hidden(_)) {
            self.mouse_area.get()
        } else {
            if unsafe { IsWindowVisible(hwnd) } == 0 {
                return;
            }
            let mut rect = ScreenRect::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) } == 0
                || rect.right <= rect.left
                || rect.bottom <= rect.top
            {
                return;
            }
            self.mouse_area.set(Some(rect));
            Some(rect)
        };
        let Some(area) = area else {
            return;
        };
        // 系统 DPI/样式通知可能重新显示原生窗口；鼠标仍在外面时维持隐藏。
        if state == State::Hidden(Reason::Mouse)
            && !area.contains(point)
            && unsafe { IsWindowVisible(hwnd) } != 0
        {
            unsafe {
                ShowWindow(hwnd, 0);
            }
        }
        let next = state.mouse(area.contains(point));
        if next == State::Visible && next != state {
            if matches!(state, State::Hidden(_)) {
                self.restore(hwnd, false);
            } else {
                self.state.set(next);
            }
        } else if next != state {
            self.state.set(next);
            self.ctx.request_repaint();
        }
    }
    fn fail(&self, hwnd: Hwnd, error: String) {
        self.restore(hwnd, false);
        if self.state.get() == State::Visible {
            self.state.set(State::Disabled);
            self.stop_timer(hwnd);
            unsafe {
                UnregisterHotKey(hwnd, self.hotkey_id.get());
            }
            self.hotkey_registered.set(false);
        }
        *self.error.borrow_mut() = Some(error);
    }
}
unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    id: usize,
    data: usize,
) -> isize {
    // 安装回调及本次调用各持有一个引用；同步窗口消息导致重入/销毁时也有效。
    // 所有引用计数操作均在 HWND 所属线程。
    let pointer = data as *const Hook;
    unsafe {
        Rc::increment_strong_count(pointer);
    }
    let hook = unsafe { Rc::from_raw(pointer) };
    if message == WM_NCDESTROY {
        hook.state.set(State::Destroyed);
        hook.stop_timer(hwnd);
        unsafe {
            UnregisterHotKey(hwnd, hook.hotkey_id.get());
            RemoveWindowSubclass(hwnd, window_proc, id);
        }
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        // 窗口销毁后不再有回调，释放安装回调时保留的引用。
        unsafe {
            drop(Rc::from_raw(data as *const Hook));
        }
        return result;
    } else if message == WM_TIMER && wparam == hook.timer.get() && wparam != 0 {
        hook.mouse_tick(hwnd);
        return 0;
    } else if message == WM_ENTERSIZEMOVE {
        hook.moving.set(true);
    } else if message == WM_EXITSIZEMOVE {
        hook.moving.set(false);
    } else if message == WM_HOTKEY
        && wparam == hook.hotkey_id.get() as usize
        && hook.hotkey_registered.get()
        && hook.shortcut.get().matches_message(lparam)
    {
        let next = hook.state.get().boss_toggle();
        if next == State::Visible {
            hook.restore(hwnd, true);
        } else {
            hook.state.set(next);
            if let Err(error) = hook.sync_timer(hwnd) {
                *hook.error.borrow_mut() = Some(error);
            }
            if next == State::Hidden(Reason::BossKey) {
                hook.winit_hidden.set(true);
                hook.ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            }
            hook.ctx.request_repaint();
        }
        return 0;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// 只能在窗口所属线程创建/释放；Rc 也使其无法 Send/Sync。
pub struct WindowVisibility {
    hwnd: Hwnd,
    hook: Rc<Hook>,
    restore_on_drop: bool,
    hotkey_error: Option<String>,
    _thread_affinity: PhantomData<Rc<()>>,
}
impl WindowVisibility {
    pub fn register(hwnd: isize, ctx: &Context, shortcut: BossShortcut) -> Result<Self, String> {
        let hwnd = hwnd as Hwnd;
        if unsafe { IsWindow(hwnd) } == 0 {
            return Err("无效窗口句柄".into());
        }
        if unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) != GetCurrentThreadId() } {
            return Err("必须在窗口所属线程注册老板键".into());
        }
        let hook = Rc::new(Hook {
            ctx: ctx.clone(),
            state: Cell::new(State::Visible),
            hotkey_registered: Cell::new(false),
            hotkey_id: Cell::new(0),
            shortcut: Cell::new(shortcut),
            timer: Cell::new(0),
            auto_hide_enabled: Cell::new(false),
            moving: Cell::new(false),
            mouse_area: Cell::new(None),
            winit_hidden: Cell::new(false),
            restored: Cell::new(false),
            error: RefCell::new(None),
        });
        let callback_ref = Rc::into_raw(hook.clone());
        if unsafe { SetWindowSubclass(hwnd, window_proc, SUBCLASS_ID, callback_ref as usize) } == 0
        {
            unsafe {
                drop(Rc::from_raw(callback_ref));
            }
            return Err("无法安装窗口显隐消息回调".into());
        }
        // 老板键占用只禁用热键，鼠标显隐仍可独立使用。
        let mut visibility = Self {
            hwnd,
            hotkey_error: None,
            hook,
            restore_on_drop: true,
            _thread_affinity: PhantomData,
        };
        visibility.hotkey_error = visibility.set_shortcut(shortcut).err();
        Ok(visibility)
    }
    pub fn set_shortcut(&mut self, shortcut: BossShortcut) -> Result<(), String> {
        shortcut.validate()?;
        if !self.auto_hide_available() || unsafe { IsWindow(self.hwnd) } == 0 {
            return Err("窗口句柄不可用，无法注册老板键".into());
        }
        if shortcut == self.hook.shortcut.get() && self.available() {
            return Ok(());
        }
        let active = self
            .hook
            .hotkey_registered
            .get()
            .then(|| self.hook.hotkey_id.get());
        let next = replace_registration(
            active,
            |id| {
                if unsafe { RegisterHotKey(self.hwnd, id, shortcut.modifiers(), shortcut.key) } == 0
                {
                    Err(format!(
                        "{} 注册失败，可能被其他程序占用或由系统保留：{}",
                        shortcut.label(),
                        std::io::Error::last_os_error()
                    ))
                } else {
                    Ok(())
                }
            },
            |id| unsafe {
                UnregisterHotKey(self.hwnd, id);
            },
        )?;
        self.hook.hotkey_id.set(next);
        self.hook.hotkey_registered.set(true);
        self.hook.shortcut.set(shortcut);
        self.hotkey_error = None;
        Ok(())
    }
    pub fn hide_pending(&self) -> bool {
        matches!(self.hook.state.get(), State::HidePending(_))
    }
    pub fn mouse_hide_pending(&self) -> bool {
        self.hook.state.get() == State::HidePending(Reason::Mouse)
    }
    pub fn hidden(&self) -> bool {
        matches!(self.hook.state.get(), State::Hidden(_))
    }
    pub fn available(&self) -> bool {
        self.hook.hotkey_registered.get()
            && !matches!(self.hook.state.get(), State::Disabled | State::Destroyed)
    }
    pub fn hotkey_error(&self) -> Option<&str> {
        self.hotkey_error.as_deref()
    }
    pub fn auto_hide_available(&self) -> bool {
        !matches!(self.hook.state.get(), State::Disabled | State::Destroyed)
    }
    pub fn set_auto_hide(&self, enabled: bool) -> Result<(), String> {
        self.hook.auto_hide_enabled.set(enabled);
        if enabled {
            if !self.auto_hide_available() {
                return Err("窗口自动隐藏不可用".into());
            }
            self.hook.sync_timer(self.hwnd)?;
        } else {
            self.hook.stop_timer(self.hwnd);
            if matches!(
                self.hook.state.get(),
                State::Hidden(Reason::Mouse) | State::HidePending(Reason::Mouse)
            ) {
                self.hook.restore(self.hwnd, false);
            }
        }
        Ok(())
    }
    pub fn take_restored(&self) -> bool {
        self.hook.restored.replace(false)
    }
    pub fn take_error(&self) -> Option<String> {
        self.hook.error.borrow_mut().take()
    }
    /// Reader 捕获本帧状态后、保存前调用；不修改书籍/排版/全屏状态。
    /// 隐藏不以保存成功为条件；隐藏本身失败仍走 fail() 恢复并报告错误。
    pub fn hide_now(&self) {
        if !self.hide_pending() {
            return;
        }
        if unsafe { IsWindow(self.hwnd) } == 0 {
            self.hook.state.set(State::Disabled);
            *self.hook.error.borrow_mut() = Some("窗口句柄已失效，老板键已禁用".into());
            return;
        }
        let State::HidePending(reason) = self.hook.state.get() else {
            return;
        };
        // 提交前重新检测，鼠标已经返回时取消自动隐藏。
        if reason == Reason::Mouse {
            if self.hook.moving.get()
                || unsafe { IsIconic(self.hwnd) != 0 || !GetCapture().is_null() }
            {
                self.hook.state.set(State::Visible);
                return;
            }
            self.hook.mouse_tick(self.hwnd);
            if !self.hide_pending() {
                return;
            }
        }
        self.hook.state.set(State::Hidden(reason));
        if let Err(error) = self.hook.sync_timer(self.hwnd) {
            *self.hook.error.borrow_mut() = Some(error);
        }
        unsafe {
            ShowWindow(self.hwnd, 0);
        }
        if self.hook.state.get() == State::Destroyed {
            return;
        }
        // ShowWindow 返回的是先前可见性，不是成功标志。
        if unsafe { IsWindowVisible(self.hwnd) } != 0 {
            self.hook
                .fail(self.hwnd, "无法隐藏窗口，自动隐藏和老板键已禁用".into());
        } else if reason == Reason::BossKey {
            // 同步 winit 的 VISIBLE 标志，避免全屏/样式刷新把窗口重新显示。
            self.hook.winit_hidden.set(true);
            self.hook
                .ctx
                .send_viewport_cmd(ViewportCommand::Visible(false));
        }
        // 鼠标隐藏期间冻结 UI，无样式命令；仅原生隐藏/显示，避免 winit 的
        // Visible(true) 使用 SW_SHOW 激活窗口。老板键隐藏仍同步 winit。
    }
    /// 正常退出时不短暂显示窗口；随 self 的 Drop 注销并移除回调。
    pub fn shutdown(mut self) {
        self.restore_on_drop = false;
    }
}
impl Drop for WindowVisibility {
    fn drop(&mut self) {
        if self.hook.state.get() != State::Destroyed {
            // 显式释放而窗口仍存活时，不能留下无法恢复的隐藏窗口。
            if self.restore_on_drop && self.hidden() {
                self.hook.restore(self.hwnd, false);
            }
            if self.hook.state.get() == State::Destroyed {
                return;
            }
            // 即便移除失败，残留的已排队 WM_HOTKEY 也不能再隐藏窗口。
            self.hook.state.set(State::Disabled);
            self.hook.stop_timer(self.hwnd);
            unsafe {
                UnregisterHotKey(self.hwnd, self.hook.hotkey_id.get());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hwnd,
            instance: Hwnd,
            param: Hwnd,
        ) -> Hwnd;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
    }
    struct TestWindow(Hwnd);
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    #[test]
    fn native_mouse_timer_stops_for_boss_and_resumes_after_restore() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let window = TestWindow(unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0x80000000,
                -3000,
                -3000,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        });
        assert!(!window.0.is_null());
        let hook = Hook {
            ctx: Context::default(),
            state: Cell::new(State::Hidden(Reason::Mouse)),
            hotkey_registered: Cell::new(false),
            hotkey_id: Cell::new(0),
            shortcut: Cell::new(BossShortcut::default()),
            timer: Cell::new(0),
            auto_hide_enabled: Cell::new(true),
            moving: Cell::new(false),
            mouse_area: Cell::new(None),
            winit_hidden: Cell::new(false),
            restored: Cell::new(false),
            error: RefCell::new(None),
        };
        hook.sync_timer(window.0).unwrap();
        assert_ne!(
            hook.timer.get(),
            0,
            "mouse hiding must retain its recovery timer"
        );
        hook.state.set(hook.state.get().boss_toggle());
        hook.sync_timer(window.0).unwrap();
        assert_eq!(hook.timer.get(), 0);
        hook.mouse_tick(window.0); // A queued tick after KillTimer cannot restore the window.
        assert_eq!(hook.state.get(), State::Hidden(Reason::BossKey));
        hook.restore(window.0, false);
        assert_eq!(hook.state.get(), State::Visible);
        assert_ne!(hook.timer.get(), 0);
        hook.auto_hide_enabled.set(false);
        hook.sync_timer(window.0).unwrap();
        assert_eq!(hook.timer.get(), 0);
        hook.state.set(State::Hidden(Reason::BossKey));
        hook.restore(window.0, false);
        assert_eq!(hook.timer.get(), 0, "disabled preference survives restore");
        assert!(hook.error.borrow().is_none());
    }
}
