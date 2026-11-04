//! The card: one 72×19 cell grid per animation time (pure, deterministic).
//!
//! `compose(view, Some(t))` is the frame `t` seconds into the reveal;
//! `compose(view, None)` is the final frame. The terminal and the PNG export
//! both draw these cells, so the shared image equals the last terminal frame.

use crate::stats::{runtime_color, runtime_name, CardStats, Period};

pub const WIDTH: usize = 72;
pub const HEIGHT: usize = 19;
pub const REPO_URL: &str = "github.com/corral-dev/corral";

pub const BG: &str = "#14110F";
const BORDER_DIM: &str = "#3A332E";
const BORDER: &str = "#6B5F55";
const TEXT: &str = "#EDE6DE";
const DIM: &str = "#8A817A";
const FAINT: &str = "#4E4640";
const GOLD: &str = "#D9A441";
const GOLD_HI: &str = "#FFD77A";
const WHITE: &str = "#FFFFFF";
const HERO_FROM: (u8, u8, u8) = (0xFF, 0xD7, 0x7A);
const HERO_TO: (u8, u8, u8) = (0xF0, 0x56, 0x3A);
const HEAT: [&str; 5] = ["#262019", "#4A2A1E", "#7A2E1C", "#B83A1E", "#E8762B"];

const STAT_X: [usize; 3] = [3, 26, 49];
const BAR_X: usize = 13;
const BAR_W: usize = 56;
const HEAT_X: usize = 13;
const HEAT_Y: usize = 13;
const HERO_X: usize = 3;
const HERO_Y: usize = 2;

// Storyboard, seconds after the data is complete.
const BORDER_END: f64 = 0.5;
const TITLE_START: f64 = 0.1;
const TITLE_DUR: f64 = 0.4;
const HERO_START: f64 = 0.3;
const HERO_STAGGER: f64 = 0.09;
const HERO_SPIN: f64 = 0.55;
const NUM_START: f64 = 0.9;
const NUM_STAGGER: f64 = 0.12;
const NUM_DUR: f64 = 1.1;
const FLASH: f64 = 0.15;
const BAR_START: f64 = 1.4;
const BAR_DUR: f64 = 0.8;
const HEAT_START: f64 = 1.8;
const HEAT_SPAN: f64 = 1.2;
const POP: f64 = 0.1;
const PEAK_START: f64 = 2.8;
const PEAK_BLINK: f64 = 0.6;
const BEAMS: [(usize, f64, i32); 2] = [(7, 3.1, 1), (14, 3.25, -1)];
const BEAM_DUR: f64 = 0.55;
const BEAM_TRAIL: f64 = 12.0;
pub const REVEAL_END: f64 = 3.8;
pub const SHIMMER_EVERY: f64 = 4.0;
pub const SHIMMER_DUR: f64 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: &'static str,
    pub bg: &'static str,
    pub bold: bool,
    /// Optional true-color override (hero gradient), used instead of `fg`.
    pub rgb: Option<(u8, u8, u8)>,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', fg: TEXT, bg: BG, bold: false, rgb: None }
    }
}

pub type Grid = Vec<Vec<Cell>>;

#[derive(Clone, Debug)]
pub struct View {
    pub title: &'static str,
    pub period_label: &'static str,
    pub hero: String,
    pub stats: [(&'static str, f64, bool, String); 3], // label, value, compact, detail
    pub shares: Vec<(&'static str, &'static str, f64)>, // name, color, fraction
    pub heat: [[u8; 24]; 7],
    pub busiest: Option<(usize, usize)>,
    /// (done, total) while reading; total < 0 while scanning.
    pub progress: Option<(usize, i64)>,
    /// No session in the period: the hero explains instead of showing zeros.
    pub empty: bool,
}

pub fn compact(value: f64) -> String {
    if value.abs() < 1_000_000.0 {
        return thousands(value.round() as i64);
    }
    for (unit, size) in [("B", 1e9), ("M", 1e6)] {
        if value.abs() >= size {
            let scaled = value / size;
            let digits = if scaled < 10.0 { 2 } else if scaled < 100.0 { 1 } else { 0 };
            return format!("{scaled:.digits$}{unit}");
        }
    }
    thousands(value as i64)
}

pub fn thousands(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn heat_levels(heat: &[[f64; 24]; 7]) -> [[u8; 24]; 7] {
    let peak = heat.iter().flatten().copied().fold(0.0, f64::max);
    let mut out = [[0u8; 24]; 7];
    if peak <= 0.0 {
        return out;
    }
    for d in 0..7 {
        for h in 0..24 {
            let v = heat[d][h];
            out[d][h] = if v <= 0.0 { 0 } else { (1 + (3.999 * (v / peak).sqrt()) as u8).min(4) };
        }
    }
    out
}

pub fn build_view(stats: &CardStats, period: Period, peak_label: String, progress: Option<(usize, i64)>) -> View {
    let agents = stats.by_runtime.len();
    let total = stats.sessions.max(1) as f64;
    View {
        title: period.title(),
        period_label: period.label(),
        hero: compact(stats.tokens as f64),
        stats: [
            ("SESSIONS", stats.sessions as f64, false, format!("{agents} agent{}", if agents == 1 { "" } else { "s" })),
            ("TOKENS / DAY", stats.tokens_per_day, true, String::new()),
            ("PEAK PARALLEL", stats.peak_parallel as f64, false, peak_label),
        ],
        shares: stats
            .by_runtime
            .iter()
            .map(|(id, n)| (runtime_name(id), runtime_color(id), *n as f64 / total))
            .collect(),
        heat: heat_levels(&stats.heatmap),
        busiest: stats.busiest(),
        progress,
        empty: progress.is_none() && stats.sessions == 0,
    }
}

// ------------------------------------------------------------- helpers

fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn ease_out(x: f64) -> f64 {
    1.0 - (1.0 - clamp(x)).powi(3)
}

fn ease_out_expo(x: f64) -> f64 {
    let x = clamp(x);
    if x >= 1.0 {
        1.0
    } else {
        1.0 - 2f64.powf(-10.0 * x)
    }
}

fn hash01(parts: &[i64]) -> f64 {
    let mut h: u32 = 2166136261;
    for p in parts {
        h = (h ^ (*p as u32)).wrapping_mul(16777619);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1E995);
    h ^= h >> 15;
    h as f64 / 4294967296.0
}

fn blank() -> Grid {
    vec![vec![Cell::default(); WIDTH]; HEIGHT]
}

fn put(grid: &mut Grid, x: usize, y: usize, text: &str, fg: &'static str, bold: bool) {
    for (i, ch) in text.chars().enumerate() {
        if x + i < WIDTH && y < HEIGHT {
            let bg = grid[y][x + i].bg;
            grid[y][x + i] = Cell { ch, fg, bg, bold, rgb: None };
        }
    }
}

fn perimeter() -> Vec<(usize, usize)> {
    let mut p = Vec::new();
    p.extend((0..WIDTH).map(|x| (x, 0)));
    p.extend((1..HEIGHT).map(|y| (WIDTH - 1, y)));
    p.extend((0..WIDTH - 1).rev().map(|x| (x, HEIGHT - 1)));
    p.extend((1..HEIGHT - 1).rev().map(|y| (0, y)));
    p
}

fn border_char(x: usize, y: usize) -> char {
    match (x, y) {
        (0, 0) => '╭',
        (x, 0) if x == WIDTH - 1 => '╮',
        (0, y) if y == HEIGHT - 1 => '╰',
        (x, y) if x == WIDTH - 1 && y == HEIGHT - 1 => '╯',
        (_, y) if y == 0 || y == HEIGHT - 1 => '─',
        _ => '│',
    }
}

// ------------------------------------------------------------- hero font

/// 7-row pixel glyphs ('#' = on). Two pixel rows share one terminal cell
/// (upper/lower half block), so pixels are square; 8 pixel rows = 4 cells.
fn glyph(ch: char) -> &'static [&'static str] {
    match ch {
        '0' => &[".####.", "##..##", "##..##", "##..##", "##..##", "##..##", ".####."],
        '1' => &["..##..", ".###..", "..##..", "..##..", "..##..", "..##..", ".####."],
        '2' => &[".####.", "##..##", "....##", "...##.", "..##..", ".##...", "######"],
        '3' => &[".####.", "##..##", "....##", "..###.", "....##", "##..##", ".####."],
        '4' => &["...##.", "..###.", ".####.", "##.##.", "######", "...##.", "...##."],
        '5' => &["######", "##....", "#####.", "....##", "....##", "##..##", ".####."],
        '6' => &["..###.", ".##...", "##....", "#####.", "##..##", "##..##", ".####."],
        '7' => &["######", "....##", "...##.", "..##..", ".##...", ".##...", ".##..."],
        '8' => &[".####.", "##..##", "##..##", ".####.", "##..##", "##..##", ".####."],
        '9' => &[".####.", "##..##", "##..##", ".#####", "....##", "...##.", ".###.."],
        '.' => &["..", "..", "..", "..", "..", "##", "##"],
        ',' => &["..", "..", "..", "..", "..", "##", ".#"],
        'B' => &["#####.", "##..##", "##..##", "#####.", "##..##", "##..##", "#####."],
        'M' => &["##...##", "###.###", "#######", "##.#.##", "##...##", "##...##", "##...##"],
        'K' => &["##..##", "##.##.", "####..", "###...", "####..", "##.##.", "##..##"],
        _ => &["......"; 7],
    }
}

const HERO_ROWS: usize = 4;

fn glyph_width(ch: char) -> usize {
    glyph(ch)[0].len()
}

/// (x offset, char) per glyph, one blank pixel column between glyphs.
fn hero_layout(text: &str) -> Vec<(usize, char)> {
    let mut x = 0;
    let mut out = Vec::new();
    for ch in text.chars() {
        out.push((x, ch));
        x += glyph_width(ch) + 1;
    }
    out
}

pub fn hero_width(text: &str) -> usize {
    hero_layout(text).last().map(|(x, ch)| x + glyph_width(*ch)).unwrap_or(0)
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * clamp(t)).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

fn draw_glyph(grid: &mut Grid, x0: usize, ch: char, color: impl Fn(usize) -> (u8, u8, u8), bold: bool) {
    let rows = glyph(ch);
    let on = |r: usize, c: usize| rows.get(r).and_then(|row| row.as_bytes().get(c)).is_some_and(|b| *b == b'#');
    for cy in 0..HERO_ROWS {
        for c in 0..glyph_width(ch) {
            let (top, bottom) = (on(cy * 2, c), on(cy * 2 + 1, c));
            let chr = match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                _ => continue,
            };
            let x = x0 + c;
            if x < WIDTH - 1 {
                grid[HERO_Y + cy][x] = Cell { ch: chr, fg: TEXT, bg: BG, bold, rgb: Some(color(x)) };
            }
        }
    }
}

// ------------------------------------------------------------- layers

fn draw_border(grid: &mut Grid, t: Option<f64>, loading_t: Option<f64>) {
    let url = format!(" {REPO_URL} ");
    let url_x = WIDTH - 3 - url.chars().count();
    let per = perimeter();
    let total = per.len() as f64;
    let (head, lit_until) = if let Some(lt) = loading_t {
        (Some((lt / 1.6).fract() * total), -1.0)
    } else {
        match t {
            Some(t) if t < BORDER_END => {
                let l = total * ease_out(t / BORDER_END);
                (Some(l), l)
            }
            _ => (None, total),
        }
    };
    for (i, &(x, y)) in per.iter().enumerate() {
        let lit = (i as f64) < lit_until;
        let in_url = y == HEIGHT - 1 && x >= url_x && x < url_x + url.chars().count();
        let (ch, mut fg) = if in_url && lit {
            (url.chars().nth(x - url_x).unwrap(), DIM)
        } else {
            (border_char(x, y), if lit { BORDER } else { BORDER_DIM })
        };
        if let Some(h) = head {
            let behind = if loading_t.is_some() { (h - i as f64).rem_euclid(total) } else { h - i as f64 };
            if (0.0..1.0).contains(&behind) {
                fg = GOLD_HI;
            } else if (0.0..7.0).contains(&behind) {
                fg = GOLD;
            }
        }
        grid[y][x] = Cell { ch, fg, bg: BG, bold: false, rgb: None };
    }
}

fn draw_title(grid: &mut Grid, view: &View, t: Option<f64>) {
    let shown = match t {
        Some(t) if t < TITLE_START + TITLE_DUR => {
            let n = (view.title.len() as f64 * clamp((t - TITLE_START) / TITLE_DUR)).ceil() as usize;
            &view.title[..n.min(view.title.len())]
        }
        _ => view.title,
    };
    put(grid, HERO_X, 1, shown, GOLD, true);
    let right = WIDTH - 3;
    if let Some((done, total)) = view.progress {
        let line = if total < 0 { "scanning history".to_string() } else { format!("reading {} / {} sessions", thousands(done as i64), thousands(total)) };
        put(grid, right - line.chars().count(), 1, &line, DIM, false);
    } else {
        put(grid, right - view.period_label.len(), 1, view.period_label, DIM, false);
    }
}

fn draw_hero(grid: &mut Grid, view: &View, t: Option<f64>, loading_t: Option<f64>, idle_t: Option<f64>) {
    if view.empty && loading_t.is_none() {
        put(grid, HERO_X, HERO_Y + 1, "No Claude Code, Codex, OpenCode or Pi sessions", TEXT, true);
        let tail = format!("found in the {}.", view.period_label);
        put(grid, HERO_X, HERO_Y + 2, &tail, DIM, false);
        return;
    }
    let text: String = view.hero.clone();
    let layout = hero_layout(&text);
    let width = hero_width(&text).max(1);
    let gradient = |x: usize| mix(HERO_FROM, HERO_TO, (x - HERO_X) as f64 / width as f64);
    let shimmer = idle_t.and_then(|it| {
        let phase = it % SHIMMER_EVERY;
        (phase < SHIMMER_DUR).then(|| -8.0 + (width as f64 + 16.0) * (phase / SHIMMER_DUR))
    });
    let frame = ((loading_t.or(t).unwrap_or(0.0)) * 24.0) as i64;
    for (i, &(dx, ch)) in layout.iter().enumerate() {
        let x0 = HERO_X + dx;
        let digit = ch.is_ascii_digit();
        if let Some(_lt) = loading_t {
            let spin = if digit { char::from(b'0' + (hash01(&[i as i64, frame]) * 10.0) as u8) } else { ch };
            draw_glyph(grid, x0, spin, |_| (0x6B, 0x5A, 0x3A), false);
            continue;
        }
        if let Some(t) = t {
            let lock = HERO_START + HERO_STAGGER * i as f64 + HERO_SPIN;
            if t < HERO_START + HERO_STAGGER * i as f64 {
                continue;
            }
            if t < lock && digit {
                let spin = char::from(b'0' + (hash01(&[i as i64, frame]) * 10.0) as u8);
                draw_glyph(grid, x0, spin, |x| mix(gradient(x), (0x55, 0x48, 0x40), 0.5), false);
                continue;
            }
            if t < lock + 0.08 {
                draw_glyph(grid, x0, ch, |_| (0xFF, 0xFF, 0xFF), true);
                continue;
            }
        }
        draw_glyph(grid, x0, ch, |x| {
            match shimmer {
                Some(s) if ((x - HERO_X) as f64 - s).abs() < 2.5 => (0xFF, 0xF4, 0xD6),
                _ => gradient(x),
            }
        }, true);
    }
    let label_x = HERO_X + width + 3;
    let visible = loading_t.is_none() && t.is_none_or(|t| t >= HERO_START + HERO_STAGGER * layout.len() as f64 + HERO_SPIN);
    if visible && label_x + 6 < WIDTH - 2 {
        put(grid, label_x, HERO_Y + HERO_ROWS - 1, "tokens", DIM, false);
    }
}

fn format_value(v: f64, compact_fmt: bool) -> String {
    if compact_fmt {
        compact(v)
    } else {
        thousands(v.round() as i64)
    }
}

fn draw_stats(grid: &mut Grid, view: &View, t: Option<f64>) {
    for (i, (label, value, compact_fmt, detail)) in view.stats.iter().enumerate() {
        let x = STAT_X[i];
        put(grid, x, 6, label, DIM, false);
        if view.progress.is_some() {
            put(grid, x, 7, &format_value(*value, *compact_fmt), DIM, true);
            continue;
        }
        let start = NUM_START + NUM_STAGGER * i as f64;
        let (shown, color) = match t {
            None => (*value, TEXT),
            Some(t) if t >= start + NUM_DUR + FLASH => (*value, TEXT),
            Some(t) if t < start => (0.0, FAINT),
            Some(t) if t < start + NUM_DUR => (value * ease_out_expo((t - start) / NUM_DUR), TEXT),
            Some(_) => (*value, GOLD_HI),
        };
        put(grid, x, 7, &format_value(shown, *compact_fmt), color, true);
        if detail.is_empty() {
            continue;
        }
        if i == 2 {
            match t {
                Some(t) if t < PEAK_START => {}
                Some(t) if t < PEAK_START + PEAK_BLINK => {
                    let n = (detail.len() as f64 * clamp((t - PEAK_START) / PEAK_BLINK)) as usize + 1;
                    put(grid, x, 8, &detail[..n.min(detail.len())], GOLD, false);
                }
                _ => put(grid, x, 8, detail, DIM, false),
            }
        } else if t.is_none_or(|t| t >= start + NUM_DUR) {
            put(grid, x, 8, detail, DIM, false);
        }
    }
}

fn share_widths(view: &View) -> Vec<usize> {
    let mut w: Vec<usize> = view.shares.iter().map(|s| ((s.2 * BAR_W as f64).round() as usize).max(1)).collect();
    while w.iter().sum::<usize>() > BAR_W {
        let i = w.iter().enumerate().max_by_key(|(_, v)| **v).map(|(i, _)| i).unwrap();
        w[i] -= 1;
    }
    let sum: usize = w.iter().sum();
    if !w.is_empty() && sum < BAR_W {
        w[0] += BAR_W - sum;
    }
    w
}

fn draw_agents(grid: &mut Grid, view: &View, t: Option<f64>) {
    put(grid, 3, 10, "AGENTS", DIM, false);
    if view.progress.is_some() || view.shares.is_empty() {
        put(grid, BAR_X, 10, &"─".repeat(BAR_W), FAINT, false);
        return;
    }
    let mut colors: Vec<&'static str> = Vec::new();
    for (s, w) in view.shares.iter().zip(share_widths(view)) {
        colors.extend(std::iter::repeat_n(s.1, w));
    }
    let grow = match t {
        None => BAR_W as f64,
        Some(t) => BAR_W as f64 * ease_out((t - BAR_START) / BAR_DUR),
    };
    for i in 0..BAR_W {
        let cell = if (i as f64) < grow.floor() {
            Cell { ch: '█', fg: colors[i], bg: BG, bold: false, rgb: None }
        } else if i == grow.floor() as usize && grow < BAR_W as f64 && t.is_some_and(|t| t >= BAR_START) {
            Cell { ch: '▌', fg: WHITE, bg: BG, bold: false, rgb: None }
        } else {
            Cell { ch: '─', fg: FAINT, bg: BG, bold: false, rgb: None }
        };
        grid[10][BAR_X + i] = cell;
    }
    let mut x = BAR_X;
    for (k, (name, color, frac)) in view.shares.iter().enumerate() {
        let item = format!("{name} {:.0}%", frac * 100.0);
        if x + 2 + item.len() > BAR_X + BAR_W {
            put(grid, x, 11, &format!("+{}", view.shares.len() - k), DIM, false);
            break;
        }
        let appear = BAR_START + 0.2 + 0.15 * k as f64;
        if t.is_some_and(|t| t < appear) {
            break;
        }
        let fresh = t.is_some_and(|t| t < appear + 0.12);
        put(grid, x, 11, "●", color, false);
        put(grid, x + 2, 11, &item, if fresh { DIM } else { TEXT }, false);
        x += item.len() + 4;
    }
}

fn heat_color(view: &View, d: usize, h: usize, t: Option<f64>) -> &'static str {
    let level = view.heat[d][h] as usize;
    let is_peak = view.busiest == Some((d, h));
    if view.progress.is_some() {
        return HEAT[0];
    }
    if let Some(t) = t {
        let reveal = HEAT_START + HEAT_SPAN * ((h + 2 * d) as f64 / 35.0);
        if t < reveal {
            return if t >= HEAT_START - 0.2 { HEAT[0] } else { BG };
        }
        if t < reveal + POP && level > 0 {
            return if level >= 3 { GOLD_HI } else { HEAT[(level + 1).min(4)] };
        }
        if is_peak && (PEAK_START..PEAK_START + PEAK_BLINK).contains(&t) {
            return if ((t - PEAK_START) / 0.1) as i64 % 2 == 0 { GOLD_HI } else { HEAT[level] };
        }
    }
    if is_peak && t.is_none_or(|t| t >= PEAK_START) {
        return GOLD_HI;
    }
    HEAT[level]
}

fn draw_heatmap(grid: &mut Grid, view: &View, t: Option<f64>) {
    put(grid, 3, HEAT_Y, "ACTIVITY", DIM, false);
    for pair in 0..4 {
        let y = HEAT_Y + pair;
        let (top_d, bot_d) = (pair * 2, pair * 2 + 1);
        for h in 0..24 {
            let top = heat_color(view, top_d, h, t);
            let bottom = if bot_d < 7 { heat_color(view, bot_d, h, t) } else { BG };
            let x = HEAT_X + h * 2;
            grid[y][x] = Cell { ch: '▀', fg: top, bg: bottom, bold: false, rgb: None };
            grid[y][x + 1] = Cell { ch: '▀', fg: top, bg: bottom, bold: false, rgb: None };
        }
    }
    let axis = HEAT_Y + 4;
    for (h, label) in [(0, "0h"), (6, "6h"), (12, "12h"), (18, "18h")] {
        put(grid, HEAT_X + h * 2, axis, label, FAINT, false);
    }
    put(grid, HEAT_X + 45, axis, "24h", FAINT, false);
    let side = HEAT_X + 50;
    put(grid, side, HEAT_Y, "busiest", FAINT, false);
    if let (Some((d, h)), None) = (view.busiest, view.progress) {
        if t.is_none_or(|t| t >= PEAK_START) {
            put(grid, side, HEAT_Y + 1, "■", GOLD_HI, false);
            put(grid, side + 2, HEAT_Y + 1, ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d], TEXT, false);
            put(grid, side + 2, HEAT_Y + 2, &format!("{h:02}h"), TEXT, false);
        }
    }
}

fn draw_beams(grid: &mut Grid, t: Option<f64>) {
    let Some(t) = t else { return };
    for (row, start, dir) in BEAMS {
        if !(start..start + BEAM_DUR).contains(&t) {
            continue;
        }
        let travel = -BEAM_TRAIL + (WIDTH as f64 + 2.0 * BEAM_TRAIL) * ease_out((t - start) / BEAM_DUR);
        let head = if dir > 0 { travel } else { WIDTH as f64 - 1.0 - travel };
        for (x, slot) in grid[row].iter_mut().enumerate().take(WIDTH - 1).skip(1) {
            let behind = (head - x as f64) * dir as f64;
            if !(0.0..BEAM_TRAIL).contains(&behind) {
                continue;
            }
            let fg = if behind < 1.0 { WHITE } else if behind < 4.0 { GOLD_HI } else { GOLD };
            let cell = *slot;
            if cell.ch != ' ' {
                *slot = Cell { fg, bold: true, rgb: None, ..cell };
            } else if behind < 6.0 {
                *slot = Cell { ch: if behind < 2.0 { '━' } else { '─' }, fg, ..cell };
            }
        }
    }
}

pub fn compose(view: &View, t: Option<f64>, loading_t: Option<f64>, idle_t: Option<f64>) -> Grid {
    let mut grid = blank();
    let t = if loading_t.is_some() { None } else { t };
    draw_border(&mut grid, t, loading_t);
    draw_title(&mut grid, view, t);
    draw_hero(&mut grid, view, t, loading_t, idle_t);
    draw_stats(&mut grid, view, t);
    draw_agents(&mut grid, view, t);
    draw_heatmap(&mut grid, view, t);
    draw_beams(&mut grid, t);
    grid
}

pub fn shimmer_active(idle_t: f64) -> bool {
    idle_t % SHIMMER_EVERY < SHIMMER_DUR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(g: &Grid) -> String {
        g.iter().map(|r| r.iter().map(|c| c.ch).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    fn sample() -> View {
        let mut s = CardStats { sessions: 266, tokens: 7_445_700_237, tokens_per_day: 1.06e9, peak_parallel: 13, ..CardStats::default() };
        s.by_runtime = vec![("opencode", 200), ("pi", 50), ("claude", 16)];
        s.heatmap[4][11] = 3600.0;
        build_view(&s, Period::Week, "Fri 11:55".into(), None)
    }

    #[test]
    fn final_frame_has_values_and_repo_and_no_removed_items() {
        let t = text(&compose(&sample(), None, None, None));
        for want in ["CODING WEEK", "last 7 days", "266", "1.06B", "13", "Fri 11:55", "OpenCode", REPO_URL, "tokens"] {
            assert!(t.contains(want), "missing {want}");
        }
        for gone in ["TOOL CALLS", "Mon-Sun", "without token data", "CORRAL", "2026"] {
            assert!(!t.contains(gone), "should be gone: {gone}");
        }
    }

    #[test]
    fn reveal_ends_on_the_final_frame_and_grid_is_fixed() {
        let v = sample();
        assert_eq!(compose(&v, Some(REVEAL_END), None, None), compose(&v, None, None, None));
        for t in [0.0, 0.5, 1.5, 3.0] {
            let g = compose(&v, Some(t), None, None);
            assert_eq!(g.len(), HEIGHT);
            assert!(g.iter().all(|r| r.len() == WIDTH));
        }
    }

    #[test]
    fn hero_fits_inside_the_card() {
        assert!(HERO_X + hero_width("888.88B") + 10 < WIDTH);
    }

    #[test]
    fn empty_period_explains_instead_of_zeros() {
        let v = build_view(&CardStats::default(), Period::Week, String::new(), None);
        let t = text(&compose(&v, None, None, None));
        assert!(t.contains("No Claude Code, Codex, OpenCode or Pi sessions"));
        assert!(t.contains("found in the last 7 days."));
        assert!(!t.contains("tokens"));
    }

    #[test]
    fn zero_glyph_is_hollow() {
        assert_eq!(glyph('0')[3], "##..##");
    }

    #[test]
    fn compact_formats() {
        assert_eq!(compact(36813.0), "36,813");
        assert_eq!(compact(7_445_700_237.0), "7.45B");
        assert_eq!(compact(52_100_000.0), "52.1M");
    }
}
