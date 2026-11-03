//! 今日业务与共享任务编辑；命令仍交给唯一 DB worker。

mod editor;
mod session;
mod worker;

pub(crate) use editor::{DraftFields, EditorSession};
pub(crate) use session::{DetailPurpose, PendingTask, TaskSession, Transition};
pub(crate) use worker::{execute, query};

#[cfg(test)]
mod test_support;
