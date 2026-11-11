//! DIARY-01：列表只读当前月份摘要，单日正文按需加载，500ms 防抖保存。

mod attachments;
mod connect;
mod editing;
mod render;
mod replies;

use std::path::PathBuf;
use std::rc::Rc;

use chrono::{Datelike, NaiveDate};
use fastnote_core::derive::calendar::{calendar_week_days, WeekStart};
use fastnote_core::{JournalAttachment, JournalImageUsage, JournalSummary};
use slint::ModelRc;

use crate::journal::{JournalEditor, JournalRead};
use crate::routing::Route;

use super::runtime::Runtime;

/// 日记列表的两种视图；默认月视图。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum JournalView {
    /// 整个自然月的日记列表。
    #[default]
    Month,
    /// 当前周的 7 天列表，包含没有日记的日期。
    Week,
}

/// 列表当前应展示的闭区间；用于判断是否需要重新查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct JournalRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// 某日是否允许进入编辑器；未来不可写，过去只能编辑已有日记。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JournalAccess {
    /// 今天，或过去已有日记。
    Editable,
    /// 还没到这一天。
    Future,
    /// 过去没有日记，不能补写。
    MissingPast,
}

/// 判断日记写入权限；创建只允许今天，编辑允许今天及以前已有条目。
pub(super) fn journal_access(date: NaiveDate, today: NaiveDate, has_entry: bool) -> JournalAccess {
    if date > today {
        JournalAccess::Future
    } else if date < today && !has_entry {
        JournalAccess::MissingPast
    } else {
        JournalAccess::Editable
    }
}

/// 日记页面的轻量会话：视图、摘要、单日草稿与图片查看器。
#[derive(Default)]
pub(super) struct JournalSession {
    /// 当前打开的单日草稿；None 表示列表。
    pub editor: Option<JournalEditor>,
    /// 列表当前选中的日期，也是切换视图时的锚点。
    pub selected_date: Option<NaiveDate>,
    /// 月视图或周视图。
    pub view: JournalView,
    /// 已加载的摘要（按日期倒序）。
    pub rows: Vec<JournalSummary>,
    pub list_request: u64,
    /// 在途查询对应的区间；回包据此写入 `loaded_range`。
    pub pending_range: Option<JournalRange>,
    /// 已成功装载的区间；与目标区间不一致时才重新查询。
    pub loaded_range: Option<JournalRange>,
    pub entry_request: u64,
    pub pending_date: Option<NaiveDate>,
    /// 保存成功后才继续的导航/预览动作。
    pub after_save: Option<String>,
    pub preview: bool,
    /// 当前日期的附件元数据。
    pub attachments: Vec<JournalAttachment>,
    /// 已解码的缩略图，直接投影到 UI；关闭页面时释放。
    pub attachment_views: Vec<AttachmentTile>,
    pub attachment_request: u64,
    /// 已成功加载过缩略图的日期，避免隐藏后重开重复解码。
    pub attachments_loaded_for: Option<NaiveDate>,
    pub add_request: Option<u64>,
    pub remove_request: Option<(u64, String)>,
    /// 全部日记图片占用。
    pub usage: JournalImageUsage,
    /// 尚未提交的添加/删除，等待保存成功后继续。
    pub pending_add: Option<PathBuf>,
    pub pending_remove: Option<String>,
    /// 超过 5MB 等待用户确认的图片。
    pub pending_image: Option<PendingImage>,
    pub viewer: Option<JournalViewer>,
    pub viewer_request: u64,
    pub viewer_image: Option<slint::Image>,
}

/// 一张已解码的缩略图，供 Slint 模型使用。
pub(super) struct AttachmentTile {
    pub id: String,
    pub image: slint::Image,
    pub label: String,
}

/// 超过 5MB 的图片在入库前需要确认。
pub(super) struct PendingImage {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

/// 应用内图片查看器；只保留一张大图。
pub(super) struct JournalViewer {
    pub index: usize,
    pub loading: bool,
}

impl JournalSession {
    /// 存在未保存、在途或失败的内容。
    pub(super) fn protected(&self) -> bool {
        self.editor.as_ref().is_some_and(JournalEditor::protected)
    }

    pub(super) fn editor_date(&self) -> Option<NaiveDate> {
        self.editor.as_ref().map(|editor| editor.entry_date)
    }

    /// 目标区间既不是已装载的，也不在途时才需要新查询。
    pub(super) fn needs_load(&self, range: JournalRange) -> bool {
        self.loaded_range != Some(range) && self.pending_range != Some(range)
    }
}

impl Runtime {
    /// 页面、内容区与外壳路由全部就绪，迟到回包才能投影。
    pub(super) fn journal_visible(&self) -> bool {
        self.main_content_visible()
            && self.controller.borrow().shell_view().page_label == "日记"
            && self
                .with_ui(|ui| ui.shell.component().get_current_page() == "日记")
                .unwrap_or(false)
    }

    /// 首次显示/重新进入时只装载当前视图区间；正文由用户点击后按需读取。
    pub(in crate::desktop) fn load_journal(self: &Rc<Self>) {
        if !self.ready.get() || self.finished.get() || !self.journal_visible() {
            return;
        }
        let route_date = match self.controller.borrow().route() {
            Route::Journal { date } => date,
            _ => return,
        };
        let week_start = self.settings.borrow().week_start;
        let protected = self.journal_protected();
        let (range, needs_load) = {
            let mut session = self.journal.borrow_mut();
            if !protected || session.editor.is_none() {
                session.selected_date = Some(route_date);
            } else {
                session.selected_date = session.editor_date();
            }
            let selected = session.selected_date.unwrap_or(route_date);
            let range = range_for(session.view, selected, week_start);
            (range, range.is_some_and(|range| session.needs_load(range)))
        };
        if let (true, Some(range)) = (needs_load, range) {
            {
                let mut session = self.journal.borrow_mut();
                session.loaded_range = None;
                session.rows.clear();
            }
            self.request_journal_range(range);
        }
        self.render_journal_rows();
        self.sync_journal();
        self.sync_journal_attachments();
        self.sync_journal_viewer();
        let missing_attachments = {
            let session = self.journal.borrow();
            session.editor.is_some()
                && session.attachment_request == 0
                && session.attachments_loaded_for != session.editor_date()
        };
        if missing_attachments {
            let date = self.journal.borrow().editor_date();
            if let Some(date) = date {
                self.request_journal_attachments(date);
            }
        }
    }

    fn request_journal_range(&self, range: JournalRange) {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .journal_query(
                id,
                JournalRead::Summaries {
                    start: range.start,
                    end: range.end,
                },
            )
            .is_err()
        {
            self.notice(
                fastnote_ui::ToastKind::Error,
                "日记读取队列不可用，请稍后重试。",
            );
            return;
        }
        let mut session = self.journal.borrow_mut();
        session.list_request = id;
        session.pending_range = Some(range);
    }

    /// 重读当前视图区间；保存或图片增删后刷新摘要与图片数。
    pub(super) fn refresh_journal_summaries(&self) {
        let week_start = self.settings.borrow().week_start;
        let (view, selected) = {
            let session = self.journal.borrow();
            (session.view, session.selected_date)
        };
        if let Some(range) = selected.and_then(|date| range_for(view, date, week_start)) {
            self.request_journal_range(range);
        }
    }

    fn request_journal_entry(&self, date: NaiveDate) {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .journal_query(id, JournalRead::Entry(date))
            .is_err()
        {
            self.notice(
                fastnote_ui::ToastKind::Error,
                "日记读取队列不可用，请稍后重试。",
            );
            return;
        }
        let mut session = self.journal.borrow_mut();
        session.entry_request = id;
        session.pending_date = Some(date);
    }

    /// 未保存/失败/在途的草稿不能被隐藏、回收或路由切换丢弃。
    pub(in crate::desktop) fn journal_protected(&self) -> bool {
        self.journal.borrow().protected()
    }

    /// 离开页面时先尝试保存；保护中的草稿保留在会话里等迟到确认。
    pub(in crate::desktop) fn release_journal(&self) {
        self.journal_save_timer.stop();
        if self.journal_protected() {
            self.flush_journal();
        }
        if self.journal_protected() {
            return;
        }
        {
            let mut session = self.journal.borrow_mut();
            session.editor = None;
            session.rows.clear();
            session.selected_date = None;
            session.list_request = 0;
            session.pending_range = None;
            session.loaded_range = None;
            session.entry_request = 0;
            session.pending_date = None;
            session.after_save = None;
            session.preview = false;
            session.attachments.clear();
            session.attachment_views.clear();
            session.attachment_request = 0;
            session.attachments_loaded_for = None;
            session.add_request = None;
            session.remove_request = None;
            session.usage = JournalImageUsage::default();
            session.pending_add = None;
            session.pending_remove = None;
            session.pending_image = None;
            session.viewer = None;
            session.viewer_request = 0;
            session.viewer_image = None;
        }
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_journal_rows(ModelRc::default());
            window.set_journal_editing(false);
            window.set_journal_content("".into());
            window.set_journal_error("".into());
            window.set_journal_save_label("".into());
            window.set_journal_preview_mode(false);
            window.set_journal_preview_blocks(Default::default());
            window.set_journal_attachments(Default::default());
            window.set_journal_image_usage("".into());
            window.set_journal_add_image_busy(false);
            window.set_journal_pending_image(false);
            window.set_journal_pending_image_label("".into());
            window.set_journal_viewer_visible(false);
            window.set_journal_viewer_image(Default::default());
            window.set_journal_viewer_title("".into());
            window.set_journal_viewer_loading(false);
        });
    }
}

pub(super) fn month_start(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

/// 当前视图与锚点对应的查询区间；周视图跟随设置的一周起始日。
pub(super) fn range_for(
    view: JournalView,
    anchor: NaiveDate,
    week_start: WeekStart,
) -> Option<JournalRange> {
    match view {
        JournalView::Month => {
            let (start, end) = month_bounds(anchor);
            Some(JournalRange { start, end })
        }
        JournalView::Week => {
            let days = calendar_week_days(anchor, week_start)?;
            Some(JournalRange {
                start: *days.first()?,
                end: *days.last()?,
            })
        }
    }
}

/// 闭区间：月初到月末，供仓储只读取当前月份。
pub(super) fn month_bounds(month: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = month_start(month);
    let (year, next) = if start.month() == 12 {
        (start.year() + 1, 1)
    } else {
        (start.year(), start.month() + 1)
    };
    let next_start = NaiveDate::from_ymd_opt(year, next, 1).unwrap_or(start);
    let end = next_start - chrono::Duration::days(1);
    (start, end)
}

/// 在月份之间移动时保留尽量接近的日号，月末不足时取当月最后一天。
pub(super) fn shift_month(month: NaiveDate, day: u32, delta: i32) -> NaiveDate {
    let index = month.year() * 12 + month.month0() as i32 + delta;
    let year = index.div_euclid(12);
    let month_number = index.rem_euclid(12) as u32 + 1;
    let first = NaiveDate::from_ymd_opt(year, month_number, 1).unwrap_or(month);
    let (_, last) = month_bounds(first);
    first.with_day(day.min(last.day())).unwrap_or(first)
}

#[cfg(test)]
mod tests;
