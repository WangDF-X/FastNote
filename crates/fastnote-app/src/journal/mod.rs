//! 日记单日草稿、500ms 防抖保存、图片处理与预览；不预读历史正文（DIARY-01）。

mod editor;
mod image;
mod worker;

pub(crate) use editor::{JournalEditor, SavePlan};
pub(crate) use worker::{
    add_image, execute, query, AttachmentView, JournalRead, JournalReadResult,
};
