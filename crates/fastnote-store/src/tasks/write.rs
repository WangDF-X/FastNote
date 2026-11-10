use fastnote_core::{Task, TaskError, TaskId, TaskKind, TaskStatus};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use crate::database::check_write_version;

use super::{read, TaskStoreError};

pub(super) fn insert(connection: &mut Connection, task: &Task) -> Result<(), TaskStoreError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    insert_in_transaction(&tx, task)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn insert_in_transaction(
    tx: &Transaction<'_>,
    task: &Task,
) -> Result<(), TaskStoreError> {
    task.validate()?;
    if task.kind != TaskKind::Task || task.plan_id.is_some() || task.parent_id.is_some() {
        return Err(TaskError::Conflict.into());
    }
    let maximum: i64 = tx.query_row(
        "SELECT COALESCE(max(sort_order),0) FROM tasks
        WHERE parent_id IS NULL AND plan_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    let sort_order = maximum.checked_add(1024).ok_or(TaskError::Conflict)?;
    tx.execute(
        "INSERT INTO tasks(id,kind,title,description,status,priority,start_date,due_date,
        completed_at,status_changed_at,sort_order,created_at,updated_at,start_time,due_time)
        VALUES(?1,'task',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            task.id.to_string(),
            task.title,
            task.description,
            task.status.map(TaskStatus::as_str),
            task.priority as i32,
            task.start_date.map(|date| date.to_string()),
            task.due_date.map(|date| date.to_string()),
            task.completed_at.map(|at| at.to_rfc3339()),
            task.status_changed_at.to_rfc3339(),
            sort_order,
            task.created_at.to_rfc3339(),
            task.updated_at.to_rfc3339(),
            task.start_time.map(|time| time.format("%H:%M").to_string()),
            task.due_time.map(|time| time.format("%H:%M").to_string())
        ],
    )?;
    Ok(())
}

pub(super) fn replace(
    connection: &mut Connection,
    expected: &Task,
    task: &Task,
) -> Result<(), TaskStoreError> {
    task.validate()?;
    // 本命令不能伪装成移动/阶段修改，也不能重写创建时间和树顺序。
    if task.id != expected.id
        || task.plan_id != expected.plan_id
        || task.parent_id != expected.parent_id
        || task.kind != expected.kind
        || task.sort_order != expected.sort_order
        || task.created_at != expected.created_at
    {
        return Err(TaskError::Conflict.into());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let current = read::detail(&tx, task.id)?.ok_or(TaskError::NotFound)?;
    if current.context.read_only {
        return Err(TaskError::PlanArchived.into());
    }
    if current.task != *expected {
        return Err(TaskError::Conflict.into());
    }
    tx.execute(
        "UPDATE tasks SET title=?2,description=?3,status=?4,priority=?5,
        start_date=?6,due_date=?7,completed_at=?8,status_changed_at=?9,updated_at=?10,
        start_time=?11,due_time=?12 WHERE id=?1",
        params![
            task.id.to_string(),
            task.title,
            task.description,
            task.status.map(TaskStatus::as_str),
            task.priority as i32,
            task.start_date.map(|date| date.to_string()),
            task.due_date.map(|date| date.to_string()),
            task.completed_at.map(|at| at.to_rfc3339()),
            task.status_changed_at.to_rfc3339(),
            task.updated_at.to_rfc3339(),
            task.start_time.map(|time| time.format("%H:%M").to_string()),
            task.due_time.map(|time| time.format("%H:%M").to_string())
        ],
    )?;
    // REM-01：完成与暂停必须在同一事务；重开不自动恢复提醒。
    if task.status == Some(TaskStatus::Done) && expected.status != Some(TaskStatus::Done) {
        tx.execute("UPDATE reminders SET state='paused',next_fire_at=NULL,revision=revision+1,updated_at=?2
            WHERE task_id=?1 AND state='active'", params![task.id.to_string(), task.updated_at.to_rfc3339()])?;
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn delete(
    connection: &mut Connection,
    id: TaskId,
    cascade: bool,
) -> Result<Vec<TaskId>, TaskStoreError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_write_version(&tx)?;
    let detail = read::detail(&tx, id)?.ok_or(TaskError::NotFound)?;
    if detail.context.read_only {
        return Err(TaskError::PlanArchived.into());
    }
    let ids = {
        let mut statement = tx.prepare(
            "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM tasks WHERE id=?1
            UNION SELECT c.id FROM tasks c JOIN subtree s ON c.parent_id=s.id
        ) SELECT id FROM subtree",
        )?;
        let strings = statement
            .query_map([id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        strings
            .into_iter()
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| TaskStoreError::from(TaskError::Conflict))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if !cascade && ids.len() > 1 {
        return Err(TaskError::HasChildren.into());
    }
    tx.execute("DELETE FROM tasks WHERE id=?1", [id.to_string()])?;
    tx.commit()?;
    Ok(ids)
}
