//! 1200×630 PNG of a card grid: 16×32 cells from an embedded Spleen subset
//! (BSD-2-Clause, `assets/SPLEEN-LICENSE.txt`), block elements drawn
//! geometrically, bold as a one-pixel overstrike, zlib via `flate2`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::card::{self, Grid, HEIGHT, WIDTH};

pub const IMAGE_W: usize = 1200;
pub const IMAGE_H: usize = 630;
const CELL_W: usize = 16;
/// Spleen glyph height; the image draws 28-row cells so 21 card rows fit 630 px.
const GLYPH_H: usize = 32;
const CELL_H: usize = 28;
/// Glyph rows dropped from the top (blank padding for every text glyph except a
/// few that shift down instead, e.g. `/`, brackets).
const CROP_TOP: usize = GLYPH_H - CELL_H;
const _: () = assert!(HEIGHT * CELL_H <= IMAGE_H && WIDTH * CELL_W <= IMAGE_W, "card must fit the image");

const FONT_JSON: &str = include_str!("../assets/spleen-16x32-subset.json");
/// Fusion Pixel 12px (OFL-1.1) subset: 24px double-width glyphs for the
/// Chinese card labels, vertically placed inside the 28px cell.
const WIDE_FONT_JSON: &str = include_str!("../assets/fusion-pixel-12px-subset.json");

fn font() -> &'static HashMap<char, [u16; GLYPH_H]> {
    static FONT: OnceLock<HashMap<char, [u16; GLYPH_H]>> = OnceLock::new();
    FONT.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(FONT_JSON).expect("embedded font");
        let mut out = HashMap::new();
        if let Some(glyphs) = v["glyphs"].as_object() {
            for (k, hex) in glyphs {
                let (Some(ch), Some(hex)) = (k.chars().next(), hex.as_str()) else { continue };
                let mut rows = [0u16; GLYPH_H];
                for (i, row) in rows.iter_mut().enumerate() {
                    *row = u16::from_str_radix(hex.get(i * 4..i * 4 + 4).unwrap_or("0000"), 16).unwrap_or(0);
                }
                out.insert(ch, rows);
            }
        }
        out
    })
}

/// Double-width glyphs are stored as 24 rows of 24 bits, placed 4 pixels in
/// from either side of the 32px cell and 2 rows down in the 28px cell.
fn wide_font() -> &'static HashMap<char, [u32; CELL_H]> {
    static WIDE: OnceLock<HashMap<char, [u32; CELL_H]>> = OnceLock::new();
    WIDE.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(WIDE_FONT_JSON).expect("embedded wide font");
        let mut out = HashMap::new();
        if let Some(glyphs) = v["glyphs"].as_object() {
            for (k, hex) in glyphs {
                let (Some(ch), Some(hex)) = (k.chars().next(), hex.as_str()) else { continue };
                let mut rows = [0u32; CELL_H];
                for (i, row) in rows.iter_mut().enumerate().skip(2).take(24) {
                    let bits = u32::from_str_radix(hex.get((i - 2) * 6..(i - 2) * 6 + 6).unwrap_or("000000"), 16).unwrap_or(0);
                    *row = bits << 4;
                }
                out.insert(ch, rows);
            }
        }
        out
    })
}

pub fn hex(color: &str) -> (u8, u8, u8) {
    let c = color.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(c.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    (p(0), p(2), p(4))
}

fn block_mask(ch: char) -> Option<[u16; CELL_H]> {
    let mut rows = [0u16; CELL_H];
    match ch {
        '█' => rows.fill(0xFFFF),
        '▀' => rows[..CELL_H / 2].fill(0xFFFF),
        '▄' => rows[CELL_H / 2..].fill(0xFFFF),
        '▌' => rows.fill(0xFF00),
        _ => return None,
    }
    Some(rows)
}

#[cfg(test)]
pub fn has_glyph(ch: char) -> bool {
    block_mask(ch).is_some() || font().contains_key(&ch) || wide_font().contains_key(&ch)
}

/// Fit a 32-row glyph into a 28-row cell: drop the top padding; a text glyph
/// inked in that padding moves down as far as its bottom allows. Box-drawing
/// and block glyphs keep their rows so lines stay joined across cells.
fn crop(glyph: &[u16; GLYPH_H], ch: char) -> [u16; CELL_H] {
    let first = glyph.iter().position(|r| *r != 0).unwrap_or(GLYPH_H);
    let last = glyph.iter().rposition(|r| *r != 0).unwrap_or(0);
    let line_art = ('\u{2500}'..='\u{259F}').contains(&ch);
    let room = GLYPH_H - 1 - last.min(GLYPH_H - 1);
    let shift = if line_art { 0 } else { CROP_TOP.saturating_sub(first).min(room) };
    let mut out = [0u16; CELL_H];
    for (y, row) in out.iter_mut().enumerate() {
        let src = y + CROP_TOP - shift;
        *row = glyph[src];
    }
    out
}

fn mask(ch: char, bold: bool) -> [u16; CELL_H] {
    if let Some(m) = block_mask(ch) {
        return m;
    }
    let glyph = font().get(&ch).or_else(|| font().get(&'?')).copied().unwrap_or([0; GLYPH_H]);
    let mut rows = crop(&glyph, ch);
    if bold && ch != ' ' {
        for r in rows.iter_mut() {
            *r |= *r >> 1;
        }
    }
    rows
}

/// Raw RGB bytes of the whole image, card centered on the page color.
pub fn render_rgb(grid: &Grid) -> Vec<u8> {
    let mut img = vec![0u8; IMAGE_W * IMAGE_H * 3];
    let page = hex(card::PAGE);
    for px in img.chunks_exact_mut(3) {
        px.copy_from_slice(&[page.0, page.1, page.2]);
    }
    let left = (IMAGE_W - WIDTH * CELL_W) / 2;
    let top = (IMAGE_H - HEIGHT * CELL_H) / 2;
    for (cy, row) in grid.iter().enumerate() {
        for (cx, cell) in row.iter().enumerate() {
            if cell.ch == card::CONT {
                continue;
            }
            let fg = cell.rgb.unwrap_or_else(|| hex(cell.fg));
            let bg = hex(cell.bg);
            if let Some(rows) = wide_font().get(&cell.ch) {
                for (py, bits) in rows.iter().enumerate() {
                    let bits = if cell.bold { bits | (bits >> 1) } else { *bits };
                    let y = top + cy * CELL_H + py;
                    let base = (y * IMAGE_W + left + cx * CELL_W) * 3;
                    for px in 0..CELL_W * 2 {
                        let c = if bits & (0x8000_0000 >> px) != 0 { fg } else { bg };
                        let o = base + px * 3;
                        img[o..o + 3].copy_from_slice(&[c.0, c.1, c.2]);
                    }
                }
                continue;
            }
            let m = mask(cell.ch, cell.bold);
            for (py, bits) in m.iter().enumerate() {
                let y = top + cy * CELL_H + py;
                let base = (y * IMAGE_W + left + cx * CELL_W) * 3;
                for px in 0..CELL_W {
                    let c = if bits & (0x8000 >> px) != 0 { fg } else { bg };
                    let o = base + px * 3;
                    img[o..o + 3].copy_from_slice(&[c.0, c.1, c.2]);
                }
            }
        }
    }
    img
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

pub fn encode(width: usize, height: usize, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * 3 + 1) * height);
    for row in rgb.chunks_exact(width * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&raw).expect("in-memory zlib");
    let idat = z.finish().expect("in-memory zlib");
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Atomic write (temp file in the target directory, then rename).
pub fn write(grid: &Grid, path: &Path) -> std::io::Result<PathBuf> {
    let data = encode(IMAGE_W, IMAGE_H, &render_rgb(grid));
    let target = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir()?.join(path) };
    let dir = target.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let tmp = dir.join(format!(".agentflex-{}.png.tmp", std::process::id()));
    std::fs::write(&tmp, &data)?;
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_card_character_has_a_glyph() {
        use crate::i18n::Lang;
        use crate::stats::{CardStats, Period};
        let mut st = CardStats { sessions: 9, tokens: 123_456_789, tokens_per_day: 1.7e7, peak_parallel: 3, ..CardStats::default() };
        st.by_runtime = vec![("claude", 5), ("codex", 2), ("opencode", 1), ("pi", 1)];
        st.by_model = vec![("opus-5.5".into(), 90_000_000), ("gpt-6.1-sol".into(), 30_000_000), ("unknown".into(), 3_456_789)];
        st.heatmap[1][9] = 100.0;
        let mut frames = Vec::new();
        for lang in [Lang::En, Lang::Zh] {
            let peak = match lang {
                Lang::En => "Tue 09:30".to_string(),
                Lang::Zh => "周二 09:30".to_string(),
            };
            for period in [Period::Week, Period::Month, Period::All] {
                let view = crate::card::build_view(&st, period, peak.clone(), None, lang);
                frames.push(crate::card::compose(&view, None, None, None));
                for t in [0.2, 0.6, 1.0, 1.6, 2.2, 2.9, 3.2, 3.4] {
                    frames.push(crate::card::compose(&view, Some(t), None, None));
                }
            }
            let loading = crate::card::build_view(&CardStats::default(), Period::Week, String::new(), Some((0, -1)), lang);
            let reading = crate::card::build_view(&CardStats::default(), Period::Month, String::new(), Some((3, 10)), lang);
            let empty = crate::card::build_view(&CardStats::default(), Period::All, String::new(), None, lang);
            frames.push(crate::card::compose(&loading, None, Some(0.4), None));
            frames.push(crate::card::compose(&reading, None, Some(0.4), None));
            frames.push(crate::card::compose(&empty, None, None, None));
        }
        frames.push(crate::card::compose(&crate::card::build_view(&st, Period::Week, "Tue 09:30".into(), None, Lang::En), None, None, Some(0.3)));
        for g in &frames {
            for c in g.iter().flatten() {
                if c.ch == crate::card::CONT {
                    continue;
                }
                assert!(has_glyph(c.ch), "no glyph for {:?}", c.ch);
            }
        }
    }

    #[test]
    fn every_localized_character_has_a_glyph() {
        use crate::i18n::{self, Lang};
        use crate::stats::Period;
        let mut samples = vec![
            i18n::stat_labels(Lang::Zh).join(" "),
            i18n::agents_detail(Lang::Zh, 4),
            i18n::models_label(Lang::Zh).to_string(),
            i18n::other_model(Lang::Zh).to_string(),
            i18n::other_runtime(Lang::Zh).to_string(),
            i18n::unknown_model(Lang::Zh).to_string(),
            i18n::activity_label(Lang::Zh).to_string(),
            i18n::busiest_label(Lang::Zh).to_string(),
            i18n::empty_lines(Lang::Zh, Period::Week).join(" "),
            i18n::scanning_history(Lang::Zh).to_string(),
            i18n::reading_sessions(Lang::Zh, "3", "10"),
            i18n::saved(Lang::Zh, "/tmp/a.png"),
            i18n::save_failed(Lang::Zh, "disk full"),
            i18n::small_terminal(Lang::Zh, 72, 23),
        ];
        for p in [Period::Week, Period::Month, Period::All] {
            samples.push(p.label(Lang::Zh).to_string());
        }
        for d in 0..7 {
            samples.push(i18n::weekday(Lang::Zh, d).to_string());
        }
        for s in &samples {
            for ch in s.chars() {
                if !ch.is_ascii() {
                    assert!(has_glyph(ch), "no wide glyph for {ch:?} in {s:?}");
                }
            }
        }
    }

    #[test]
    fn cropping_keeps_every_text_glyph_whole() {
        for (ch, glyph) in font() {
            if ('\u{2500}'..='\u{259F}').contains(ch) {
                continue;
            }
            let ink = |rows: &[u16]| rows.iter().map(|r| r.count_ones()).sum::<u32>();
            assert_eq!(ink(&crop(glyph, *ch)), ink(glyph), "glyph {ch:?} lost ink");
        }
    }

    #[test]
    fn png_has_signature_size_and_valid_crc() {
        let grid = vec![vec![crate::card::Cell::default(); WIDTH]; HEIGHT];
        let data = encode(IMAGE_W, IMAGE_H, &render_rgb(&grid));
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(data[16..20].try_into().unwrap()), IMAGE_W as u32);
        assert_eq!(u32::from_be_bytes(data[20..24].try_into().unwrap()), IMAGE_H as u32);
    }
}
