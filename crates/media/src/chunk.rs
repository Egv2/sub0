//! Chunks are cut at silence midpoints, so no word straddles a boundary and
//! we don't need overlap/dedupe logic.

use crate::ffmpeg::Silence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPlan {
    pub start_ms: u32,
    pub end_ms: u32,
    /// False when no silence was available and we hard-cut.
    pub cut_at_silence: bool,
}

impl ChunkPlan {
    pub fn duration_ms(&self) -> u32 {
        self.end_ms - self.start_ms
    }
}

/// Silences closer than `target_ms / 4` to the previous cut are ignored to
/// avoid sliver chunks. With no usable silence, we hard-cut at `target_ms`.
pub fn plan_chunks(
    total_ms: u32,
    silences: &[Silence],
    target_ms: u32,
    max_ms: u32,
) -> Vec<ChunkPlan> {
    debug_assert!(target_ms > 0 && max_ms >= target_ms);
    if total_ms == 0 {
        return Vec::new();
    }

    let mut candidates: Vec<u32> = silences
        .iter()
        .map(|s| ((s.start_ms as u64 + s.end_ms as u64) / 2) as u32)
        .filter(|&m| m > 0 && m < total_ms)
        .collect();
    candidates.sort_unstable();
    candidates.dedup();

    let min_cut = target_ms / 4;
    let mut plans = Vec::new();
    let mut start = 0u32;
    loop {
        let remaining = total_ms - start;
        if remaining <= target_ms {
            plans.push(ChunkPlan {
                start_ms: start,
                end_ms: total_ms,
                cut_at_silence: false,
            });
            break;
        }
        let desired = start + target_ms;
        let hard_limit = start + max_ms;
        // Prefer the closest silence at or before the target length; otherwise
        // the first silence still within the hard cap.
        let cut = candidates
            .iter()
            .rev()
            .find(|&&m| m <= desired && m >= start + min_cut)
            .or_else(|| {
                candidates
                    .iter()
                    .find(|&&m| m > desired && m <= hard_limit && m >= start + min_cut)
            })
            .copied();
        match cut {
            Some(end) => {
                plans.push(ChunkPlan {
                    start_ms: start,
                    end_ms: end,
                    cut_at_silence: true,
                });
                start = end;
            }
            None => {
                plans.push(ChunkPlan {
                    start_ms: start,
                    end_ms: desired,
                    cut_at_silence: false,
                });
                start = desired;
            }
        }
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(start_ms: u32, end_ms: u32) -> Silence {
        Silence { start_ms, end_ms }
    }

    #[test]
    fn hard_cuts_without_silences() {
        let plans = plan_chunks(1_000_000, &[], 240_000, 480_000);
        assert_eq!(plans.len(), 5);
        assert!(plans.iter().all(|p| !p.cut_at_silence));
        assert_eq!(
            plans[0],
            ChunkPlan {
                start_ms: 0,
                end_ms: 240_000,
                cut_at_silence: false
            }
        );
        assert_eq!(
            plans[4],
            ChunkPlan {
                start_ms: 960_000,
                end_ms: 1_000_000,
                cut_at_silence: false
            }
        );
    }

    #[test]
    fn prefers_cutting_at_silence_near_target() {
        let silences = vec![s(400_000, 405_000)];
        let plans = plan_chunks(1_000_000, &silences, 240_000, 480_000);
        assert_eq!(
            plans[0],
            ChunkPlan {
                start_ms: 0,
                end_ms: 402_500,
                cut_at_silence: true
            }
        );
        assert_eq!(plans[0].duration_ms(), 402_500);
        assert!(plans[1..].iter().all(|p| !p.cut_at_silence));
    }

    #[test]
    fn ignores_silences_too_close_to_previous_cut() {
        let silences = vec![s(10_000, 11_000)];
        let plans = plan_chunks(1_000_000, &silences, 240_000, 480_000);
        assert!(plans.iter().all(|p| !p.cut_at_silence));
    }

    #[test]
    fn silence_beyond_hard_cap_is_not_used_for_that_chunk() {
        let silences = vec![s(500_000, 501_000)];
        let plans = plan_chunks(1_000_000, &silences, 240_000, 480_000);
        assert_eq!(
            plans[0],
            ChunkPlan {
                start_ms: 0,
                end_ms: 240_000,
                cut_at_silence: false
            }
        );
        // From 240s the 500.5s silence is within [desired 480s, cap 720s].
        assert_eq!(
            plans[1],
            ChunkPlan {
                start_ms: 240_000,
                end_ms: 500_500,
                cut_at_silence: true
            }
        );
    }

    #[test]
    fn short_input_is_one_chunk() {
        let plans = plan_chunks(100_000, &[], 240_000, 480_000);
        assert_eq!(
            plans,
            vec![ChunkPlan {
                start_ms: 0,
                end_ms: 100_000,
                cut_at_silence: false
            }]
        );
    }

    #[test]
    fn chunks_are_contiguous_and_cover_input() {
        let silences = vec![s(90_000, 95_000), s(300_000, 310_000), s(700_000, 701_000)];
        let plans = plan_chunks(900_000, &silences, 240_000, 480_000);
        let mut cursor = 0u32;
        for p in &plans {
            assert_eq!(p.start_ms, cursor);
            assert!(p.duration_ms() <= 480_000);
            cursor = p.end_ms;
        }
        assert_eq!(cursor, 900_000);
    }
}
