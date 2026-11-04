//! Card values aggregated from the SessKit engine projection (no parsing here).

use sesskit_stats::{Runtime, SessionStats};
use time::{OffsetDateTime, UtcOffset};

/// Consecutive activity at most this far apart is one active interval.
pub const ACTIVE_GAP: f64 = 300.0;
/// A lone event keeps its session active this long, so single events overlap.
pub const EVENT_TAIL: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Week,
    Month,
    All,
}

impl Period {
    pub fn days(self) -> Option<i64> {
        match self {
            Period::Week => Some(7),
            Period::Month => Some(30),
            Period::All => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Period::Week => "last 7 days",
            Period::Month => "last 30 days",
            Period::All => "all history",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Period::Week => "CODING WEEK",
            Period::Month => "CODING MONTH",
            Period::All => "CODING SO FAR",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CardStats {
    pub sessions: usize,
    pub tokens: i64,
    pub tokens_per_day: f64,
    pub peak_parallel: usize,
    pub peak_at: Option<f64>,
    /// (runtime id, sessions) sorted by sessions descending.
    pub by_runtime: Vec<(&'static str, usize)>,
    /// [weekday Mon=0][hour] active seconds.
    pub heatmap: [[f64; 24]; 7],
    pub days: f64,
    pub elapsed: f64,
}

impl CardStats {
    pub fn busiest(&self) -> Option<(usize, usize)> {
        let mut best: Option<(f64, usize, usize)> = None;
        for (d, row) in self.heatmap.iter().enumerate() {
            for (h, v) in row.iter().enumerate() {
                if *v > 0.0 && best.is_none_or(|b| *v > b.0) {
                    best = Some((*v, d, h));
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }
}

pub fn merge_intervals(times: &[f64]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &t in times {
        match out.last_mut() {
            Some(last) if t - last.1 <= ACTIVE_GAP => last.1 = t,
            _ => out.push((t, t)),
        }
    }
    out.into_iter().map(|(a, b)| (a, b + EVENT_TAIL)).collect()
}

pub fn peak_overlap(intervals: &[(f64, f64)]) -> (usize, Option<f64>) {
    let mut points: Vec<(f64, i32)> = Vec::with_capacity(intervals.len() * 2);
    for &(a, b) in intervals {
        points.push((a, 1));
        points.push((b, -1));
    }
    // Ends sort before starts at the same instant: touching is not parallel.
    points.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let (mut cur, mut peak, mut at) = (0i32, 0i32, None);
    for (t, d) in points {
        cur += d;
        if cur > peak {
            peak = cur;
            at = Some(t);
        }
    }
    (peak.max(0) as usize, at)
}

/// Split intervals at local hour boundaries into the weekday × hour map.
pub fn add_hours(heat: &mut [[f64; 24]; 7], intervals: &[(f64, f64)], offset: UtcOffset) {
    for &(start, end) in intervals {
        let mut cur = start;
        while cur < end {
            let Ok(dt) = OffsetDateTime::from_unix_timestamp(cur.floor() as i64) else { break };
            let local = dt.to_offset(offset);
            let into_hour = (local.minute() as f64) * 60.0 + local.second() as f64 + cur.fract();
            let stop = (cur - into_hour + 3600.0).min(end);
            let day = local.weekday().number_days_from_monday() as usize;
            heat[day][local.hour() as usize] += stop - cur;
            if stop <= cur {
                break;
            }
            cur = stop;
        }
    }
}

pub fn aggregate(sessions: &[SessionStats], start: f64, end: f64, offset: UtcOffset) -> CardStats {
    let mut stats = CardStats { days: ((end - start) / 86400.0).max(1.0), ..CardStats::default() };
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for s in sessions.iter().filter(|s| s.excluded.is_none() && !s.activity.is_empty()) {
        stats.sessions += 1;
        stats.tokens += s.window_tokens;
        let id = runtime_id(s.runtime);
        match counts.iter_mut().find(|c| c.0 == id) {
            Some(c) => c.1 += 1,
            None => counts.push((id, 1)),
        }
        let merged = merge_intervals(&s.activity);
        add_hours(&mut stats.heatmap, &merged, offset);
        intervals.extend(merged);
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    stats.by_runtime = counts;
    let (peak, at) = peak_overlap(&intervals);
    stats.peak_parallel = peak;
    stats.peak_at = at;
    stats.tokens_per_day = stats.tokens as f64 / stats.days;
    stats
}

pub fn runtime_id(r: Runtime) -> &'static str {
    r.id()
}

pub fn runtime_name(id: &str) -> &'static str {
    match id {
        "claude" => "Claude",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "pi" => "Pi",
        "cursor" => "Cursor",
        _ => "Other",
    }
}

/// Same hues as Corral's runtime labels.
pub fn runtime_color(id: &str) -> &'static str {
    match id {
        "claude" => "#D97757",
        "codex" => "#60A5FA",
        "opencode" => "#34D399",
        "pi" => "#FBBF24",
        "cursor" => "#A78BFA",
        _ => "#8A817A",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_gap_splits_intervals_and_touching_is_not_parallel() {
        let a = merge_intervals(&[0.0, 86400.0]);
        let b = merge_intervals(&[43200.0, 43260.0]);
        assert_eq!(a.len(), 2);
        assert_eq!(peak_overlap(&[a, b].concat()).0, 1);
        assert_eq!(peak_overlap(&[(0.0, 100.0), (100.0, 200.0)]).0, 1);
        assert_eq!(peak_overlap(&[(0.0, 100.0), (50.0, 150.0), (60.0, 70.0)]), (3, Some(60.0)));
    }

    #[test]
    fn hours_split_at_boundaries() {
        let mut heat = [[0.0; 24]; 7];
        // 2026-11-02 12:50:00 UTC is a Monday.
        let t = 1_793_623_800.0;
        add_hours(&mut heat, &[(t, t + 1200.0)], UtcOffset::UTC);
        assert!((heat[0][12] - 600.0).abs() < 1e-6);
        assert!((heat[0][13] - 600.0).abs() < 1e-6);
    }
}
