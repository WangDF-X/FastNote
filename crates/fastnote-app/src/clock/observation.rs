use fastnote_core::{DateTime, NaiveDateTime, TaskClock, Utc};

/// 一次 UTC 读取及同一时刻的本地转换，不保存页面数据或依赖 UI。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClockSample {
    pub utc: DateTime<Utc>,
    pub local: NaiveDateTime,
}

/// 仅保留上一次观察，供无主窗内核检测日期、时区和时钟回拨。
#[derive(Default)]
pub(crate) struct ClockTracker {
    previous: Option<ClockSample>,
}

/// 本次时钟样本与变化结果；首次只建立基线，不表示页面已经加载。
pub(crate) struct ClockCheck {
    pub sample: ClockSample,
    pub changed: bool,
}

impl ClockTracker {
    /// SHELL-03 T2.4：注入时钟，单次采样；隐藏期间同样更新轻量基线。
    pub(crate) fn observe(&mut self, clock: &impl TaskClock) -> ClockCheck {
        let utc = clock.now();
        let sample = ClockSample {
            utc,
            local: clock.local_datetime(utc),
        };
        let changed = self.previous.is_some_and(|previous| {
            sample.utc < previous.utc
                || sample.local < previous.local
                || sample.local.date() != previous.local.date()
                || sample.local.signed_duration_since(sample.utc.naive_utc())
                    != previous
                        .local
                        .signed_duration_since(previous.utc.naive_utc())
        });
        self.previous = Some(sample);
        ClockCheck { sample, changed }
    }
}

#[cfg(test)]
mod tests;
