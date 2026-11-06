use std::rc::Rc;
use std::time::Duration;

use fastnote_core::TaskId;
use fastnote_ui::{DialogOutcome, ShellFocus, ToastKind};
use slint::{ComponentHandle, Timer};

use crate::tasks::DetailPurpose;

use super::super::runtime::Runtime;

impl Runtime {
    /// 只读取已挂载的今日页，模型解除绑定引起的零滚动不会覆盖恢复锚点。
    pub(in crate::desktop) fn capture_task_resume(&self) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if !window.get_today_mounted() || window.get_current_page() != "今日任务" {
                return;
            }
            let anchor = window.global::<ShellFocus>().get_anchor();
            let mut tasks = self.tasks.borrow_mut();
            let selected = match tasks.queries.get(&tasks.latest_open) {
                Some(DetailPurpose::Open(id)) => Some(*id),
                _ => tasks.editor.as_ref().map(|editor| editor.detail.task.id),
            };
            tasks
                .resume
                .capture(selected, anchor.as_str(), window.get_today_scroll_y());
        });
    }

    /// 没有正文缓存时只为当前可见页重查一个详情；冷重开不预载其它实体。
    pub(in crate::desktop) fn restore_selected_task(&self) {
        let selected = {
            let tasks = self.tasks.borrow();
            if tasks.editor.is_some() || tasks.latest_open != 0 {
                return;
            }
            tasks.resume.selected
        };
        if let Some(id) = selected {
            self.request_task_detail(DetailPurpose::Open(id));
        }
    }

    /// 数据/布局就绪后按有效 ID 恢复；旧页/旧窗的延迟恢复无权操作新 UI。
    pub(in crate::desktop) fn defer_task_resume(self: &Rc<Self>) {
        let target = self.page_target();
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = target.upgrade() {
                runtime.apply_task_resume();
            }
        });
    }

    fn apply_task_resume(&self) {
        if !self.can_focus_tasks() || self.tasks.borrow().today.is_none() {
            return;
        }
        let (scroll, focus, waiting) = {
            let mut tasks = self.tasks.borrow_mut();
            let waiting =
                tasks.resume.selected.is_some() && tasks.editor.is_none() && tasks.latest_open != 0;
            let scroll =
                std::mem::take(&mut tasks.resume.scroll_pending).then_some(tasks.resume.scroll_y);
            let focus = if !waiting && std::mem::take(&mut tasks.resume.focus_pending) {
                tasks.resume.focus.clone()
            } else {
                None
            };
            (scroll, focus, waiting)
        };
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if let Some(scroll) = scroll {
                window.set_today_scroll_y(scroll);
            }
            if waiting {
                return;
            }
            if let Some(anchor) = focus {
                let anchor = self.valid_resume_focus(&anchor);
                window.global::<ShellFocus>().set_anchor(anchor.into());
                window.invoke_restore_task_focus();
            }
        });
    }

    fn valid_resume_focus(&self, anchor: &str) -> String {
        let tasks = self.tasks.borrow();
        if anchor.starts_with("editor:") {
            return match tasks.editor.as_ref() {
                Some(editor) if editor.detail.context.read_only => "editor:close".to_owned(),
                Some(_) => anchor.to_owned(),
                None => "today:quick".to_owned(),
            };
        }
        if let Some(id) = anchor
            .strip_prefix("task:")
            .and_then(|id| id.parse::<TaskId>().ok())
        {
            if !tasks.visible_ids().contains(&id) {
                return "today:quick".to_owned();
            }
        }
        if anchor.starts_with("today:section:") || anchor.starts_with("today:more:") {
            let index = anchor
                .rsplit(':')
                .next()
                .and_then(|index| index.parse::<usize>().ok());
            if !index.is_some_and(|index| {
                tasks.today.as_ref().is_some_and(|view| {
                    view.sections
                        .iter()
                        .any(|section| section.kind as usize == index)
                })
            }) {
                return "today:quick".to_owned();
            }
        }
        anchor.to_owned()
    }

    /// 快速输入与日期弹层未提交时不自动建任务或丢输入，只阻止正常收起/离开。
    pub(in crate::desktop) fn transient_input_blocks_close(&self) -> bool {
        let message = self
            .with_ui(|ui| {
                let window = ui.shell.component();
                if window.get_task_picker_open() {
                    Some("请先确认或取消日期选择。")
                } else if !window.get_task_quick_title().is_empty() {
                    Some("快速添加中有未提交输入，请先添加任务或按 Esc 清空。")
                } else {
                    None
                }
            })
            .flatten();
        if let Some(message) = message {
            self.notice(ToastKind::Info, message);
            true
        } else {
            false
        }
    }

    /// 对话框取消只撤销匹配许可，绝不执行原确认命令，也不清理其它排队决策。
    pub(in crate::desktop) fn observe_dialog_resolution(
        &self,
        command: &str,
        outcome: DialogOutcome,
    ) {
        if outcome != DialogOutcome::Cancelled {
            return;
        }
        self.tasks.borrow_mut().cancel_confirmation(command);
        if command == "desktop:force-quit" {
            self.force_allowed.set(false);
        }
    }
}
