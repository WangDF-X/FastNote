# ENTRY-02 系统托盘与生命周期

> 文档编号：ENTRY-02 ｜ 模块：02 桌面常驻入口 ｜ 状态：已定稿 v0.1 ｜ 更新日期：2026-10-23
> 上游：[ARCH-06](../00-architecture/06-platform-integration.md)、[product §1.4](../../product.md) ｜ 下游：[SHELL-01](../01-app-shell/01-overview.md)、[SET-02](../08-settings/02-general-and-appearance.md)

---

## 1. 目的与范围

定义 FastNote 在 tray（系统托盘 / 菜单栏图标）模式下的完整行为：托盘图标的创建、菜单结构、点击响应、应用退出流程中的文本冲刷与数据持久化，以及悬浮球/tray 模式的互斥切换规则。

**范围内**: 托盘创建、菜单项与交互、关闭按钮映射、启动参数 `--hidden` 处理、退出冲刷流程。
**范围外**: 悬浮球实现细节（ENTRY-01）、全局快捷键（ENTRY-03）。

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
| 图标 | 白色/黑色单色 SVG 图标（跟随主题自动切换） |
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
| `--hidden` | 不显示主窗口；只创建入口（ball 或 tray）并驻留后台 |
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
| `entry`（默认） | 隐藏主窗口（hide），保留悬浮球或托盘 |
| `quit` | 执行完整退出流程 |

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
7. 保存窗口几何与入口位置到 app_meta.json
8. drop 所有 Arc<TokioChannel> / crossbeam Sender
9. join 后台线程
10. std::process::exit(0)
```

> **已确认（2026-10-26）Q-ENTRY-05：** Linux SNI（StatusNotifierItem）不支持动态改变托盘菜单时，是否强制切换图标样式？→ **当前默认处理：** 不自动更改设置；在 SET-02 的设置页内以 inline error 提示「当前桌面环境不支持完整的托盘菜单」，提供恢复原值的操作。

> **已确认（2026-10-26）Q-ENTRY-06：** 用户在快速捕获窗口输入了大量文本后调用退出，如何在 2s 内确保文本安全落盘？→ **当前默认处理：** 退出流程中同步提交一次文本（阻塞退出直到 flush 完成或超时 2s），随后 force quit。若文本未成功 flush，下次启动时提示「上次未正常关闭」。

---

## 7. 资源管理

- 空闲时 CPU ≤ 0.5%（无活动页面时暂停 DOM 更新）。
- 内存占用控制在 80MB 以内（Slint cached image + DB connection + 事件订阅者）。
- 不持有任务/日记正文的长期内存缓存（按需查询）。
- 托盘/球模式下继续处理命令队列（通过有界 channel 背压）。

---

## 8. 验收标准

| 编号 | 验收标准 |
| --- | --- |
| AC-ENTRY-11 | 设置中将 entry_mode 改为 tray 后，悬浮球消失，系统托盘出现 FastNote 图标，右键菜单正确。 |
| AC-ENTRY-12 | 托盘图标左键点击能打开主窗口并导航到 Today；重复点击无重复效果。 |
| AC-ENTRY-13 | 托盘右键菜单包含六个模块跳转 + 设置 + 退出，各选项均正确跳转。 |
| AC-ENTRY-14 | macOS 下无 Dock 图标，菜单栏显示 FastNote 下拉菜单；开机自启用 SMAppService agent。 |
| AC-ENTRY-15 | `--hidden` 启动不显示主窗口，仅驻留入口；再次手动启动显示主窗口。 |
| AC-ENTRY-16 | 退出前未提交的文本（包括快速捕获窗口内容）被安全冲刷；异常退出下次启动有提示。 |
| AC-ENTRY-17 | 空闲状态下托盘/球进程的 CPU < 0.5%，内存 < 80MB。 |
| AC-ENTRY-18 | 切换 ball ↔ tray 时入口平滑过渡，不出现重叠或空白期超过 200ms。 |

---

## 9. 待确认问题

以下为本文件中所有未决问题的索引，全部已登记至 [QA-02 风险与开放问题](../10-quality/02-risks-and-open-questions.md) §4。具体条目见本文档对应章节。

| 编号 | 位置 | 问题摘要 | 当前默认处理 |
| --- | --- | --- | --- |
| Q-ENTRY-04 | §4.2 | macOS `--hidden` 使用何种自启机制 | 优先 SMAppService agent，低版本回退 |
| Q-ENTRY-05 | §6.1 | Linux SNI 缺失时的处理 | 不自动改设置，inline 提示 |
| Q-ENTRY-06 | §6.2 | 退出时 2s 内保证文本安全落盘 | 同步 flush + force quit 兜底 |
