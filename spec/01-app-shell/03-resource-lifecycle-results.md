# SHELL-03 P3–P7 验证与交付记录

> 日期：2026-11-04，回归修复更新 2026-11-05 ｜ 分支：`feature/resource-lifecycle` ｜ 对应：[执行计划 §6](03-resource-lifecycle-plan.md)
> 状态：资源生命周期实现与自动验证交付；真实 UI/资源及性能验收尚未全部通过。未提交/推送/合并，不标记 spec Done。

## 1. 实际改动

| 阶段 | 改动 | 主要位置 |
| --- | --- | --- |
| P3 | 保存确认且实际隐藏成功起算，最多 120 秒；重开取消旧代；到期真正移除唯一 UI 会话 | `desktop/lease.rs`、`desktop/resources.rs`、`desktop/close.rs` |
| P3 图形 | 使用现有 Slint/winit 的进程内 `SLINT_DESTROY_WINDOW_ON_HIDE` 开关，hide 时 suspend 原生/图形资源，外壳最多限时保留；任务栏 `set_minimized` 不受影响 | `desktop/mod.rs`、`desktop/window/ui_events.rs` |
| P4 | 普通读/延迟 UI 回调携带 session/page 代次；隐藏/离页撤销；SaveAck/Delete/写结果仍由内核处理 | `tasks/lifetime.rs`、`desktop/resources/callbacks.rs`、`desktop/tasks/` |
| P4 重数据 | 隐藏/离页先释放今日 DTO、所有绑定行模型、已保存正文及页面实例；关闭详情也清空正文绑定 | `tasks/residency.rs`、`desktop/ui_session.rs`、`fastnote-ui/src/today/presentation.rs` |
| P4 查询 | SQLite 逐行标量候选 + core 有界排序堆；仅为已选前缀补关系信息；应用读取使用同一个只读 SQLite 快照 | `fastnote-core/src/derive/today/bounded.rs`、`fastnote-store/src/tasks/read/`、`tasks/snapshot.rs` |
| P5 | 原有 30 秒内核 Tick 采样工作集与系统负载，隐藏后立即再采样；超热预算/系统压力提前驱逐，不清空工作集 | `fastnote-platform/src/windows/resources.rs`、`desktop/resources.rs` |
| P6 | 保护脏/在途/失败/待读回、快速输入、日期选择与未决确认；取消只撤销匹配许可；冷重开重查一个详情并恢复有效焦点/滚动 | `tasks/residency.rs`、`desktop/resources/restoration.rs`、`fastnote-ui/src/shell/retained.rs` |
| P7 | 确定性测试、隔离假库生成/仓储计时、指定已有 Release PID 的只读采样 | `fastnote-store/examples/resource_fixture.rs`、`scripts/measure-resources.ps1` |

没有新增依赖、线程、产品设置键或其它模块业务。Cargo 只启用已有 `windows` 依赖的 ProcessStatus/SystemInformation/Threading 功能；锁文件不手改。Slint 只增加卸载/恢复状态挂钩，不改颜色、布局、尺寸或字体。

## 2. 资源白名单与边界

- **冷**：入口/配置/路由/必要 DB 与日志/既有热键、单个时钟观察及轻量恢复；今日 DTO、正文、业务页和主窗会话均不持有。
- **热**：实际 hide 后最多 120 秒，先释放大模型/已保存正文/原生与图形上下文。后台设置、球/菜单操作、主题及 DB 回包只核验/失效，不续期；睡眠时间计入。
- **活动**：只加载当前页；今日每组初始 200、最多 1000，八组有界，不改完整计数、分组/排序/隐藏偏好。暂未实现的日历/计划树/小记/日记/独立捕获不能冒充资源场景通过。
- **保护**：输入/未决决策不能被期限或内存压力强制丢弃；不能伪报为 cold。保护解除后沿用原 Hot deadline，不再赠予两分钟。
- 轻量保留：一个当前详情 ID、一个 ≤64 字节白名单焦点锚点、滚动位置、八组折叠/分页。没有遍历过全部实体的缓存字典。
- 固定 UI 选项：24 小时、60 分钟、101 年份；大行模型全部释放。Toast 纯状态最多保留 32 条（余下新反馈进入已有最多 32 条待呈现队列），Banner 最多四种。活动 Dialog FIFO 未改变，任何未决 Dialog 都阻止卸载。
- 预算：冷 **80,000,000** 字节，热/活动 **130,000,000** 字节，总 Working Set；Private Bytes 另列。系统负载 ≥90% 或热工作集超预算是提前驱逐信号，不是放宽预算。不能强制驱逐活动窗口/正在编辑的草稿。

## 3. 自动验证与发布产物

| 项目 | 结果 |
| --- | --- |
| 确定性逻辑 | 租约边界/睡眠/非续期/旧代、保护/确认取消、重缓存/模型别名释放、五轮所有权、有界排序与 SQLite 投影测试已补齐 |
| 完整自动检查 | 最后诊断时序修正后的 `scripts/test.ps1` 再次通过：格式、workspace 全目标 Clippy、543 项测试（较 P2 新增 50 项）及 Debug 链接；不把测试数量外推为 UI 验收 |
| Release | Windows x64 / Rust 1.99.1 / Slint 1.18.1 / winit + femtovg，最终 `cargo build -p fastnote-app --release --locked` 通过（最后增量链接 47.46 秒，非启动耗时）；最终标签读回 Medium |
| 采样工具 | PowerShell 语法与只读 C# 原生绑定编译验证通过；没有执行 UI 操作或自动采样真实窗口。实际采样只针对用户已启动并指定的 FastNote Release PID |
| 未验证 | `cargo-deny` 未安装；远端 CI 未运行；未给出分支覆盖率百分比；实际 80/130MB、启动/空闲 CPU/原生及 GPU 资源结果未提供 |

发布产物为 `target/release/fastnote-app.exe`。重新链接确实再次继承 Low；用户明确授权重复单文件修正。最后一次 UAC 请求超时，Agent 读回仍为 Low，未伪报成功；用户选择重新授权后，管理员命令仅执行该 exe 的 `/setintegritylevel M`，退出码 0，最终读回 **`Mandatory Label\Medium Mandatory Level:(NW)`**。整个过程未修改目录/数据库权限，未以管理员启动应用，未清空工作集。

- 2026-11-04 交付产物 SHA-256：`3567556324D1E5A5A6B9FE841303BD2761BB49D08D046DFDAB6C12AA148C540E`；已被用户反馈存在最小化/恢复回归，最新修复产物以 §7 为准，不能沿用此哈希宣称修复通过。
- 最终标签修正前后 SHA-256 完全相同；修正不改变 exe 字节。以后再次构建仍可能继承 Low，不能静默提升项目/target 目录。
- 差异空白检查与新增/修改 Rust 文件 ≤400 行检查通过；Git 分支仍为 `feature/resource-lifecycle`，尚未提交本次修改。

## 4. 隔离假数据与仓储计时

已在 `target/resource-fixture-p3-20261104-001/` 生成 **10,000 条假任务**，含一条当前任务上限的 10,000 字描述；不打开默认用户库、不创建真实用户内容。生成器拒绝已存在目标目录；计时要求假库标识文件，不删除/重建已有目录。

```powershell
# 可使用上述已生成目录；如需全新假库，目标目录必须尚不存在
$fixture = Join-Path (Get-Location) 'target\resource-fixture-p3-20261104-001'
cargo run -p fastnote-store --release --locked --example resource_fixture -- benchmark $fixture (Get-Date -Format yyyy-MM-dd)
```

计时工具使用应用实际的只读查询路径；每种加载上限预热 10 次、采样 100 次，统计中位数/p95/最大值，单位微秒。不是 criterion 结果或参考机认证；同机结果只能如实记录，不能由它推导 UI/RSS。

| 版本/场景 | 数量 / 实际加载 | 中位数 / p95 / 最大（µs） | 判定 |
| --- | --- | --- | --- |
| 最终只读快照路径，limit=200 | 10000 / 200 | 22910 / 24943 / 26281 | p95 **未达到 <10ms** |
| 最终只读快照路径，limit=1000 | 10000 / 1000 | 26490 / 34275 / 38359 | p95 **未达到 <10ms** |

上表在发布构建结束后采样，不与其它 cargo 构建并行；当时只有授权的单文件标签操作。本轮解决了查询层全量分配/大缓存回收，但 **QA-03 今日查询 p95 <10ms 仍未达标**，不是已通过项目，不将其藏在“待手工测试”之中。最终诊断修正不改变查询路径，因此保留上述实际失败结果。

## 5. 待你手工验证：集中六项

完全退出旧实例。用普通 Windows PowerShell 手工启动；如验收假库，临时改变 **当前终端/子进程** 的 LOCALAPPDATA，退出后恢复，绝不修改系统环境。

```powershell
$savedLocalAppData = $env:LOCALAPPDATA
$env:LOCALAPPDATA = (Resolve-Path '.\target\resource-fixture-p3-20261104-001').Path
try { & '.\target\release\fastnote-app.exe' --hidden }
finally { $env:LOCALAPPDATA = $savedLocalAppData }
```

| 项目 | 操作与预期 | 结果 |
| --- | --- | --- |
| 1 冷启动 | `--hidden` 只入口空闲 10 分钟；入口/已注册快捷键可用，无主窗/普通页面读；记录冷 Working Set ≤80MB、CPU 与 HWND | [ ] 待确认 |
| 2 真实卸载 | 打开今日，保存后关闭回入口；过 130 秒重开，日志应有到期释放/组件释放探针，再创建新会话；隐藏后不查询，入口仍可用 | [ ] 待确认 |
| 3 热重开/睡眠 | 60 秒内重开复用外壳但重查已释放的数据；再次关闭按新 deadline。拖球/菜单/主题不续期；睡眠超过 deadline 后首次入口操作/重开先回收，不多赠两分钟 | [ ] 待确认 |
| 4 1000 行/五轮 | 展开未安排、点四次“显示更多”到 1000；保持折叠/滚动和干净详情后关闭/到期重开，共五轮；有效 ID/分页/焦点恢复，工作集/句柄/USER/GDI 不持续阶梯增长 | [ ] 待确认 |
| 5 草稿/确认 | 编辑立即关闭须先确认保存；无效/失败输入、快速添加未提交、日期选择/确认未解决时不能收起/丢输入。删除/重载分别取消再重试，取消不执行且不永久阻塞后续收起 | [ ] 待确认 |
| 6 平台/设置 | 切非今日页、明暗/系统主题、偏好及球/托盘；冷重建路由与最新确认设置保留；普通托盘任务栏最小化超过 120 秒不卸载任务栏窗口，恢复后补读必要页；Toast 从剩余时长恢复 | [ ] 待确认 |

保护/压力已用注入信号测试；不要为手工验证弄坏真实数据库、调整系统时间、强制制造内存耗尽或用管理员运行应用。独立捕获/其它业务页尚未实现，本轮不验收这些业务。

## 6. 只读 Release 资源证据

在另一个普通终端指定你手工启动的 **FastNote PID**，不要把 cargo/rustc/PowerShell 的 PID 传入：

```powershell
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File .\scripts\measure-resources.ps1 -AppPid 12345 -Scenario lifecycle-five-cycles -Seconds 1800
```

CSV 写入 `target/resource-measurements/` 的新文件，记录时间/PID/exe 哈希/Working Set/Private Bytes/句柄/USER/GDI/主窗 HWND/入口 HWND/CPU。采样器不启动、关闭、模拟鼠标键盘、发消息或截图，不改变标签、申请内存压力或清空工作集。GPU 字节当前 API 不提供，留空 **不是测得为零**。

日志辅助字段在假库的 `FastNote/logs/`：

- `kernel_initialized ui_present=false`：初始无主窗；`ui_session_created session_generation=...`：按需创建。
- `page_query_queued` / `detail_query_queued` 携带 session/page 代次；`page_snapshot_applied loaded_rows=...` 记录当前前缀数量。
- `ui_lease_armed deadline_tick_ms=...`：绝对期限；`ui_session_released release_reason=Expired/Pressure/Budget`：实际移除会话。
- `renderer_teardown`：后端图形上下文的 teardown 通知；`ui_release_probe component_released=true`：当前事件引用结束后弱组件已失效。
- `old_hwnd_present` 仅检查旧 HWND 数值是否仍属该进程，HWND 可能被入口/新窗复用，不能独立作为泄漏结论；应结合采样器主窗数量及五轮趋势。
- `resource_sample lifecycle_state=...` 同时保留 Working Set 与 Private Bytes。日志有界/不可用时可丢记录，“没日志”不能单独证明零工作。
- `main_first_frame elapsed_ms` 是从应用装配起点起算的辅助打点，只有首次主窗才可辅助观察启动；不是每次重开的耗时，也不代表精确的进程启动/入口首帧量测。

| 场景 | Working Set / Private Bytes（原始字节） | 主窗 / USER / GDI / 句柄 / 图形通知 | 判定 |
| --- | --- | --- | --- |
| 冷 `--hidden` 空闲 10 分钟 | — / — | 待记录 | 未测 |
| 活动 1000 行 | — / — | 待记录 | 未测 |
| 保存后隐藏 0–120 秒 | — / — | 待记录 | 未测 |
| 到期后冷空闲 10 分钟 | — / — | 待记录 | 未测 |
| 五轮开关的各活动峰值/冷稳定值 | — / — | 待记录 | 未测 |
| 睡眠/压力/保护例外 | — / — | 待用户场景/可取得指标 | 逻辑自动验证，实际未测 |

没有旧版本同机 Release 对照数据，不能填“下降 X MB”。分配器/驱动自然保留工作集时也必须按实际冷预算和多轮稳定性判定，不做人工 trim 冒充回收。

## 7. 2026-11-05 用户回归：悬浮球切换失败与任务栏二次点击

**反馈与证据：** 用户从 Git Bash `--hidden` 启动，点球打开主窗后最小化提示「无法切换到桌面入口，请在主窗口重试」，再次收起/点球会只出现任务栏、需要二次点击。只读检查既有安全日志发现多次 `renderer_teardown` 紧接 `presentation_switch error_kind=ShowTarget`、随后 renderer setup 的回滚序列。不是 Git Bash 启动方式或数据库故障。

**根因：** Slint 1.18/winit 的 hide 销毁原生资源后，`show()` 可以仅登记待创建窗口，真正 HWND 由后续事件循环创建。原代码立即配置球 HWND，把 `None` 当作失败并回滚；主窗也在原生窗口尚未创建/还原时申请焦点，实际创建后没有补做激活。原生重建还会丢失部分工具窗口属性，不能仅依赖首次创建时的 hook。

**修复：**

- 球/菜单 show 被接受但 HWND 未就绪时保留一次性待配置状态，不伪报失败；由原生事件在事件返回后完成配置。真实配置失败仍走可追溯错误与主窗恢复路径。
- 每次入口重建恢复跳过任务栏、不可调整尺寸、置顶/工具窗口属性；隐藏取消该入口的待配置意图。
- 主窗新增独立 `Waiting → Restoring → Idle` 激活状态，等真实原生窗口就绪后恢复尺寸/最大化、解除最小化，再申请一次前台焦点并启动当前页读取；不是每次后台事件都抢焦点。
- 重建的主窗沿用 UI 会话代次隔离，隐藏取消未完成激活；还原阶段不把旧的最小化状态当作用户再次要求回球。没有禁用资源卸载、放宽期限或删除错误提示来掩盖故障。

**已自动验证：** `scripts/test.ps1` 通过：格式、Clippy、550 项 workspace 测试、Debug 构建；新增 7 项覆盖延迟 HWND 不误报、真实配置错误回滚、先还原再激活、隐藏取消、五轮延迟切换及生产事件接线。没有自动启动、点击、截图或操作用户窗口。

**修复版产物：** `cargo build -p fastnote-app --release --locked` 通过（1 分 38 秒是构建时间）。`target/release/fastnote-app.exe` 的 SHA-256 为 `B0A4E3E93CAEA076C0B340C165E586B41E72E2E73F6C67FCFC951B8159952855`。用户明确授权后，仅将这个 exe 的标签从重新链接继承的 Low 修正为 Medium；管理员命令退出码 0，读回 `Mandatory Label\Medium Mandatory Level:(NW)`，标签修正前后哈希相同。未改变目录/数据库权限，未用管理员启动应用，实际交互仍待下列手工复验。

**待用户手工验证：** 完全退出旧实例，普通 Git Bash 运行新版 `/e/FindWork/Project/FastNote/target/release/fastnote-app.exe --hidden`：

1. 首次点球打开 → 点击最小化：回到球，无「无法切换」误报。
2. 再点球：主窗直接显示并能输入，不需要点击任务栏；快速往返至少五次。
3. 主窗最大化后收起/重开，以及隐藏超过 130 秒后的冷重开：窗口正常还原并激活；未保存编辑仍先保存/保护。
4. 右键球反复打开/关闭菜单；切托盘后普通任务栏最小化/恢复：入口不混入任务栏，原生最小化行为不变。

用户之前的 P1/P2 确认不能外推为本次修复手工验收通过；本分支与相关 spec 继续保持 In Progress，未提交/推送/合并。
