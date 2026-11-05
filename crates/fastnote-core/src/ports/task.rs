use chrono::NaiveDate;

use crate::{Task, TaskDetail, TaskError, TaskId, TaskSummary, TodayPreferences, TodayView};

/// 统一任务仓储；实现必须在 DB 线程内使用单连接和原子写事务。
pub trait TaskRepository {
    /// 模块错误保留领域码或原始存储错误链。
    type Error: From<TaskError>;

    /// 今日候选投影，不读取长描述；core 统一归组和派生。
    fn today_tasks(&self, date: NaiveDate) -> Result<Vec<TaskSummary>, Self::Error>;
    /// 有界查询的逐行候选入口；关系信息可延后为已选前缀补齐，实际 SQLite 必须流式读取。
    fn visit_today_tasks(
        &self,
        date: NaiveDate,
        visitor: &mut dyn FnMut(TaskSummary),
    ) -> Result<(), Self::Error> {
        for task in self.today_tasks(date)? {
            visitor(task);
        }
        Ok(())
    }
    /// 仅补齐排序前缀的关系投影，不读取长正文；默认实现用于已返回完整投影的假仓储。
    fn complete_today_projection(&self, _view: &mut TodayView) -> Result<(), Self::Error> {
        Ok(())
    }
    /// 同一快照中的任务与计划/子任务/依赖信息；不存在为 None。
    fn task_detail(&self, id: TaskId) -> Result<Option<TaskDetail>, Self::Error>;
    /// 读取已登记的今日偏好；非法值回退默认。
    fn today_preferences(&self) -> Result<TodayPreferences, Self::Error>;
    /// 插入独立任务，在事务中分配同父排序值；重复 ID 拒绝。
    fn insert_task(&mut self, task: &Task) -> Result<(), Self::Error>;
    /// 比较完整原实体后替换可编辑字段；不覆盖其他写入或移动。
    fn replace_task(&mut self, expected: &Task, task: &Task) -> Result<(), Self::Error>;
    /// 明确确认后删除；无级联确认且存在后代时拒绝，返回真正删除的 ID。
    fn delete_task(&mut self, id: TaskId, cascade: bool) -> Result<Vec<TaskId>, Self::Error>;
}
