use std::rc::{Rc, Weak};

use crate::tasks::{PageScope, PageStamp};

use super::super::runtime::Runtime;

/// 延迟回调只携带弱内核和代次，既不会延长 UI 寿命，也不能操作重建后的窗口。
#[derive(Clone)]
pub(in crate::desktop) struct UiTarget {
    runtime: Weak<Runtime>,
    session: Option<u64>,
    page: Option<PageStamp>,
}

impl UiTarget {
    pub(in crate::desktop) fn upgrade(&self) -> Option<Rc<Runtime>> {
        let runtime = self.runtime.upgrade()?;
        if runtime.finished.get()
            || self.session.is_none()
            || runtime.ui_epoch.get().session() != self.session
        {
            return None;
        }
        if self
            .page
            .is_some_and(|stamp| !runtime.ui_epoch.get().accepts(stamp))
        {
            return None;
        }
        Some(runtime)
    }
}

impl Runtime {
    pub(in crate::desktop) fn ui_target(self: &Rc<Self>) -> UiTarget {
        UiTarget {
            runtime: Rc::downgrade(self),
            session: self.ui_epoch.get().session(),
            page: None,
        }
    }

    pub(in crate::desktop) fn page_target(self: &Rc<Self>) -> UiTarget {
        UiTarget {
            page: self.ui_epoch.get().stamp(PageScope::Today),
            ..self.ui_target()
        }
    }
}
