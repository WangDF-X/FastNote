use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use ::windows::Win32::Graphics::Gdi::ScreenToClient;
use ::windows::Win32::UI::HiDpi::GetDpiForWindow;
use ::windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsZoomed, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT,
    HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, WM_NCDESTROY, WM_NCHITTEST,
};

use super::{NativeWindowHandle, PlatformError};

const SUBCLASS_ID: usize = 0x464E4341;
// 与 ui/components/title-bar.slint 及 MainWindow 的布局契约一致。
const CAPTION_HEIGHT: f64 = 40.0;
const CAPTION_BUTTON_WIDTH: f64 = 46.0;
const RESIZE_BORDER: f64 = 6.0;

/// SHELL-01 §3.1：为自绘标题栏保留原生拖动、双击、系统菜单及八向缩放。
/// 子类化只计算命中区域，不保存窗口/业务状态，HWND 销毁时自动卸载。
pub fn configure_custom_caption(handle: NativeWindowHandle) -> Result<(), PlatformError> {
    // SAFETY: HWND 来自 UI 线程的即时借用；回调是静态函数，refdata 不含 Rust 指针。
    unsafe { SetWindowSubclass(handle.hwnd(), Some(caption_proc), SUBCLASS_ID, 0) }.ok()?;
    Ok(())
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_NCHITTEST {
        let mut point = message_point(lparam.0);
        let mut client = RECT::default();
        // SAFETY: POINT/RECT 是独占栈输出；API 不保留指针，HWND 由系统验证。
        let (valid, dpi, maximized) = unsafe {
            (
                ScreenToClient(hwnd, &mut point).as_bool()
                    && GetClientRect(hwnd, &mut client).is_ok(),
                GetDpiForWindow(hwnd),
                IsZoomed(hwnd).as_bool(),
            )
        };
        if valid && dpi != 0 {
            return LRESULT(hit_test(
                (f64::from(point.x), f64::from(point.y)),
                (f64::from(client.right), f64::from(client.bottom)),
                f64::from(dpi) / 96.0,
                maximized,
            ) as isize);
        }
    } else if message == WM_NCDESTROY {
        // SAFETY: 回调只移除它自己，不拥有 HWND 或其它 Rust 上下文。
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(caption_proc), SUBCLASS_ID) };
    }
    // SAFETY: 未处理消息按原始参数转交 winit/Windows，保持其生命周期与窗口行为。
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn message_point(lparam: isize) -> POINT {
    POINT {
        x: i32::from(lparam as u16 as i16),
        y: i32::from((lparam >> 16) as u16 as i16),
    }
}

fn hit_test(point: (f64, f64), size: (f64, f64), scale: f64, maximized: bool) -> u32 {
    let (x, y) = (point.0 / scale, point.1 / scale);
    let (width, height) = (size.0 / scale, size.1 / scale);
    if x < 0.0 || y < 0.0 || x >= width || y >= height {
        return HTCLIENT;
    }
    if !maximized {
        let left = x < RESIZE_BORDER;
        let right = x >= width - RESIZE_BORDER;
        let top = y < RESIZE_BORDER;
        let bottom = y >= height - RESIZE_BORDER;
        let border = match (left, right, top, bottom) {
            (true, _, true, _) => Some(HTTOPLEFT),
            (_, true, true, _) => Some(HTTOPRIGHT),
            (true, _, _, true) => Some(HTBOTTOMLEFT),
            (_, true, _, true) => Some(HTBOTTOMRIGHT),
            (true, _, _, _) => Some(HTLEFT),
            (_, true, _, _) => Some(HTRIGHT),
            (_, _, true, _) => Some(HTTOP),
            (_, _, _, true) => Some(HTBOTTOM),
            _ => None,
        };
        if let Some(border) = border {
            return border;
        }
    }
    if y < CAPTION_HEIGHT && x < width - 3.0 * CAPTION_BUTTON_WIDTH {
        HTCAPTION
    } else {
        // 按钮和正文由 Slint 处理，不能落回不可见的系统标题栏命中区。
        HTCLIENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caption_preserves_native_drag_but_leaves_buttons_and_body_to_slint() {
        let size = (1280.0, 800.0);
        assert_eq!(hit_test((100.0, 20.0), size, 1.0, false), HTCAPTION);
        for point in [(1142.0, 20.0), (1270.0, 20.0), (100.0, 40.0)] {
            assert_eq!(hit_test(point, size, 1.0, false), HTCLIENT);
        }
    }

    #[test]
    fn test_button_centers_remain_clickable_at_supported_widths_and_dpi() {
        for width in [1024.0, 1280.0, 1920.0] {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                for maximized in [false, true] {
                    for inset in [23.0, 69.0, 115.0] {
                        assert_eq!(
                            hit_test(
                                ((width - inset) * scale, 20.0 * scale),
                                (width * scale, 800.0 * scale),
                                scale,
                                maximized,
                            ),
                            HTCLIENT
                        );
                    }
                    assert_eq!(
                        hit_test(
                            ((width - 139.0) * scale, 20.0 * scale),
                            (width * scale, 800.0 * scale),
                            scale,
                            maximized,
                        ),
                        HTCAPTION
                    );
                }
            }
        }
    }

    #[test]
    fn test_all_eight_resize_edges_are_dpi_independent() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (x, y, expected) in [
                (2.0, 2.0, HTTOPLEFT),
                (1278.0, 2.0, HTTOPRIGHT),
                (2.0, 798.0, HTBOTTOMLEFT),
                (1278.0, 798.0, HTBOTTOMRIGHT),
                (2.0, 400.0, HTLEFT),
                (1278.0, 400.0, HTRIGHT),
                (640.0, 2.0, HTTOP),
                (640.0, 798.0, HTBOTTOM),
            ] {
                assert_eq!(
                    hit_test(
                        (x * scale, y * scale),
                        (1280.0 * scale, 800.0 * scale),
                        scale,
                        false
                    ),
                    expected
                );
            }
        }
    }

    #[test]
    fn test_maximized_caption_has_no_resize_border_and_outside_points_are_not_actions() {
        let size = (1280.0, 800.0);
        assert_eq!(hit_test((2.0, 2.0), size, 1.0, true), HTCAPTION);
        for point in [(1278.0, 2.0), (2.0, 798.0), (-1.0, 20.0), (1280.0, 20.0)] {
            assert_eq!(hit_test(point, size, 1.0, true), HTCLIENT);
        }
    }

    #[test]
    fn test_message_coordinates_preserve_negative_monitor_origins() {
        let encoded = ((-120_i16 as u16 as isize) << 16) | (-1920_i16 as u16 as isize);
        let point = message_point(encoded);
        assert_eq!((point.x, point.y), (-1920, -120));
    }
}
