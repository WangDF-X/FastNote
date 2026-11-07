use fastnote_core::{
    NaiveDate, Query, QueryResult, Task, TaskClock, TaskDetail, TaskError, TaskId, TaskRepository,
    TaskService, TaskSummary, TodayPreferences, TodayView,
};
use rusqlite::Connection;

use crate::SqliteSettings;

use super::{preferences, read, TaskStoreError};

impl SqliteSettings {
    /// 在单个 SQLite 只读快照中查询任务；设置、候选及有界关系投影不混合外部写入版本。
    /// 不读长正文列表、不新增连接；错误/提前返回时由 Transaction 自动回滚读事务。
    pub fn query_tasks(
        &self,
        query: Query,
        clock: impl TaskClock,
    ) -> Result<QueryResult, TaskStoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut repository = ReadSnapshot {
            connection: &transaction,
        };
        let result = TaskService::new(&mut repository, clock).query(query)?;
        transaction.commit()?;
        Ok(result)
    }
}

struct ReadSnapshot<'a> {
    connection: &'a Connection,
}

impl TaskRepository for ReadSnapshot<'_> {
    type Error = TaskStoreError;

    fn today_tasks(&self, date: NaiveDate) -> Result<Vec<TaskSummary>, Self::Error> {
        read::today(self.connection, date)
    }
    fn calendar_tasks(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<TaskSummary>, Self::Error> {
        read::calendar(self.connection, start, end)
    }
    fn visit_today_tasks(
        &self,
        date: NaiveDate,
        visitor: &mut dyn FnMut(TaskSummary),
    ) -> Result<(), Self::Error> {
        read::visit_today(self.connection, date, visitor)
    }
    fn complete_today_projection(&self, view: &mut TodayView) -> Result<(), Self::Error> {
        read::complete_today(self.connection, view)
    }
    fn task_detail(&self, id: TaskId) -> Result<Option<TaskDetail>, Self::Error> {
        read::detail(self.connection, id)
    }
    fn today_preferences(&self) -> Result<TodayPreferences, Self::Error> {
        preferences::load(self.connection)
    }

    // 防御性只读适配：只传给上面的 query，不允许 TaskService 执行写命令。
    fn insert_task(&mut self, _task: &Task) -> Result<(), Self::Error> {
        Err(TaskError::Conflict.into())
    }
    fn replace_task(&mut self, _expected: &Task, _task: &Task) -> Result<(), Self::Error> {
        Err(TaskError::Conflict.into())
    }
    fn delete_task(&mut self, _id: TaskId, _cascade: bool) -> Result<Vec<TaskId>, Self::Error> {
        Err(TaskError::Conflict.into())
    }
}
