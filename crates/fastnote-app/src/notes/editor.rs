use fastnote_core::{validate_note_content, NoteCommand, NoteDetail, NoteError, NoteId};

pub(crate) enum SavePlan {
    Clean,
    Waiting,
    Refresh(NoteId),
    Invalid(NoteError),
    Submit(NoteCommand),
}

/// 只保留当前小记的保存基线与必要草稿，迟到确认不能覆盖后续输入。
pub(crate) struct NoteEditor {
    pub detail: NoteDetail,
    pub content: String,
    pub pending: Option<u64>,
    pub error: Option<String>,
    version_stale: bool,
}

impl NoteEditor {
    pub fn new(detail: NoteDetail) -> Self {
        Self {
            content: detail.note.content.clone(),
            detail,
            pending: None,
            error: None,
            version_stale: false,
        }
    }

    pub fn dirty(&self) -> bool {
        self.content != self.detail.note.content
    }

    pub fn protected(&self) -> bool {
        self.dirty() || self.pending.is_some() || self.error.is_some() || self.version_stale
    }

    pub fn edit(&mut self, content: String) {
        self.content = content;
        self.error = None;
    }

    /// 仅在没有写入进行时丢弃草稿，回到最后一次确认保存的版本。
    pub fn discard(&mut self) -> bool {
        if self.pending.is_some() || self.version_stale {
            return false;
        }
        self.content.clone_from(&self.detail.note.content);
        self.error = None;
        true
    }

    pub fn save_plan(&self) -> SavePlan {
        if self.pending.is_some() {
            return SavePlan::Waiting;
        }
        if self.version_stale {
            return SavePlan::Refresh(self.detail.note.id);
        }
        if !self.dirty() {
            return SavePlan::Clean;
        }
        if let Err(error) = validate_note_content(&self.content) {
            return SavePlan::Invalid(error);
        }
        SavePlan::Submit(NoteCommand::Update {
            id: self.detail.note.id,
            content: self.content.clone(),
            expected_updated_at: self.detail.note.updated_at,
        })
    }

    pub fn acknowledge(&mut self, request: u64, detail: NoteDetail) -> bool {
        if self.pending != Some(request) || self.detail.note.id != detail.note.id {
            return false;
        }
        self.pending = None;
        self.error = None;
        self.detail = detail;
        true
    }

    pub fn fail(&mut self, request: u64, message: String) {
        if self.pending == Some(request) {
            self.pending = None;
            self.error = Some(message);
        }
    }

    /// 移动/重命名改变存储版本后，刷新前不能用旧基线保存。
    pub fn invalidate_version(&mut self) {
        self.version_stale = true;
    }

    /// 仅更新当前文件的保存基线，不覆盖草稿或切换预览模式。
    pub fn refresh_metadata(&mut self, detail: NoteDetail) -> bool {
        if !self.version_stale
            || self.pending.is_some()
            || self.detail.note.id != detail.note.id
            || detail.note.updated_at < self.detail.note.updated_at
            || detail.note.content != self.detail.note.content
        {
            return false;
        }
        self.detail = detail;
        self.version_stale = false;
        self.error = None;
        true
    }

    #[cfg(test)]
    pub fn refresh(&mut self, detail: NoteDetail) {
        if self.protected() || self.detail.note.id != detail.note.id {
            return;
        }
        self.content = detail.note.content.clone();
        self.detail = detail;
    }

    #[cfg(test)]
    pub fn baseline_time(&self) -> fastnote_core::DateTime<fastnote_core::Utc> {
        self.detail.note.updated_at
    }
}

#[cfg(test)]
mod tests;
