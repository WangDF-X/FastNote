# QA-01 需求与验收追溯

> 文档编号：QA-01 ｜ 模块：10 质量与验收 ｜ 状态：已定稿 v0.1 ｜ 更新：2026-10-23
> 上游：[product.md](../../product.md)、[spec/README](../README.md) ｜ 下游：评审、测试、发布

---

## 1. 目的与方法

建立「产品需求 → 设计文档 → 验收标准 → 测试」的双向追溯，确保 [product.md](../../product.md) 第一版范围内的每条需求都有明确落点，且不出现无依据的功能扩张。

**追溯规则**

1. 产品需求以 `product.md` 的章节与条目为准；每条需求至少映射到一份 spec 文档的具体章节。
2. 模块验收标准编号为 `AC-<模块>-NN`；跨模块标准为 `AC-CORE/ARCH/UI/PLAT/QA-NN`。
3. 未实现或调整的需求必须在 §6「范围外与差异」中显式说明，不得静默缺失。
4. 需求变更时，先更新本表与对应模块文档，再改代码。

---

## 2. 常驻入口与整体形态（product §1、§3）

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| §1.2 用户可在设置中选择悬浮球或状态栏模式 | [ENTRY-02 §4](../02-desktop-entry/02-tray-and-lifecycle.md)、[SET-02](../08-settings/02-general-and-appearance.md) | AC-ENTRY-*、AC-SET-* |
| §1.3 悬浮球左键打开主界面，默认今日任务 | [ENTRY-01 §5](../02-desktop-entry/01-floating-ball.md)、[SHELL-01 §4.2](../01-app-shell/01-overview.md) | AC-ENTRY-*、AC-SHELL-01 |
| §1.3 右键菜单含今日任务/日历/小记/日记/计划板/设置 | [ENTRY-01 §6](../02-desktop-entry/01-floating-ball.md) | AC-ENTRY-* |
| §1.3 悬浮球不承载复杂任务编辑 | [ENTRY-01 §1](../02-desktop-entry/01-floating-ball.md) | 设计评审 |
| §1.4 状态栏点击打开主界面、菜单含模块入口/设置/切换模式/退出 | [ENTRY-02 §2–3](../02-desktop-entry/02-tray-and-lifecycle.md) | AC-ENTRY-*、AC-PLAT-03 |
| §1.5 入口轻量、低干扰 | [ENTRY-01 §3](../02-desktop-entry/01-floating-ball.md)、[ARCH-01 §5](../00-architecture/01-tech-and-runtime.md) | AC-ARCH-03、AC-ENTRY-* |
| §1.5 切换入口模式不影响数据 | [ENTRY-02 §4](../02-desktop-entry/02-tray-and-lifecycle.md) | AC-ENTRY-*、AC-ARCH-05 |
| §1.5 关闭主界面后是否驻留由设置决定 | [ENTRY-02 §5](../02-desktop-entry/02-tray-and-lifecycle.md)、[SHELL-01 §5](../01-app-shell/01-overview.md) | AC-SHELL-02、AC-PLAT-12 |
| §3.1 悬浮球可拖动、调整大小和透明度 | [ENTRY-01 §3–4](../02-desktop-entry/01-floating-ball.md)、[SET-02](../08-settings/02-general-and-appearance.md) | AC-ENTRY-*、AC-PLAT-02 |
| §3.1 支持后台驻留、全局快捷键、关闭后驻留 | [ENTRY-02](../02-desktop-entry/02-tray-and-lifecycle.md)、[ENTRY-03](../02-desktop-entry/03-global-hotkeys.md) | AC-ENTRY-*、AC-PLAT-04/05/12 |
| §3.2 五个核心模块 + 设置独立入口 | [SHELL-01 §2](../01-app-shell/01-overview.md) | AC-SHELL-01 |

---

## 3. 核心功能（product §4）

### 3.1 今日任务

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| 快速添加、编辑、完成、删除任务 | [TODAY-02](../03-today-tasks/02-task-actions.md)、[TODAY-03](../03-today-tasks/03-task-detail.md) | AC-TODAY-* |
| 查看当天需要完成的任务 | [TODAY-01 §3](../03-today-tasks/01-overview-and-sections.md)、[ARCH-03 §8.2](../00-architecture/03-domain-model.md) | AC-CORE-02 |
| 查看当天计划开始的任务 | 同上（`starts_today`） | AC-CORE-02 |
| 查看等待处理/等待反馈的任务 | 同上（`waiting`） | AC-CORE-02 |
| 查看尚未安排日期的任务 | 同上（`unscheduled`） | AC-CORE-02 |
| 逾期任务提醒 | [TODAY-01 §5](../03-today-tasks/01-overview-and-sections.md)、[SHELL-02 §9](../01-app-shell/02-interaction-and-feedback.md) | AC-TODAY-*、AC-SHELL-18 |
| 查看任务所属计划 | [TODAY-01 §4](../03-today-tasks/01-overview-and-sections.md) | AC-TODAY-* |
| 明日任务独立折叠区、默认不展开 | [TODAY-01 §3](../03-today-tasks/01-overview-and-sections.md)、[SET-03](../08-settings/03-hotkeys-task-prefs-and-about.md) | AC-TODAY-*、AC-SET-* |
| 不填日期/优先级即可创建 | [TODAY-02 §2](../03-today-tasks/02-task-actions.md)、[ARCH-03 §7.1](../00-architecture/03-domain-model.md) | AC-CORE-01、AC-TODAY-* |

### 3.2 日历

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| 按日期查看任务 | [CAL-01](../04-calendar/01-overview-and-views.md) | AC-CAL-* |
| 查看某一天的任务安排 | [CAL-01 §4](../04-calendar/01-overview-and-views.md) | AC-CAL-* |
| 从日历快速创建任务 | [CAL-01 §5](../04-calendar/01-overview-and-views.md) | AC-CAL-* |
| 调整计划开始/完成日期 | [CAL-02](../04-calendar/02-date-operations-and-conflicts.md)、[TODAY-03](../03-today-tasks/03-task-detail.md) | AC-CAL-*、AC-TODAY-* |
| 识别逾期及日期冲突 | [CAL-02 §4](../04-calendar/02-date-operations-and-conflicts.md)、[ARCH-03 §8.6](../00-architecture/03-domain-model.md) | AC-CAL-*、AC-CORE-* |
| 与今日任务、计划板同步 | [ARCH-02 §5](../00-architecture/02-architecture.md) | AC-ARCH-09、AC-CORE-01 |

### 3.3 小记

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| 快速创建和编辑小记 | [NOTE-01](../05-notes/01-overview-and-list.md)、[NOTE-02 §2](../05-notes/02-quick-capture-and-convert.md) | AC-NOTE-* |
| 保存临时想法/备忘/短内容 | [NOTE-01 §2](../05-notes/01-overview-and-list.md) | AC-NOTE-* |
| 查看已有小记 | [NOTE-01 §3](../05-notes/01-overview-and-list.md) | AC-NOTE-* |
| 小记转任务 | [NOTE-02 §3](../05-notes/02-quick-capture-and-convert.md) | AC-NOTE-* |
| 转换后小记保留，除非主动删除 | [NOTE-02 §3](../05-notes/02-quick-capture-and-convert.md) | AC-NOTE-* |
| 创建时不要求分类/日期等 | [NOTE-01 §2](../05-notes/01-overview-and-list.md)、[ARCH-03 §3.4](../00-architecture/03-domain-model.md) | AC-NOTE-* |

### 3.4 日记

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| 按日期创建、查看、编辑日记 | [DIARY-01](../06-journal/01-overview-and-editor.md) | AC-DIARY-* |
| 方便浏览过去日记 | [DIARY-01 §3](../06-journal/01-overview-and-editor.md) | AC-DIARY-* |
| 记录工作进展、感受、复盘 | [DIARY-01 §4](../06-journal/01-overview-and-editor.md) | AC-DIARY-* |
| 结合当天完成任务回顾 | [DIARY-02](../06-journal/02-review-and-task-link.md) | AC-DIARY-* |
| 日记与任务关联但不强制创建任务 | [DIARY-02 §3](../06-journal/02-review-and-task-link.md) | AC-DIARY-* |
| 独立、自然的记录体验 | [DIARY-01 §1](../06-journal/01-overview-and-editor.md)、[SHELL-01 §3](../01-app-shell/01-overview.md) | 设计评审 |

### 3.5 计划板

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| 多计划并行管理 | [PLAN-01](../07-plan-board/01-overview-and-plan-list.md) | AC-PLAN-* |
| 计划名称、描述、开始/目标结束日期 | [PLAN-02 §2](../07-plan-board/02-plan-management.md) | AC-PLAN-* |
| 阶段路线图（非甘特图） | [PLAN-03](../07-plan-board/03-stage-roadmap.md) | AC-PLAN-* |
| 阶段展开/折叠 | [PLAN-03 §4](../07-plan-board/03-stage-roadmap.md) | AC-PLAN-* |
| 任意层级子任务 | [PLAN-04](../07-plan-board/04-task-tree-and-move.md)、[ARCH-03 §2](../00-architecture/03-domain-model.md) | AC-CORE-04 |
| 任意层级添加任务，不强制归属阶段 | [PLAN-03 §3](../07-plan-board/03-stage-roadmap.md)、[PLAN-04 §3](../07-plan-board/04-task-tree-and-move.md) | AC-PLAN-* |
| 查看计划整体与各阶段进度 | [PLAN-03 §5](../07-plan-board/03-stage-roadmap.md)、[ARCH-03 §8.5](../00-architecture/03-domain-model.md) | AC-CORE-05 |
| 计划/阶段/任务日期 | [PLAN-02](../07-plan-board/02-plan-management.md)、[TODAY-03](../03-today-tasks/03-task-detail.md) | AC-PLAN-* |
| 任务移动到其它阶段 | [PLAN-04 §5](../07-plan-board/04-task-tree-and-move.md) | AC-PLAN-* |
| 日期冲突、逾期、进度异常提示 | [PLAN-03 §6](../07-plan-board/03-stage-roadmap.md)、[ARCH-03 §8.6](../00-architecture/03-domain-model.md) | AC-CORE-*、AC-PLAN-* |
| 任务前置依赖 | [PLAN-05](../07-plan-board/05-dependencies.md) | AC-CORE-06、AC-PLAN-* |
| 归档已结束计划并保留历史 | [PLAN-02 §4](../07-plan-board/02-plan-management.md)、[ARCH-03 §7.6](../00-architecture/03-domain-model.md) | AC-CORE-07、AC-CORE-08 |
| 依赖：前置未完成提示、不强制锁定 | [PLAN-05 §4](../07-plan-board/05-dependencies.md)、[ARCH-03 §8.4](../00-architecture/03-domain-model.md) | AC-CORE-06 |
| 依赖：日期冲突提示、不自动修改日期 | [PLAN-05 §5](../07-plan-board/05-dependencies.md) | AC-PLAN-* |
| 依赖表达逻辑顺序而非强制流程 | [ARCH-03 §8.4](../00-architecture/03-domain-model.md)、[PLAN-05 §1](../07-plan-board/05-dependencies.md) | 设计评审 |

---

## 4. 统一任务与设置（product §5、§6）

| 产品需求 | 设计落点 | 验收 |
| --- | --- | --- |
| §5.1 同一任务在多模块出现，修改同步 | [ARCH-03](../00-architecture/03-domain-model.md)、[ARCH-02 §5](../00-architecture/02-architecture.md) | AC-CORE-01、AC-ARCH-09 |
| §5.2 任务字段（标题/描述/状态/日期/优先级/计划/父级/依赖） | [ARCH-03 §3.2](../00-architecture/03-domain-model.md)、[TODAY-03](../03-today-tasks/03-task-detail.md) | AC-TODAY-*、AC-CORE-* |
| §5.2 字段非全部必填、渐进补充 | [ARCH-03 §7.1](../00-architecture/03-domain-model.md)、[TODAY-02 §2](../03-today-tasks/02-task-actions.md) | AC-TODAY-* |
| §5.3 四种状态，与日期独立 | [ARCH-03 §4.2](../00-architecture/03-domain-model.md) | AC-CORE-* |
| §5.4 普通任务独立存在、可逐步纳入计划 | [ARCH-03 §2.1](../00-architecture/03-domain-model.md)、[TODAY-03 §6](../03-today-tasks/03-task-detail.md) | AC-CORE-04 |
| §5.4 任意层级子任务 | [ARCH-03 §2.1](../00-architecture/03-domain-model.md)、[PLAN-04](../07-plan-board/04-task-tree-and-move.md) | AC-CORE-04 |
| §5.4 移动保留日期、冲突提示、不擅自调整 | [ARCH-03 §7.3](../00-architecture/03-domain-model.md)、[PLAN-04 §5](../07-plan-board/04-task-tree-and-move.md) | AC-PLAN-* |
| §6.1 全局唤起/新建任务/新建小记快捷键 | [ENTRY-03](../02-desktop-entry/03-global-hotkeys.md) | AC-PLAT-04 |
| §6.1 快捷键可自定义、可恢复默认 | [SET-03](../08-settings/03-hotkeys-task-prefs-and-about.md) | AC-SET-* |
| §6.1 默认组合兼顾平台与输入法 | [ENTRY-03 §2](../02-desktop-entry/03-global-hotkeys.md) | AC-PLAT-04 |
| §6.2 常规与待机设置 | [SET-02 §2](../08-settings/02-general-and-appearance.md) | AC-SET-* |
| §6.2 快捷键查看/修改/冲突/恢复 | [SET-03 §2](../08-settings/03-hotkeys-task-prefs-and-about.md) | AC-SET-* |
| §6.2 外观（主题/密度/球尺寸透明度） | [SET-02 §3](../08-settings/02-general-and-appearance.md) | AC-SET-*、AC-UI-* |
| §6.2 任务与计划偏好 | [SET-03 §3](../08-settings/03-hotkeys-task-prefs-and-about.md) | AC-SET-* |
| §6.2 数据位置、备份恢复、导入导出 | [DATA-01](../09-data-management/01-storage-location-and-lifecycle.md)、[DATA-02](../09-data-management/02-backup-and-restore.md)、[DATA-03](../09-data-management/03-import-export.md) | AC-DATA-* |
| §6.2 设置精简、只保留可用项 | [SET-01 §2](../08-settings/01-overview.md) | 设计评审 |

---

## 5. 验收标准追溯（product §9）

| # | 产品验收标准 | 设计落点 | 验证方式 |
| --- | --- | --- | --- |
| 1 | 快速记录任务，不必填写大量信息 | [TODAY-02 §2](../03-today-tasks/02-task-actions.md)、[ARCH-03 §7.1](../00-architecture/03-domain-model.md) | 快速添加仅输入标题即可创建；字段全部可选（日期/优先级/计划） |
| 2 | 打开应用清楚今天做什么、哪些逾期 | [TODAY-01 §3](../03-today-tasks/01-overview-and-sections.md)、[SHELL-02 §9](../01-app-shell/02-interaction-and-feedback.md) | 今日分组 + 逾期徽标 + 逾期提醒 |
| 3 | 大型事项拆成阶段与任意层级子任务 | [PLAN-03](../07-plan-board/03-stage-roadmap.md)、[PLAN-04](../07-plan-board/04-task-tree-and-move.md) | 创建计划 → 阶段 → 多层子任务并移动 |
| 4 | 直观查看整体进展，无需学习复杂图表 | [PLAN-01](../07-plan-board/01-overview-and-plan-list.md)、[PLAN-03 §5](../07-plan-board/03-stage-roadmap.md) | 计划进度条 + 阶段进度 + 计数，无甘特图 |
| 5 | 任务在今日/日历/计划板一致 | [ARCH-02 §5](../00-architecture/02-architecture.md) | 同一任务三处修改互相同步（AC-ARCH-09） |
| 6 | 自由调整状态和日期，仅在必要时提示 | [ARCH-03 §7.2](../00-architecture/03-domain-model.md)、[SHELL-02 §6](../01-app-shell/02-interaction-and-feedback.md) | 任意状态互转；冲突仅警告；确认后可继续 |
| 7 | 依赖帮助理解顺序但不限制操作 | [PLAN-05 §4](../07-plan-board/05-dependencies.md) | 阻塞任务可开始（确认后）；不锁定 UI |
| 8 | 通过悬浮球或快捷键快速进入 | [ENTRY-01](../02-desktop-entry/01-floating-ball.md)、[ENTRY-03](../02-desktop-entry/03-global-hotkeys.md) | 点击/快捷键唤起延迟与行为 |
| 9 | 低资源长期驻留 | [ARCH-01 §5](../00-architecture/01-tech-and-runtime.md)、[QA-03 §8](03-test-strategy.md) | 内存/CPU/启动基线 |
| 10 | 结构支持未来 AI Agent 而不改核心方式 | [ARCH-02 §9](../00-architecture/02-architecture.md) | 架构评审：Command/Query 边界、只读查询、变更提案预留 |

---

## 6. 范围外与差异说明

### 6.1 第一版明确不做（product §8）

| 项 | 处理 |
| --- | --- |
| 自动排期、关键路径分析 | 不实现；依赖只提示（D-17）。 |
| 复杂甘特图 | 不实现；使用阶段路线图（PLAN-03）。 |
| 云同步、多人协作 | 不实现；纯本地（D-06）。 |
| Agent 自动拆分/执行 | 不实现；仅预留接口边界（ARCH-02 §9）。 |
| MCP 服务、LLM 调用 | 不实现（product §7、D-15）。 |
| 企业级项目管理（资源、工时、权限） | 不实现。 |

### 6.2 「可以逐步完善」项的第一版处理

| 项 | 第一版处理 |
| --- | --- |
| 更丰富的筛选和排序 | 不做全局筛选/搜索；仅提供 `tasks.default_sort` 与今日分组显示开关。 |
| 更方便的计划进度展示 | 提供计划/阶段进度条与计数；更丰富报表后续。 |
| 日记与任务的关联体验 | 提供当日完成/到期回顾与插入摘要；不建硬关联。 |
| 更完善的外观设置 | 主题、密度、球尺寸/透明度；不含字体/强调色自定义。 |
| 数据导入导出体验 | 全量 JSON 导入/导出；Markdown/CSV 后续。 |
| 更多平台驻留优化 | 三平台入口完整；Wayland 受限（D-22）。 |

### 6.3 设计中对产品文档的补充（需产品确认）

| 补充 | 理由 | 落点 |
| --- | --- | --- |
| 今日页增加「进行中」分组 | 进行中跨天任务若只靠「未安排/今日开始」会漏看（product §5.3 允许无日期进行中）。 | TODAY-01、ARCH-03 §8.2 |
| 悬浮球/托盘菜单增加「退出」 | 球模式下无其它退出路径（product 仅列模块入口）。 | ENTRY-01、ENTRY-02 |
| 删除计划必须先归档 | 防止误删进行中的数据（D-11）。 | PLAN-02 |
| 阶段无执行状态、只有进度 | 避免「阶段完成但任务未完成」矛盾（D-08）。 | ARCH-03 §4、PLAN-03 |
| 归档计划任务从今日/日历隐藏 | 归档代表已结束，避免长期噪音（D-09）；日记历史仍保留。 | ARCH-03 §8、DIARY-02 |
| 小记转任务一对一 | 简化关联与界面（D-13）。 | NOTE-02 |
| **已确认变更（2026-10-25）**：月视图 + 周视图 + 日期详情，Today 独立，无独立日视图 | 用户确认，替换仅月视图默认。 | CAL-01/02、ARCH-02/03 |
| **已确认变更（2026-10-25）**：v1 系统提醒 | 任务/小记单次/每天/每周；仅运行时，最近 24 小时错过项汇总补发。 | [REM-01](../11-reminders/01-reminders-and-notifications.md)、ARCH-04/06、SHELL-02 |
| 悬浮球大小/透明度归入「常规与待机」 | 与入口模式强耦合；避免托盘模式下外观分组整组禁用（product §6.2 列在外观下）。 | SET-01 D-SET-03、QA-02 Q-24 |

> 以上补充已写入各模块文档；若产品评审不同意，按变更流程调整对应文档与验收。

---

## 7. 模块验收索引

| 模块 | 文档 | 验收编号 |
| --- | --- | --- |
| 架构与运行时 | [ARCH-01](../00-architecture/01-tech-and-runtime.md) | AC-ARCH-01–08 |
| 架构与数据流 | [ARCH-02](../00-architecture/02-architecture.md) | AC-ARCH-09–15 |
| 领域模型 | [ARCH-03](../00-architecture/03-domain-model.md) | AC-CORE-01–10 |
| 存储 | [ARCH-04](../00-architecture/04-storage-and-data.md) | AC-ARCH-16–24 |
| UI 系统 | [ARCH-05](../00-architecture/05-ui-design-system.md) | AC-UI-01–08 |
| 平台集成 | [ARCH-06](../00-architecture/06-platform-integration.md) | AC-PLAT-01–12 |
| 应用外壳 | [SHELL-01](../01-app-shell/01-overview.md)、[SHELL-02](../01-app-shell/02-interaction-and-feedback.md) | AC-SHELL-01–20 |
| 桌面入口 | [ENTRY-01](../02-desktop-entry/01-floating-ball.md)–[03](../02-desktop-entry/03-global-hotkeys.md) | AC-ENTRY-* |
| 今日任务 | [TODAY-01](../03-today-tasks/01-overview-and-sections.md)–[03](../03-today-tasks/03-task-detail.md) | AC-TODAY-* |
| 日历 | [CAL-01](../04-calendar/01-overview-and-views.md)–[02](../04-calendar/02-date-operations-and-conflicts.md) | AC-CAL-* |
| 小记 | [NOTE-01](../05-notes/01-overview-and-list.md)–[02](../05-notes/02-quick-capture-and-convert.md) | AC-NOTE-* |
| 日记 | [DIARY-01](../06-journal/01-overview-and-editor.md)–[02](../06-journal/02-review-and-task-link.md) | AC-DIARY-* |
| 计划板 | [PLAN-01](../07-plan-board/01-overview-and-plan-list.md)–[05](../07-plan-board/05-dependencies.md) | AC-PLAN-* |
| 设置 | [SET-01](../08-settings/01-overview.md)–[03](../08-settings/03-hotkeys-task-prefs-and-about.md) | AC-SET-* |
| 数据管理 | [DATA-01](../09-data-management/01-storage-location-and-lifecycle.md)–[03](../09-data-management/03-import-export.md) | AC-DATA-* |
| 提醒与系统通知 | [REM-01](../11-reminders/01-reminders-and-notifications.md) | AC-REM-* |
| 质量 | [QA-02](02-risks-and-open-questions.md)、[QA-03](03-test-strategy.md) | AC-QA-01–15 |

> `*` 表示该模块文档中定义的完整编号集合；具体条目见各文档末节。

---

## 8. 发布前签署清单

- [ ] product §8「必须具备」逐项有实现与验收记录。
- [ ] product §9 十条验收全部通过（含手工验证证据）。
- [ ] 本追溯表无「无落点」需求；范围外项已获产品确认。
- [ ] 所有模块文档的 `AC-*` 在测试计划中有对应用例或手工步骤。
- [ ] 已知限制（Wayland、长文本、IME 等）已记录在 QA-02 与用户可见说明中。
- [ ] 变更记录已更新，spec 版本与实现版本对应。

---

## 10. 待确认问题

> **当前状态：无待确认项。** 
> 开发过程中如遇到未决问题，在此登记并注明「当前默认处理」。已关闭的问题归档至 [QA-02](../10-quality/02-risks-and-open-questions.md) §7。