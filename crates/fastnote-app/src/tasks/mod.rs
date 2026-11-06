//! 今日业务与共享任务编辑；命令仍交给唯一 DB worker。

mod editor;
mod lifetime;
mod residency;
mod session;
mod worker;

pub(crate) use editor::{DraftFields, EditorSession};
pub(crate) use lifetime::{PageScope, PageStamp, UiEpoch};
pub(crate) use residency::TaskResume;
pub(crate) use session::{today_clock_stale, DetailPurpose, PendingTask, TaskSession, Transition};
pub(crate) use worker::{execute, query};

#[cfg(test)]
mod test_support;
