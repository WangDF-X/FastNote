use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use super::runtime::Runtime;

thread_local! {
    /// 只在 UI 线程访问的内核弱引用；跨线程闭包不捕获 Rc 或任何窗口。
    static KERNEL: RefCell<Weak<Runtime>> = const { RefCell::new(Weak::new()) };
}

/// worker/原生入口经 Slint 事件循环投递唤醒，不要求主窗组件存在。
pub(super) fn make_wakeup() -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(|| {
        if slint::invoke_from_event_loop(dispatch).is_err() {
            tracing::debug!("event loop unavailable during kernel wakeup");
        }
    })
}

/// Rc 装配完成后登记；启动期已排队的唤醒会在事件循环开始后读取此弱引用。
pub(super) fn register(runtime: &Rc<Runtime>) {
    KERNEL.with(|slot| *slot.borrow_mut() = Rc::downgrade(runtime));
}

/// 退出只清除自身注册，后续唤醒不能重新处理已结束的应用。
pub(super) fn unregister(runtime: &Runtime) {
    KERNEL.with(|slot| clear_matching(slot, runtime));
}

fn dispatch() {
    KERNEL.with(|slot| deliver(slot, |runtime| runtime.wakeup()));
}

fn deliver<T>(slot: &RefCell<Weak<T>>, handle: impl FnOnce(Rc<T>)) {
    let target = slot.borrow().upgrade();
    if let Some(target) = target {
        // 不跨处理回调借用注册槽；退出和重入可以立即清除注册。
        handle(target);
    }
}

fn clear_matching<T>(slot: &RefCell<Weak<T>>, expected: &T) {
    if std::ptr::eq(slot.borrow().as_ptr(), expected) {
        *slot.borrow_mut() = Weak::new();
    }
}

#[cfg(test)]
mod tests;
