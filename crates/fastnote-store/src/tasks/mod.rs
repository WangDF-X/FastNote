//! 设置与任务共享 SqliteSettings 的同一写连接，不打开第二个数据库。

mod error;
mod preferences;
mod read;
mod write;

use fastnote_core::{
    NaiveDate, Task, TaskDetail, TaskId, TaskRepository, TaskSummary, TodayPreferences,
};

use crate::SqliteSettings;

pub use error::TaskStoreError;

impl TaskRepository for SqliteSettings {
    type Error = TaskStoreError;

    fn today_tasks(&self, date: NaiveDate) -> Result<Vec<TaskSummary>, TaskStoreError> {
        read::today(&self.connection, date)
    }

    fn task_detail(&self, id: TaskId) -> Result<Option<TaskDetail>, TaskStoreError> {
        read::detail(&self.connection, id)
    }

    fn today_preferences(&self) -> Result<TodayPreferences, TaskStoreError> {
        preferences::load(&self.connection)
    }

    fn insert_task(&mut self, task: &Task) -> Result<(), TaskStoreError> {
        write::insert(&mut self.connection, task)
    }

    fn replace_task(&mut self, expected: &Task, task: &Task) -> Result<(), TaskStoreError> {
        write::replace(&mut self.connection, expected, task)
    }

    fn delete_task(&mut self, id: TaskId, cascade: bool) -> Result<Vec<TaskId>, TaskStoreError> {
        write::delete(&mut self.connection, id, cascade)
    }
}
