use ::windows::core::Error;
use ::windows::Win32::Foundation::{GetLastError, SetLastError, ERROR_SUCCESS, HWND, POINT, RECT};
use ::windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONEAREST};
use ::windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsIconic,
    IsZoomed, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWMAXIMIZED, WINDOWPLACEMENT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WPF_RESTORETOMAXIMIZED, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_MAXIMIZE, WS_MINIMIZE,
};
use fastnote_core::WindowGeometry;

use super::monitors::monitor_info;
use super::PlatformError;

/// 临时借用的 Win32 HWND 数值；不延长原生窗口生命周期，也不持有 winit Arc。
#[derive(Clone, Copy, Debug)]
pub struct NativeWindowHandle(pub isize);

impl NativeWindowHandle {
    fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

/// 透明悬浮球设为工具窗口、置顶、不激活、不进入任务栏/Alt+Tab。
/// 参数必须来自当前 UI 线程的实时 winit 窗口借用；失效句柄由 Win32 返回错误。
pub fn configure_ball_window(handle: NativeWindowHandle) -> Result<(), PlatformError> {
    configure_tool_window(handle, true)
}

/// 自绘入口菜单使用可激活工具窗口，不进入任务栏/Alt+Tab，保留键盘导航。
pub fn configure_menu_window(handle: NativeWindowHandle) -> Result<(), PlatformError> {
    configure_tool_window(handle, false)
}

fn configure_tool_window(
    handle: NativeWindowHandle,
    nonactivating: bool,
) -> Result<(), PlatformError> {
    let hwnd = handle.hwnd();
    // SAFETY: HWND 作为不透明输入交给系统验证；无 Rust 指针解引用。
    // SetWindowLongPtr 的 0 可能是合法旧值，配合 LastError 区分失败。
    unsafe {
        let old = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = (old | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
        let style = if nonactivating {
            style | WS_EX_NOACTIVATE.0
        } else {
            style & !WS_EX_NOACTIVATE.0
        };
        SetLastError(ERROR_SUCCESS);
        let previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        if previous == 0 && GetLastError() != ERROR_SUCCESS {
            return Err(Error::from_thread().into());
        }
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(())
}

/// 在真实鼠标事件中读取指针物理坐标，用于计算拖动；不移动/模拟用户鼠标。
pub fn cursor_position() -> Result<(i32, i32), PlatformError> {
    let mut point = POINT::default();
    // SAFETY: 输出指向独占栈结构，GetCursorPos 不保留该指针。
    unsafe {
        GetCursorPos(&mut point)?;
    }
    Ok((point.x, point.y))
}

/// 读取普通还原矩形，即使窗口最大化/最小化也不会保存整个屏幕或 (-32000,-32000)。
/// `w/h` 换算成客户区逻辑尺寸，与 Slint set_size 对应。
pub fn capture_geometry(handle: NativeWindowHandle) -> Result<WindowGeometry, PlatformError> {
    let hwnd = handle.hwnd();
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: Windows 仅同步读取 HWND 并填充尺寸正确的栈结构，不保留指针。
    unsafe {
        GetWindowPlacement(hwnd, &mut placement)?;
    }
    // 普通（含贴边）窗口读取实际位置/客户区；最大化、最小化才读取还原矩形。
    // SAFETY: 两个 API 只读取系统管理的 HWND，不解引用 Rust 指针。
    let minimized = unsafe { IsIconic(hwnd) }.as_bool();
    // SAFETY: 同上，IsZoomed 是无输出指针的查询。
    let zoomed = unsafe { IsZoomed(hwnd) }.as_bool();
    let mut normal = placement.rcNormalPosition;
    let mut client = RECT::default();
    if !minimized && !zoomed {
        // SAFETY: 两个 RECT 输出在栈上独占，API 不保存指针。
        unsafe {
            GetWindowRect(hwnd, &mut normal)?;
            GetClientRect(hwnd, &mut client)?;
        }
    }
    // SAFETY: normal 是有效 RECT 栈值，Windows 仅在调用期间读取。
    let monitor = unsafe { MonitorFromRect(&normal, MONITOR_DEFAULTTONEAREST) };
    let info = monitor_info(monitor)?.monitorInfo;
    // SAFETY: HWND 为不透明输入，返回整数无所有权。
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        return Err(Error::from_thread().into());
    }
    let scale = f64::from(dpi) / 96.0;
    let mut frame = RECT::default();
    // SAFETY: 样式按值输入，frame 是独占且尺寸正确的输出 RECT。
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & !(WS_MAXIMIZE.0 | WS_MINIMIZE.0);
        AdjustWindowRectExForDpi(
            &mut frame,
            WINDOW_STYLE(style),
            false,
            WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32),
            dpi,
        )?;
    }
    // 主窗不是 tool window：rcNormalPosition 为工作区坐标，补回左/上任务栏偏移。
    let workspace_coordinates = minimized || zoomed;
    let x = f64::from(normal.left)
        + if workspace_coordinates {
            f64::from(info.rcWork.left - info.rcMonitor.left)
        } else {
            0.0
        };
    let y = f64::from(normal.top)
        + if workspace_coordinates {
            f64::from(info.rcWork.top - info.rcMonitor.top)
        } else {
            0.0
        };
    let (width, height) = if workspace_coordinates {
        (
            f64::from(normal.right)
                - f64::from(normal.left)
                - (f64::from(frame.right) - f64::from(frame.left)),
            f64::from(normal.bottom)
                - f64::from(normal.top)
                - (f64::from(frame.bottom) - f64::from(frame.top)),
        )
    } else {
        (f64::from(client.right), f64::from(client.bottom))
    };
    let maximized = zoomed
        || placement.showCmd == SW_SHOWMAXIMIZED.0 as u32
        || (minimized && placement.flags.contains(WPF_RESTORETOMAXIMIZED));
    let origin_x = f64::from(info.rcMonitor.left);
    let origin_y = f64::from(info.rcMonitor.top);
    Ok(from_physical(
        (x, y),
        (width, height),
        (origin_x, origin_y),
        scale,
        maximized,
    ))
}

fn from_physical(
    position: (f64, f64),
    size: (f64, f64),
    origin: (f64, f64),
    scale: f64,
    maximized: bool,
) -> WindowGeometry {
    WindowGeometry {
        x: origin.0 + (position.0 - origin.0) / scale,
        y: origin.1 + (position.1 - origin.1) / scale,
        w: size.0 / scale,
        h: size.1 / scale,
        maximized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_geometry_conversion_preserves_normal_client_size_and_negative_origin() {
        // 纯数据转换，不创建窗口、不操作桌面。
        let geometry = from_physical(
            (-1740.0, 150.0),
            (1920.0, 1200.0),
            (-1920.0, 0.0),
            1.5,
            true,
        );
        assert_eq!(geometry.x, -1800.0);
        assert_eq!(geometry.y, 100.0);
        assert_eq!((geometry.w, geometry.h), (1280.0, 800.0));
        assert!(geometry.maximized);
    }
}
