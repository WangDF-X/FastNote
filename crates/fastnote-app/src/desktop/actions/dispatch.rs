use fastnote_ui::ShellUpdate;

/// 有会话沿用既有外壳动作入口；无会话只处理一次内核动作，返回待投影快照。
pub(super) fn dispatch_or_handle(
    action: &str,
    dispatch: impl FnOnce(&str) -> bool,
    handle: impl FnOnce(&str) -> ShellUpdate,
) -> Option<ShellUpdate> {
    if dispatch(action) {
        None
    } else {
        Some(handle(action))
    }
}

/// SHELL-03 T2.3：只有可见主窗的今日路由进入才安排页面加载，隐藏导航只失效。
pub(super) fn should_refresh_today(update: &ShellUpdate, content_visible: bool) -> bool {
    update.route_changed && update.view.page_label == "今日任务" && content_visible
}

#[cfg(test)]
mod tests;
