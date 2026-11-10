use chrono::{DateTime, Utc};

/// 最新删除批次的无正文令牌；期限从删除提交后起算，不受悬停影响。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteUndoTicket {
    /// 应用层生成的命令令牌。
    pub token: u64,
    /// 严格的 UTC 截止时间。
    pub expires_at: DateTime<Utc>,
}

/// DB 工作线程持有的撤销状态；最多一个批次且不缓存任何正文。
#[derive(Debug, Default)]
pub struct NoteUndoState {
    ticket: Option<NoteUndoTicket>,
}

impl NoteUndoState {
    /// 激活最新批次，先前批次立即失效。
    pub fn activate(&mut self, token: u64, now: DateTime<Utc>) -> NoteUndoTicket {
        let ticket = NoteUndoTicket {
            token,
            expires_at: now + chrono::Duration::seconds(2),
        };
        self.ticket = Some(ticket);
        ticket
    }
    /// 最新令牌，不携带被删除的小记内容。
    pub fn ticket(&self) -> Option<NoteUndoTicket> {
        self.ticket
    }
    /// 只有最新批次且严格早于截止时间才可撤销。
    pub fn permits(&self, token: u64, now: DateTime<Utc>) -> bool {
        self.ticket
            .is_some_and(|ticket| ticket.token == token && now < ticket.expires_at)
    }
    /// 当前批次是否需要从临时数据库销毁。
    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        self.ticket.is_some_and(|ticket| now >= ticket.expires_at)
    }
    /// 恢复或销毁成功后清除令牌。
    pub fn clear(&mut self) {
        self.ticket = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_only_accepts_latest_ticket_before_exact_two_second_deadline() {
        let now: DateTime<Utc> = "2026-11-09T08:00:00Z".parse().unwrap();
        let mut state = NoteUndoState::default();
        assert!(!state.permits(1, now));
        assert!(!state.expired(now));
        assert_eq!(state.ticket(), None);
        state.activate(1, now);
        let latest = state.activate(2, now);
        assert!(!state.permits(1, now));
        assert!(state.permits(2, latest.expires_at - chrono::Duration::microseconds(1)));
        assert!(!state.permits(2, latest.expires_at));
        assert!(state.expired(latest.expires_at));
        state.clear();
        assert!(!state.expired(latest.expires_at));
    }
}
