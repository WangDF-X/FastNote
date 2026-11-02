//! 领域基础类型；实体标识见 ARCH-03 §3。

mod id;

pub use id::{IdError, NoteId, PlanId, TaskId};
