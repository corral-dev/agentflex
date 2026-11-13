//! Card values aggregated from the SessKit engine projection (no parsing here).

use sesskit_stats::aggregate::{active_intervals, add_weekday_hours, peak_concurrency};
use sesskit_stats::{Runtime, SessionStats};
use crate::i18n::Lang;
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

    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Period::Week, Lang::En) => "last 7 days",
            (Period::Month, Lang::En) => "last 30 days",
            (Period::All, Lang::En) => "all history",
            (Period::Week, Lang::Zh) => "最近 7 天",
            (Period::Month, Lang::Zh) => "最近 30 天",
            (Period::All, Lang::Zh) => "全部历史",
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
    /// (display model name, window tokens) sorted by tokens descending.
    pub by_model: Vec<(String, i64)>,
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
        for (model, tokens) in &s.window_models {
            let name = model_display(model.as_deref());
            match stats.by_model.iter_mut().find(|m| m.0 == name) {
                Some(m) => m.1 += tokens,
                None => stats.by_model.push((name, *tokens)),
            }
        }
        let merged = active_intervals(&s.activity);
        add_weekday_hours(&mut stats.heatmap, &merged, offset);
        intervals.extend(merged);
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    stats.by_runtime = counts;
    stats.by_model.retain(|m| m.1 > 0);
    stats.by_model.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
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
        "antigravity" => "Antigravity",
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
        "antigravity" => "#22D3EE",
        _ => "#8A817A",
    }
}

/// Short display name for a native model string (card design §10.7): drop a
/// provider prefix, a bracket suffix, a trailing ` (...)` effort suffix, a date
/// suffix and a leading `claude-`; internal whitespace normalizes to `-` and a
/// trailing `-N-M` version reads `-N.M`. Display only; grouping by the result
/// merges spellings of one model.
pub fn model_display(raw: Option<&str>) -> String {
    let Some(raw) = raw.map(str::trim).filter(|m| !m.is_empty()) else { return "unknown".into() };
    let mut name = raw.rsplit('/').next().unwrap_or(raw).to_ascii_lowercase();
    if let Some(i) = name.find('[') {
        name.truncate(i);
    }
    let mut trimmed = name.trim().to_string();
    while trimmed.ends_with(')') {
        let Some(i) = trimmed.rfind('(') else { break };
        let prefix = trimmed[..i].trim_end();
        if prefix.is_empty() {
            break;
        }
        trimmed.truncate(prefix.len());
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = trimmed
        .split(|c: char| c == '-' || c.is_ascii_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    let mut keep = parts.len();
    if keep > 1 && parts[keep - 1].len() == 8 && digits(parts[keep - 1]) {
        keep -= 1; // -YYYYMMDD
    } else if keep > 3 && parts[keep - 3].len() == 4 && digits(parts[keep - 3]) && digits(parts[keep - 2]) && digits(parts[keep - 1]) && parts[keep - 1].len() == 2 {
        keep -= 3; // -YYYY-MM-DD
    }
    let mut parts: Vec<String> = parts[..keep].iter().map(|p| p.to_string()).collect();
    if parts.len() > 1 && parts[0] == "claude" {
        parts.remove(0);
    }
    let n = parts.len();
    if n >= 3 && digits(&parts[n - 2]) && digits(&parts[n - 1]) && parts[n - 1].len() <= 2 {
        let minor = parts.pop().unwrap_or_default();
        parts[n - 2] = format!("{}.{minor}", parts[n - 2]);
    }
    let name = parts.join("-");
    if name.is_empty() { raw.to_string() } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_labels_include_antigravity() {
        assert_eq!(runtime_name("antigravity"), "Antigravity");
        assert_eq!(runtime_color("antigravity"), "#22D3EE");
    }

    #[test]
    fn model_display_shortens_known_spellings() {
        assert_eq!(model_display(Some("claude-opus-5-5")), "opus-5.5");
        assert_eq!(model_display(Some("claude-opus-5-5[1m]")), "opus-5.5");
        assert_eq!(model_display(Some("anthropic/claude-sonnet-5-5-20261001")), "sonnet-5.5");
        assert_eq!(model_display(Some("claude-haiku-4-5-2025-10-01")), "haiku-4.5");
        assert_eq!(model_display(Some("gpt-6.1-sol")), "gpt-6.1-sol");
        assert_eq!(model_display(Some("opencode-go/muse-spark-1.3-contributor")), "muse-spark-1.3-contributor");
        assert_eq!(model_display(Some("deepseek-v4")), "deepseek-v4");
        assert_eq!(model_display(Some("Gemini 4 Carbon (High)")), "gemini-4-carbon");
        assert_eq!(model_display(Some("Gemini 4 Hydrogen (XHigh)")), "gemini-4-hydrogen");
        assert_eq!(model_display(Some("Claude Opus 5.5 (High)")), "opus-5.5");
        assert_eq!(model_display(Some("Claude Sonnet 4 6 (Thinking) (Medium)")), "sonnet-4.6");
        assert_eq!(model_display(Some("  ")), "unknown");
        assert_eq!(model_display(None), "unknown");
    }
}
