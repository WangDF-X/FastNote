use std::path::PathBuf;
use std::rc::Rc;

use chrono::Datelike;

use fastnote_core::{JournalCommand, JournalImageUsage, NaiveDate, MAX_JOURNAL_ATTACHMENT_BYTES};
use fastnote_store::JournalStoreError;
use fastnote_ui::{JournalAttachmentView, ToastKind};
use slint::{ModelRc, SharedPixelBuffer, VecModel};

use super::super::runtime::Runtime;
use super::{AttachmentTile, JournalViewer, PendingImage};
use crate::journal::{AttachmentView, JournalRead};

/// 超过该大小的图片在入库前需要用户确认。
const LARGE_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

impl Runtime {
    /// 打开系统文件选择器；取消时不改变任何状态。
    pub(super) fn pick_journal_image(self: &Rc<Self>) {
        if !self.journal_visible() || self.journal.borrow().add_request.is_some() {
            return;
        }
        let Some(path) = fastnote_platform::pick_image_file(self.main_native.get()) else {
            return;
        };
        let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if size > MAX_JOURNAL_ATTACHMENT_BYTES as u64 {
            self.notice(ToastKind::Error, "图片不能超过 10MB。");
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("图片")
            .to_owned();
        if size > LARGE_IMAGE_BYTES {
            self.journal.borrow_mut().pending_image = Some(PendingImage { path, name, size });
            self.sync_journal_attachments();
            return;
        }
        self.queue_journal_image(path);
    }

    pub(super) fn confirm_journal_image(self: &Rc<Self>) {
        let pending = self.journal.borrow_mut().pending_image.take();
        self.sync_journal_attachments();
        if let Some(pending) = pending {
            self.queue_journal_image(pending.path);
        }
    }

    pub(super) fn cancel_journal_image(&self) {
        self.journal.borrow_mut().pending_image = None;
        self.sync_journal_attachments();
    }

    /// 有未保存正文时先保存，成功后再添加图片。
    fn queue_journal_image(self: &Rc<Self>, path: PathBuf) {
        if !self.journal_visible() {
            return;
        }
        let date = self.journal.borrow().editor_date();
        let Some(date) = date else {
            return;
        };
        if self.journal_protected() {
            self.journal.borrow_mut().pending_add = Some(path);
            if self.flush_journal() {
                let pending = self.journal.borrow_mut().pending_add.take();
                let date = self.journal.borrow().editor_date();
                if let (Some(path), Some(date)) = (pending, date) {
                    self.send_journal_add(date, path);
                }
            }
            return;
        }
        self.send_journal_add(date, path);
    }

    pub(super) fn send_journal_add(self: &Rc<Self>, date: NaiveDate, path: PathBuf) {
        if self.journal.borrow().add_request.is_some() {
            return;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self.worker.journal_add_image(id, date, path).is_ok() {
            self.journal.borrow_mut().add_request = Some(id);
        } else {
            self.notice(ToastKind::Error, "图片处理队列不可用，请稍后重试。");
        }
        self.sync_journal_attachments();
    }

    pub(super) fn remove_journal_image(self: &Rc<Self>, id: String) {
        if self.journal.borrow().remove_request.is_some() {
            return;
        }
        if self.journal_protected() {
            self.journal.borrow_mut().pending_remove = Some(id);
            if self.flush_journal() {
                let pending = self.journal.borrow_mut().pending_remove.take();
                if let Some(id) = pending {
                    self.send_journal_remove(id);
                }
            }
            return;
        }
        self.send_journal_remove(id);
    }

    pub(super) fn send_journal_remove(self: &Rc<Self>, id: String) {
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        if self
            .worker
            .journal_execute(request, JournalCommand::RemoveAttachment { id: id.clone() })
            .is_ok()
        {
            self.journal.borrow_mut().remove_request = Some((request, id));
        } else {
            self.notice(ToastKind::Error, "图片删除队列不可用，请稍后重试。");
        }
        self.sync_journal_attachments();
    }

    /// 单日附件与全局占用在同一快照中读取。
    pub(in crate::desktop) fn request_journal_attachments(&self, date: NaiveDate) {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .journal_query(id, JournalRead::Attachments(date))
            .is_err()
        {
            self.notice(ToastKind::Error, "图片读取队列不可用，请稍后重试。");
            return;
        }
        self.journal.borrow_mut().attachment_request = id;
        self.journal.borrow_mut().attachments_loaded_for = None;
    }

    pub(in crate::desktop) fn apply_journal_attachments(
        &self,
        date: NaiveDate,
        views: Vec<AttachmentView>,
        usage: JournalImageUsage,
    ) {
        let mut attachments = Vec::with_capacity(views.len());
        let mut tiles = Vec::with_capacity(views.len());
        for view in views {
            let label = format!(
                "{}×{} · {}",
                view.attachment.width,
                view.attachment.height,
                format_bytes(
                    u64::from(view.attachment.byte_size)
                        + u64::from(view.attachment.thumbnail_size)
                )
            );
            tiles.push(AttachmentTile {
                id: view.attachment.id.clone(),
                image: image_from_rgba(&view.rgba, view.width, view.height),
                label,
            });
            attachments.push(view.attachment);
        }
        {
            let mut session = self.journal.borrow_mut();
            session.attachments = attachments;
            session.attachment_views = tiles;
            session.usage = usage;
            session.attachments_loaded_for = Some(date);
        }
        self.sync_journal_attachments();
        self.sync_journal_viewer();
    }

    pub(super) fn open_journal_viewer(self: &Rc<Self>, index: i32) {
        let count = self.journal.borrow().attachments.len();
        if count == 0 {
            return;
        }
        let index = usize::try_from(index).unwrap_or(0).min(count - 1);
        self.journal.borrow_mut().viewer = Some(JournalViewer {
            index,
            loading: true,
        });
        self.journal.borrow_mut().viewer_image = None;
        self.sync_journal_viewer();
        self.load_journal_viewer_image();
    }

    pub(super) fn step_journal_viewer(self: &Rc<Self>, delta: i32) {
        let next = {
            let session = self.journal.borrow();
            let Some(viewer) = session.viewer.as_ref() else {
                return;
            };
            let count = session.attachments.len();
            if count == 0 {
                return;
            }
            let index = if delta < 0 {
                viewer.index.saturating_sub(1)
            } else {
                (viewer.index + 1).min(count - 1)
            };
            if index == viewer.index {
                return;
            }
            index
        };
        if let Some(viewer) = self.journal.borrow_mut().viewer.as_mut() {
            viewer.index = next;
            viewer.loading = true;
        }
        self.journal.borrow_mut().viewer_image = None;
        self.sync_journal_viewer();
        self.load_journal_viewer_image();
    }

    pub(super) fn close_journal_viewer(&self) {
        self.journal.borrow_mut().viewer = None;
        self.journal.borrow_mut().viewer_request = 0;
        self.journal.borrow_mut().viewer_image = None;
        self.sync_journal_viewer();
    }

    fn load_journal_viewer_image(self: &Rc<Self>) {
        let id = {
            let session = self.journal.borrow();
            let Some(viewer) = session.viewer.as_ref() else {
                return;
            };
            session
                .attachments
                .get(viewer.index)
                .map(|attachment| attachment.id.clone())
        };
        let Some(id) = id else { return };
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        if self
            .worker
            .journal_query(request, JournalRead::AttachmentView(id))
            .is_err()
        {
            self.notice(ToastKind::Error, "图片读取队列不可用，请稍后重试。");
            self.close_journal_viewer();
            return;
        }
        // 新请求覆盖旧 ID；旧回包会在 journal_queried 中被忽略。
        self.journal.borrow_mut().viewer_request = request;
    }

    pub(in crate::desktop) fn apply_journal_viewer(&self, view: AttachmentView) {
        let accepted = {
            let session = self.journal.borrow();
            session.viewer.as_ref().is_some_and(|viewer| {
                session
                    .attachments
                    .get(viewer.index)
                    .is_some_and(|attachment| attachment.id == view.attachment.id)
            })
        };
        if !accepted {
            return;
        }
        self.journal.borrow_mut().viewer_image =
            Some(image_from_rgba(&view.rgba, view.width, view.height));
        if let Some(viewer) = self.journal.borrow_mut().viewer.as_mut() {
            viewer.loading = false;
        }
        self.sync_journal_viewer();
    }

    pub(super) fn sync_journal_attachments(&self) {
        let (views, usage_label, busy, pending) = {
            let session = self.journal.borrow();
            let views: Vec<JournalAttachmentView> = session
                .attachment_views
                .iter()
                .enumerate()
                .map(|(index, tile)| JournalAttachmentView {
                    id: tile.id.clone().into(),
                    image: tile.image.clone(),
                    label: tile.label.clone().into(),
                    index: index as i32,
                })
                .collect();
            let busy = session.add_request.is_some() || session.remove_request.is_some();
            let pending = session
                .pending_image
                .as_ref()
                .map(|pending| format!("{} · {}", pending.name, format_bytes(pending.size)));
            (views, format_bytes(session.usage.bytes), busy, pending)
        };
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_journal_attachments(ModelRc::new(VecModel::from(views)));
            window.set_journal_image_usage(usage_label.into());
            window.set_journal_add_image_busy(busy);
            window.set_journal_pending_image(pending.is_some());
            window.set_journal_pending_image_label(pending.unwrap_or_default().into());
        });
    }

    pub(super) fn sync_journal_viewer(&self) {
        let session = self.journal.borrow();
        let (open, title, loading, image, index) = match session.viewer.as_ref() {
            Some(viewer) => {
                let date = session
                    .editor_date()
                    .map(|date| format!("{}年{}月{}日", date.year(), date.month(), date.day()))
                    .unwrap_or_default();
                (
                    true,
                    format!(
                        "{} · {} / {}",
                        date,
                        viewer.index + 1,
                        session.attachments.len()
                    ),
                    viewer.loading,
                    session.viewer_image.clone().unwrap_or_default(),
                    viewer.index as i32,
                )
            }
            None => (false, String::new(), false, slint::Image::default(), 0),
        };
        drop(session);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_journal_viewer_visible(open);
            window.set_journal_viewer_title(title.into());
            window.set_journal_viewer_loading(loading);
            window.set_journal_viewer_image(image);
            window.set_journal_viewer_index(index);
        });
    }

    /// 关闭编辑器或离开页面时释放缩略图与大图内存。
    pub(in crate::desktop) fn clear_journal_attachments(&self) {
        {
            let mut session = self.journal.borrow_mut();
            session.attachments.clear();
            session.attachment_views.clear();
            session.attachment_request = 0;
            session.attachments_loaded_for = None;
            session.viewer = None;
            session.viewer_request = 0;
            session.viewer_image = None;
            session.pending_image = None;
        }
        self.sync_journal_attachments();
        self.sync_journal_viewer();
    }
}

fn image_from_rgba(rgba: &[u8], width: u32, height: u32) -> slint::Image {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return slint::Image::default();
    }
    let mut buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
    buffer.make_mut_bytes().copy_from_slice(rgba);
    slint::Image::from_rgba8(buffer)
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

/// 领域错误映射为添加/删除图片的中文提示。
pub(super) fn attachment_error_message(error: &JournalStoreError) -> &'static str {
    use fastnote_core::JournalError;
    match error.domain_error() {
        Some(JournalError::Attachment(fastnote_core::AttachmentError::TooLarge)) => {
            "图片不能超过 10MB。"
        }
        Some(JournalError::Attachment(fastnote_core::AttachmentError::Limit)) => {
            "每天最多添加 20 张图片。"
        }
        Some(JournalError::Attachment(fastnote_core::AttachmentError::Pixels)) => {
            "图片像素过多，请先缩小后再添加。"
        }
        Some(JournalError::Attachment(fastnote_core::AttachmentError::Format))
        | Some(JournalError::Attachment(fastnote_core::AttachmentError::Corrupt)) => {
            "图片格式不支持或已损坏。"
        }
        Some(JournalError::NotFound) => "图片已不存在，正在刷新。",
        _ => "图片保存失败，请重试。",
    }
}
