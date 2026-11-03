use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Arc;

use ::windows::core::{w, Error};
use ::windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_CLASS_ALREADY_EXISTS, ERROR_SUCCESS, HINSTANCE, HWND, LPARAM,
    LRESULT, WPARAM,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, RegisterClassW,
    SetWindowLongPtrW, CREATESTRUCTW, GWLP_USERDATA, WM_NCCREATE, WM_NCDESTROY, WM_SETTINGCHANGE,
    WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use crossbeam_channel::{Sender, TrySendError};

use crate::NativeEvent;

use super::PlatformError;

/// ARCH-06 §9.1：读取 Windows 的应用亮/暗设置，不改变注册表。
/// 缺失、类型错误或非法值返回可追溯错误，由应用层回退浅色。
pub fn system_theme_is_dark() -> Result<bool, PlatformError> {
    let mut value = 1_u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: RegGetValueW 同步填充独占的 DWORD 与长度，不保留栈指针；
    // RRF_RT_REG_DWORD 拒绝其它注册表类型，输出容量精确为四字节。
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut value).cast()),
            Some(&mut size),
        )
        .ok()?;
    }
    decode_theme(value, size).ok_or(PlatformError::InvalidTheme)
}

fn decode_theme(value: u32, size: u32) -> Option<bool> {
    match (value, size) {
        (0, 4) => Some(true),
        (1, 4) => Some(false),
        _ => None,
    }
}

struct ThemeEvents {
    sender: Sender<NativeEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl ThemeEvents {
    fn dispatch(&self) {
        match self.sender.try_send(NativeEvent::SystemThemeChanged) {
            Ok(()) => {
                // 公共唤醒回调不允许把 panic 穿过 Win32 ABI 边界。
                if catch_unwind(AssertUnwindSafe(|| (self.wake)())).is_err() {
                    tracing::error!(phase = "system_theme_wakeup", "theme wakeup panicked");
                }
            }
            Err(TrySendError::Full(_)) => {
                tracing::warn!(
                    phase = "system_theme",
                    "native queue full, newest event dropped"
                );
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

/// UI 线程拥有的不可见顶层工具窗口；只接收设置广播，不显示或抢占焦点。
/// 不使用消息专用窗口，因为 WM_SETTINGCHANGE 不会广播给它；不创建线程或轮询。
pub struct WindowsThemeMonitor {
    hwnd: HWND,
    events: Option<Box<ThemeEvents>>,
    // 创建、消息分发与销毁必须留在同一 UI 线程。
    _ui_thread: PhantomData<Rc<()>>,
}

impl WindowsThemeMonitor {
    /// 将系统设置变化经已有容量 256 的通道交给 UI；回调不读写业务状态。
    pub fn new(
        sender: Sender<NativeEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, PlatformError> {
        // SAFETY: 获取当前进程模块的不透明句柄，不增加 Rust 所有权。
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(theme_window_proc),
            hInstance: instance,
            lpszClassName: w!("FastNote.ThemeNotifications"),
            ..Default::default()
        };
        // SAFETY: 类结构与类名在调用期间有效；回调是进程生命周期内的静态函数。
        // 类注册可复用，进程退出时 Windows 回收它。
        unsafe {
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_thread().into());
            }
        }
        let mut events = Box::new(ThemeEvents { sender, wake });
        // SAFETY: Box 的地址不随移动改变；它从创建前到 DestroyWindow 完成始终有效。
        // 没有 WS_VISIBLE，且为 TOOLWINDOW/NOACTIVATE，不进入任务栏或 Alt+Tab。
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.lpszClassName,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                Some(std::ptr::from_mut(events.as_mut()).cast()),
            )?
        };
        Ok(Self {
            hwnd,
            events: Some(events),
            _ui_thread: PhantomData,
        })
    }
}

impl Drop for WindowsThemeMonitor {
    fn drop(&mut self) {
        // SAFETY: !Send 标记保证同线程销毁；DestroyWindow 同步完成最后的回调，
        // events 的 Box 此时仍然有效。
        if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
            tracing::warn!(phase = "system_theme_cleanup", code = error.code().0);
            // 系统若仍保留 HWND，不释放它引用的上下文，避免悬空指针；仅失败时泄漏。
            if let Some(events) = self.events.take() {
                Box::leak(events);
            }
        }
    }
}

unsafe extern "system" fn theme_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE if lparam.0 != 0 => {
            // SAFETY: Windows 为 WM_NCCREATE 提供有效的 CREATESTRUCTW；
            // lpCreateParams 来自 new 的稳定 Box，生命周期覆盖整个 HWND。
            let pointer = unsafe { (*(lparam.0 as *const CREATESTRUCTW)).lpCreateParams };
            if pointer.is_null() {
                return LRESULT(0);
            }
            // SAFETY: GWLP_USERDATA 存储不透明指针值；LastError 区分合法旧值 0 与失败。
            unsafe {
                SetLastError(ERROR_SUCCESS);
                if SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize) == 0
                    && GetLastError() != ERROR_SUCCESS
                {
                    return LRESULT(0);
                }
            }
            LRESULT(1)
        }
        WM_SETTINGCHANGE => {
            // 不解引用广播携带的字符串；其它设置广播也可触发一次轻量核验。
            // SAFETY: 该槽只由本回调初始化/清空；所有回调同线程且 Box 仍由 guard 持有。
            let events = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ThemeEvents };
            if !events.is_null() {
                // SAFETY: 见上；只借用通道与唤醒函数，不持有可变业务状态。
                unsafe { &*events }.dispatch();
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: 结束窗口生命周期时清除不透明指针，不在回调中释放 guard 的 Box。
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            // SAFETY: 原始 Windows 消息参数按值转交默认过程。
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        // SAFETY: 其余消息保持系统默认处理，不解引用任何消息指针。
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::decode_theme;

    #[test]
    fn test_system_theme_requires_a_valid_boolean_dword() {
        assert_eq!(decode_theme(0, 4), Some(true));
        assert_eq!(decode_theme(1, 4), Some(false));
        for (value, size) in [(2, 4), (u32::MAX, 4), (0, 0), (1, 8)] {
            assert_eq!(decode_theme(value, size), None);
        }
    }
}
