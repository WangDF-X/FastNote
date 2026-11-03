use std::sync::OnceLock;

use ::windows::core::{Error, PCWSTR};
use ::windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_SUCCESS, HINSTANCE, HWND, LPARAM, POINT, RECT, WPARAM,
};
use ::windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONEAREST};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsIconic,
    IsZoomed, LoadImageW, SendMessageW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
    HWND_TOPMOST, ICON_BIG, IMAGE_ICON, LR_DEFAULTSIZE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_SHOWMAXIMIZED, WINDOWPLACEMENT, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_SETICON, WPF_RESTORETOMAXIMIZED, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_MAXIMIZE, WS_MINIMIZE,
};
use fastnote_core::WindowGeometry;

use super::monitors::monitor_info;
use super::PlatformError;

/// 临时借用的 Win32 HWND 数值；不延长原生窗口生命周期，也不持有 winit Arc。
#[derive(Clone, Copy, Debug)]
pub struct NativeWindowHandle(pub isize);

impl NativeWindowHandle {
    pub(super) fn hwnd(self) -> HWND {
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

/// 品牌图标在 exe 资源里的编号，与 `crates/fastnote-app/build.rs` 的 `1 ICON` 一致。
const BRAND_ICON_RESOURCE_ID: u16 = 1;

/// 只加载一次的窗口大图标；进程退出时由系统回收。
/// 窗口隐藏会销毁原生窗口，重新显示时需要再次把同一个 HICON 设置回去。
static BRAND_BIG_ICON: OnceLock<Option<isize>> = OnceLock::new();

/// 把 exe 资源里的品牌图标设为窗口大图标（任务栏、Alt+Tab）。
///
/// winit 的 `set_window_icon` 只写 ICON_SMALL（标题栏），任务栏读取的 ICON_BIG
/// 依赖外壳回退到 exe 图标；这里显式设置，不依赖外壳的图标缓存与回退顺序。
pub fn set_window_big_icon(handle: NativeWindowHandle) -> Result<(), PlatformError> {
    let icon = brand_big_icon().ok_or(PlatformError::Icon)?;
    // SAFETY: HWND 是不透明输入；HICON 由本模块持有到进程退出，窗口只保存引用。
    unsafe {
        SendMessageW(
            handle.hwnd(),
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon)),
        );
    }
    Ok(())
}

fn brand_big_icon() -> Option<isize> {
    *BRAND_BIG_ICON.get_or_init(|| {
        // SAFETY: 模块句柄与资源编号都是进程内不透明值；MAKEINTRESOURCE(1) 只是
        // 指针大小的编号，LoadImageW 不会解引用它，返回的 HICON 由静态变量持有。
        // LR_DEFAULTSIZE 让系统按当前 DPI 的图标尺寸挑选 ico 中最接近的一档，
        // 不指定时 0×0 会退回 ico 的第一帧（16px），任务栏会发虚。
        unsafe {
            let module = GetModuleHandleW(PCWSTR::null()).ok()?;
            let icon = LoadImageW(
                Some(HINSTANCE(module.0)),
                PCWSTR::from_raw(std::ptr::without_provenance(
                    BRAND_ICON_RESOURCE_ID as usize,
                )),
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE,
            )
            .ok()?;
            Some(icon.0 as isize)
        }
    })
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
    capture_window_geometry(handle, true)
}

/// 读取自绘标题栏窗口的还原矩形；不扣除并不存在的原生标题栏/边框。
pub fn capture_frameless_geometry(
    handle: NativeWindowHandle,
) -> Result<WindowGeometry, PlatformError> {
    capture_window_geometry(handle, false)
}

fn capture_window_geometry(
    handle: NativeWindowHandle,
    decorated: bool,
) -> Result<WindowGeometry, PlatformError> {
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
    if decorated {
        // SAFETY: 样式按值输入，frame 是独占且尺寸正确的输出 RECT。
        unsafe {
            let style =
                GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & !(WS_MAXIMIZE.0 | WS_MINIMIZE.0);
            AdjustWindowRectExForDpi(
                &mut frame,
                WINDOW_STYLE(style),
                false,
                WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32),
                dpi,
            )?;
        }
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
        restored_client_size(&normal, &frame, decorated)
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

fn restored_client_size(normal: &RECT, frame: &RECT, decorated: bool) -> (f64, f64) {
    let width = f64::from(normal.right) - f64::from(normal.left);
    let height = f64::from(normal.bottom) - f64::from(normal.top);
    if decorated {
        (
            width - (f64::from(frame.right) - f64::from(frame.left)),
            height - (f64::from(frame.bottom) - f64::from(frame.top)),
        )
    } else {
        (width, height)
    }
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
    fn test_frameless_restore_does_not_subtract_a_native_caption() {
        let normal = RECT {
            left: 100,
            top: 100,
            right: 1380,
            bottom: 900,
        };
        let frame = RECT {
            left: -8,
            top: -31,
            right: 8,
            bottom: 8,
        };
        assert_eq!(
            restored_client_size(&normal, &frame, false),
            (1280.0, 800.0)
        );
        assert_eq!(restored_client_size(&normal, &frame, true), (1264.0, 761.0));
    }

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
