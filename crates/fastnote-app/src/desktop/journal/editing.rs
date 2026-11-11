use std::rc::Rc;
use std::time::Duration;

use chrono::Datelike;
use fastnote_core::{JournalError, NaiveDate};
use slint::{Timer, TimerMode};

use super::super::runtime::Runtime;
use super::{journal_access, month_start, range_for, shift_month, JournalAccess, JournalView};
use crate::journal::{JournalEditor, SavePlan};

impl Runtime {
    /// 用户输入后投影草稿，并在停止输入 500ms 后写入（D-16 / DIARY-01 §5）。
    pub(super) fn edit_journal(self: &Rc<Self>, content: String) {
        if self.journal_syncing.get() || !self.journal_visible() {
            return;
        }
        {
            let mut session = self.journal.borrow_mut();
            let Some(editor) = session.editor.as_mut() else {
                return;
            };
            editor.edit(content);
        }
        self.sync_journal();
        if self.journal_save_timer.running() {
            self.journal_save_timer.restart();
        } else {
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
    }

    /// 冲刷当前草稿；返回 true 仅表示没有需要提交的内容。
    pub(in crate::desktop) fn flush_journal(&self) -> bool {
        self.journal_save_timer.stop();
        if self.journal_syncing.get() {
            return false;
        }
        let (plan, entry_date, has_baseline) = {
            let session = self.journal.borrow();
            let editor = session.editor.as_ref();
            (
                editor.map(JournalEditor::save_plan),
                editor.map(|editor| editor.entry_date),
                editor.map(|editor| editor.baseline.is_some()),
            )
        };
        match plan {
            None | Some(SavePlan::Clean) => true,
            Some(SavePlan::Waiting) => false,
            Some(SavePlan::Invalid(error)) => {
                let message = if error == JournalError::TooLong {
                    "日记不能超过 10,000 字符"
                } else {
                    "日记内容无效，请修改后重试。"
                };
                if let Some(editor) = self.journal.borrow_mut().editor.as_mut() {
                    editor.error = Some(message.to_owned());
                }
                self.sync_journal();
                false
            }
            Some(SavePlan::Submit(command)) => {
                // 过去条目被清空删除后，基线为 None；不能借草稿重新创建过去日记。
                if entry_date.is_some_and(|date| date < self.local_today())
                    && has_baseline == Some(false)
                {
                    if let Some(editor) = self.journal.borrow_mut().editor.as_mut() {
                        editor.error = Some("过去日期不能补写日记。".to_owned());
                    }
                    self.sync_journal();
                    return false;
                }
                let id = self.next_id.get();
                self.next_id.set(id.saturating_add(1));
                if self.worker.journal_execute(id, command).is_ok() {
                    if let Some(editor) = self.journal.borrow_mut().editor.as_mut() {
                        editor.error = None;
                        editor.pending = Some(id);
                    }
                } else if let Some(editor) = self.journal.borrow_mut().editor.as_mut() {
                    editor.error = Some("保存队列不可用，编辑内容已保留。".to_owned());
                }
                self.sync_journal();
                false
            }
        }
    }

    /// 打开某日的编辑器；只有今天可新建，过去只能编辑已有日记。
    pub(super) fn open_journal(self: &Rc<Self>, date: NaiveDate) {
        if !self.journal_visible() {
            return;
        }
        let today = self.local_today();
        let has_entry = self
            .journal
            .borrow()
            .rows
            .iter()
            .any(|summary| summary.entry_date == date);
        match journal_access(date, today, has_entry) {
            JournalAccess::Editable => {}
            JournalAccess::Future => {
                self.notice(fastnote_ui::ToastKind::Info, "还没到这一天，不能写日记。");
                return;
            }
            JournalAccess::MissingPast => {
                self.notice(fastnote_ui::ToastKind::Info, "过去日期没有日记，不能补写。");
                return;
            }
        }
        let already_open = self.journal.borrow().editor_date() == Some(date);
        if already_open {
            self.journal.borrow_mut().selected_date = Some(date);
            self.sync_journal();
            return;
        }
        if self.defer_journal_transition(format!("journal-date:{date}")) {
            return;
        }
        self.controller.borrow_mut().retain_journal_date(date);
        self.clear_journal_attachments();
        self.prepare_journal_date(date);
        self.request_journal_entry(date);
    }

    /// 按天移动；在编辑中会先保存再切换日期。
    pub(super) fn journal_navigate(self: &Rc<Self>, delta: i32) {
        let current = {
            let session = self.journal.borrow();
            session.editor_date().or(session.selected_date)
        };
        let date = current.unwrap_or_else(|| self.local_today())
            + chrono::Duration::days(i64::from(delta));
        self.open_journal(date);
    }

    /// T 键 / 今天按钮：回到本地今日并打开编辑器。
    pub(super) fn journal_today(self: &Rc<Self>) {
        self.open_journal(self.local_today());
    }

    /// 返回列表；有未保存内容时先保存，失败则留在编辑器。
    pub(super) fn close_journal(self: &Rc<Self>) {
        if self.defer_journal_transition("journal-close".to_owned()) {
            return;
        }
        {
            let mut session = self.journal.borrow_mut();
            session.editor = None;
            session.preview = false;
        }
        self.clear_journal_attachments();
        self.sync_journal();
        self.render_journal_rows();
    }

    /// 同一内容区切换编辑/预览；失败保存会阻止进入预览。
    pub(super) fn toggle_journal_preview(self: &Rc<Self>) {
        if self.journal.borrow().editor.is_none() {
            return;
        }
        if self.journal.borrow().preview {
            self.journal.borrow_mut().preview = false;
            self.sync_journal();
            return;
        }
        if self.defer_journal_transition("journal-preview".to_owned()) {
            return;
        }
        self.journal.borrow_mut().preview = true;
        self.sync_journal();
    }

    /// 按当前视图翻页：月视图保留日号翻月，周视图平移 7 天。
    pub(super) fn shift_journal_range(self: &Rc<Self>, delta: i32) {
        if self.journal_protected() || self.journal.borrow().editor.is_some() {
            return;
        }
        let week_start = self.settings.borrow().week_start;
        let (view, base) = {
            let session = self.journal.borrow();
            (
                session.view,
                session.selected_date.unwrap_or_else(|| self.local_today()),
            )
        };
        let next = match view {
            JournalView::Month => shift_month(month_start(base), base.day(), delta),
            JournalView::Week => {
                let Some(next) =
                    base.checked_add_signed(chrono::Duration::days(i64::from(delta) * 7))
                else {
                    return;
                };
                next
            }
        };
        if !(1900..=9999).contains(&next.year()) {
            return;
        }
        self.controller.borrow_mut().retain_journal_date(next);
        {
            let mut session = self.journal.borrow_mut();
            session.selected_date = Some(next);
            session.loaded_range = None;
            session.pending_range = None;
            session.rows.clear();
        }
        if let Some(range) = range_for(view, next, week_start) {
            self.request_journal_range(range);
        }
        self.render_journal_rows();
        self.sync_journal();
    }

    /// 在月视图与周视图之间切换，保留当前锚点日期。
    pub(super) fn set_journal_view(self: &Rc<Self>, view: JournalView) {
        if self.journal_protected() || self.journal.borrow().editor.is_some() {
            return;
        }
        if self.journal.borrow().view == view {
            return;
        }
        let week_start = self.settings.borrow().week_start;
        let base = {
            let session = self.journal.borrow();
            session.selected_date.unwrap_or_else(|| self.local_today())
        };
        {
            let mut session = self.journal.borrow_mut();
            session.view = view;
            session.selected_date = Some(base);
            session.loaded_range = None;
            session.pending_range = None;
            session.rows.clear();
        }
        if let Some(range) = range_for(view, base, week_start) {
            self.request_journal_range(range);
        }
        self.render_journal_rows();
        self.sync_journal();
    }

    /// 失败提示旁的「重试」：只重发当前草稿。
    pub(super) fn retry_journal(self: &Rc<Self>) {
        self.flush_journal();
    }

    /// 草稿成功保存前保留后续动作，失败时不丢弃编辑内容。
    pub(in crate::desktop) fn defer_journal_transition(self: &Rc<Self>, action: String) -> bool {
        if !self.journal_protected() {
            return false;
        }
        self.journal.borrow_mut().after_save = Some(action);
        if self.flush_journal() {
            self.journal.borrow_mut().after_save = None;
            return false;
        }
        true
    }

    pub(super) fn resume_journal_action(self: &Rc<Self>, action: String) {
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                if action == "journal-preview" {
                    runtime.journal.borrow_mut().preview = true;
                    runtime.sync_journal();
                } else if action == "journal-close" {
                    runtime.close_journal();
                } else if let Some(date) = action.strip_prefix("journal-date:") {
                    if let Ok(date) = date.parse() {
                        runtime.open_journal(date);
                    }
                } else {
                    runtime.dispatch_action(action);
                }
            }
        });
    }

    /// 切换日期前同步列表区间，重复请求只保留最后一次。
    fn prepare_journal_date(&self, date: NaiveDate) {
        let week_start = self.settings.borrow().week_start;
        let (needs_load, range) = {
            let mut session = self.journal.borrow_mut();
            session.selected_date = Some(date);
            let range = range_for(session.view, date, week_start);
            (range.is_some_and(|range| session.needs_load(range)), range)
        };
        if needs_load {
            if let Some(range) = range {
                {
                    let mut session = self.journal.borrow_mut();
                    session.loaded_range = None;
                    session.rows.clear();
                }
                self.request_journal_range(range);
            }
        }
    }

    fn local_today(&self) -> NaiveDate {
        self.observe_task_clock().sample.local.date()
    }
}
