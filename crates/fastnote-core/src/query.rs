use chrono::NaiveDate;

use crate::{TaskDetail, TaskId, TodayView};

/// 今日和共享详情查询；各组加载上限是会话展示状态，不持久化。
#[derive(Clone, Copy, Debug)]
pub enum Query {
    /// 每组初始 200，最多 1000。
    Today { date: NaiveDate, limits: [u32; 8] },
    /// 全局复用的任务详情。
    TaskDetail(TaskId),
}

/// 查询结果只包含 core 定义的读模型。
#[derive(Clone, Debug)]
pub enum QueryResult {
    /// 今日视图。
    Today(TodayView),
    /// 任务详情。
    TaskDetail(Box<TaskDetail>),
}
