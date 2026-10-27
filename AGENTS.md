# AGENTS.md — FastNote 项目协作规则

本文件定义 Agent（以及任何协作者）在本项目中必须遵循的工作方式。目标：让开发过程可控、可追溯、可回滚，避免"边想边写"的 vibe coding 失控，同时保持 git 历史干净、易于排查问题。

---

## 0. 最高优先级规则

1. **不允许直接在 `main` 分支上提交或修改代码。** 详见第 1 节。
2. **任何功能在合并回 `main` 之前，必须完成第 4 节的提交前 Review 流程。**
3. 如果 spec 文档中标记了"待确认问题"，或用户的需求超出了已 Confirmed 的范围，**必须先停下来向用户提问**，禁止直接猜测实现。
4. **Agent 不主动提交**：任何修改完成后，先向用户汇报改动并让其查看实际效果；只有用户明确指示（如"提交"/"推送"）时才执行 `git commit` / `git push`。详见 1.4 第 4 条。

---

## 1. Git 工作流规范

### 1.1 分支模型

本项目采用简化版 GitHub Flow + 显式 hotfix。

分支类型：

| 分支前缀 | 用途 | 从哪切出 | 合入哪里 |
|---|---|---|---|
| `main` | 唯一的稳定分支，任何时候都应该能正常构建/运行 | - | - |
| `feature/<module>-<slug>` | 开发一个已 Confirmed 的功能 | `main` | `main` |
| `fix/<slug>` | 修复日常开发中发现的非紧急 bug | `main` | `main` |
| `hotfix/<slug>` | 修复已发布版本的紧急问题 | 最新 release tag 或 `main` | `main`，并立即打 tag |
| `docs/<slug>` | 只修改 `plan/`、`README` 等文档，不涉及代码 | `main` | `main` |
| `chore/<slug>` | 依赖升级、构建配置、格式化等 | `main` | `main` |

**规则：**
- `main` 分支受保护，Agent 不直接在 `main` 上 `git commit`，一律先开分支再改代码。
- 一个分支只做一件事，尽量对应一个 `spec/<module>/*.md`（或其中一条验收标准）。
- 分支存活时间尽量短（几天内合并）。

### 1.2 命名规范

`<类型>/<简短描述，小写中划线分隔>`，例如：

```
feature/today-section-filter
fix/calendar-month-overlap
hotfix/db-corruption-recovery
docs/plan/rust-code-conventions
chore/bump-rusqlite
```

### 1.3 提交信息（Commit Message）规范

主题行使用 **Conventional Commits 类型前缀** + 中文附加信息：

```
<type>(<scope>): <subject>
```

- `type`：`feat` / `fix` / `docs` / `chore` / `refactor` / `test` / `style` / `perf` / `build`
- `scope`：模块名，例如 `core` / `store` / `platform` / `ui` / `app`
- 简洁、祈使句、主题行 ≤ 50 字符、不加句号

示例：

```
feat(core): add TodayView seven-day grouping
fix(store): migrate v1 schema to v2 safely
docs(plan): confirm Rust panic policy for release
chore(deps): bump slint to 1.x.latest
```

### 1.4 合并策略与避免冲突

1. 合并前先 rebase `origin/main`；**自己的未分享分支可以 rebase**，禁止对已推送共享的分支做 force-push。
2. 合并回 `main` 使用 `--no-ff`，保留合并提交以便整体撤销。
3. 每次开发 session 开始时先 `git fetch` 并 rebase 一次，尽早发现冲突。
4. **提交时机由用户把控**：Agent 完成修改后先汇报，用户明确指示后才提交。
5. 不要手改生成文件/锁文件来"解决冲突"，出现冲突时重新生成。

### 1.5 Tag 与发布

- 每完成一个 Phase 或每次提交商店审核前，在 `main` 上打 tag：`v0.1.0`、`v0.2.0`（语义化版本）。
- Tag 只打在 `main` 上。

---

## 2. Spec-Driven 开发规则

### 2.1 文档位置

所有产品/功能规格文档都在 `spec/` 目录下：

```
spec/
├── README.md                    文档规范与流程说明
├── 00-architecture/             总览与架构
│   ├── 01-tech-and-runtime.md    技术栈与运行时
│   ├── 02-architecture.md        系统架构与数据流
│   ├── 03-domain-model.md        领域模型与业务规则
│   ├── 04-storage-and-data.md    数据存储与配置项
│   ├── 05-ui-design-system.md    UI 设计系统
│   └── 06-platform-integration.md 平台集成
├── 01-app-shell/                应用外壳
├── 02-desktop-entry/            桌面入口（悬浮球/托盘）
├── 03-today-tasks/              今日任务
├── 04-calendar/                 日历
├── 05-notes/                    小记
├── 06-journal/                  日记
├── 07-plan-board/               计划板
├── 08-settings/                 设置中心
├── 09-data-management/          数据管理
├── 10-quality/                  质量与验收
└── 11-reminders/                提醒通知
```

新增模块时复制已有的 spec 模板到 `spec/<模块编号>-<模块名>/`。

### 2.2 Spec 状态机

每个 spec 顶部有 `状态` 字段：

```
Draft → Confirmed → In Progress → Done
```

- **Draft**：初稿，可能存在"待确认问题"，**不允许基于它写正式功能代码**。
- **Confirmed**：所有"待确认问题"已回答，范围和边界清晰，**可以开始开发**。
- **In Progress**：已经开了对应 feature 分支在开发。
- **Done**：功能已合并进 `main` 并通过 Review。

### 2.3 Agent 的强制行为

- 开始写功能代码前，确认对应 `spec/<module>/` 是否存在且状态为 `Confirmed`/`In Progress`。
- 如果 spec 不存在、还是 `Draft`、或者用户需求超出 spec 范围 —— **停下来提问**，不要凭经验猜测。
- 遇到 spec 描述不清楚/与实际冲突时，暂停编码，更新 spec 的"待确认问题"并询问用户。
- 唯一允许跳过 spec 的场景：一次性 spike/原型验证，写在独立分支（`spike/xxx`），**不合并进 main**。

---

## 3. 工程约束与 Rust 开发规范

以下约束适用于所有 Rust 代码模块。本文档覆盖语言层面规范、编码风格、错误处理、并发模型、测试、构建与依赖管理。任何 spec 都不得违反这些约束，除非用户明确要求修改本文件。

### 3.1 语言基线

| 项 | 要求 |
|---|---|
| Edition | `edition = "2021"`，所有 crate 统一 |
| MSRV | 1.80（写在本文件头部 `ARCH-01 §2.1` 也需同步） |
| Target | **v1 开发只跑 Windows**（x64）。技术栈选择 Slint/winit 等跨平台方案是为了后续扩展到 macOS/Linux 时不需要重构，不是当前开发目标。 |
| Release 优化 | `opt-level = 3`、`lto = "thin"`、`panic = "abort"`、`strip = true` |

### 3.2 Crate Workspace 结构

```
Cargo.toml                  # workspace root
crates/
├── fastnote-core/          # 纯领域：实体、规则、服务 trait、事件、DTO
├── fastnote-store/         # SQLite 仓储实现：迁移、备份、SQL
├── fastnote-platform/      # 跨平台能力：托盘/悬浮球/快捷键/自启/单实例/主题
├── fastnote-ui/            # Slint .slint 文件、视图模型、页面组件
└── fastnote-app/           # 二进制入口：装配、生命周期、命令总线
```

**依赖方向（严格单向，不可反向）**：

```
fastnote-app → fastnote-ui → fastnote-core
      │              │              ↑
      ├────→ fastnote-platform       │
      └────→ fastnote-store ─────────┘
```

- `fastnote-core` 是纯 Rust 域，**不依赖任何 IO/UI/平台库**。只引入 `serde`、`chrono`、`uuid`、`thiserror` 等轻量数据型依赖。
- `fastnote-store` 实现 core 定义的 Repository trait，不引用上层模块。
- `fastnote-ui` 只依赖 core 的 DTO 与服务 trait，通过 AppFacade（app 层注入）调用命令。
- `fastnote-platform` 不持有业务状态；通过回调/事件与 app 交互。

**新 crate 审批规则**：新增 crate 必须在 PR 中说明理由和依赖方向，禁止随意拆分。

### 3.3 错误处理（Error Handling）

#### 3.3.1 核心原则

1. **生产代码禁止 `unwrap()`**。仅在以下情况例外：
   - 启动期不可恢复的不等式（invariant）检查，使用 `expect("...")` 并附原因。
   - 单元测试内部（测试函数体内）。
2. **所有错误必须可追溯**。使用 `thiserror` 定义模块级枚举错误类型：

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Validation: {code}")]
    Validation { code: ErrorCode, field: Option<String>, detail: Option<String> },
    #[error("Not found: {entity}={id}")]
    NotFound { entity: EntityKind, id: String },
    #[error("Conflict: {code}")]
    Conflict { code: ErrorCode, related: Vec<String> },
    #[error("Storage error (log_id={log_id}): {detail}")]
    Storage { code: StorageErrorCode, log_id: String, detail: String },
    // ... 其它
}
```

3. **错误码稳定**。`AppError` 中的错误码一旦写入 spec 就不再变更值，仅扩展新码。UI 通过错误码映射中文文案。
4. **内部详情不外露**。业务逻辑返回具体错误类型（枚举）；日志记录原始详情；**界面只展示可读中文文案**。
5. **Fallible vs Infallible**：
   - IO 操作、网络（未来）、文件读写 → 返回 `Result<T, AppError>`。
   - 纯函数/领域规则 → 根据上下文决定；计算不变量可以用 `panic!` 但必须有防御性前置校验。
6. **Context chaining 谨慎使用**。不使用 `anyhow`；用 `thiserror` 的错误源链（`#[from]`）即可，避免丢失原始错误类型信息。

#### 3.3.2 Result 风格约定

- 函数签名明确返回 `Result` 的类型；调用处按场景选择 `?` / `match` / `ok_or()` / `unwrap_or_else()`。
- 批量操作中单个失败需要部分成功时，收集错误列表而非早期 return：

```rust
pub fn batch_update(items: &[TaskId]) -> BatchResult {
    let mut errors = Vec::new();
    let mut success_count = 0;
    for item in items {
        if update_single(item).is_err() {
            errors.push(item);
        } else {
            success_count += 1;
        }
    }
    BatchResult { success_count, failed: errors }
}
```

### 3.4 命名约定

| 项 | 规则 | 示例 |
|---|---|---|
| 模块（文件名/目录） | snake_case | `task_service.rs`, `plan_board/` |
| 类型（struct/enum/trait） | PascalCase | `TaskSummary`, `PlanService`, `Repository` |
| 常量 | UPPER_SNAKE_CASE | `MAX_TITLE_LENGTH`, `DEFAULT_SORT_ORDER` |
| 变量/参数 | snake_case | `task_id`, `start_date` |
| 函数/方法 | snake_case | `create_task`, `get_today_view` |
| 宏 | snake_case | `macro_rules! check_bounds` |
| 类型参数 | 单字母或简短描述性 | `T`, `RowType`, `EventSink` |
| Error 类型 | `<Module> + Error` | `AppError`, `StoreError`, `PlatformError` |
| 事件类型 | `<Subject>Changed` 或 `<Action>` | `TasksChanged`, `CreateTask` |
| 命令类型 | `<Verb>Noun` | `CreateTask`, `UpdateSettings` |
| Query 类型 | `<Noun>` 或 `<Subject>` | `TodayQuery`, `TaskDetail` |
| Feature gate 标志 | crate_name_feature | `fastnote_platform_macos` |
| 公共 API pub fn/struct | 参考 rust-lang/api-guidelines | 见下方 |

**API 稳定性层级标记**（public API 建议）：
- `pub`：公开接口，需遵循语义化版本。
- `pub(crate)`：仅 crate 内部可见，随 refactoring 调整不受限。
- `#[doc(hidden)]`：测试 helper 或过渡层，不出 public 文档。

### 3.5 模块组织与代码分割

每个 crate 推荐如下分层结构（以 `fastnote-core` 为例）：

```
crates/fastnote-core/src/
├── lib.rs                     # pub mod 声明 + 公共导出
├── domain/
│   ├── entity.rs              # Plan, Task, Note, JournalEntry 等实体 struct
│   ├── status.rs              # TaskStatus, Priority, PlanStatus 等枚举
│   └── invariant.rs           # INV-xx 验证函数
├── service/
│   ├── task_service.rs        # TaskService（CRUD + 派生计算）
│   ├── plan_service.rs
│   ├── note_service.rs
│   └── settings_service.rs
├── derive/                    # 派生规则函数（逾期、进度、今日分组等）
│   ├── overdue.rs
│   ├── today_view.rs
│   └── progress.rs
├── ports/
│   ├── repository.rs          # Repository trait 定义
│   ├── clock.rs               # Clock trait（测试友好）
│   └── event_sink.rs          # EventSink trait
├── dto/                       # 读模型 DTO（TodayView, TaskSummary, CalendarMonth...）
│   ├── task.rs
│   ├── calendar.rs
│   └── plan.rs
├── event.rs                   # AppEvent, Command, Query 枚举
├── command.rs
├── query.rs
├── error.rs                   # AppError 定义 + 错误码
└── constants.rs               # 全局常量（长度限制、默认值等）
```

**规则：**
- 每个 `.rs` 文件不超过 **400 行**；超过则按职责拆分模块。
- 模块间导入顺序：`std` → `external crates` → `crate internal` → `self`。
- `lib.rs` 只做 `pub mod` 和 `pub use` 重导出，不含业务逻辑。
- 测试放在同一文件的 `#[cfg(test)] mod tests { ... }` 块中；集成测试放 `tests/` 目录。

### 3.6 并发与线程模型

本项目**不使用异步运行时**（Tokio 等），全部使用 `std::thread` + `crossbeam-channel`。

#### 3.6.1 线程安全原则

1. **无共享可变状态**。线程间通信仅通过通道（channel），不使用 `Arc<Mutex<>>` 传递业务对象。
2. **命令/查询走 channel**。UI 线程将 `Command` / `Query` 放入有界通道（容量 256），DB 工作线程消费。
3. **跨线程 UI 更新必须经 `invoke_from_event_loop`**。Slint 的 GUI 只能在主线程调用。

```rust
// UI 线程投递命令
command_tx.send(Command::CreateTask(create_cmd)).ok();

// DB 工作线程处理
loop {
    let cmd = command_rx.recv()?;
    let result = execute_command(&services, cmd);
    // 结果回传
    ui_tx.invoke(...)?;
}
```

4. **Channel 容量**。所有业务通道设为有界 `bounded(256)`；满时丢弃最新或阻塞生产者（根据场景选择）。
5. **线程退出清理**。所有后台线程需 join 或在 drop 前发送终止信号。

#### 3.6.2 数据一致性

- SQLite 使用单写连接 + WAL 模式；写事务使用 `BEGIN IMMEDIATE`。
- 所有写操作在一个事务内完成；成功后统一提交，失败 `ROLLBACK`。
- 禁止在事务内调用 UI 回调或进行阻塞 IO。

### 3.7 Unsafe 代码规则

1. **默认关闭 unsafe**。所有代码优先使用 safe Rust。
2. **必须用 unsafe 的场景**（需逐一评估）：
   - FFI 绑定（如平台原生 API）→ 应使用已有 safe wrapper crate（如 `winit`、`muda`）。
   - SIMD/内联汇编 → 暂不需要，若后续性能分析表明必需才引入。
   - 零拷贝解析 → 需谨慎评估生命周期。
3. **unsafe 块必须附带 `// SAFETY:` 注释**，解释为什么这段 unsafe 操作是安全的：

```rust
unsafe {
    // SAFETY: ptr 来自 Box::into_raw，已被转换为裸指针，
    // 此处仅为解引用同一个合法内存地址，生命周期由 Caller 保证。
    *ptr.add(index) = value;
}
```

4. **CI 静态检查**：`cargo clippy -- -D warnings` 会拦截常见 unsafe 滥用；`cargo deny` 检查许可证和漏洞。

### 3.8 测试策略

| 层级 | 位置 | 内容 | 要求 |
|---|---|---|---|
| 单元测试 | `src/file.rs` 内的 `#[cfg(test)]` | 纯函数、领域规则、派生计算 | **必须覆盖** §8、§9、§10 的全部分支 |
| 仓储集成测试 | `crates/fastnote-store/tests/` | 真实 SQLite 上的 CRUD、迁移、事务回滚、备份恢复 | 使用临时目录 + `:memory:` 或文件 |
| UI 冒烟测试 | `crates/fastnote-ui/` | Slint 渲染是否崩溃、关键页面初始化 | CI 自动化 |
| 端到端（手动） | 各模块验收标准 AC-* | 三平台冒烟测试 | 人工执行 + 截图存档 |

**测试编码规范**：
- 测试函数以 `test_` 前缀开头（不用 `_` 后缀）。
- 测试名称描述意图而非实现细节："test_overdue_returns_false_when_no_due_date"。
- 领域层的纯函数测试必须达到分支覆盖率 ≥ 90%。
- 使用 `assert!(...)` / `assert_eq!(...)` / `assert_matches!(...)`；不用第三方断言库。
- 时间相关的测试使用注入的 `Clock` trait（不依赖真实时间）。

### 3.9 格式与静态检查

CI 流水线（或本地 pre-commit）必须通过：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check licenses vulnerabilities
cargo test --workspace --all-features
```

**本地开发习惯**：
- 每次提交前跑一遍 `cargo fmt` 和 `cargo clippy`。
- 禁止提交带有 `clippy` 警告的代码。
- `Cargo.lock` 纳入版本控制，禁止手动编辑。

### 3.10 依赖管理规则

1. **新增依赖需论证**。PR 中必须说明：目的、体积影响、内存占用、许可证、替代方案。
2. **禁止的依赖类别**：
   - 网络/遥测类 SDK（第一版纯本地）。
   - WebView 框架（Electron/Tauri/Browser）。
   - ORM 框架（Diesel/SeaORM，使用手写 SQL）。
   - 异步运行时（Tokio/async-std，用 std::thread）。
3. **版本锁定**：使用 `=` 精确锁定主版本号，允许 patch 级更新：

```toml
[dependencies]
slint = "1.7"          # 允许 1.7.x 更新
rusqlite = { version = "0.31", features = ["bundled", "backup"] }
chrono = { version = "0.4", default-features = false, features = ["clock"] }
uuid = { version = "1", features = ["v7", "serde"] }
serde = { version = "1", features = ["derive"] }
thiserror = "1"
tracing = "0.1"
crossbeam-channel = "0.5"
```

4. **Feature flags 最小化**。每个依赖只启用需要的 feature；关闭不必要的可选特性以减少编译时间。
5. **定期依赖审计**：`cargo deny check` 检查许可证合规性和已知 CVE。

### 3.11 日志规范

- 使用 `tracing` crate；release 下零开销（level 低于阈值时无运行时成本）。
- **禁止记录敏感数据**：任务标题、描述、小记/日记正文绝不能写入日志。
- 允许记录：ID、数量、耗时、错误码。
- 日志策略：按日滚动，单文件 ≤ 5MB，保留 7 天。
- 关键事件必须记录：启动耗时、DB 迁移版本、命令耗时（>50ms 记 warn）、长任务进度。

### 3.12 序列化与数据格式

- JSON 用于文件边界（导入导出、备份元数据）；不作为主存储格式。
- 日期存储：`NaiveDate` → `TEXT` 列格式 `YYYY-MM-DD`；`DateTime<Utc>` → RFC 3339 UTC。
- ID：UUID v7 小写字符串；所有实体主键由应用层生成，数据库不依赖自增。
- `serde` 的 `rename` 属性用于 JSON 字段名与 Rust 字段名的映射；字段改名需考虑向后兼容。

### 3.13 文档与内联注释

- 公共 API（`pub`）必须编写 `/// doc comment`，包含用途说明和简要示例（如有）。
- 复杂业务规则使用引用 spec 编号的内联注释：

```rust
// INV-07: start_date <= due_date（两者都有时）
// 见 spec/03-domain-model.md §4
```

- 不编写无意义的 `// TODO` 而不跟进；TODO 必须关联 spec 问题编号或有明确截止日期。

### 3.14 密码学与安全性

- 第一版**不加密**：数据库和备份均为明文 SQLite 文件。
- 依赖操作系统账户权限保护数据。
- 不在日志中记录任何形式的密钥、token 或凭证。

---

## 4. 提交前 Review 流程（Definition of Done）

每个 feature/fix 分支在执行 `git merge` 回 `main` 之前，必须走完以下 checklist；发现问题要先修复，而不是直接合并：

- [ ] **Spec 对齐**：代码行为与对应 spec 文档一致，无额外假设。
- [ ] **Rust 规范检查**：
  - [ ] `cargo fmt --all --check` 通过
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过
  - [ ] 无 `unwrap()`（除启动期和测试外）
  - [ ] 无不必要的 `unsafe` 块
  - [ ] 所有 `pub` API 有文档注释
- [ ] **架构合规**：依赖方向正确，未出现反向依赖。
- [ ] **测试覆盖**：新增/修改的逻辑有对应的单元测试。
- [ ] **错误处理**：所有 fallible 操作正确处理，错误码与 UI 映射已定义。
- [ ] **跨模块一致性**：若有多个页面受影响，刷新矩阵中的所有相关页面都已处理。
- [ ] **性能预算**：关键操作未超出 §5.1 目标（启动 ≤600ms、查询 p95 <10ms）。
- [ ] **无敏感数据泄露**：日志、错误提示不包含正文内容。
- [ ] **CI 通过**：所有 CI 阶段（lint/test/build）绿灯。

---

## 5. 目录约定

```
FastNote/
├── Cargo.toml                 # workspace 根
├── crates/
│   ├── fastnote-core/         # 领域核心（实体、规则、服务、事件、DTO）
│   ├── fastnote-store/        # 数据存储（SQLite、迁移、备份）
│   ├── fastnote-platform/     # 平台适配（托盘、悬浮球、快捷键等）
│   ├── fastnote-ui/           # UI 层（Slint 组件、视图模型）
│   └── fastnote-app/          # 应用入口（二进制、装配、生命周期）
├── ui/                        # 共享图片/图标资源（SVG/PNG）
├── spec/                      # 需求规格与设计文档
├── plan/                      # 产品规划文档（vision、路线图）
├── product.md                 # 产品需求总纲
└── AGENTS.md                  # 本文件（协作规则）
```

保持一个 spec 对应一组代码目录，方便判断"这次改动属于哪个 spec、应该开在哪个分支"。

---

## 6. 当规则本身需要修改时

本文件（`AGENTS.md`）不是产品 spec，而是流程规则。修改它本身也要走 `docs/` 分支 + 明确告知用户改了什么、为什么改，不允许 Agent 单方面静默调整规则。
