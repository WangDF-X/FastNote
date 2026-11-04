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

/// SHELL-03 T1.2 批次 2：无会话导航只失效，不为导航发起今日查询。
pub(super) fn should_refresh_today(update: &ShellUpdate, has_ui: bool) -> bool {
    update.route_changed && update.view.page_label == "今日任务" && has_ui
}

#[cfg(test)]
mod tests;
