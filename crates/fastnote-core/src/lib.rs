//! Domain types and rules shared across FastNote crates.

pub mod domain;
pub mod ports;

pub use chrono::NaiveDate;
pub use domain::{IdError, NoteId, PlanId, TaskId};
pub use ports::Clock;
