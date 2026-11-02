//! 应用层传给外壳的展示数据；不在 UI 中另存一份可变路由。

use crate::MainWindow;

/// 页面标题与路由参数的展示快照，由应用控制器统一生成。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellView {
    /// 侧边栏模块名；计划详情仍高亮「计划板」。
    pub page_label: &'static str,
    /// 页面标题；业务模块接入前，计划详情使用占位标题。
    pub title: &'static str,
    /// 日期、选中 ID 或设置分组等上下文，不包含正文。
    pub context: String,
    /// 仅供开发预览展示的规范化路由动作。
    pub route_debug: String,
    /// 设置或计划详情页是否显示返回按钮。
    pub can_go_back: bool,
    /// 是否为计划详情，用于选择正确的占位空态。
    pub is_plan_detail: bool,
    /// 是否显示本阶段的通用窗口设置，而不是其它设置分组的占位页。
    pub is_general_settings: bool,
}

impl ShellView {
    pub(super) fn apply(&self, window: &MainWindow) {
        window.set_current_page(self.page_label.into());
        window.set_page_title(self.title.into());
        window.set_route_context(self.context.clone().into());
        window.set_route_debug(self.route_debug.clone().into());
        window.set_can_go_back(self.can_go_back);
        window.set_plan_detail(self.is_plan_detail);
        window.set_general_settings(self.is_general_settings);
    }
}

/// 导航失败类别；UI 只展示固定中文文案，不外露动作或错误源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationIssue {
    /// 目标页面无效。
    Target,
    /// 参数名称、格式、枚举值或必填参数无效。
    Parameters,
    /// 日期格式或范围无效。
    Date,
    /// 实体 ID 格式、版本或变体无效。
    Id,
}

impl NavigationIssue {
    /// 返回 SHELL-01 §4.1.1 定义的稳定错误码。
    pub fn code(self) -> &'static str {
        match self {
            Self::Target => "E_ROUTE_TARGET",
            Self::Parameters => "E_ROUTE_PARAMETERS",
            Self::Date => "E_DATE_RANGE",
            Self::Id => "E_ROUTE_ID",
        }
    }

    /// 返回可直接展示的中文提示，不包含原始输入。
    pub fn message(self) -> &'static str {
        match self {
            Self::Target => "无法跳转，目标页面无效。",
            Self::Parameters => "无法跳转，导航参数无效。",
            Self::Date => "无法跳转，日期无效（1900-01-01–9999-12-31）。",
            Self::Id => "无法跳转，内容标识无效。",
        }
    }
}

/// 处理一次动作后的外壳更新，由应用层判断完整路由是否改变。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellUpdate {
    /// 新的展示快照。
    pub view: ShellView,
    /// 参数改变也算路由改变，用于清理旧页面的临时反馈。
    pub route_changed: bool,
    /// 可选导航错误，当前路由保持不变。
    pub issue: Option<NavigationIssue>,
}

#[cfg(test)]
mod tests {
    use super::NavigationIssue;

    #[test]
    fn test_navigation_codes_and_messages_are_fixed() {
        for (issue, code) in [
            (NavigationIssue::Target, "E_ROUTE_TARGET"),
            (NavigationIssue::Parameters, "E_ROUTE_PARAMETERS"),
            (NavigationIssue::Date, "E_DATE_RANGE"),
            (NavigationIssue::Id, "E_ROUTE_ID"),
        ] {
            assert_eq!(issue.code(), code);
            assert!(issue.message().starts_with("无法跳转，"));
        }
    }
}
