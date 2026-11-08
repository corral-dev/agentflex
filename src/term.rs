//! Terminal playback: loading animation, reveal, idle shimmer; diffed frames.

use std::io::{self, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};

use crate::card::{self, Cell, Grid, View, HEIGHT, WIDTH};
use crate::i18n::{self, Lang};
use crate::png;

const FRAME: Duration = Duration::from_millis(33);
const OSC8_CLOSE: &str = "\x1b]8;;\x1b\\";

/// Bottom-border link cell range (start, end exclusive) for the visible URL text.
fn link_bounds() -> (usize, usize) {
    let n = card::REPO_URL.chars().count();
    let url_x = WIDTH - 3 - (n + 2);
    (url_x + 1, url_x + 1 + n)
}

fn osc8_open() -> String {
    format!("\x1b]8;;{}\x1b\\", card::REPO_HREF)
}

fn color(cell: &Cell) -> Color {
    let (r, g, b) = cell.rgb.unwrap_or_else(|| png::hex(cell.fg));
    Color::Rgb { r, g, b }
}

fn bg(cell: &Cell) -> Color {
    rgb(cell.bg)
}

fn rgb(hex: &str) -> Color {
    let (r, g, b) = png::hex(hex);
    Color::Rgb { r, g, b }
}

/// The line under the card: the key hint, or a transient note (e.g. "Saved ...").
enum Hint {
    Keys,
    Note(String),
}

impl Hint {
    fn spans(&self, lang: Lang) -> Vec<(&str, &'static str)> {
        match self {
            Hint::Keys => {
                let mut out = Vec::new();
                let keys = i18n::keys(lang);
                for (i, (key, label)) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(("  ·  ", card::FAINT));
                    }
                    out.push((*key, card::GOLD));
                    out.push((" ", card::DIM));
                    out.push((*label, card::DIM));
                }
                out
            }
            Hint::Note(text) => vec![(text.as_str(), card::DIM)],
        }
    }
}

/// Print a grid as true-color ANSI (or plain text) to stdout, e.g. after exit.
pub fn print_static(grid: &Grid, colored: bool) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let (ls, le) = link_bounds();
    let open = osc8_open();
    for (y, row) in grid.iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            if c.ch == card::CONT {
                continue;
            }
            let is_link = colored && y == HEIGHT - 1 && x >= ls && x < le;
            if is_link && x == ls {
                queue!(out, Print(&open))?;
            }
            if colored {
                queue!(out, SetForegroundColor(color(c)), SetBackgroundColor(bg(c)))?;
                if c.bold {
                    queue!(out, SetAttribute(Attribute::Bold))?;
                }
                queue!(out, Print(c.ch), SetAttribute(Attribute::Reset))?;
            } else {
                queue!(out, Print(c.ch))?;
            }
            if is_link && x + 1 == le {
                queue!(out, Print(OSC8_CLOSE))?;
            }
        }
        if colored {
            queue!(out, ResetColor)?;
        }
        queue!(out, Print("\n"))?;
    }
    out.flush()
}

struct Screen {
    prev: Option<Grid>,
    origin: (u16, u16),
    small: bool,
    /// Width of the last hint drawn, so a shorter one fully covers it.
    hint_w: usize,
    lang: Lang,
    cols: usize,
}

impl Screen {
    fn layout(&mut self) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        self.small = (w as usize) < WIDTH || (h as usize) < HEIGHT + 2;
        let x = (w as usize).saturating_sub(WIDTH) / 2;
        let y = (h as usize).saturating_sub(HEIGHT + 2) / 2;
        self.origin = (x as u16, y as u16);
        self.cols = w as usize;
        self.prev = None;
        self.paint_stage(w, h)
    }

    /// Paint every cell with the page color so a light terminal theme never
    /// frames the dark card in white. Spaces are written explicitly rather
    /// than relying on background-color erase, which not every terminal honors.
    fn paint_stage(&self, w: u16, h: u16) -> io::Result<()> {
        let mut out = io::stdout().lock();
        let blank = " ".repeat(w as usize);
        queue!(out, ResetColor, Clear(ClearType::All), SetBackgroundColor(rgb(card::PAGE)))?;
        for y in 0..h {
            queue!(out, MoveTo(0, y), Print(&blank))?;
        }
        out.flush()
    }

    fn draw(&mut self, grid: &Grid, hint: &Hint) -> io::Result<()> {
        let mut out = io::stdout().lock();
        if self.small {
            if self.prev.is_none() {
                let msg = i18n::small_terminal(self.lang, WIDTH, HEIGHT + 2);
                let msg = card::truncate_width(&msg, self.cols);
                queue!(out, MoveTo(0, 0), SetBackgroundColor(rgb(card::PAGE)), SetForegroundColor(rgb(card::DIM)), Print(msg))?;
                self.prev = Some(grid.clone());
            }
            return out.flush();
        }
        let (ox, oy) = self.origin;
        let (ls, le) = link_bounds();
        let open = osc8_open();
        let link_chars: Vec<char> = card::REPO_URL.chars().collect();
        let mut link_open = false;
        for (y, row) in grid.iter().enumerate() {
            for (x, c) in row.iter().enumerate() {
                if c.ch == card::CONT {
                    continue;
                }
                if self.prev.as_ref().is_some_and(|p| p[y][x] == *c) {
                    continue;
                }
                // Only hyperlink cells that actually show the URL text (not the
                // border-drawing phase where the footer is still `─`).
                let wants_link = y == HEIGHT - 1
                    && x >= ls
                    && x < le
                    && x - ls < link_chars.len()
                    && c.ch == link_chars[x - ls];
                if wants_link != link_open {
                    if wants_link {
                        queue!(out, Print(&open))?;
                    } else {
                        queue!(out, Print(OSC8_CLOSE))?;
                    }
                    link_open = wants_link;
                }
                queue!(out, MoveTo(ox + x as u16, oy + y as u16), SetForegroundColor(color(c)), SetBackgroundColor(bg(c)))?;
                queue!(out, SetAttribute(if c.bold { Attribute::Bold } else { Attribute::NormalIntensity }), Print(c.ch))?;
            }
        }
        if link_open {
            queue!(out, Print(OSC8_CLOSE))?;
        }
        queue!(out, SetAttribute(Attribute::Reset), MoveTo(ox, oy + HEIGHT as u16 + 1), SetBackgroundColor(rgb(card::PAGE)))?;
        let mut used = 0;
        for (text, fg) in hint.spans(self.lang) {
            queue!(out, SetForegroundColor(rgb(fg)), Print(text))?;
            used += card::text_width(text);
        }
        // Overwrite a longer previous hint with stage-colored blanks.
        queue!(out, Print(" ".repeat(self.hint_w.max(WIDTH).saturating_sub(used))), ResetColor)?;
        self.hint_w = used;
        self.prev = Some(grid.clone());
        out.flush()
    }
}

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub enum Outcome {
    Quit(Option<Box<View>>),
    Failed(String),
}

/// Play the card. `load` runs on a worker thread and returns the final view.
pub fn run(loading_view: View, load: impl FnOnce() -> Result<View, String> + Send + 'static, animate: bool, png_name: String, lang: Lang) -> io::Result<Outcome> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(load());
    });
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;
    let _guard = Guard;
    let mut screen = Screen { prev: None, origin: (0, 0), small: false, hint_w: 0, lang, cols: 0 };
    screen.layout()?;

    let started = Instant::now();
    let mut view: Option<View> = None;
    let mut reveal_at: Option<Instant> = None;
    let mut idle_at: Option<Instant> = None;
    let mut hint = Hint::Keys;
    let mut hint_until: Option<Instant> = None;

    loop {
        if view.is_none() {
            match rx.try_recv() {
                Ok(Ok(v)) => {
                    view = Some(v);
                    if animate {
                        reveal_at = Some(Instant::now());
                    } else {
                        idle_at = None;
                    }
                }
                Ok(Err(e)) => return Ok(Outcome::Failed(e)),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Ok(Outcome::Failed("loader stopped".into())),
            }
        }
        if hint_until.is_some_and(|u| Instant::now() >= u) {
            hint = Hint::Keys;
            hint_until = None;
        }
        // Compose the current frame and decide how long to sleep.
        let (grid, wait) = match (&view, reveal_at) {
            (None, _) => {
                let lt = if animate { Some(started.elapsed().as_secs_f64()) } else { Some(0.0) };
                (card::compose(&loading_view, None, lt, None), if animate { FRAME } else { Duration::from_millis(50) })
            }
            (Some(v), Some(r)) => {
                let t = r.elapsed().as_secs_f64();
                if t >= card::REVEAL_END {
                    reveal_at = None;
                    idle_at = Some(Instant::now());
                    (card::compose(v, None, None, None), FRAME)
                } else {
                    (card::compose(v, Some(t), None, None), FRAME)
                }
            }
            (Some(v), None) => match idle_at.filter(|_| animate) {
                Some(i) => {
                    let it = i.elapsed().as_secs_f64();
                    if card::shimmer_active(it) {
                        (card::compose(v, None, None, Some(it)), FRAME)
                    } else {
                        let next = card::SHIMMER_EVERY - it % card::SHIMMER_EVERY;
                        (card::compose(v, None, None, None), Duration::from_secs_f64(next))
                    }
                }
                None => (card::compose(v, None, None, None), Duration::from_millis(500)),
            },
        };
        let wait = match hint_until {
            Some(u) => wait.min(u.saturating_duration_since(Instant::now())),
            None => wait,
        };
        screen.draw(&grid, &hint)?;

        if event::poll(wait)? {
            match event::read()? {
                Event::Key(KeyEvent { code, modifiers, .. }) => match code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Outcome::Quit(view.map(Box::new))),
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(Outcome::Quit(view.map(Box::new))),
                    KeyCode::Char('r') if view.is_some() && animate => {
                        idle_at = None;
                        reveal_at = Some(Instant::now());
                    }
                    KeyCode::Char('s') => {
                        if let Some(v) = &view {
                            let final_grid = card::compose(v, None, None, None);
                            hint = Hint::Note(match png::write(&final_grid, std::path::Path::new(&png_name)) {
                                Ok(p) => i18n::saved(lang, &p.display().to_string()),
                                Err(e) => i18n::save_failed(lang, &e.to_string()),
                            });
                            hint_until = Some(Instant::now() + Duration::from_secs(4));
                        }
                    }
                    _ => {}
                },
                Event::Resize(..) => screen.layout()?,
                _ => {}
            }
        }
    }
}
