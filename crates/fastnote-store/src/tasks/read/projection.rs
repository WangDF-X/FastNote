use std::collections::HashMap;

use fastnote_core::{TaskError, TaskSummary, TodayView};
use rusqlite::{params_from_iter, Connection};

use super::{columns, decode, TaskStoreError};

const PROJECTION_BATCH: usize = 128;

pub(in crate::tasks) fn complete_today(
    connection: &Connection,
    view: &mut TodayView,
) -> Result<(), TaskStoreError> {
    for section in &mut view.sections {
        for batch in section.items.chunks_mut(PROJECTION_BATCH) {
            complete_batch(connection, batch)?;
        }
    }
    Ok(())
}

fn complete_batch(
    connection: &Connection,
    batch: &mut [TaskSummary],
) -> Result<(), TaskStoreError> {
    if batch.is_empty() {
        return Ok(());
    }
    let placeholders = (0..batch.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT {} FROM tasks t LEFT JOIN plans p ON p.id=t.plan_id WHERE t.id IN ({placeholders}) AND t.kind='task'", columns(false));
    let ids: Vec<_> = batch.iter().map(|task| task.id.to_string()).collect();
    let mut statement = connection.prepare(&sql)?;
    let mut contexts = HashMap::with_capacity(batch.len());
    let rows = statement.query_map(params_from_iter(&ids), decode)?;
    for row in rows {
        let (task, context) = row?;
        contexts.insert(task.id, (task.updated_at, context));
    }
    for summary in batch {
        let (updated_at, context) = contexts.remove(&summary.id).ok_or(TaskError::Conflict)?;
        // 外部进程若在两次读取间改变实体，拒绝混合版本，不把失效标记当作可编辑确认值。
        if updated_at != summary.updated_at || context.read_only {
            return Err(TaskError::Conflict.into());
        }
        summary.context = context;
    }
    Ok(())
}
