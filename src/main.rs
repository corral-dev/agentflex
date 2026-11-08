//! agentflex: animated, shareable card of your coding-agent activity.

mod card;
mod png;
mod stats;
mod term;

use std::io::IsTerminal;
use std::time::{Instant, SystemTime};

use sesskit_stats::{collect, home_dir, Window};
use stats::Period;
use time::{OffsetDateTime, UtcOffset};

const HELP: &str = "agentflex: an animated card of your local coding-agent activity

Usage: agentflex [--month | --all] [--png PATH | --json] [--no-anim]

  (default)    last 7 days, animated in the terminal; s saves a PNG, r replays, q quits
  --month      last 30 days
  --all        all retained history
  --png PATH   write the 1200x630 card image to PATH and exit
  --json       print the card values as JSON and exit
  --no-anim    show the final card without animation
  --frame T    with --png: render the frame T seconds into the animation
  --version    print the version

Reads Claude Code, Codex, OpenCode and Pi histories on this machine. Nothing is uploaded.
";

fn local_start(now: f64, days: i64, offset: UtcOffset) -> f64 {
    let local = OffsetDateTime::from_unix_timestamp(now as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH).to_offset(offset);
    (local.replace_time(time::Time::MIDNIGHT) - time::Duration::days(days - 1)).unix_timestamp() as f64
}

fn peak_label(at: Option<f64>, offset: UtcOffset) -> String {
    let Some(at) = at else { return String::new() };
    let Ok(dt) = OffsetDateTime::from_unix_timestamp(at as i64) else { return String::new() };
    let fmt = time::macros::format_description!("[weekday repr:short] [hour]:[minute]");
    dt.to_offset(offset).format(&fmt).unwrap_or_default()
}

fn main() {
    // Must run before any thread exists (time's local-offset soundness rule).
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--help") || has("-h") {
        print!("{HELP}");
        return;
    }
    if has("--version") || has("-V") {
        println!("agentflex {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Some(bad) = args.iter().find(|a| {
        a.starts_with('-') && !["--month", "--all", "--png", "--json", "--no-anim", "--frame"].contains(&a.as_str())
    }) {
        eprintln!("agentflex: unknown option {bad}\n\n{HELP}");
        std::process::exit(2);
    }
    let png_path = args.iter().position(|a| a == "--png").map(|i| args.get(i + 1).cloned());
    if png_path == Some(None) {
        eprintln!("agentflex: --png needs a file path");
        std::process::exit(2);
    }
    let period = if has("--all") { Period::All } else if has("--month") { Period::Month } else { Period::Week };
    let Some(home) = home_dir() else {
        eprintln!("agentflex: cannot determine the home directory");
        std::process::exit(1);
    };
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let start = period.days().map_or(0.0, |d| local_start(now, d, offset));
    let window = Window { start, end: now, full: false };

    let load = move || {
        let t0 = Instant::now();
        let sessions = collect(&home, &window);
        let mut s = stats::aggregate(&sessions, start.max(earliest(&sessions, start)), now, offset);
        s.elapsed = t0.elapsed().as_secs_f64();
        s
    };

    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if has("--json") || png_path.is_some() || !interactive {
        let s = load();
        if has("--json") {
            let out = serde_json::json!({
                "period": {"key": format!("{period:?}").to_lowercase(), "start": start, "end": now},
                "sessions": s.sessions,
                "tokens": s.tokens,
                "tokens_per_day": s.tokens_per_day,
                "peak_parallel": s.peak_parallel,
                "peak_at": s.peak_at,
                "by_runtime": s.by_runtime.iter().map(|(k, v)| (k.to_string(), *v)).collect::<std::collections::BTreeMap<_, _>>(),
                "by_model": s.by_model.iter().map(|(name, tokens)| serde_json::json!({"model": name, "tokens": tokens})).collect::<Vec<_>>(),
                "heatmap_active_seconds": s.heatmap,
                "elapsed_s": s.elapsed,
            });
            println!("{out}");
            return;
        }
        let view = card::build_view(&s, period, peak_label(s.peak_at, offset), None);
        let frame: Option<f64> = args.iter().position(|a| a == "--frame").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok());
        let grid = card::compose(&view, frame, None, None);
        if let Some(Some(path)) = png_path {
            match png::write(&grid, std::path::Path::new(&path)) {
                Ok(p) => println!("{}", p.display()),
                Err(e) => {
                    eprintln!("agentflex: could not write {path}: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        let _ = term::print_static(&grid, std::io::stdout().is_terminal());
        return;
    }

    let loading = card::build_view(&stats::CardStats::default(), period, String::new(), Some((0, -1)));
    let png_name = format!(
        "agentflex-{}.png",
        OffsetDateTime::from_unix_timestamp(now as i64)
            .map(|d| d.to_offset(offset))
            .ok()
            .and_then(|d| d.format(&time::macros::format_description!("[year][month][day]")).ok())
            .unwrap_or_else(|| "card".into())
    );
    let animate = !has("--no-anim");
    let result = term::run(
        loading,
        move || {
            let s = load();
            Ok(card::build_view(&s, period, peak_label(s.peak_at, offset), None))
        },
        animate,
        png_name,
    );
    match result {
        Ok(term::Outcome::Quit(Some(view))) => {
            let _ = term::print_static(&card::compose(&view, None, None, None), true);
        }
        Ok(term::Outcome::Quit(None)) => {}
        Ok(term::Outcome::Failed(e)) => {
            eprintln!("agentflex: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("agentflex: terminal error: {e}");
            std::process::exit(1);
        }
    }
}

/// For `--all`, the period starts at the earliest activity actually seen.
fn earliest(sessions: &[sesskit_stats::SessionStats], start: f64) -> f64 {
    if start > 0.0 {
        return start;
    }
    let first = sessions.iter().filter_map(|s| s.activity.first().copied()).fold(f64::INFINITY, f64::min);
    if first.is_finite() { first } else { start }
}
