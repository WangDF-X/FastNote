# ENTRY-02 系统托盘与生命周期

> 文档编号：ENTRY-02 ｜ 模块：02 桌面常驻入口 ｜ 状态：Confirmed ｜ 更新日期：2026-11-04
> 上游：[ARCH-06](../00-architecture/06-platform-integration.md)、[product §1.4](../../product.md) ｜ 下游：[SHELL-01](../01-app-shell/01-overview.md)、[SET-02](../08-settings/02-general-and-appearance.md)

---

## 1. 目的与范围

定义 FastNote 在 tray（系统托盘 / 菜单栏图标）模式下的完整行为：托盘图标的创建、菜单结构、点击响应、应用退出流程中的文本冲刷与数据持久化，以及悬浮球/tray 模式的互斥切换规则。

**范围内**: 托盘创建、菜单项与交互、关闭按钮映射、启动参数 `--hidden` 处理、退出冲刷流程。
**范围外**: 悬浮球实现细节（ENTRY-01）、全局快捷键（ENTRY-03）。

> **本轮 Windows 范围（2026-11-02 已确认）：** 以 SHELL-01 §5.4 为准。入口菜单复用已提交品牌图标，增加「打开主界面」与互斥切换；保留当前路由恢复。自启、单实例和正文冲刷未接入，本轮不宣称对应 AC 完成。

---

## 2. 需求追溯

| 编号 | 来源 | 需求 | 承载位置 |
| --- | --- | --- | --- |
| ENTRY-05 | product §1.4 | 状态栏作为替代常驻入口 | §3 |
| ENTRY-06 | ARCH-06 §3.1 | macOS SMAppService + 菜单栏图标 | §4.2 |
| ENTRY-07 | ARCH-06 §3.2 | Windows/Linux tray icon 与 context menu | §4.1 |
| ENTRY-08 | SHELL-01 §5 | 关闭按钮映射到 entry 模式设置 | §6.1 |
| ENTRY-09 | SHELL-01 §5.1 | 隐藏主窗口保留驻留、quit 则退出 | §6.2 |
| ENTRY-10 | product §5 | 常驻期间保持低资源占用 | §7 |

---

## 3. 基本规范

### 3.1 Tray 图标

| 项 | 值 |
| --- | --- |
| 图标 | Windows v1 沿用已定稿彩色品牌位图 `png/fastnote-32.png`，亮/暗主题均不重新着色；原生菜单亮暗由 Windows 绘制 |
| Tooltip | 固定文案 `FastNote` |
| 可见性 | 仅当 `entry_mode == tray` 时显示；切换到 ball 后立即移除 |
| 右键菜单 | 同 ENTRY-01 §4.2（与悬浮球右键菜单完全一致） |
| 左键单击 | 等同于悬浮球左键（打开/恢复主窗口） |

### 3.2 菜单结构

与悬浮球右键菜单相同，逐项绑定到对应路由跳转：

```
[今日任务] → Today
[日历]     → Calendar {selected: today}
[小记]     → Notes
[日记]     → Journal {date: today}
[计划板]   → PlanList {tab: active}
─────────── (分隔线)
[设置]     → Settings
[退出]     → 执行退出流程
```

- macOS 下菜单项不显示在图标右键菜单中，而是融入标准菜单栏的 FastNote 下拉菜单（如果可用）；否则使用原生 `NSMenu`。
- Linux KDE/GNOME 下菜单通过 NativeMenu/DBus 注册到系统菜单。

---

## 4. 平台差异

### 4.1 Windows / Linux

- 使用 `muda::TrayIcon` 或 Slint 内置平台托盘支持。
- 托盘图标位于系统托盘区域（Windows 任务栏右侧；Linux 面板通知区）。
- 菜单使用 native popup menu。

### 4.2 macOS

- 不使用 `muda`，而使用 `SMAppService` 注册为 agent，配合菜单栏图标（MenuBarExtra）。
- `LSUIElement=true` 使应用无 Dock 图标。
- 菜单栏图标菜单直接嵌入 App Menu bar 的 FastNote 条目下。
- 开机自启使用 `SMAppService.agents["fastnote"] = ...`，而非简单的 Login Item。

> **已确认（2026-10-26）Q-ENTRY-04：** macOS 下 `--hidden` 标志是否使用 `SMAppService` 的 hidden 选项？→ **当前默认处理：** 优先使用 `SMAppService` agent 方式（macOS 13+），agent 设置 `isEnaled=true` + `launchOptions=["LSUIElement=YES"]`；低版本回退到 Login Items API。

---

## 5. 启动参数行为

| 参数 | 行为 |
| --- | --- |
| 无参数（手动启动） | 打开主窗口，进入 `Today` |
| `--hidden` | 进入冷驻留；只创建入口与必要后台服务，不创建主窗/业务页或预查列表/正文（D-RES-01） |
| `--version` | 输出版本号后退出 |
| `--log-file <path>` | 将日志写入指定文件而非默认位置 |
| 第二次启动（已有实例运行） | 发送 IPC 消息唤醒旧实例的主窗口；不创建新进程 |

- `--hidden` 通常由系统自启触发；用户不应主动传入。
- 单实例机制见 [ARCH-06 §4](../00-architecture/06-platform-integration.md)。

---

## 6. 关闭窗口与退出行为

### 6.1 关闭按钮映射

根据设置 `general.close_behavior` 决定：

| close_behavior | 行为 |
| --- | --- |
| `entry`（默认） | 保存成功后隐藏主窗，保留球/托盘；重资源先回收，低成本主窗热缓存最多 120 秒后卸载 |
| `quit` | 执行完整退出流程，取消热缓存计时，不等待两分钟 |

- Windows 球模式的系统最小化切回球，与关闭设置无关；主窗与球互斥。托盘模式保持平台默认任务栏最小化，托盘图标仍可与可见主窗共存（Q-SHELL-06）。
- macOS 下 `⌘W` 等同关闭按钮行为。
- Linux KDE 下「关机」菜单项也遵循此行为。

### 6.2 退出流程

触发场景：托盘/球菜单选择「退出」、设置中退出按钮、`close_behavior==quit` 时关闭窗口。

```text
1. 提交所有未提交的编辑（文本防抖缓冲区的实时内容）
2. 停止所有页面定时器与重绘循环
3. 关闭详情面板与浮层
4. 注销全局快捷键（如已注册）
5. 从托盘/桌移除入口图标
6. checkpoint 写数据库事务
7. 保存窗口几何与入口位置到 SQLite app_settings（ARCH-04）
8. 关闭有界 crossbeam 通道（本项目不使用 Tokio）
9. join 后台线程
10. std::process::exit(0)
```

> **已确认（2026-10-26）Q-ENTRY-05：** Linux SNI（StatusNotifierItem）不支持动态改变托盘菜单时，是否强制切换图标样式？→ **当前默认处理：** 不自动更改设置；在 SET-02 的设置页内以 inline error 提示「当前桌面环境不支持完整的托盘菜单」，提供恢复原值的操作。

> **已确认（2026-10-26）Q-ENTRY-06：** 用户在快速捕获窗口输入了大量文本后调用退出，如何在 2s 内确保文本安全落盘？→ **当前默认处理：** 退出流程中同步提交一次文本（阻塞退出直到 flush 完成或超时 2s），随后 force quit。若文本未成功 flush，下次启动时提示「上次未正常关闭」。

---

## 7. 资源管理

资源唯一规则为 [ARCH-01 §5.4–5.5](../00-architecture/01-tech-and-runtime.md) 的 D-RES-01；球与托盘使用同一常驻/回收策略，不因入口形态保留两套业务模型。

- Release 仅入口冷驻留 ≤80MB，隐藏后的热驻留 ≤130MB、最长 120 秒；空闲 CPU <0.5%，页面查询/刷新/重绘停止，低成本缓存无无限增长。
- 保存确认且主窗实际隐藏成功才开始单调期限；重开取消，再隐藏重新计时。球/菜单交互、主题/设置/后台回包不续期；睡眠时间计入，唤醒检查过期立即回收。
- 长正文、大量任务行/完整计划树与昂贵绘图资源优先释放；低成本外壳可限时复用，到期/内存压力/超预算必须解除主窗及缓存持有，不以原生 `hide()` 代替销毁。
- 托盘/球继续处理有界命令回包、快捷键和必要提醒，不依赖主窗存在；隐藏页面仅失效、重开再加载当前范围，不能因通知而预建全部 UI。
- 路由、日期/ID、折叠/分页等轻量会话可保留。保存失败或在途编辑不得被超时驱逐，完整退出仍遵循既有数据安全流程；独立快速捕获不因主窗回收关闭。

> 两分钟及白名单为已确认的文档要求，不是当前实现完成声明；Release 量测与真实窗口释放证据见 QA-03 §8，Debug 数字或编译通过不算资源验收。

---

## 8. 验收标准

| 编号 | 验收标准 |
| --- | --- |
| AC-ENTRY-11 | 设置中将 entry_mode 改为 tray 后，悬浮球消失，系统托盘出现 FastNote 图标，右键菜单正确。 |
| AC-ENTRY-12 | 托盘图标左键点击能打开/聚焦现有主窗口并保持当前路由；新的进程首次手动启动进入 Today。 |
| AC-ENTRY-13 | 托盘右键菜单包含六个模块跳转 + 设置 + 退出，各选项均正确跳转。 |
| AC-ENTRY-14 | macOS 下无 Dock 图标，菜单栏显示 FastNote 下拉菜单；开机自启用 SMAppService agent。 |
| AC-ENTRY-15 | `--hidden` 仅入口/必要服务冷驻留，不预建主窗/业务模型或查询；再次唤起按需创建主窗并恢复会话。 |
| AC-ENTRY-16 | 退出前未提交的文本（包括快速捕获窗口内容）被安全冲刷；异常退出下次启动有提示。 |
| AC-ENTRY-17 | Release 仅入口冷驻留 ≤80MB、热驻留 ≤130MB，空闲 CPU <0.5%；按 QA-03 §8 多轮回收后稳定，不随已访问模块累积。 |
| AC-ENTRY-18 | 切换 ball ↔ tray 时入口平滑过渡，不出现重叠或空白期超过 200ms。 |
| AC-ENTRY-25 | 主窗保存成功且隐藏后最多热缓存 120 秒，交互/后台事件不续期；重开取消旧回收，超预算可提前卸载，退出不等待期限。 |
| AC-ENTRY-26 | 缓存到期后球/托盘、全局唤起与必要 DB/提醒仍可用；冷重开保持路由/轻量状态，保存失败草稿不丢，迟到回调不重建页面或误销毁新窗。 |

---

## 9. 待确认问题

以下为本文件中所有未决问题的索引，全部已登记至 [QA-02 风险与开放问题](../10-quality/02-risks-and-open-questions.md) §4。具体条目见本文档对应章节。

| 编号 | 位置 | 问题摘要 | 当前默认处理 |
| --- | --- | --- | --- |
| Q-ENTRY-04 | §4.2 | macOS `--hidden` 使用何种自启机制 | 优先 SMAppService agent，低版本回退 |
| Q-ENTRY-05 | §6.1 | Linux SNI 缺失时的处理 | 不自动改设置，inline 提示 |
| Q-ENTRY-06 | §6.2 | 退出时 2s 内保证文本安全落盘 | 同步 flush + force quit 兜底 |
