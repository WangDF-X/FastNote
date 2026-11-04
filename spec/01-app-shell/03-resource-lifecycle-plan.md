# SHELL-03 按需加载与资源回收执行计划

> 文档编号：SHELL-03 ｜ 模块：01 应用外壳 ｜ 状态：In Progress ｜ 更新日期：2026-11-04
> 上游：[ARCH-01 §5](../00-architecture/01-tech-and-runtime.md)、[ARCH-02 §3.3](../00-architecture/02-architecture.md)、[SHELL-01 §5.5](01-overview.md)、[ENTRY-02 §7](../02-desktop-entry/02-tray-and-lifecycle.md) ｜ 下游：[QA-03 §8](../10-quality/03-test-strategy.md)、[QA-01 §6.4](../10-quality/01-acceptance-traceability.md)

---

## 1. 目的与范围

把 D-RES-01（低内存常驻优先、分级加载/回收、最多 120 秒热缓存）拆解为可执行、可验收、可分阶段合并的工程任务，作为下一步开发的执行计划。

**范围内**：任务编号、改动位置、执行步骤、测试、完成标准、依赖顺序与验收映射。
**范围外**：具体代码实现（本文不含补丁）、UI 视觉调整、新增设置项/依赖/线程、其它模块的业务功能。

**本轮优先**：只做「按需加载」（§5）。热缓存计时、真实卸载与压力驱逐属于后续阶段（§6），本轮不实现。

> 状态说明：本文是计划文档，不代表任何任务已完成。任务开始/完成时更新 §5、§6 的勾选与日期。

---

## 2. 现状与差距

### 2.1 结构性差距

| 现状 | 差距 |
| --- | --- |
| `Runtime` 同时持有内核（配置、DB worker、入口、路由）与 UI（主窗、今日模型、页面定时器） | 无法只丢弃 UI，保留应用可用 |
| `Runtime::new` 无条件创建 `ShellHandle` | `--hidden` 也会建主窗、建主题/字体/模型 |
| `ShellHandle::hide()` 仅 `window.hide()` | 组件、模型、图形上下文仍驻留；不代表回收 |
| 启动后立即 `start_tasks()` → `request_today()` | 冷驻留发起业务查询 |
| 业务回包、Toast、路由直接调用 `self.shell.component()` | 主窗不存在即无法安全处理；改动必须先收口 |

### 2.2 改造面（`self.shell.*` 共 81 处 / 11 个文件）

| 文件 | 处数 | 备注 |
| --- | --- | --- |
| `desktop/tasks/replies.rs` | 15 | 回包投影最集中 |
| `desktop/window.rs` | 9 | 原生窗口事件绑定 |
| `desktop/presentation/mod.rs` | 8 | 显示/隐藏/最小化 |
| `desktop/tasks/requests.rs` | 8 | 查询与命令发起 |
| `desktop/tasks/view.rs` | 8 | 编辑器/焦点投影 |
| `desktop/tasks/transitions.rs` | 8 | 转场与对话框 |
| `desktop/lifecycle.rs` | 6 | 启动/保存回包/清理 |
| `desktop/actions.rs` | 4 | 动作分发 |
| `desktop/runtime.rs` | 4 | 忙碌态与 Toast |
| `desktop/close.rs` | 3 | 关闭/最小化 |
| `desktop/task_preferences.rs` | 3 | 偏好投影与焦点 |
| `desktop/ball.rs` | 2 | 入口可用性 |
| `desktop/theme.rs` | 2 | 主题投影 |

`fastnote-ui/src/shell/handle.rs`（81 行）负责组件生命周期，是本次唯一需要的 UI crate 改动点。

### 2.3 明确不做的事

- 不拆进程、不改动态库、不引入 async runtime。
- 不用 `hide()`、`PrintWindow`、截图或脚本模拟替代真实卸载验证。
- 不先做“全部页面懒加载框架”，只围绕当前已实现的今日任务闭环落地。

---

## 3. 目标结构

```text
AppKernel（常驻，可独立运行）
├── settings / worker / native / ball / tray / hotkey / entry
├── controller（路由，唯一持有者）
├── 轻量会话：日期、选中 ID、折叠/分页、滚动锚点
├── 反馈队列（Toast/Dialog/Banner 待呈现意图）
├── 失效标记（各页面 scope）
└── ui: Option<UiSession> ── 仅在需要时存在

UiSession（可丢弃）
├── ShellHandle（MainWindow 组件 + 原生窗口）
├── TodayPresenter（VecModel）
├── 页面定时器
└── 页面级临时投影（今日状态、编辑器投影）
```

**三条硬规则**

1. 内核不得强持有 `UiSession` 之外依赖窗口的对象；`UiSession` 不得反向强持有内核（只用 `Weak`）。
2. 无会话时业务入口/回包必须安全：入队、标记失效或返回 `None`，禁止 panic、禁止顺手建窗。
3. 卸载 = `ui.take()` 后自然 drop，`Drop` 内停止定时器；不得以“设为不可见”代替。

---

## 4. 阶段总览

| 阶段 | 内容 | 本轮 | 展开章节 |
| --- | --- | --- | --- |
| P0 | 基线采样 | 建议先做 | §5.1 |
| P1 | 会话拆分 + 访问收口 | 是 | §5.2 |
| P2 | 冷启动按需加载 | 是 | §5.3 |
| P3 | 120 秒热缓存与真实卸载 | 否 | §6.1 |
| P4 | 页面查询代次与失效 | 否 | §6.2 |
| P5 | 预算与内存压力 | 否 | §6.3 |
| P6 | 草稿与边界保护 | 否 | §6.4 |
| P7 | 验收留档 | 否 | §6.5 |

---

## 5. 本轮：按需加载（P0–P2）

### 5.1 P0 基线（建议，0.5 天）

| 任务 | 内容 | 改动位置 | 完成标准 |
| --- | --- | --- | --- |
| T0.1 | 从 `main` 切 `feature/resource-lifecycle`，同步本文状态为 In Progress | 分支 + 本文 | 分支建立，基线 `scripts/test.ps1` 通过 |
| T0.2 | 记录改造前 Release 指标：冷启动 10 分钟、打开今日、1000 行今日、关闭后 1 分钟 | 外部工具即可，不强制写代码 | 有一张可对照的数值表（工作集/专用提交内存） |
| T0.3 | 记录当前 `--hidden` 是否创建主窗、是否发起今日查询 | 手工观察 + 日志时间线 | 结论写入 §9 留档 |

> 说明：T0.2/T0.3 可用任务管理器 + 现有日志完成；不因此新增常驻采样线程。若跳过 P0，P2 完成后仍必须补一次对照。

### 5.2 P1 会话拆分与访问收口（约 3–4 天）

**T1.1 定义 `UiSession`（S）**

- 新增 `crates/fastnote-app/src/desktop/ui_session.rs`（目标 <200 行）。
- 迁入：`shell: ShellHandle`、`today_presenter: TodayPresenter`、`task_save_timer`、`task_day_timer`、`task_feedback_timer`、`task_saving_visible: Cell<bool>`。
- `Runtime` 改为 `ui: RefCell<Option<UiSession>>`；`impl Drop for UiSession` 停止全部定时器。
- **不改行为**：本任务只搬家，不新增 `Option` 判断。
- 完成标准：`cargo test --workspace` 全绿；应用启动、关闭、球切换与改造前一致。

**T1.2 `with_ui` 与 `UiEffect`（M）**

- 新增 `crates/fastnote-app/src/desktop/effects.rs`：
  - `enum UiEffect { Toast(ToastRequest), Dialog(DialogRequest), Banner(BannerRequest), Navigate(String), RefreshToday, Focus(FocusTarget), SetDesktopState(u8), SetBusy(bool) }`
  - `Runtime` 增加待呈现队列与“无会话时入队/合并”的策略（同键合并，避免堆积）。
- 增加访问辅助：`fn with_ui<R>(&self, f: impl FnOnce(&UiSession) -> R) -> Option<R>`。
- 按文件分批改造，**每批单独提交、单独跑测试**：

| 批次 | 文件 | 处数 | 要点 |
| --- | --- | --- | --- |
| 1 | `presentation/mod.rs`、`lifecycle.rs` | 14 | 显示/隐藏与启动回包；`set_desktop_state` 转 effect |
| 2 | `actions.rs`、`close.rs`、`ball.rs` | 9 | 动作/关闭路径；无会话时只改内核状态 |
| 3 | `tasks/replies.rs` | 15 | 先写确认状态与失效标记，再尝试投影 |
| 4 | `tasks/requests.rs`、`tasks/view.rs`、`tasks/transitions.rs` | 24 | 查询发起与编辑器投影；无会话时只置 dirty |
| 5 | `window.rs`、`theme.rs`、`task_preferences.rs`、`runtime.rs` | 17 | 原生事件与主题；事件绑定改为随会话创建 |
| 6 | `fastnote-ui/src/shell/handle.rs` | — | 明确 `hide`/drop 语义注释；不改公开 API |

- 完成标准：`grep "self\.shell\." crates/fastnote-app/src` 为 0；无会话时所有入口返回而不 panic；界面行为无变化。

**T1.3 契约测试（S）**

- 新增 `desktop/session_contract.rs`（与现有源码契约测试同风格）：
  - 断言 `desktop` 源码中不再出现 `self.shell.`；
  - 断言 `UiSession::drop` 调用三个 `Timer::stop`；
  - 断言 `Runtime::new` 不再无条件创建 `ShellHandle`（T2.1 后生效，可先写为“仅 hidden 分支不创建”）。
- 完成标准：测试失败信息能直指违规文件名。

**T1.4 用户手工回归（S）**

- 关闭/最小化/球与托盘切换；Toast、对话框、主题切换；打开与关闭任务详情。
- 完成标准：行为与改造前一致，由用户确认；Agent 不写模拟点击脚本。

### 5.3 P2 冷启动按需加载（约 2 天）

**T2.1 `--hidden` 不创建会话（M）**

- `Runtime::new` 流程调整为：先建内核与 `Rc<Runtime>`，再按需 `ensure_ui()`；`hidden == true` 时不创建。
- 注意 `ShellHandle::new` 的回调需要 `Weak<Runtime>`，因此必须在 `Rc` 建立之后创建会话；保留现有 `Rc<RefCell<Weak<Self>>>` 占位模式。
- 完成标准：`--hidden` 启动后无主窗 HWND、无 `MainWindow` 构造；球/托盘/快捷键可用。

**T2.2 `ensure_ui` 与首次唤起（M）**

- 新增 `fn ensure_ui(self: &Rc<Self>) -> Result<(), SwitchError>`：创建会话、应用当前 `ShellView`、按当前路由加载所需页面。
- 调用点：`show_main()`、菜单导航、全局唤起、托盘显示；已存在则直接复用。
- 完成标准：手动启动、`--hidden` 后点球、菜单直达日历（占位页）均正常；路由不会因首次创建被重置为 Today。

**T2.3 启动任务拆分（M）**

- `desktop/tasks/requests.rs`：
  - `start_tasks()` 拆为 `start_kernel_tasks()`（日期变化检测、失效标记）与 `start_page_tasks()`（今日查询）。
  - `request_today()` 增加前置：仅当“今日页可见”时发起；否则置 `today_dirty` 并返回。
- `lifecycle.rs::loaded()` 只调用 kernel 部分；今日查询由 `ensure_ui` / 路由进入触发。
- 完成标准：`--hidden` 启动后今日查询次数为 0（用现有日志或测试计数器断言）。

**T2.4 日期刷新收敛（S）**

- 30 秒日期定时器：改为只更新内核日期与失效标记；今日页可见时才 `request_today()`。
- 完成标准：隐藏 10 分钟无查询、无重绘；跨日时重开仍能刷新（可注入时钟测试）。

**T2.5 测试与交付（S）**

- 单元/契约：冷启动不构造会话、不发起页面查询、首次打开只查当前页、无效入口不 panic。
- 手工清单（交付用户）：
  1. `cargo run -p fastnote-app -- --hidden`，确认无主窗、球正常。
  2. 点球打开，进入今日任务，任务与分组正常。
  3. 关闭窗口回球，再从球打开，路由保持。
  4. 手动启动（无参数）仍直接进入今日任务。
- 完成标准：`scripts/test.ps1` 通过 + 用户确认上述四项。

### 5.4 本轮结束时的预期与边界

- **预期收益**：冷启动不再建主窗/模型、不查今日；只驻留入口时内存应明显低于改造前。
- **明确未完成**：关闭主窗后仍在 120 秒内保留会话（P3 才卸载）；因此“隐藏后内存不降”在本轮仍会出现，属预期，不得据此宣称 D-RES-01 完成。
- **不在本轮**：热缓存计时、超预算驱逐、草稿保护、页面代次。P3 起按 §6 执行。

---

## 6. 后续阶段（未启动，按序执行）

### 6.1 P3 热缓存与真实卸载

| 任务 | 内容 | 对应验收 |
| --- | --- | --- |
| T3.1 | 纯状态机 `lease.rs`：`Cold/Active/Hot{deadline,generation}` + `LeaseEvent`，注入 `TickSource` | AC-ARCH-27、AC-ARCH-32 |
| T3.2 | deadline 用 `GetTickCount64()`（含睡眠）；不用 `Instant` 判定 120 秒 | AC-ARCH-27 |
| T3.3 | 接线：保存成功且隐藏成功→Hot；重开→Active；退出→Cold；失败不进入 | AC-SHELL-22 |
| T3.4 | 到期/驱逐→`ui.take()` drop；验证原生窗口/图形资源释放 | AC-ARCH-28 |
| T3.5 | 旧代计时回调与旧查询回包丢弃 | AC-ARCH-30、AC-SHELL-23 |

### 6.2 P4 页面查询代次与失效

- 查询附加 `session_generation` + 页面 scope；回包不匹配丢弃。
- 事件只标记 dirty，可见页才查询；隐藏页零查询。
- 重资源（1000 行今日、计划全树、长正文）先于外壳回收。
- 对应 AC-ARCH-26、AC-ARCH-31。

### 6.3 P5 预算与内存压力

- 复用现有 30 秒定时器采样 `GlobalMemoryStatusEx`，不新增线程；触发提前驱逐。
- `HOT_LEASE = 120s`、预算与白名单常量集中定义，不新增设置键。
- 对应 AC-ARCH-29。

### 6.4 P6 草稿与边界

- 脏/在途/失败草稿阻止进入 Hot；必要时内核保留最小草稿。
- 独立快速捕获窗不随主窗回收关闭。
- 对应 AC-ARCH-30、QA-03 §5 生命周期用例。

### 6.5 P7 验收留档

- 逻辑测试补齐；Release 分态量测（冷/热/活动、5 轮开关）；手工清单交付。
- 对应 AC-QA-16、QA-03 §8。

---

## 7. 依赖、顺序与分支

```text
T0.1 → T0.2/T0.3 → T1.1 → T1.2(批次1→6) → T1.3 → T1.4 → T2.1 → T2.2 → T2.3 → T2.4 → T2.5
                                                                                    ↓
                                                         （停止，按需开始 P3）
```

- 分支：`feature/resource-lifecycle`；一个任务一个提交，提交信息遵循 `feat(shell): ...` / `refactor(shell): ...`。
- 每个提交前运行 `scripts/test.ps1`；跨文件重构批次必须单独提交，便于回退。
- T1.2 是唯一大范围改动；若某批次失败，回退该批次而非整段重构。
- 遵守 AGENTS：不主动提交/推送，完成后汇报并由用户确认。

---

## 8. 风险与回退

| 编号 | 风险 | 影响 | 应对 |
| --- | --- | --- | --- |
| R1 | 81 处调用漏改，主窗缺失时 panic 或偷偷建窗 | 高 | 分 6 批 + 源码契约测试 + `grep` 为 0 的门槛 |
| R2 | 内核与 `UiSession` 形成强引用环，drop 后不释放 | 高 | 只允许 `Weak` 回指；T3.4 用探针与 HWND 计数验证 |
| R3 | 收口时把业务确认逻辑一起搬进 UI，导致无窗时命令结果丢失 | 高 | 先更新确认状态/失效标记，再尝试投影；T1.2 批次 3/4 单独评审 |
| R4 | 首次唤起时序错误（会话未建就切 presentation） | 中 | `show_main` 统一走 `ensure_ui`；T2.2 覆盖手动/球/菜单三条路径 |
| R5 | 误以为 P2 已完成资源目标 | 中 | §5.4 明确边界；P7 前不得宣称 D-RES-01 完成 |
| R6 | 为省内存牺牲草稿/提醒 | 高 | P6 前不进入“自动驱逐”；本轮不实现驱逐 |

回退策略：P1 各批次独立可回退；P2 若出现启动问题，可临时恢复“总是创建会话”，保留收口后的代码结构，再定位。

---

## 9. 验收映射与留档

| 任务 | 覆盖验收 |
| --- | --- |
| T1.x | AC-ARCH-32（契约测试）、AC-SHELL-24（行为不回归） |
| T2.1–T2.4 | AC-ARCH-25、AC-ENTRY-15、AC-SHELL-22（部分） |
| T2.5 | AC-QA-16（本轮子集） |
| P3–P7 | AC-ARCH-26–31、AC-SHELL-23/24、AC-ENTRY-17/25/26、AC-QA-05/16 |

留档要求：

- 每个里程碑记录：提交号、构建模式、`--hidden` 是否有主窗、页面查询次数、冷/热状态数值。
- `scripts/test.ps1` 通过只代表自动检查通过，不代表 UI 或资源验收通过。
- 用户手工确认项（球打开、关闭回球、路由保持、焦点）必须单独记录。

---

## 10. 不在本次范围

- 120 秒热缓存、提前驱逐、内存压力阈值（P3/P5）。
- 页面查询代次与失效矩阵全量落地（P4）。
- 草稿下沉或独立草稿存储（P6 再定方案）。
- 日历/小记/日记/计划板的实际业务实现；本轮只保证接入时不被预建。
- 任何新增设置项、依赖、线程、后台服务、UI 视觉调整。

---

## 11. 待确认问题

> **当前状态：无新增待确认项。** 120 秒上限、80/130MB 预算、白名单有界与提前回收已在 D-RES-01 确认。

| 编号 | 位置 | 问题摘要 | 当前处理 |
| --- | --- | --- | --- |
| Q-SHELL-08 | §5.3 T2.3 | 今日页不可见时是否允许后台预取 | 不预取；只置失效标记，进入页面再查询（低内存优先） |
| Q-SHELL-09 | §6.4 P6 | 草稿保护的实现形态（阻止回收 vs 内核最小草稿） | 默认阻止回收，与现有保存失败阻止关闭一致；若实测不可行再评估下沉 |
