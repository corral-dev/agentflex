//! Card values aggregated from the SessKit engine projection (no parsing here).

use sesskit_stats::aggregate::{active_intervals, add_weekday_hours, peak_concurrency};
use sesskit_stats::{Runtime, SessionStats};
use time::UtcOffset;

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
        let merged = active_intervals(&s.activity);
        add_weekday_hours(&mut stats.heatmap, &merged, offset);
        intervals.extend(merged);
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    stats.by_runtime = counts;
    let (peak, at) = peak_concurrency(&intervals);
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
