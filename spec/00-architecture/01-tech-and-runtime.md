# ARCH-01 技术栈与运行时

> 文档编号：ARCH-01 ｜ 模块：00 总览与架构 ｜ 状态：已定稿 v0.1 ｜ 更新：2026-10-23
> 上游：[product.md](../../product.md) §1.1、§8 ｜ 下游：[ARCH-02 架构](02-architecture.md)、[ARCH-06 平台](06-platform-integration.md)

---

## 1. 目的与范围

定义 FastNote 第一版的技术选型、运行时模型、性能预算、构建发布方式与编码约定，作为所有模块实现的技术基线。

**范围内**：语言/框架/关键依赖、线程模型、启动与退出流程、资源预算、目录结构、发布目标。
**范围外**：具体 Slint 组件实现（后续详尽级文档）、CI 流水线脚本、商店发布材料。

---

## 2. 技术选型

### 2.1 基线

| 项 | 选型 | 说明 |
| --- | --- | --- |
| 语言 | Rust stable，Edition 2021，MSRV 1.80 | 稳定、单二进制、无 GC、跨平台。 |
| UI | Slint 1.x 最新稳定版 | 声明式 UI、编译期检查、原生渲染、内存占用低。 |
| 存储 | SQLite 3（`rusqlite`，`bundled` + `backup`） | 本地单文件、WAL、事务、在线备份。 |
| 平台集成 | `winit` 原生窗口 + `tray-icon` / `muda` / `global-hotkey` / `auto-launch` / `single-instance` | 与 Slint 的 winit 后端同源，避免引入第二套窗口系统。 |
| 并发 | `std::thread` + `crossbeam-channel` | 不引入 async runtime（Tokio 等），降低常驻内存与复杂度。 |

> **设计决策 D-18：** 第一版不引入异步运行时。应用只有「UI 线程 + DB 工作线程 + 少量短生命周期后台线程」，用通道通信即可满足需求；async 生态会显著增加二进制体积和内存基线。

### 2.2 关键依赖

| 依赖 | 用途 | 约束 |
| --- | --- | --- |
| `slint` / `slint-build` | UI 与构建 | 仅启用 `backend-winit`、`renderer-femtovg`；不使用 Qt 后端。 |
| `rusqlite` | 数据访问 | `bundled` 编入 SQLite，避免用户环境缺库；`backup` 用于在线备份。 |
| `chrono` | 日期时间 | 日期用 `NaiveDate`，时刻用 `DateTime<Utc>`。 |
| `uuid` (v7) | 实体 ID | 时间有序、字符串存储，便于调试与合并。 |
| `serde` / `serde_json` | 导入导出、配置 JSON 值 | 仅用于文件边界，不作为主存储。 |
| `thiserror` | 领域错误 | 错误码稳定，供 UI 映射文案。 |
| `tracing` + `tracing-subscriber` + `tracing-appender` | 日志 | 按日滚动，单文件 ≤ 5MB，保留 7 天。 |
| `crossbeam-channel` | 线程通信 | 有界通道，命令队列容量 256。 |
| `tray-icon` / `muda` | 托盘图标与原生菜单 | Linux 需要 AppIndicator 运行时支持，见 ARCH-06。 |
| `global-hotkey` | 全局快捷键 | X11 可用；Wayland 走 Portal 或降级。 |
| `auto-launch` | 开机启动 | 三平台分别使用注册表 / LaunchAgent / autostart。 |
| `single-instance` | 单实例 | 第二次启动唤醒已运行实例。 |
| `directories` | 标准数据目录 | 按 OS 约定定位数据/日志/备份目录。 |
| `dark-light` | 跟随系统主题 | 失败时回退浅色，并允许手动指定。 |
| `rfd` | 原生文件选择/保存对话框 | 备份、恢复、导入、导出。 |
| `arboard` | 剪贴板 | 复制小记、复制任务信息。 |
| `open` | 打开数据目录 | 调用系统文件管理器。 |

依赖新增规则：新增第三方依赖必须在 PR 中说明体积/内存影响；禁止引入网络、遥测、WebView 类依赖（与 D-06 冲突）。

### 2.3 不使用的技术

- **Electron / Tauri / WebView**：与轻量目标冲突。
- **Tokio / async-std**：无高并发 IO 需求。
- **ORM（Diesel/SeaORM）**：查询简单且需精细控制，使用手写 SQL + 仓储层。
- **前端状态框架**：Slint 模型 + 应用服务层足够。
- **云服务/SDK**：第一版纯本地。

---

## 3. 运行时模型

### 3.1 线程与职责

| 线程 | 职责 | 约束 |
| --- | --- | --- |
| UI 线程（main） | Slint 事件循环、窗口、视图模型、用户输入 | 禁止任何阻塞 IO / DB 调用超过 5ms；原生文件对话框等用户主动等待的模态交互除外。 |
| DB 工作线程 | 唯一写连接、事务、查询、迁移、备份 | 串行执行命令；崩溃不影响已提交事务。 |
| 平台事件线程 | 托盘/全局快捷键/主题变化回调 | 只投递事件到 UI 线程，不直接改状态。 |
| 短任务线程 | 导出、备份校验等长任务 | 带进度回调，可取消；同一时刻最多 1 个。 |

> 单写连接避免 SQLite 多写锁竞争；WAL 模式下读操作也走同一工作线程，换取实现简单与顺序一致。数据量（≤1 万任务）下查询延迟远低于一帧预算。

### 3.2 命令与查询流

```text
用户操作
  → Slint 回调（UI 线程）
  → UiCommand（校验轻量输入、置 loading）
  → crossbeam channel
  → DB 工作线程：AppService → Repository → 事务提交
  → DomainEvent + Result
  → invoke_from_event_loop
  → UI 线程：刷新受影响模型 / Toast / 错误提示
```

- 所有跨线程 UI 更新必须经 `slint::invoke_from_event_loop`。
- 命令携带 `request_id`，用于丢弃过期响应（例如快速切换日期时的旧查询）。
- 查询同样走工作线程；UI 先展示上一次结果 + 轻量 loading，避免闪烁。

### 3.3 启动流程

1. 解析启动参数与环境（`--hidden` 表示开机自启，仅显示常驻入口；`--restored` 表示刚从备份恢复，主窗首帧后展示一次性提示）。
2. 单实例检查；已有实例则发送「唤醒」消息并退出本进程。
3. 初始化日志、加载配置（设置表）。
4. 打开数据库（`PRAGMA` 配置）→ 执行迁移 → 完整性快检；失败进入「数据恢复」流程（见 09）。
5. 创建主窗口视图模型（不显示）、创建常驻入口（悬浮球或托盘）。
6. 注册全局快捷键、开机启动状态校验。
7. `--hidden` 或开机自启：只显示入口；否则显示主界面并进入今日任务。
8. 启动日期滚动定时器（30s 检查本地日期是否变化）；DB 就绪后启动 `ReminderScheduler`，执行最近 24 小时错过提醒汇总，再按最近到期时刻调度（[REM-01](../11-reminders/01-reminders-and-notifications.md)）。`--hidden` 启动同样启用；`--restored` 则先重建当前时刻基线，不补发备份历史。

**启动时间预算**：进程启动到入口可见 ≤ 600ms；到主窗口首帧 ≤ 1.0s（参考机见 §5.2）。数据库迁移/完整性检查超时 300ms 时先显示 UI，在后台完成。

### 3.4 退出流程

1. 捕获退出意图（托盘菜单、悬浮球菜单、设置页、系统会话结束）。
2. 冲刷未落盘的文本编辑（日记/小记立即提交）。
3. 停止提醒领取与定时器，收敛在途投递结果（不为通知无限等待）；注销全局快捷键、移除托盘图标/悬浮球窗口。完全退出后不触发提醒；关闭主窗口但驻留入口时继续调度。
4. 关闭数据库前执行 `PRAGMA wal_checkpoint(TRUNCATE)`（尽力而为）。
5. 正常退出码 0；系统会话结束允许 2s 超时强制退出。

关闭主窗口 ≠ 退出应用：默认最小化到常驻入口（可在设置改为退出），见 02-desktop-entry/02。

### 3.5 崩溃与恢复

- 所有写操作单事务提交；WAL 保证崩溃后已提交数据不丢。
- panic hook 记录日志（不含正文）并尽力 checkpoint；release 使用 `panic = "abort"`。
- 启动时若数据库损坏：保留原文件副本 → 提示用户从备份恢复或新建空库，禁止静默重建。

---

## 4. 工程结构

```text
FastNote/
├── Cargo.toml                 # workspace
├── crates/
│   ├── fastnote-core/         # 领域实体、规则、服务、仓储 trait、错误、事件、DTO
│   ├── fastnote-store/        # SQLite 仓储实现、迁移、备份
│   ├── fastnote-platform/     # 托盘、悬浮球窗口、全局快捷键、自启、主题、单实例
│   ├── fastnote-ui/           # .slint 文件、视图模型、页面组件、文案映射
│   └── fastnote-app/          # 二进制入口、依赖装配、生命周期、命令总线、后台任务
├── ui/                        # 共享图片/图标资源（SVG/PNG）
├── spec/                      # 本设计文档
└── product.md
```

**依赖方向（单向）**

```text
fastnote-app → fastnote-ui → fastnote-core
      │              │              ↑
      ├──────→ fastnote-platform    │
      └──────→ fastnote-store ──────┘
```

- `fastnote-core` 不依赖任何 IO/UI/平台库。
- `fastnote-store` 实现 `fastnote-core` 定义的仓储 trait。
- `fastnote-ui` 只依赖 core 的 DTO/服务 trait，不直接访问 SQLite。
- `fastnote-platform` 不依赖 UI；通过回调/事件与 app 层交互。

---

## 5. 性能与资源预算

### 5.1 指标

| 指标 | 目标 | 测量方式 |
| --- | --- | --- |
| 入口可见（冷启动） | ≤ 600ms | 进程创建 → 悬浮球/托盘首次绘制 |
| 主窗口首帧（冷启动） | ≤ 1.0s | 进程创建 → 今日任务首帧 |
| 常驻内存（主窗隐藏） | ≤ 80MB RSS | 启动 10 分钟后，仅入口 |
| 主窗打开内存 | ≤ 130MB RSS | 今日任务 + 悬浮球 |
| 空闲 CPU | < 0.5% | 无交互 5 分钟均值 |
| 列表滚动 | 60fps | 1000 条任务列表快速滚动 |
| DB 查询 p95 | < 10ms | 1 万任务、5 千小记、3 千日记 |
| 任务创建到界面可见 | < 100ms | 本地命令往返 |
| 安装包体积 | ≤ 25MB | Windows MSI / macOS DMG / AppImage |

### 5.2 参考机

- Windows 11，4 核 2.0GHz，8GB RAM，SSD，集显。
- macOS 12+，Apple Silicon（M1）与 Intel 各一台。
- Ubuntu 22.04，4 核，8GB RAM，X11；Wayland 单独记录。

### 5.3 资源控制策略

- 悬浮球/托盘窗口无持续动画；仅 hover/press 时重绘，空闲不产生帧。
- 主窗口隐藏时停止其定时器与重绘；保留模型但释放不可见列表的渲染资源。
- 列表使用虚拟化（仅渲染可视行 + 缓冲 5 行），单页加载上限 200 行，滚动分页。
- 图片/图标全部编译进二进制或按需加载小尺寸 SVG，不引入大图。
- 小记超过 20,000 字符（上限）、日记超过 20,000 字符时启用分段渲染/输入限流；日记上限 50,000（见 DIARY-01）。不因长文本阻塞 UI 线程。
- 日志异步写入，不阻塞 UI。

---

## 6. 构建与发布

### 6.1 Release 配置

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

### 6.2 平台产物

| 平台 | 最低版本 | 产物 | 说明 |
| --- | --- | --- | --- |
| Windows | Windows 10 1809+ | `.msi`（WiX）或 NSIS `.exe` | x64 + arm64 可选；WebView 不依赖。 |
| macOS | 12 Monterey+ | `.dmg` / `.app` | Universal（arm64 + x86_64）；需签名与公证。 |
| Linux | glibc 2.31+ | AppImage + `.deb` | AppImage 打包 AppIndicator 运行库；X11 完整，Wayland 尽力。 |

- SQLite 使用 `bundled`，不依赖系统库。
- Linux 托盘依赖 `libayatana-appindicator3`；AppImage 内置，deb 声明依赖。
- macOS 使用 `LSUIElement=1`（无 Dock 图标），入口为菜单栏图标或悬浮球。
- 版本号语义化 `MAJOR.MINOR.PATCH`；数据库 schema 版本独立递增。

### 6.3 发布前检查

- 三平台冒烟：启动、入口、快捷键、创建任务、备份/恢复。
- 升级迁移：用上一版本数据库副本启动新版本，数据完整。
- 资源预算回归（§5.1）。
- 代码签名/公证（macOS）、安装包静默安装/卸载测试（Windows）。

---

## 7. 编码约定

1. **错误**：`fastnote-core::AppError` 使用 `thiserror`；生产代码禁止 `unwrap()`，仅允许在测试与启动期不可恢复不变量使用 `expect("...")`。
2. **日期**：业务日期只用 `NaiveDate`（无时区）；时间戳统一 UTC；转换只在一处（`time` 模块）。
3. **ID**：UUID v7 小写字符串；所有实体主键由应用层生成，数据库不依赖自增。
4. **SQL**：全部参数化；迁移脚本只增不改；仓储层不得拼接用户输入。
5. **UI 文案**：中文文案集中常量管理，错误通过 `E_*` 码映射；禁止在 UI 拼接内部错误细节。
6. **日志**：`tracing`；禁止记录任务标题、描述、小记/日记正文；允许记录 ID、数量、耗时。
7. **可测试性**：领域规则纯函数化；仓储 trait 便于内存实现；时间通过 `Clock` trait 注入（便于测试跨日）。
8. **格式与静态检查**：`cargo fmt`、`cargo clippy -- -D warnings`、`cargo deny` 检查许可证与漏洞。
9. **提交**：小而聚焦；涉及 schema/规则的提交必须同步 spec 文档。

---

## 8. 技术风险与预研项

| 编号 | 风险 | 影响 | 应对 |
| --- | --- | --- | --- |
| SPIKE-01 | Slint 透明/无边框/置顶悬浮球窗口在三平台表现不一 | 入口形态不可用 | 第 1 周做三平台最小验证；必要时用 `winit` 原生窗口承载 Slint 渲染并设置窗口属性 |
| SPIKE-02 | Slint 中文输入法（IME）候选框定位与组合输入 | 日记/小记不可用 | 尽早在三平台实测；记录规避方案（独立输入区域、升级 Slint） |
| SPIKE-03 | Linux 托盘（GNOME 默认无 AppIndicator） | 状态栏模式不可见 | 文档提示安装扩展；悬浮球作为默认；检测失败时提示切换 |
| SPIKE-04 | Wayland 全局快捷键/窗口定位限制 | 快捷键失效、球无法固定位置 | 走 XDG Portal GlobalShortcuts；失败时禁用并说明；建议 Wayland 用户用托盘模式 |
| SPIKE-05 | 长文本编辑性能（>2 万字符） | 输入卡顿 | 限制长度、分段渲染、降级为只读预览 |
| SPIKE-06 | 三平台代码签名/公证成本 | 无法分发 | 早期申请证书；Linux 提供免签 AppImage |

> **待确认 Q-01：** Slint 稳定版对 `no-frame`、`always-on-top`、透明背景的支持程度需以实际版本文档与 Spike 结果为准；本 spec 只约定产品行为，不锁定 API。当前默认：优先 Slint 原生属性，不足时通过 winit 原生窗口句柄设置。

---

## 9. 验收标准

| 编号 | 验收标准 |
| --- | --- |
| AC-ARCH-01 | 三平台均可完成安装、启动、显示常驻入口、打开主界面。 |
| AC-ARCH-02 | 冷启动与内存指标达到 §5.1 目标（参考机，误差 +15% 内可接受）。 |
| AC-ARCH-03 | 无交互 5 分钟 CPU 均值 < 0.5%，无持续重绘。 |
| AC-ARCH-04 | 所有数据库写入均在事务内，强制结束进程后重启无部分写入。 |
| AC-ARCH-05 | 从上一版本数据库升级后，计划/任务/小记/日记/设置数量与内容一致。 |
| AC-ARCH-06 | 生产代码无 `unwrap()`（CI 静态检查），无网络依赖。 |
| AC-ARCH-07 | 日志不包含任何任务/日记正文内容。 |
| AC-ARCH-08 | 单实例生效：重复启动不产生第二个进程，原窗口被唤醒。 |

---

## 10. 待确认问题

以下为本文件中所有未决问题的索引（正文见对应章节）：

| 编号 | 位置 | 问题摘要 | 当前默认处理 |
| --- | --- | --- | --- |
| Q-01 | §8 SPIKE-01 | Slint 稳定版对透明/无边框/置顶的支持程度 | 优先 Slint 属性；不足时通过 winit 原生窗口句柄设置 |
