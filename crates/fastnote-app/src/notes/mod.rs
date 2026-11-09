//! 小记读写通道与可独立测试的当前草稿；不持有历史正文集合。

mod editor;
mod tree;
mod worker;

pub(crate) use editor::{NoteEditor, SavePlan};
pub(crate) use tree::TreeState;
pub(crate) use worker::{execute, expire, query, NoteRead, NoteReadResult};
