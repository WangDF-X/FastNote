use std::collections::{HashMap, HashSet};
use std::path::Path;

use fastnote_core::{
    BallPosition, CloseBehavior, DateTime, DesktopSetting, DesktopSettings, EntryMode,
    SettingsRepository, ThemeMode, Utc, WindowGeometry,
};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::{database, StoreError};

/// SQLite 设置仓储；实例及其唯一写连接只在 DB 工作线程使用。
pub struct SqliteSettings {
    connection: Connection,
}

impl SqliteSettings {
    /// 安全打开/初始化正式 schema；旧库先备份，高版本/损坏库拒绝写入。
    pub fn open(path: &Path, now: DateTime<Utc>) -> Result<Self, StoreError> {
        Ok(Self {
            connection: database::open(path, now)?,
        })
    }

    /// 退出前将 WAL 合并入主库；外部占用时返回错误，不谎称持久化完成。
    pub fn checkpoint(&self) -> Result<(), StoreError> {
        let busy: i64 =
            self.connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(StoreError::sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                None,
            )));
        }
        Ok(())
    }
}

impl SettingsRepository for SqliteSettings {
    type Error = StoreError;

    fn load_desktop(&self) -> Result<DesktopSettings, StoreError> {
        // 未知键留在库中，不读入缓存；异常大值按非法配置回退，避免分配巨型字符串。
        let mut statement = self.connection.prepare(
            "SELECT key,
            CASE WHEN length(CAST(value AS BLOB)) <= 4096 THEN value ELSE char(0) END
            FROM app_settings WHERE key IN ('general.entry_mode', 'general.close_behavior',
            'hotkeys.enabled', 'general.first_run_done', 'hotkeys.show_app',
            'window.main_geometry', 'appearance.ball_position', 'appearance.theme')",
        )?;
        let values: HashMap<String, String> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut result = DesktopSettings::default();
        result.entry_mode = read(
            &values,
            "general.entry_mode",
            result.entry_mode,
            EntryMode::parse,
        );
        result.theme = read(&values, "appearance.theme", result.theme, ThemeMode::parse);
        result.close_behavior = read(
            &values,
            "general.close_behavior",
            result.close_behavior,
            CloseBehavior::parse,
        );
        result.hotkeys_enabled = read(
            &values,
            "hotkeys.enabled",
            result.hotkeys_enabled,
            parse_bool,
        );
        result.first_run_done = read(
            &values,
            "general.first_run_done",
            result.first_run_done,
            parse_bool,
        );
        result.show_hotkey = read(&values, "hotkeys.show_app", result.show_hotkey, |value| {
            fastnote_core::is_valid_hotkey(value).then(|| value.to_owned())
        });
        result.geometry = read(&values, "window.main_geometry", None, |value| {
            if value.is_empty() {
                return Some(None);
            }
            serde_json::from_str::<WindowGeometry>(value)
                .ok()
                .filter(|g| g.is_valid())
                .map(Some)
        });
        result.ball_position = read(&values, "appearance.ball_position", None, |value| {
            if value.is_empty() {
                return Some(None);
            }
            serde_json::from_str::<BallPosition>(value)
                .ok()
                .filter(|p| p.is_valid())
                .map(Some)
        });
        Ok(result)
    }

    fn save_desktop(
        &mut self,
        changes: &[DesktopSetting],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let mut keys = HashSet::new();
        let mut values = Vec::with_capacity(changes.len());
        for change in changes {
            if !keys.insert(change.key()) {
                return Err(StoreError::invalid(change.key()));
            }
            values.push((change.key(), encode(change)?));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > crate::migrations::CURRENT_VERSION {
            return Err(StoreError::newer(
                version,
                crate::migrations::CURRENT_VERSION,
            ));
        }
        if version != crate::migrations::CURRENT_VERSION {
            return Err(StoreError::integrity());
        }
        for (key, value) in values {
            tx.execute("INSERT INTO app_settings(key, value, updated_at) VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![key, value, now.to_rfc3339()])?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn read<T>(
    values: &HashMap<String, String>,
    key: &'static str,
    default: T,
    parse: impl FnOnce(&str) -> Option<T>,
) -> T {
    match values.get(key) {
        Some(value) => match parse(value) {
            Some(value) => value,
            None => {
                tracing::warn!(setting_key = key, "invalid setting, using default");
                default
            }
        },
        None => default,
    }
}

fn encode(change: &DesktopSetting) -> Result<String, StoreError> {
    let value = match change {
        DesktopSetting::EntryMode(value) => value.as_str().to_owned(),
        DesktopSetting::Theme(value) => value.as_str().to_owned(),
        DesktopSetting::CloseBehavior(value) => value.as_str().to_owned(),
        DesktopSetting::FirstRunDone(value) => value.to_string(),
        DesktopSetting::MainGeometry(Some(value)) => {
            if !value.is_valid() {
                return Err(StoreError::invalid(change.key()));
            }
            serde_json::to_string(value).map_err(|_| StoreError::invalid(change.key()))?
        }
        DesktopSetting::BallPosition(Some(value)) => {
            if !value.is_valid() {
                return Err(StoreError::invalid(change.key()));
            }
            serde_json::to_string(value).map_err(|_| StoreError::invalid(change.key()))?
        }
        DesktopSetting::MainGeometry(None) | DesktopSetting::BallPosition(None) => String::new(),
    };
    Ok(value)
}
