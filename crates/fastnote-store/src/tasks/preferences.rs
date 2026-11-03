use std::collections::HashMap;

use fastnote_core::{TodayPreferences, TodaySort};
use rusqlite::Connection;

use crate::settings::{parse_bool, read};

use super::TaskStoreError;

pub(super) fn load(connection: &Connection) -> Result<TodayPreferences, TaskStoreError> {
    let mut statement = connection.prepare("SELECT key,
        CASE WHEN length(CAST(value AS BLOB))<=4096 THEN value ELSE char(0) END
        FROM app_settings WHERE key IN ('tasks.default_sort','today.show_in_progress',
            'today.show_waiting','today.show_unscheduled','today.show_completed','today.tomorrow_collapsed')")?;
    let values: HashMap<String, String> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(from_values(&values))
}

/// 设置页面快照与今日查询共用同一校验/回退规则，不能出现两套默认值。
pub(crate) fn from_values(values: &HashMap<String, String>) -> TodayPreferences {
    let default = TodayPreferences::default();
    TodayPreferences {
        sort: read(values, "tasks.default_sort", default.sort, TodaySort::parse),
        show_in_progress: read(
            values,
            "today.show_in_progress",
            default.show_in_progress,
            parse_bool,
        ),
        show_waiting: read(
            values,
            "today.show_waiting",
            default.show_waiting,
            parse_bool,
        ),
        show_unscheduled: read(
            values,
            "today.show_unscheduled",
            default.show_unscheduled,
            parse_bool,
        ),
        show_completed: read(
            values,
            "today.show_completed",
            default.show_completed,
            parse_bool,
        ),
        tomorrow_collapsed: read(
            values,
            "today.tomorrow_collapsed",
            default.tomorrow_collapsed,
            parse_bool,
        ),
    }
}
