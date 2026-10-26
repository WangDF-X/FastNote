# ARCH-04 数据存储与配置项

> 文档编号：ARCH-04 ｜ 模块：00 总览与架构 ｜ 状态：已定稿 v0.1 ｜ 更新：2026-10-23
> 上游：[ARCH-01](01-tech-and-runtime.md)、[ARCH-03](03-domain-model.md) ｜ 下游：[09-data-management](../09-data-management/)、[08-settings](../08-settings/)

---

## 1. 目的与范围

定义 SQLite 数据库文件、表结构、索引、迁移、事务策略、备份格式以及全部用户配置项（键、类型、默认值、归属模块）。开发不得在本文之外新增表或配置键，除非先更新本文。

**范围内**：物理存储、DDL、迁移、连接参数、配置项注册表、数据文件布局、完整性策略。
**范围外**：业务规则（ARCH-03）、备份/恢复的用户流程（DATA-02）、导入导出文件格式（DATA-03）、界面字段（各模块）。

---

## 2. 存储选型与文件布局

### 2.1 选型结论

| 方案 | 结论 | 理由 |
| --- | --- | --- |
| **SQLite（选用）** | `rusqlite` + WAL，单文件 | 任务树/日期范围/依赖查询方便；事务与崩溃恢复成熟；单文件便于备份；无服务进程。 |
| JSON 单文件 | 否 | 每次写入需全量序列化，数据量增大后卡顿；崩溃可能截断文件。 |
| 嵌入式 KV（sled/redb） | 否 | 日期范围、聚合统计需自行建索引，开发成本高。 |

- 使用 `bundled` 特性编入 SQLite，避免依赖系统库版本。
- 主库单文件 `fastnote.db`，WAL 模式产生 `-wal`、`-shm` 伴生文件。

### 2.2 数据目录

| 平台 | 数据目录 | 日志目录 | 默认备份目录 |
| --- | --- | --- | --- |
| Windows | `%LOCALAPPDATA%\FastNote\` | 数据目录下 `logs\` | `%USERPROFILE%\Documents\FastNote\Backups\` |
| macOS | `~/Library/Application Support/FastNote/` | `~/Library/Logs/FastNote/` | `~/Documents/FastNote/Backups/` |
| Linux | `$XDG_DATA_HOME/fastnote/`（缺省 `~/.local/share/fastnote/`） | 数据目录下 `logs/` | `~/Documents/FastNote/Backups/` |

```text
<data_dir>/
├── fastnote.db            # 主数据库
├── fastnote.db-wal        # WAL（运行时存在）
├── fastnote.db-shm        # 共享内存（运行时存在）
├── backups/               # 默认备份位置（可被设置覆盖）
│   └── fastnote-backup-20261023-180000.fnsbackup
└── logs/
    └── fastnote.log.2026-10-23
```

- 数据目录不存放缓存图片、附件；第一版无附件。
- 卸载应用**不删除**用户数据；文档与卸载提示需说明。
- 用户可在设置中查看并打开数据目录（DATA-01）。

---

## 3. 连接与事务

### 3.1 连接参数（每次打开连接执行）

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA wal_autocheckpoint = 1000;
```

| 参数 | 取舍 |
| --- | --- |
| `WAL` | 读写不互斥；崩溃恢复能力强；代价是额外两个文件。 |
| `NORMAL` | 应用崩溃不丢已提交事务；极端断电可能丢失最近事务但不损坏数据库。换成 `FULL` 会显著增加每次输入自动保存的 fsync 开销。 |
| `foreign_keys=ON` | 依赖删除、计划级联、笔记关联依赖外键；必须每连接开启。 |
| `busy_timeout=5000` | 单写线程下几乎不会触发；为外部工具（备份/查看器）占用留余量。 |

### 3.2 事务策略

1. 写事务使用 `BEGIN IMMEDIATE`，避免读事务升级写锁时的 `SQLITE_BUSY`。
2. 一个用户命令 = 一个事务；成功后统一提交，失败 `ROLLBACK`。
3. 任务树移动、计划级联删除、导入等涉及多表的操作必须在单事务内完成。
4. 只读查询不显式开事务（SQLite 隐式快照），一次查询内保持一致性。
5. 禁止在事务内调用 UI 回调或阻塞等待；事件在提交成功后发布。
6. WAL checkpoint：退出时 `wal_checkpoint(TRUNCATE)`；备份前由 SQLite Backup API 处理，不手动复制文件。

### 3.3 仓储接口（core 定义，store 实现）

```rust
pub trait Repositories {
    fn plans(&self) -> &dyn PlanRepository;
    fn tasks(&self) -> &dyn TaskRepository;
    fn dependencies(&self) -> &dyn DependencyRepository;
    fn notes(&self) -> &dyn NoteRepository;
    fn journals(&self) -> &dyn JournalRepository;
    fn settings(&self) -> &dyn SettingsRepository;
}

pub trait Store: Send + Sync {
    fn read<T>(&self, f: impl FnOnce(&dyn Repositories) -> Result<T>) -> Result<T>;
    fn write<T>(&self, f: impl FnOnce(&mut dyn Repositories) -> Result<T>) -> Result<T>;
    fn backup_to(&self, dest: &Path) -> Result<BackupInfo>;
    fn inspect_backup(&self, source: &Path) -> Result<BackupInspection>;
    fn restore_from(&self, source: &Path) -> Result<RestoreReport>;
    fn integrity_check(&self, full: bool) -> Result<IntegrityReport>;
}
```

- `write` 内部负责事务、`updated_at` 之外的业务时间由服务层传入。
- 仓储方法只做持久化，不含业务判断；复杂读模型由 store 的 Query 实现组装。

---

## 4. 数据库 Schema（v1）

> 所有日期列为 `TEXT`，格式 `YYYY-MM-DD`；所有时间戳为 RFC 3339 UTC（`YYYY-MM-DDTHH:MM:SSZ`）；ID 为 UUIDv7 小写字符串。

### 4.1 元数据

```sql
CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- 预置：app_id = "fastnote"，created_at，first_run_at，
--       last_backup_at，last_backup_path，app_version

CREATE TABLE schema_migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    app_version TEXT NOT NULL
);
```

`schema_version` 同时写入 `PRAGMA user_version`，用于快速判断；`schema_migrations` 用于审计。

### 4.2 计划

```sql
CREATE TABLE plans (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    description     TEXT NOT NULL DEFAULT '',
    start_date      TEXT CHECK (start_date IS NULL OR length(start_date) = 10),
    target_end_date TEXT CHECK (target_end_date IS NULL OR length(target_end_date) = 10),
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active', 'archived')),
    sort_order      INTEGER NOT NULL DEFAULT 0,
    archived_at     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    CHECK (start_date IS NULL OR target_end_date IS NULL
           OR start_date <= target_end_date),
    CHECK ((status = 'active'  AND archived_at IS NULL)
        OR (status = 'archived' AND archived_at IS NOT NULL))
);
```

### 4.3 任务与阶段

```sql
CREATE TABLE tasks (
    id                TEXT PRIMARY KEY,
    plan_id           TEXT REFERENCES plans(id) ON DELETE CASCADE,
    parent_id         TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    kind              TEXT NOT NULL DEFAULT 'task'
                      CHECK (kind IN ('task', 'stage')),
    title             TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
    description       TEXT NOT NULL DEFAULT '',
    status            TEXT CHECK (status IN ('todo', 'in_progress', 'waiting', 'done')),
    priority          INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
    start_date        TEXT CHECK (start_date IS NULL OR length(start_date) = 10),
    due_date          TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    completed_at      TEXT,
    status_changed_at TEXT NOT NULL,
    sort_order        INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,

    -- 日期顺序
    CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date),
    -- 阶段无状态，任务必须有状态
    CHECK ((kind = 'task'  AND status IS NOT NULL)
        OR (kind = 'stage' AND status IS NULL)),
    -- 完成时间与状态一致
    CHECK ((kind = 'stage')
        OR (status = 'done' AND completed_at IS NOT NULL)
        OR (status <> 'done' AND completed_at IS NULL)),
    -- 阶段约束：必须有计划、无父节点、无优先级
    CHECK (kind = 'task'
        OR (plan_id IS NOT NULL AND parent_id IS NULL AND priority = 0))
);
```

> **说明：** SQLite 的 `CHECK` 在结果为 `NULL` 时视为通过，因此额外用「任务必须有状态」的表达式兜底；应用层仍需按 ARCH-03 §10 做完整校验。

### 4.4 依赖

```sql
CREATE TABLE task_dependencies (
    task_id             TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_task_id  TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
);
```

- 同计划、`kind=task`、无环、非祖先关系由领域服务保证（数据库无法低成本表达）。
- 删除任务自动清理依赖边，不会删除后继任务。

### 4.5 小记

```sql
CREATE TABLE notes (
    id                TEXT PRIMARY KEY,
    content           TEXT NOT NULL CHECK (length(trim(content)) > 0),
    converted_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE UNIQUE INDEX ux_notes_converted_task
    ON notes(converted_task_id) WHERE converted_task_id IS NOT NULL;
```

- 一对一约束：一个任务最多被一条小记关联。
- 内容长度上限 20,000 字符由应用层校验（数据库 `length()` 为字符数，可加 CHECK，但为便于后续调整不锁死在 schema）。

### 4.6 日记

```sql
CREATE TABLE journal_entries (
    id         TEXT PRIMARY KEY,
    entry_date TEXT NOT NULL UNIQUE CHECK (length(entry_date) = 10),
    content    TEXT NOT NULL CHECK (length(trim(content)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### 4.7 设置

```sql
CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

- 值为字符串：布尔 `true/false`，整数十进制，枚举固定字符串，JSON 对象为紧凑 JSON。
- 读取时做类型/范围校验；非法值回退默认值并记 warn 日志，不崩溃。
- 未知键保留在表中（向前兼容），不参与业务。

### 4.8a 定时提醒（schema v2 新增）

规则归属一个任务或小记，业务不变量与时间解释见 [REM-01](../11-reminders/01-reminders-and-notifications.md)；任务阶段不得绑定提醒。日期仍不改变任务的 date-only 排期。

```sql
CREATE TABLE reminders (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    note_id TEXT REFERENCES notes(id) ON DELETE CASCADE,
    schedule_kind TEXT NOT NULL CHECK(schedule_kind IN ('once','daily','weekly')),
    local_date TEXT,
    local_time TEXT NOT NULL CHECK(length(local_time)=5),
    weekday INTEGER,
    next_fire_at TEXT,
    last_handled_at TEXT,
    state TEXT NOT NULL CHECK(state IN ('active','paused','finished')),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>=1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((task_id IS NOT NULL AND note_id IS NULL)
        OR (task_id IS NULL AND note_id IS NOT NULL)),
    CHECK ((schedule_kind='once' AND local_date IS NOT NULL AND weekday IS NULL)
        OR (schedule_kind='daily' AND local_date IS NULL AND weekday IS NULL)
        OR (schedule_kind='weekly' AND local_date IS NULL AND weekday IS NOT NULL
            AND weekday BETWEEN 1 AND 7)),
    CHECK(state='active' OR next_fire_at IS NULL)
);
CREATE UNIQUE INDEX ux_reminders_task ON reminders(task_id) WHERE task_id IS NOT NULL;
CREATE UNIQUE INDEX ux_reminders_note ON reminders(note_id) WHERE note_id IS NOT NULL;
CREATE INDEX idx_reminders_next ON reminders(next_fire_at) WHERE state='active';
CREATE TABLE reminder_deliveries (
    id TEXT PRIMARY KEY,
    reminder_id TEXT NOT NULL REFERENCES reminders(id) ON DELETE CASCADE,
    rule_revision INTEGER NOT NULL CHECK(rule_revision>=1),
    scheduled_for TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN
        ('claimed','submitted','failed','missed','suppressed')),
    handled_at TEXT NOT NULL,
    UNIQUE(reminder_id,rule_revision,scheduled_for)
);
```

- `local_time` 严格解析为 `HH:MM`（24 小时制）；`local_date` 为有效自然日，`weekday` 为 ISO 1–7；服务层禁止阶段、已完成/归档任务上新建激活提醒。
- `active` 且可执行时必须有 `next_fire_at`；更新规则、暂停/恢复、完成/归档引发失效均在单写事务中维护 revision 与游标。平台投递不在事务内等待。
- 调度用 due 索引，账本以 `(reminder_id,rule_revision,scheduled_for)` 去重；不承诺系统级 exactly-once。详情见 REM-01。
- 完整备份包含两表；恢复/导入先暂停调度，重建当前时刻基线后再启用，禁止历史通知重放。

### 4.8 索引

```sql
CREATE INDEX idx_tasks_plan        ON tasks(plan_id);
CREATE INDEX idx_tasks_parent      ON tasks(parent_id);
CREATE INDEX idx_tasks_status      ON tasks(status);
CREATE INDEX idx_tasks_due         ON tasks(due_date)   WHERE status <> 'done';
CREATE INDEX idx_tasks_start       ON tasks(start_date) WHERE status <> 'done';
CREATE INDEX idx_tasks_updated     ON tasks(updated_at);
CREATE INDEX idx_tasks_sort        ON tasks(parent_id, sort_order);
CREATE INDEX idx_dep_prereq        ON task_dependencies(depends_on_task_id);
CREATE INDEX idx_notes_updated     ON notes(updated_at DESC);
CREATE INDEX idx_journal_date      ON journal_entries(entry_date DESC);
```

**查询策略**

- 今日/日历：先按日期索引取候选，再 `JOIN plans` 过滤 `plans.status='active'`（或 `plan_id IS NULL`）。
- 计划树：按 `plan_id` 一次取出该计划全部节点，在内存组装树（单计划通常 < 500 节点）。
- 计划列表进度：`GROUP BY plan_id` 统计叶子任务；在 SQL 或 Rust 中完成，单次查询。
- 小记列表分页：`ORDER BY updated_at DESC LIMIT ? OFFSET ?`；滚动加载。
- 不使用 FTS（第一版无搜索）。

---

## 5. 迁移

### 5.1 版本规则

- 当前 schema 版本：**2**。保留 `m0001_init.sql`，新增 `m0002_reminders.sql` 创建 §4.8a 两表及索引；已有 v1 库事务迁移至 v2，无原提醒数据需要转换。
- `PRAGMA user_version` 为权威版本；`schema_migrations` 记录历史。
- 迁移脚本只增不改，命名 `m0001_init.sql`、`m0002_xxx.sql`，编译期 `include_str!` 嵌入。
- 每个版本一个事务；DDL 在 SQLite 中可回滚。

### 5.2 启动流程

1. 读取 `user_version`。
2. `= 0` 且库为空 → 顺序执行 `m0001_init.sql`、`m0002_reminders.sql`，每版记录 `schema_migrations`，最终 `user_version=2`。
3. `> 当前支持版本` → 拒绝打开，提示「数据由更高版本创建，请升级应用」，不写库。
4. `< 当前版本` → 先创建预迁移备份（`backups/pre-migration-v{from}-{timestamp}.fnsbackup`），再逐版本迁移；失败回滚并保留原库。
5. 迁移后执行 `PRAGMA foreign_key_check` + `quick_check`；异常进入数据恢复流程。

### 5.3 兼容承诺

- 同一大版本内，新版本必须能打开旧版本数据库并自动迁移。
- 不做降级迁移；降级需从备份恢复。
- 备份文件内含 schema 版本，恢复时按上述规则处理。

---

## 6. 备份文件格式

- 扩展名 `.fnsbackup`，内容为**完整 SQLite 数据库文件**（不是 ZIP，不加密）。
- 生成方式：`rusqlite::backup::Backup` 在线备份 → 临时文件 → `PRAGMA integrity_check` → 写入元数据 → 原子重命名。
- 备份内 `app_meta` 追加/更新：

| 键 | 内容 |
| --- | --- |
| `backup_created_at` | 备份时间（RFC 3339 UTC） |
| `backup_app_version` | 生成备份的应用版本 |
| `backup_schema_version` | 生成时的 schema 版本 |
| `backup_counts` | 紧凑 JSON：plans/tasks/task_dependencies/notes/journals/reminders/reminder_deliveries 数量（读取 v1 备份时后两项缺省为 0） |

- 默认文件名：`fastnote-backup-YYYYMMDD-HHmmss.fnsbackup`；同秒冲突追加 `-2`。
- 备份失败不得留下半成品：临时文件命名为 `.tmp`，失败删除。
- 备份**包含**设置表（便于完整恢复），但恢复到其它设备后窗口几何与悬浮球位置会按屏幕范围重新校正。

详见 [DATA-02 备份与恢复](../09-data-management/02-backup-and-restore.md)。

---

## 7. 配置项注册表（v1）

> 这是**唯一**的配置键清单。新增键必须同时更新本表与所属模块文档。默认值即首次启动写入值；`app_settings` 中缺失时按默认值处理。

### 7.1 常规与入口

| 键 | 类型 | 默认值 | 取值 | 归属 | 说明 |
| --- | --- | --- | --- | --- | --- |
| `general.entry_mode` | enum | `ball` | `ball` / `tray` | ENTRY | 常驻入口形态；启动时生效。 |
| `general.autostart` | bool | `false` | — | ENTRY | 开机启动；变更需平台注册，失败回滚。 |
| `general.close_behavior` | enum | `entry` | `entry` / `quit` | SHELL | 关闭主窗口：最小化到入口 / 退出应用。 |
| `general.first_run_done` | bool | `false` | — | SHELL | 首次启动完成标记（用于决定是否自动显示主窗）。 |

### 7.2 快捷键

| 键 | 类型 | 默认值 | 取值 | 归属 | 说明 |
| --- | --- | --- | --- | --- | --- |
| `hotkeys.enabled` | bool | `true` | — | ENTRY | 全局快捷键总开关。 |
| `hotkeys.show_app` | hotkey | `Ctrl+Alt+Space` | 合法组合 | ENTRY | 唤起/聚焦主界面。 |
| `hotkeys.new_task` | hotkey | `Ctrl+Alt+N` | 合法组合 | ENTRY | 打开主界面并聚焦快速添加。 |
| `hotkeys.new_note` | hotkey | `Ctrl+Alt+M` | 合法组合 | ENTRY | 打开小记快速捕获窗口。 |

- 存储格式：`Ctrl+Alt+Shift+Key`，修饰键固定顺序 `Ctrl`→`Alt`→`Shift`→`Super`；键名使用 `Space`、`A`–`Z`、`0`–`9`、`F1`–`F12`。
- macOS 显示为 `⌃⌥Space` 等；内部仍存同一字符串。
- 空字符串表示未设置。

### 7.3 外观

| 键 | 类型 | 默认值 | 取值 | 归属 | 说明 |
| --- | --- | --- | --- | --- | --- |
| `appearance.theme` | enum | `system` | `system` / `light` / `dark` | SHELL | 主题；`system` 跟随系统。 |
| `appearance.density` | enum | `comfortable` | `comfortable` / `compact` | SHELL | 列表行高与间距。 |
| `appearance.ball_size` | int | `64` | 48–96（px） | ENTRY | 悬浮球直径。 |
| `appearance.ball_opacity` | int | `90` | 30–100（%） | ENTRY | 悬浮球不透明度，闲置 3s 后生效。 |
| `appearance.ball_position` | json | 空 | `{x,y,monitor}` | ENTRY | 上次位置；恢复时按可见屏幕校正。 |
| `appearance.week_starts_on` | enum | `monday` | `monday` / `sunday` | CAL | 日历周起始日。 |

### 7.4 任务与今日

| 键 | 类型 | 默认值 | 取值 | 归属 | 说明 |
| --- | --- | --- | --- | --- | --- |
| `tasks.default_sort` | enum | `priority` | `priority` / `due_date` / `created_desc` | TODAY | 今日日期类分组排序。 |
| `today.show_in_progress` | bool | `true` | — | TODAY | 显示「进行中」分组。 |
| `today.show_waiting` | bool | `true` | — | TODAY | 显示「等待中」分组。 |
| `today.show_unscheduled` | bool | `true` | — | TODAY | 显示「未安排」分组。 |
| `today.show_completed` | bool | `true` | — | TODAY | 显示「今日已完成」分组（默认折叠）。 |
| `today.tomorrow_collapsed` | bool | `true` | — | TODAY | 明日预告默认折叠。 |

### 7.5 数据与窗口

| 键 | 类型 | 默认值 | 取值 | 归属 | 说明 |
| --- | --- | --- | --- | --- | --- |
| `data.backup_dir` | path | 空 | 绝对路径 | DATA | 空表示使用平台默认备份目录。 |
| `window.main_geometry` | json | 空 | `{x,y,w,h}` | SHELL | 主窗口位置与尺寸；恢复时校正到可见区域。 |

**校验与变更**

- 取值范围外：夹取到最近合法值（尺寸/透明度）或回退默认（枚举），并记录 warn。
- 修改设置：单个事务批量写入 → 更新内存缓存 → 广播 `SettingsChanged`，相关页面即时应用；需要系统调用的项（自启、快捷键、入口模式）单独处理并回滚失败。
- 「恢复默认」按分组重置：外观、今日显示、快捷键（分别定义于各模块）。

---

## 8. 数据完整性与容量

### 8.1 完整性

- 外键：`plan_id` 级联删除；`parent_id` 级联删除；依赖级联删除；`notes.converted_task_id` 置空。
- 启动时 `quick_check`（≤100MB，通常 < 100ms）；失败则进入 DATA-02 的恢复引导，禁止继续写入。
- 创建备份后对备份文件做完整 `integrity_check`。
- 迁移后执行 `foreign_key_check`，发现孤儿数据中止并回滚。

### 8.2 容量预估与上限

| 数据 | 预期 | 设计上限 | 说明 |
| --- | --- | --- | --- |
| 计划 | 50 | 1,000 | 单计划阶段 ≤ 50，任务 ≤ 2,000 |
| 任务 | 10,000 | 100,000 | 单树深度 ≤ 50（UI 显示限制，数据不限） |
| 依赖边 | 5,000 | 50,000 | 单任务前置 ≤ 20 |
| 小记 | 5,000 | 100,000 | 单条 ≤ 20,000 字符 |
| 日记 | 3,000 | 50,000 | 单篇 ≤ 50,000 字符 |
| 数据库 | < 20MB | 500MB | 超过 200MB 时提示用户备份与清理 |

- 单页列表每次加载 ≤ 200 条；超出使用滚动分页。
- 长文本编辑超过 20,000 字符时提示性能风险（见各模块）。

---

## 9. 验收标准

| 编号 | 验收标准 |
| --- | --- |
| AC-ARCH-16 | 全新启动创建 v2 数据库，`user_version=2`，包括提醒两表及所有索引。 |
| AC-ARCH-17 | v1 数据库可迁移至 v2，原数据不变；v2 重复启动不重复迁移。 |
| AC-ARCH-18 | 迁移失败时原数据库保持可用，且已生成预迁移备份。 |
| AC-ARCH-19 | 强制结束进程后重启，已提交的事务全部存在，无半个任务树移动。 |
| AC-ARCH-20 | 数据库版本高于应用支持版本时拒绝打开并给出明确提示，不修改文件。 |
| AC-ARCH-21 | 备份文件可在另一平台/设备恢复（schema 兼容时），设置与数据完整。 |
| AC-ARCH-22 | 配置项缺失或非法时应用正常启动并使用默认值。 |
| AC-ARCH-23 | 1 万任务规模下今日/月历/计划详情查询 p95 < 10ms（参考机）。 |
| AC-ARCH-24 | 删除计划（归档后）后，其任务、依赖、笔记关联均无残留（外键检查通过）。 |

---

## 10. 待确认问题

> **当前状态：无待确认项。** 
> 如有新增未决问题，请在此登记并注明「当前默认处理」。已关闭的问题归档至 [QA-02](../10-quality/02-risks-and-open-questions.md) §7。
