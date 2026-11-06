//! 可复用的样本统计口径；不负责构造场景或声称不同页面工作量相同。

/// 输入原始微秒样本，排序后按 nearest-rank 计算中位数、p95 与最大值。
pub(super) fn summarize(samples: &mut [u128]) -> Option<(u128, u128, u128)> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let len = samples.len();
    Some((
        samples[(len - 1) / 2],
        samples[(len * 95).div_ceil(100) - 1],
        samples[len - 1],
    ))
}

#[cfg(test)]
mod tests {
    use super::summarize;

    #[test]
    fn test_summary_uses_nearest_rank_and_sorts_samples() {
        let mut samples: Vec<u128> = (1..=100).rev().collect();
        assert_eq!(summarize(&mut samples), Some((50, 95, 100)));
    }

    #[test]
    fn test_summary_handles_empty_and_small_samples() {
        assert_eq!(summarize(&mut []), None);
        assert_eq!(summarize(&mut [7]), Some((7, 7, 7)));
        assert_eq!(summarize(&mut [5, 1, 3]), Some((3, 5, 5)));
    }
}
