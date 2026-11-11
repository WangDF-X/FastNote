use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{JournalCommandResult, JournalEntry, JournalError};
use fastnote_store::JournalStoreError;
use fastnote_ui::ToastKind;
use slint::TimerMode;

use super::super::runtime::Runtime;
use super::attachments::attachment_error_message;
use crate::journal::{JournalEditor, JournalReadResult};

impl Runtime {
    /// 先区分图片命令与正文保存，再只接受当前在途请求的成功确认。
    pub(in crate::desktop) fn journal_executed(
        self: &Rc<Self>,
        id: u64,
        result: Result<JournalCommandResult, JournalStoreError>,
    ) {
        let (is_add, is_remove) = {
            let session = self.journal.borrow();
            (
                session.add_request == Some(id),
                session
                    .remove_request
                    .as_ref()
                    .is_some_and(|(request, _)| *request == id),
            )
        };
        if is_add {
            self.journal_image_added(id, result);
            return;
        }
        if is_remove {
            self.journal_image_removed(id, result);
            return;
        }
        let accepted = match &result {
            Ok(result) => self
                .journal
                .borrow_mut()
                .editor
                .as_mut()
                .is_some_and(|editor| editor.acknowledge(id, result.entry.clone())),
            Err(_) => false,
        };
        if !accepted {
            let message = save_error_message(result.as_ref().err());
            let matched = self
                .journal
                .borrow_mut()
                .editor
                .as_mut()
                .is_some_and(|editor| {
                    if editor.pending != Some(id) {
                        return false;
                    }
                    editor.fail(id, message.to_owned());
                    true
                });
            if !matched {
                tracing::warn!(request_id = id, "unexpected journal command reply");
                return;
            }
            self.sync_journal();
            self.notice(ToastKind::Error, message);
            return;
        }
        self.sync_journal();
        self.refresh_journal_summaries();
        self.resume_journal_pending();
    }

    fn journal_image_added(
        self: &Rc<Self>,
        request: u64,
        result: Result<JournalCommandResult, JournalStoreError>,
    ) {
        if self.journal.borrow().add_request != Some(request) {
            return;
        }
        self.journal.borrow_mut().add_request = None;
        match result {
            Ok(result) => {
                self.adopt_journal_baseline(result.entry);
                self.refresh_journal_attachments();
                self.sync_journal();
            }
            Err(error) => self.notice(ToastKind::Error, attachment_error_message(&error)),
        }
        self.sync_journal_attachments();
        self.resume_journal_pending();
    }

    fn journal_image_removed(
        self: &Rc<Self>,
        request: u64,
        result: Result<JournalCommandResult, JournalStoreError>,
    ) {
        let matched = self
            .journal
            .borrow()
            .remove_request
            .as_ref()
            .is_some_and(|(pending, _)| *pending == request);
        if !matched {
            return;
        }
        self.journal.borrow_mut().remove_request = None;
        self.close_journal_viewer();
        match result {
            Ok(result) => {
                self.adopt_journal_baseline(result.entry);
                self.refresh_journal_attachments();
                self.sync_journal();
            }
            Err(error) => {
                self.notice(ToastKind::Error, attachment_error_message(&error));
                self.refresh_journal_attachments();
            }
        }
        self.sync_journal_attachments();
        self.resume_journal_pending();
    }

    /// 可见且属于当前请求的回包才可装载摘要、正文或图片。
    pub(in crate::desktop) fn journal_queried(
        self: &Rc<Self>,
        id: u64,
        result: Result<JournalReadResult, JournalStoreError>,
    ) {
        let (is_entry, is_list, is_attachments, is_viewer) = {
            let session = self.journal.borrow();
            (
                session.entry_request == id,
                session.list_request == id,
                session.attachment_request == id,
                session.viewer_request == id,
            )
        };
        if is_attachments {
            self.journal.borrow_mut().attachment_request = 0;
            if !self.journal_visible() {
                return;
            }
            match result {
                Ok(JournalReadResult::Attachments { date, views, usage }) => {
                    self.apply_journal_attachments(date, views, usage);
                }
                Err(_) => self.notice(ToastKind::Error, "图片读取失败，请重试。"),
                _ => {}
            }
            return;
        }
        if is_viewer {
            self.journal.borrow_mut().viewer_request = 0;
            if !self.journal_visible() {
                return;
            }
            match result {
                Ok(JournalReadResult::AttachmentView(view)) => {
                    self.apply_journal_viewer(view);
                }
                _ => {
                    self.notice(ToastKind::Error, "图片读取失败，请重试。");
                    self.close_journal_viewer();
                }
            }
            return;
        }
        if is_entry {
            self.journal.borrow_mut().entry_request = 0;
            let pending = self.journal.borrow_mut().pending_date.take();
            if !self.journal_visible() {
                return;
            }
            match result {
                Ok(JournalReadResult::Entry { date, entry }) if Some(date) == pending => {
                    // 保护中的草稿不能被迟到的正文覆盖。
                    if self.journal_protected() {
                        return;
                    }
                    self.journal.borrow_mut().editor =
                        Some(JournalEditor::new(date, entry.map(|entry| *entry)));
                    self.journal.borrow_mut().preview = false;
                    self.sync_journal();
                    self.request_journal_attachments(date);
                }
                Err(_) => self.notice(ToastKind::Error, "日记读取失败，请重试。"),
                _ => {}
            }
            return;
        }
        if is_list {
            self.journal.borrow_mut().list_request = 0;
            let pending = self.journal.borrow_mut().pending_range.take();
            if !self.journal_visible() {
                return;
            }
            match (result, pending) {
                (Ok(JournalReadResult::Summaries(rows)), Some(range)) => {
                    {
                        let mut session = self.journal.borrow_mut();
                        session.rows = rows;
                        session.loaded_range = Some(range);
                    }
                    self.render_journal_rows();
                }
                (Err(_), _) => self.notice(ToastKind::Error, "日记读取失败，请重试。"),
                _ => {}
            }
        }
    }

    /// 图片增删或保存成功后，按顺序继续未完成的添加/删除/导航。
    pub(super) fn resume_journal_pending(self: &Rc<Self>) {
        let pending_add = self.journal.borrow_mut().pending_add.take();
        if let Some(path) = pending_add {
            let date = self.journal.borrow().editor_date();
            if let Some(date) = date {
                self.send_journal_add(date, path);
                return;
            }
        }
        let pending_remove = self.journal.borrow_mut().pending_remove.take();
        if let Some(id) = pending_remove {
            self.send_journal_remove(id);
            return;
        }
        if self.journal_protected() {
            self.schedule_journal_flush();
            return;
        }
        let action = self.journal.borrow_mut().after_save.take();
        if let Some(action) = action {
            self.resume_journal_action(action);
        }
    }

    pub(super) fn schedule_journal_flush(self: &Rc<Self>) {
        if self.journal_save_timer.running() {
            self.journal_save_timer.restart();
            return;
        }
        let weak = Rc::downgrade(self);
        self.journal_save_timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(500),
            move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.flush_journal();
                }
            },
        );
    }

    fn refresh_journal_attachments(&self) {
        let date = self.journal.borrow().editor_date();
        if let Some(date) = date {
            self.request_journal_attachments(date);
        }
        self.refresh_journal_summaries();
    }

    fn adopt_journal_baseline(&self, entry: Option<JournalEntry>) {
        if let Some(editor) = self.journal.borrow_mut().editor.as_mut() {
            editor.adopt_baseline(entry);
        }
    }
}

fn save_error_message(error: Option<&JournalStoreError>) -> &'static str {
    match error.and_then(JournalStoreError::domain_error) {
        Some(JournalError::TooLong) => "日记不能超过 10,000 字符，内容已保留。",
        Some(JournalError::Conflict) => "该日期的日记已在别处更新，当前草稿已保留。",
        _ => "日记保存失败，内容已保留。",
    }
}
