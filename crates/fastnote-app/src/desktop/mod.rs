//! Windows 驻留闭环；UI 线程拥有窗口/句柄，DB worker 拥有设置与任务的唯一 SQLite 写连接。

mod actions;
mod ball;
mod calendar;
mod close;
mod effects;
mod gesture;
mod journal;
mod lease;
mod lifecycle;
mod notes;
mod presentation;
mod resources;
mod runtime;
mod task_preferences;
mod tasks;
mod theme;
mod ui_session;
mod wakeup;
mod window;
mod window_frame;
mod worker;

#[cfg(test)]
mod session_contract;

use slint::winit_030::winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
use slint::winit_030::winit::window::{WindowAttributes, WindowLevel};

use runtime::Runtime;

fn configure_window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    match attributes.title.as_str() {
        // 创建时与后续 configure_ball_native 使用同一层级，避免先创建普通窗口再补设置顶。
        "FastNote 悬浮球" => attributes
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_skip_taskbar(true)
            .with_active(false)
            .with_drag_and_drop(false)
            .with_resizable(false),
        "FastNote 入口菜单" => attributes
            .with_skip_taskbar(true)
            .with_active(true)
            .with_drag_and_drop(false)
            .with_resizable(false),
        "FastNote" => attributes
            .with_undecorated_shadow(true)
            .with_corner_preference(CornerPreference::Round),
        _ => attributes,
    }
}

pub(crate) fn run(hidden: bool) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    // Slint 1.18/winit 在 Windows 默认 hide 只设不可见，仍保留原生与图形上下文。
    // 使用已有后端开关，让所有实际 hide 先 suspend/drop 昂贵图形资源；托盘任务栏
    // set_minimized 不走 hide，平台行为不变。仅本进程，不写系统环境或新增产品设置。
    std::env::set_var("SLINT_DESTROY_WINDOW_ON_HIDE", "1");
    slint::BackendSelector::new()
        .backend_name("winit".to_owned())
        .renderer_name("femtovg".to_owned())
        .with_winit_window_attributes_hook(configure_window_attributes)
        .select()?;
    let runtime = Runtime::new(hidden, started)?;
    let result = slint::run_event_loop_until_quit();
    // 事件循环异常返回时也要清理入口、注销热键、停止并 join 设置线程。
    runtime.cleanup();
    drop(runtime);
    result?;
    Ok(())
}

#[cfg(test)]
mod window_attributes_tests {
    use super::*;

    #[test]
    fn test_ball_is_topmost_without_activation_on_every_creation() {
        for _ in 0..2 {
            let attributes = configure_window_attributes(
                WindowAttributes::default().with_title("FastNote 悬浮球"),
            );
            assert_eq!(attributes.window_level, WindowLevel::AlwaysOnTop);
            assert!(!attributes.active);
            assert!(!attributes.resizable);
        }
    }

    #[test]
    fn test_other_windows_keep_their_default_window_level() {
        for title in ["FastNote", "FastNote 入口菜单", "other"] {
            let attributes =
                configure_window_attributes(WindowAttributes::default().with_title(title));
            assert_eq!(attributes.window_level, WindowLevel::Normal);
        }
    }
}
