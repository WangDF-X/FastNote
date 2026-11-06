//! SHELL-03 P3：只决定主窗租约，不持有窗口、正文或后台服务。

/// 热缓存的绝对上限；保存确认且真正隐藏后起算。
pub(super) const HOT_LEASE_MS: u64 = 120_000;
/// ARCH-01：Windows 总工作集预算，MB 使用十进制字节。
pub(super) const COLD_BUDGET_BYTES: u64 = 80_000_000;
pub(super) const HOT_BUDGET_BYTES: u64 = 130_000_000;
/// 保守的系统压力信号；只是驱逐条件，不申请额外内存，也不改变全局预算。
pub(super) const SYSTEM_PRESSURE_PERCENT: u32 = 90;

/// 单调启动时间，生产实现必须包含睡眠；测试注入固定 tick。
pub(super) trait TickSource {
    fn milliseconds(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum LeaseState {
    #[default]
    Cold,
    Active,
    Hot {
        deadline: u64,
        generation: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReleaseReason {
    Expired,
    Pressure,
    Budget,
    Exit,
    ShowFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeaseEvent {
    Mounted,
    Shown,
    Hidden {
        protected: bool,
    },
    Tick {
        generation: u64,
    },
    Evict {
        reason: ReleaseReason,
        protected: bool,
    },
    Discard {
        reason: ReleaseReason,
        protected: bool,
    },
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeaseAction {
    None,
    Arm { deadline: u64, generation: u64 },
    Release(ReleaseReason),
}

/// 租约代次与 UI 实例代次分开：热重开复用 UI，但旧计时回调仍失效。
#[derive(Default)]
pub(super) struct UiLease {
    state: LeaseState,
    generation: u64,
    protected_hidden: bool,
}

impl UiLease {
    pub(super) fn state(&self) -> LeaseState {
        self.state
    }

    pub(super) fn label(&self) -> &'static str {
        match self.state {
            LeaseState::Cold => "cold",
            LeaseState::Active if self.protected_hidden => "protected",
            LeaseState::Active => "active",
            LeaseState::Hot { .. } if self.protected_hidden => "protected",
            LeaseState::Hot { .. } => "hot",
        }
    }

    pub(super) fn expired(&self, clock: &impl TickSource) -> Option<u64> {
        match self.state {
            LeaseState::Hot {
                deadline,
                generation,
            } if clock.milliseconds() >= deadline => Some(generation),
            _ => None,
        }
    }

    pub(super) fn apply(&mut self, event: LeaseEvent, clock: &impl TickSource) -> LeaseAction {
        match event {
            LeaseEvent::Mounted | LeaseEvent::Shown => {
                self.state = LeaseState::Active;
                self.protected_hidden = false;
                LeaseAction::None
            }
            LeaseEvent::Hidden { protected } => self.hide(protected, clock.milliseconds()),
            LeaseEvent::Tick { generation } => match self.state {
                LeaseState::Hot {
                    deadline,
                    generation: current,
                } if current == generation && !self.protected_hidden => {
                    if clock.milliseconds() >= deadline {
                        self.release(ReleaseReason::Expired)
                    } else {
                        LeaseAction::Arm {
                            deadline,
                            generation,
                        }
                    }
                }
                _ => LeaseAction::None,
            },
            LeaseEvent::Evict { reason, protected } => {
                if matches!(self.state, LeaseState::Hot { .. }) && !protected {
                    self.release(reason)
                } else {
                    LeaseAction::None
                }
            }
            LeaseEvent::Discard { reason, protected } => {
                if protected {
                    LeaseAction::None
                } else {
                    self.release(reason)
                }
            }
            LeaseEvent::Exit => self.release(ReleaseReason::Exit),
        }
    }

    fn hide(&mut self, protected: bool, now: u64) -> LeaseAction {
        if self.state == LeaseState::Cold {
            return LeaseAction::None;
        }
        if protected {
            if !matches!(self.state, LeaseState::Hot { .. }) {
                self.state = LeaseState::Active;
            }
            self.protected_hidden = true;
            return LeaseAction::None;
        }
        if let LeaseState::Hot {
            deadline,
            generation,
        } = self.state
        {
            let was_protected = std::mem::take(&mut self.protected_hidden);
            if was_protected {
                return if now >= deadline {
                    self.release(ReleaseReason::Expired)
                } else {
                    LeaseAction::Arm {
                        deadline,
                        generation,
                    }
                };
            }
            return LeaseAction::None;
        }
        self.protected_hidden = false;
        let Some(generation) = self.generation.checked_add(1) else {
            return self.release(ReleaseReason::Expired);
        };
        let Some(deadline) = now.checked_add(HOT_LEASE_MS) else {
            return self.release(ReleaseReason::Expired);
        };
        self.generation = generation;
        self.state = LeaseState::Hot {
            deadline,
            generation,
        };
        LeaseAction::Arm {
            deadline,
            generation,
        }
    }

    fn release(&mut self, reason: ReleaseReason) -> LeaseAction {
        let mounted = self.state != LeaseState::Cold;
        self.state = LeaseState::Cold;
        self.protected_hidden = false;
        if mounted {
            LeaseAction::Release(reason)
        } else {
            LeaseAction::None
        }
    }
}

#[cfg(test)]
mod tests;
