//! Banner 集合的纯逻辑（SHELL-02 §5）。
//!
//! 页面可以同时存在多条状态横幅，但界面上永远只显示一条：
//! 按 `error > warning > info > read-only` 取最高级。集合不碰 Slint 元素，
//! 因此「选哪条、能不能关、关掉之后显示谁」都可以单元测试。

/// Banner 类型，决定背景、图标色、可关闭性和优先级。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BannerKind {
    /// 需要立即处理的故障，例如数据库异常、磁盘空间不足。
    Error,
    /// 需要注意但不阻断，例如只读降级。
    Warning,
    /// 中性提示。
    Info,
    /// 只读状态说明。
    ReadOnly,
}

impl BannerKind {
    /// 同页多条时只显示最高级的一条（SHELL-02 §5）。
    pub fn priority(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
            Self::ReadOnly => 0,
        }
    }

    /// 用户能否手动关闭；`Error` 与 `ReadOnly` 只能由状态解除后自行消失。
    pub fn closable(self) -> bool {
        matches!(self, Self::Warning | Self::Info)
    }
}

/// 横幅上的可选操作链接，点击后交给应用统一动作入口处理。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BannerAction {
    /// 链接文案，例如「打开数据管理」。
    pub label: String,
    /// 动作字符串，与导航使用同一套格式。
    pub command: String,
}

/// 一条横幅。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BannerRequest {
    /// 类型。
    pub kind: BannerKind,
    /// 文案；不得包含任务标题、正文等敏感内容。
    pub message: String,
    /// 可选操作链接（最多一个，SHELL-02 §5）。
    pub action: Option<BannerAction>,
}

impl BannerRequest {
    /// 创建一条没有操作链接的横幅。
    pub fn new(kind: BannerKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            action: None,
        }
    }

    /// 追加一个操作链接。
    pub fn with_action(mut self, label: impl Into<String>, command: impl Into<String>) -> Self {
        self.action = Some(BannerAction {
            label: label.into(),
            command: command.into(),
        });
        self
    }
}

/// 当前页面的横幅集合。
///
/// 每种类型只保留最新一条（重复提交同类型视为更新文案），
/// 显示时取优先级最高的一条，其余留在集合里等待上浮。
#[derive(Debug, Default)]
pub struct BannerSet {
    banners: Vec<BannerRequest>,
}

impl BannerSet {
    /// 创建空集合。
    pub fn new() -> Self {
        Self::default()
    }

    /// 提交一条横幅；同类型会覆盖旧内容。
    pub fn show(&mut self, request: BannerRequest) {
        match self
            .banners
            .iter_mut()
            .find(|banner| banner.kind == request.kind)
        {
            Some(existing) => *existing = request,
            None => self.banners.push(request),
        }
    }

    /// 关闭指定类型的横幅。
    ///
    /// 不可关闭的类型（`Error`、`ReadOnly`）返回 `false` 且保持原样。
    pub fn dismiss(&mut self, kind: BannerKind) -> bool {
        if !kind.closable() {
            return false;
        }
        let before = self.banners.len();
        self.banners.retain(|banner| banner.kind != kind);
        before != self.banners.len()
    }

    /// 当前应当显示的一条：优先级最高的那条。
    pub fn visible(&self) -> Option<&BannerRequest> {
        self.banners
            .iter()
            .max_by_key(|banner| banner.kind.priority())
    }

    /// 清空所有横幅。
    pub fn clear(&mut self) {
        self.banners.clear();
    }

    /// 集合内的横幅数量（包含当前未显示的）。
    pub fn len(&self) -> usize {
        self.banners.len()
    }

    /// 集合里没有任何横幅。
    pub fn is_empty(&self) -> bool {
        self.banners.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: BannerKind) -> BannerRequest {
        BannerRequest::new(kind, "状态说明")
    }

    #[test]
    fn test_empty_set_has_no_banner() {
        let set = BannerSet::new();
        assert!(set.visible().is_none());
        assert!(set.is_empty());
    }

    #[test]
    fn test_priority_order_is_error_warning_info_read_only() {
        assert!(BannerKind::Error.priority() > BannerKind::Warning.priority());
        assert!(BannerKind::Warning.priority() > BannerKind::Info.priority());
        assert!(BannerKind::Info.priority() > BannerKind::ReadOnly.priority());
    }

    #[test]
    fn test_error_shadows_lower_kinds() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::ReadOnly));
        set.show(request(BannerKind::Info));
        set.show(request(BannerKind::Warning));
        set.show(BannerRequest::new(BannerKind::Error, "数据库文件异常"));

        assert_eq!(set.len(), 4);
        assert_eq!(
            set.visible().map(|banner| banner.kind),
            Some(BannerKind::Error)
        );
        assert_eq!(
            set.visible().map(|banner| banner.message.as_str()),
            Some("数据库文件异常")
        );
    }

    #[test]
    fn test_dismiss_top_reveals_next_in_priority_order() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::Info));
        set.show(request(BannerKind::Warning));

        assert_eq!(
            set.visible().map(|banner| banner.kind),
            Some(BannerKind::Warning)
        );

        assert!(set.dismiss(BannerKind::Warning));
        assert_eq!(
            set.visible().map(|banner| banner.kind),
            Some(BannerKind::Info)
        );
    }

    #[test]
    fn test_error_and_read_only_cannot_be_dismissed() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::Error));
        set.show(request(BannerKind::ReadOnly));

        assert!(!set.dismiss(BannerKind::Error));
        assert!(!set.dismiss(BannerKind::ReadOnly));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_closable_matrix() {
        assert!(!BannerKind::Error.closable());
        assert!(BannerKind::Warning.closable());
        assert!(BannerKind::Info.closable());
        assert!(!BannerKind::ReadOnly.closable());
    }

    #[test]
    fn test_same_kind_updates_message_in_place() {
        let mut set = BannerSet::new();
        set.show(BannerRequest::new(BannerKind::Warning, "磁盘空间不足"));
        set.show(BannerRequest::new(BannerKind::Warning, "磁盘空间即将耗尽"));

        assert_eq!(set.len(), 1);
        assert_eq!(
            set.visible().map(|banner| banner.message.as_str()),
            Some("磁盘空间即将耗尽")
        );
    }

    #[test]
    fn test_action_is_kept_on_visible_banner() {
        let mut set = BannerSet::new();
        set.show(
            BannerRequest::new(BannerKind::Error, "数据库文件异常，已切换到只读模式。")
                .with_action("打开数据管理", "navigate:设置"),
        );

        let banner = set.visible().expect("banner should be visible");
        assert_eq!(
            banner.action,
            Some(BannerAction {
                label: "打开数据管理".to_owned(),
                command: "navigate:设置".to_owned(),
            })
        );
    }

    #[test]
    fn test_banner_without_action_has_none() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::Info));
        assert!(set
            .visible()
            .and_then(|banner| banner.action.as_ref())
            .is_none());
    }

    #[test]
    fn test_clear_removes_everything() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::Error));
        set.show(request(BannerKind::Info));

        set.clear();

        assert!(set.is_empty());
        assert!(set.visible().is_none());
    }

    #[test]
    fn test_dismiss_unknown_kind_changes_nothing() {
        let mut set = BannerSet::new();
        set.show(request(BannerKind::Info));

        assert!(!set.dismiss(BannerKind::Warning));
        assert_eq!(set.len(), 1);
    }
}
