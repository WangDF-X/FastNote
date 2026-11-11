use fastnote_core::{
    validate_journal_content, JournalCommand, JournalEntry, JournalError, NaiveDate,
};

/// 一次冲刷的决策；只有 Submit 会进入 DB 工作线程。
pub(crate) enum SavePlan {
    /// 与基线一致。
    Clean,
    /// 已有保存在途。
    Waiting,
    /// 内容不合法，不能提交。
    Invalid(JournalError),
    /// 需要提交的命令。
    Submit(JournalCommand),
}

/// 只保留当前日期的保存基线与草稿，迟到确认不能覆盖后续输入。
pub(crate) struct JournalEditor {
    /// 当前编辑的本地日期。
    pub entry_date: NaiveDate,
    /// 当前草稿（Markdown 源码）。
    pub content: String,
    /// 最后一次确认保存的版本；从未落库时为 None。
    pub baseline: Option<JournalEntry>,
    /// 在途保存请求。
    pub pending: Option<u64>,
    /// 可读的失败提示。
    pub error: Option<String>,
}

impl JournalEditor {
    /// 从路由日期和可选基线建立草稿。
    pub fn new(entry_date: NaiveDate, baseline: Option<JournalEntry>) -> Self {
        let content = baseline
            .as_ref()
            .map(|entry| entry.content.clone())
            .unwrap_or_default();
        Self {
            entry_date,
            content,
            baseline,
            pending: None,
            error: None,
        }
    }

    /// 草稿与基线不一致；新建空白不算脏，避免空正文落库。
    pub fn dirty(&self) -> bool {
        match &self.baseline {
            Some(entry) => self.content != entry.content,
            None => !self.content.trim().is_empty(),
        }
    }

    /// 存在未保存、在途或失败的内容时不能切换日期/离开页面。
    pub fn protected(&self) -> bool {
        self.dirty() || self.pending.is_some() || self.error.is_some()
    }

    /// 记录用户输入；输入即清除上一次失败提示。
    pub fn edit(&mut self, content: String) {
        self.content = content;
        self.error = None;
    }

    /// 计算这一次冲刷应该做什么。
    pub fn save_plan(&self) -> SavePlan {
        if self.pending.is_some() {
            return SavePlan::Waiting;
        }
        if !self.dirty() {
            return SavePlan::Clean;
        }
        if let Err(error) = validate_journal_content(&self.content) {
            return SavePlan::Invalid(error);
        }
        SavePlan::Submit(JournalCommand::Save {
            entry_date: self.entry_date,
            content: self.content.clone(),
            expected_updated_at: self.baseline.as_ref().map(|entry| entry.updated_at),
        })
    }

    /// 只接受当前在途请求的确认；空白正文删除条目时基线回到 None。
    pub fn acknowledge(&mut self, request: u64, entry: Option<JournalEntry>) -> bool {
        if self.pending != Some(request) {
            return false;
        }
        self.pending = None;
        self.error = None;
        self.baseline = entry;
        true
    }

    /// 失败时保留草稿，只记录可读提示。
    pub fn fail(&mut self, request: u64, message: String) {
        if self.pending == Some(request) {
            self.pending = None;
            self.error = Some(message);
        }
    }

    /// 附件增删后同步保存基线；添加图片不改变正文，可安全刷新版本。
    pub fn adopt_baseline(&mut self, entry: Option<JournalEntry>) -> bool {
        match entry {
            Some(entry) if entry.entry_date == self.entry_date => {
                self.baseline = Some(entry);
                true
            }
            Some(_) => false,
            None => {
                self.baseline = None;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
    }

    fn baseline(content: &str) -> JournalEntry {
        JournalEntry::new(
            date(),
            content.to_owned(),
            "2026-10-23T06:32:00Z".parse().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn test_blank_new_entry_is_not_dirty_and_never_submits() {
        let mut editor = JournalEditor::new(date(), None);
        assert!(!editor.dirty());
        assert!(!editor.protected());
        assert!(matches!(editor.save_plan(), SavePlan::Clean));
        editor.edit("  \n\t".into());
        assert!(!editor.dirty());
        assert!(matches!(editor.save_plan(), SavePlan::Clean));
    }

    #[test]
    fn test_edit_marks_dirty_and_submits_without_baseline() {
        let mut editor = JournalEditor::new(date(), None);
        editor.edit("今天完成了报告。".into());
        assert!(editor.dirty());
        assert!(editor.protected());
        match editor.save_plan() {
            SavePlan::Submit(JournalCommand::Save {
                entry_date,
                expected_updated_at,
                content,
            }) => {
                assert_eq!(entry_date, date());
                assert_eq!(expected_updated_at, None);
                assert_eq!(content, "今天完成了报告。");
            }
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn test_save_uses_baseline_version_for_existing_entry() {
        let entry = baseline("第一版");
        let mut editor = JournalEditor::new(date(), Some(entry.clone()));
        editor.edit("第二版".into());
        match editor.save_plan() {
            SavePlan::Submit(JournalCommand::Save {
                expected_updated_at,
                ..
            }) => assert_eq!(expected_updated_at, Some(entry.updated_at)),
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn test_acknowledge_requires_matching_request_and_clears_error() {
        let mut editor = JournalEditor::new(date(), None);
        editor.edit("内容".into());
        editor.pending = Some(7);
        editor.error = Some("旧错误".into());
        assert!(!editor.acknowledge(6, None));
        assert_eq!(editor.pending, Some(7));
        assert!(editor.acknowledge(7, None));
        assert_eq!(editor.pending, None);
        assert_eq!(editor.error, None);
        assert_eq!(editor.baseline, None);
    }

    #[test]
    fn test_blank_save_acknowledged_as_deleted_and_not_dirty() {
        let entry = baseline("旧内容");
        let mut editor = JournalEditor::new(date(), Some(entry));
        editor.edit("".into());
        editor.pending = Some(3);
        assert!(editor.acknowledge(3, None));
        assert!(!editor.dirty());
        assert!(!editor.protected());
    }

    #[test]
    fn test_failure_is_only_applied_to_current_request_and_keeps_draft() {
        let mut editor = JournalEditor::new(date(), None);
        editor.edit("草稿".into());
        editor.pending = Some(9);
        editor.fail(8, "迟到失败".into());
        assert_eq!(editor.error, None);
        editor.fail(9, "保存失败".into());
        assert_eq!(editor.content, "草稿");
        assert_eq!(editor.error.as_deref(), Some("保存失败"));
    }

    #[test]
    fn test_too_long_content_is_invalid() {
        let mut editor = JournalEditor::new(date(), None);
        editor.edit("长".repeat(fastnote_core::MAX_JOURNAL_LENGTH + 1));
        assert!(matches!(
            editor.save_plan(),
            SavePlan::Invalid(JournalError::TooLong)
        ));
    }
}
