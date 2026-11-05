//! 页面读取/延迟回调的代次，不持有任何 UI、正文或数据库对象。

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageScope {
    Today,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageStamp {
    pub session_generation: u64,
    pub page_generation: u64,
    pub scope: PageScope,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UiEpoch {
    session_generation: u64,
    page_generation: u64,
    mounted: bool,
}

impl UiEpoch {
    /// 无重复代次：溢出时拒绝建窗，而不是让旧回包匹配新页面。
    pub(crate) fn mount(&mut self) -> bool {
        let Some(generation) = self.session_generation.checked_add(1) else {
            return false;
        };
        self.session_generation = generation;
        self.page_generation = 1;
        self.mounted = true;
        true
    }

    pub(crate) fn unmount(&mut self) {
        self.mounted = false;
    }

    pub(crate) fn invalidate_page(&mut self) {
        // 溢出后禁止该会话继续分配页面读意图，重建主窗才可继续。
        if self.page_generation != 0 {
            self.page_generation = self.page_generation.checked_add(1).unwrap_or(0);
        }
    }

    pub(crate) fn session(&self) -> Option<u64> {
        self.mounted.then_some(self.session_generation)
    }

    pub(crate) fn stamp(&self, scope: PageScope) -> Option<PageStamp> {
        (self.mounted && self.page_generation != 0).then_some(PageStamp {
            session_generation: self.session_generation,
            page_generation: self.page_generation,
            scope,
        })
    }

    pub(crate) fn accepts(&self, stamp: PageStamp) -> bool {
        self.stamp(stamp.scope) == Some(stamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cold_does_not_allocate_a_page_stamp() {
        let epoch = UiEpoch::default();
        assert_eq!(epoch.session(), None);
        assert_eq!(epoch.stamp(PageScope::Today), None);
    }

    #[test]
    fn test_hidden_or_route_change_invalidates_reads_without_rebuilding_session() {
        let mut epoch = UiEpoch::default();
        assert!(epoch.mount());
        let stamp = epoch.stamp(PageScope::Today).unwrap();
        assert!(epoch.accepts(stamp));
        epoch.invalidate_page();
        assert!(!epoch.accepts(stamp));
        assert_eq!(epoch.session(), Some(stamp.session_generation));
        let next = epoch.stamp(PageScope::Today).unwrap();
        assert_ne!(next.page_generation, stamp.page_generation);
        assert!(epoch.accepts(next));
    }

    #[test]
    fn test_old_session_callbacks_and_page_replies_fail_after_five_rebuilds() {
        let mut epoch = UiEpoch::default();
        let mut old = Vec::new();
        for _ in 0..5 {
            assert!(epoch.mount());
            assert!(old.iter().all(|stamp| !epoch.accepts(*stamp)));
            let current = epoch.stamp(PageScope::Today).unwrap();
            old.push(current);
            epoch.unmount();
            assert!(!epoch.accepts(current));
            assert_eq!(epoch.session(), None);
        }
    }

    #[test]
    fn test_epoch_overflow_does_not_reuse_an_old_stamp() {
        let mut epoch = UiEpoch {
            session_generation: u64::MAX,
            ..Default::default()
        };
        assert!(!epoch.mount());
        assert_eq!(epoch.session(), None);
        epoch.session_generation = 0;
        assert!(epoch.mount());
        epoch.page_generation = u64::MAX;
        epoch.invalidate_page();
        assert_eq!(epoch.stamp(PageScope::Today), None);
        epoch.invalidate_page();
        assert_eq!(epoch.stamp(PageScope::Today), None);
    }
}
