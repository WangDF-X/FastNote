use std::error::Error;
use std::str::FromStr;

use fastnote_core::{
    parse_task_time, NaiveDate, NaiveTime, Priority, Task, TaskContext, TaskDetail, TaskError,
    TaskId, TaskKind, TaskStatus, TaskSummary,
};
use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};

use super::TaskStoreError;

mod projection;
pub(super) use projection::complete_today;

// 列表不读取长描述/后代计数。统一列顺序供同一解码器使用。
const COLUMNS: &str = "t.id, t.plan_id, t.parent_id, t.kind, t.title, {description},
    t.status, t.priority, t.start_date, t.due_date, t.completed_at,
    t.status_changed_at, t.sort_order, t.created_at, t.updated_at, p.name,
    (WITH RECURSIVE ancestors(id,parent_id,kind,title) AS (
        SELECT id,parent_id,kind,title FROM tasks WHERE id=t.parent_id
        UNION SELECT a.id,a.parent_id,a.kind,a.title FROM tasks a JOIN ancestors b ON a.id=b.parent_id
    ) SELECT title FROM ancestors WHERE kind='stage' LIMIT 1),
    (SELECT count(*) FROM tasks c WHERE c.parent_id=t.id AND c.kind='task'),
    (SELECT count(*) FROM tasks c WHERE c.parent_id=t.id AND c.kind='task' AND c.status='done'),
    (SELECT count(*) FROM task_dependencies d WHERE d.task_id=t.id),
    (SELECT count(*) FROM task_dependencies d JOIN tasks pre ON pre.id=d.depends_on_task_id
        WHERE d.task_id=t.id AND pre.status<>'done'),
    COALESCE(p.status='archived',0), {descendants}, t.start_time, t.due_time";

fn columns(detail: bool) -> String {
    COLUMNS
        .replace("{description}", if detail { "t.description" } else { "''" })
        .replace(
            "{descendants}",
            if detail {
                "(WITH RECURSIVE descendants(id) AS (
                SELECT id FROM tasks WHERE parent_id=t.id
                UNION SELECT c.id FROM tasks c JOIN descendants d ON c.parent_id=d.id
            ) SELECT count(*) FROM descendants)"
            } else {
                "0"
            },
        )
}

// 分组/排序仅需标量字段；关系子查询延后为已选 200–1000 条执行，避免对全部历史求计数。
const CANDIDATE_COLUMNS: &str = "t.id,t.plan_id,t.parent_id,t.kind,t.title,'',t.status,t.priority,
    t.start_date,t.due_date,t.completed_at,t.status_changed_at,t.sort_order,t.created_at,t.updated_at,
    NULL,NULL,0,0,0,0,0,0,t.start_time,t.due_time";

pub(super) fn today(
    connection: &Connection,
    date: NaiveDate,
) -> Result<Vec<TaskSummary>, TaskStoreError> {
    let mut tasks = Vec::new();
    visit_candidates(connection, date, true, &mut |task| tasks.push(task))?;
    Ok(tasks)
}

pub(super) fn calendar(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<TaskSummary>, TaskStoreError> {
    // 完成时间宽取前后一天，避免把 UTC 日期误当本地完成日。
    let sql = format!(
        "SELECT {} FROM tasks t LEFT JOIN plans p ON p.id=t.plan_id
        WHERE t.kind='task' AND (t.plan_id IS NULL OR p.status='active') AND (
          (t.status='done' AND julianday(t.completed_at)>=julianday(?1)-1
             AND julianday(t.completed_at)<julianday(?2)+2)
          OR (t.status<>'done' AND (
             t.start_date BETWEEN ?1 AND ?2 OR t.due_date BETWEEN ?1 AND ?2
             OR (t.status='in_progress' AND t.start_date<=?2
                 AND (t.due_date IS NULL OR t.due_date>=?1)))))",
        columns(false)
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![start.to_string(), end.to_string()], |row| {
        let (task, context) = decode(row)?;
        Ok(TaskSummary::from_owned_task(task, context))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// SQLite 行缓冲 + core 有界 collector；不物化全部候选/正文/计划树。
pub(super) fn visit_today(
    connection: &Connection,
    date: NaiveDate,
    visitor: &mut dyn FnMut(TaskSummary),
) -> Result<(), TaskStoreError> {
    visit_candidates(connection, date, false, visitor)
}

fn visit_candidates(
    connection: &Connection,
    date: NaiveDate,
    complete: bool,
    visitor: &mut dyn FnMut(TaskSummary),
) -> Result<(), TaskStoreError> {
    // UTC 与本地日期最多相差一天，候选宽取后交 core 精确判断；不把 UTC 日期当本地今日。
    let sql = format!(
        "SELECT {} FROM tasks t LEFT JOIN plans p ON p.id=t.plan_id
        WHERE t.kind='task' AND (p.id IS NULL OR p.status='active')
        AND (t.status<>'done' OR (julianday(t.completed_at)>=julianday(?1)-1
            AND julianday(t.completed_at)<julianday(?1)+2)) {}",
        if complete {
            columns(false)
        } else {
            CANDIDATE_COLUMNS.to_owned()
        },
        if complete {
            "ORDER BY t.priority DESC, t.created_at DESC, t.id ASC"
        } else {
            ""
        }
    );
    // 流式路径由 core 对全部候选计数并确定排序前缀，避免数据库重复排序。
    let mut statement = connection.prepare(&sql)?;
    let result = statement.query_map([date.format("%Y-%m-%d").to_string()], |row| {
        let (task, context) = decode(row)?;
        Ok(TaskSummary::from_owned_task(task, context))
    })?;
    for task in result {
        visitor(task?);
    }
    Ok(())
}

pub(super) fn detail(
    connection: &Connection,
    id: TaskId,
) -> Result<Option<TaskDetail>, TaskStoreError> {
    let sql = format!(
        "SELECT {} FROM tasks t LEFT JOIN plans p ON p.id=t.plan_id
        WHERE t.id=?1 AND t.kind='task'",
        columns(true)
    );
    Ok(connection
        .query_row(&sql, params![id.to_string()], |row| {
            let (task, context) = decode(row)?;
            Ok(TaskDetail {
                task,
                context,
                is_overdue: false,
                is_blocked: false,
            })
        })
        .optional()?)
}

fn parsed<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
{
    let value = row.get_ref(index)?.as_str()?;
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn optional<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
{
    row.get_ref(index)?
        .as_str_or_null()?
        .map(|value| {
            value.parse().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
            })
        })
        .transpose()
}

fn invalid(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(TaskError::Conflict))
}

fn optional_time(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<NaiveTime>> {
    row.get_ref(index)?
        .as_str_or_null()?
        .map(|value| {
            parse_task_time(value)
                .map_err(|_| invalid(index))?
                .ok_or_else(|| invalid(index))
        })
        .transpose()
}

fn decode(row: &Row<'_>) -> rusqlite::Result<(Task, TaskContext)> {
    let status = row
        .get_ref(6)?
        .as_str_or_null()?
        .map(|value| TaskStatus::parse(value).ok_or_else(|| invalid(6)))
        .transpose()?;
    let task = Task {
        id: parsed(row, 0)?,
        plan_id: optional(row, 1)?,
        parent_id: optional(row, 2)?,
        kind: match row.get_ref(3)?.as_str()? {
            "task" => TaskKind::Task,
            "stage" => TaskKind::Stage,
            _ => return Err(invalid(3)),
        },
        title: row.get(4)?,
        description: row.get(5)?,
        status,
        priority: Priority::parse(row.get(7)?).ok_or_else(|| invalid(7))?,
        start_date: optional(row, 8)?,
        start_time: optional_time(row, 23)?,
        due_date: optional(row, 9)?,
        due_time: optional_time(row, 24)?,
        completed_at: optional(row, 10)?,
        status_changed_at: parsed(row, 11)?,
        sort_order: row.get(12)?,
        created_at: parsed(row, 13)?,
        updated_at: parsed(row, 14)?,
    };
    task.validate().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
    })?;
    Ok((
        task,
        TaskContext {
            plan_name: row.get(15)?,
            stage_name: row.get(16)?,
            child_total: row.get(17)?,
            child_done: row.get(18)?,
            dependency_count: row.get(19)?,
            blocking_count: row.get(20)?,
            read_only: row.get(21)?,
            descendant_count: row.get(22)?,
        },
    ))
}
