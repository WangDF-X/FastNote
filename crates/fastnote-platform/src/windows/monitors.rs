use ::windows::core::{Error, BOOL};
use ::windows::Win32::Foundation::{LPARAM, POINT, RECT};
use ::windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use ::windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use fastnote_core::{LogicalRect, Monitor};

use super::PlatformError;

struct Enumeration {
    monitors: Vec<Monitor>,
    error: Option<PlatformError>,
}

/// 查询所有屏幕的逻辑工作区（排除任务栏）、设备名与每屏 DPI。
pub fn monitors() -> Result<Vec<Monitor>, PlatformError> {
    let mut context = Enumeration {
        monitors: Vec::new(),
        error: None,
    };
    // SAFETY: EnumDisplayMonitors 同步调用回调，context 在调用期间不移动且独占；
    // 回调不保存指针，所有原生输出均为尺寸正确的栈结构。
    let success = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enumerate),
            LPARAM((&mut context as *mut Enumeration) as isize),
        )
    };
    if let Some(error) = context.error {
        return Err(error);
    }
    if !success.as_bool() || context.monitors.is_empty() {
        return Err(Error::from_thread().into());
    }
    Ok(context.monitors)
}

unsafe extern "system" fn enumerate(handle: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: data 只由上方 monitors() 设置，指向本次同步枚举的唯一可变上下文。
    let context = unsafe { &mut *(data.0 as *mut Enumeration) };
    match read_monitor(handle) {
        Ok(monitor) => {
            context.monitors.push(monitor);
            BOOL(1)
        }
        Err(error) => {
            context.error = Some(error);
            BOOL(0)
        }
    }
}

/// 取得物理指针位置所在（或最近）屏幕，用于拖动跨屏后的 DPI 与工作区校正。
pub fn monitor_at_point(x: i32, y: i32) -> Result<Monitor, PlatformError> {
    // SAFETY: POINT 为按值输入；Windows 返回不透明句柄，此处不解引用。
    read_monitor(unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) })
}

pub(super) fn monitor_info(handle: HMONITOR) -> Result<MONITORINFOEXW, PlatformError> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: MONITORINFOEXW 的首字段为 MONITORINFO，cbSize 包含扩展设备名空间；
    // Win32 在同步调用中填充结构，不保留输出指针。
    if !unsafe {
        GetMonitorInfoW(
            handle,
            (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
        )
    }
    .as_bool()
    {
        return Err(Error::from_thread().into());
    }
    Ok(info)
}

fn read_monitor(handle: HMONITOR) -> Result<Monitor, PlatformError> {
    let info = monitor_info(handle)?;
    let (mut x, mut y) = (0, 0);
    // SAFETY: 有效显示器句柄来自 Win32，输出指针独占且仅在调用期间使用。
    unsafe {
        GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut x, &mut y)?;
    }
    if x == 0 {
        return Err(Error::from_thread().into());
    }
    let scale = f64::from(x) / 96.0;
    let r = info.monitorInfo.rcWork;
    let origin_x = f64::from(info.monitorInfo.rcMonitor.left);
    let origin_y = f64::from(info.monitorInfo.rcMonitor.top);
    let name_end = info
        .szDevice
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(info.szDevice.len());
    Ok(Monitor {
        name: String::from_utf16_lossy(&info.szDevice[..name_end]),
        work: LogicalRect {
            x: origin_x + (f64::from(r.left) - origin_x) / scale,
            y: origin_y + (f64::from(r.top) - origin_y) / scale,
            w: (f64::from(r.right) - f64::from(r.left)) / scale,
            h: (f64::from(r.bottom) - f64::from(r.top)) / scale,
        },
        scale,
        origin_x,
        origin_y,
        primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
    })
}
