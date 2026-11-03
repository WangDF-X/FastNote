//! Windows 驻留闭环；UI 线程拥有窗口/句柄，DB worker 拥有设置与任务的唯一 SQLite 写连接。

mod actions;
mod ball;
mod close;
mod gesture;
mod lifecycle;
mod presentation;
mod runtime;
mod tasks;
mod theme;
mod window;
mod worker;

use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;

use runtime::Runtime;

pub(crate) fn run(hidden: bool) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    slint::BackendSelector::new()
        .backend_name("winit".to_owned())
        .renderer_name("femtovg".to_owned())
        .with_winit_window_attributes_hook(|attributes| match attributes.title.as_str() {
            "FastNote 悬浮球" => attributes
                .with_skip_taskbar(true)
                .with_active(false)
                .with_drag_and_drop(false)
                .with_resizable(false),
            "FastNote 入口菜单" => attributes
                .with_skip_taskbar(true)
                .with_active(true)
                .with_drag_and_drop(false)
                .with_resizable(false),
            "FastNote" => attributes.with_undecorated_shadow(true),
            _ => attributes,
        })
        .select()?;
    let runtime = Runtime::new(hidden, started)?;
    let result = slint::run_event_loop_until_quit();
    // 事件循环异常返回时也要清理入口、注销热键、停止并 join 设置线程。
    runtime.cleanup();
    drop(runtime);
    result?;
    Ok(())
}
