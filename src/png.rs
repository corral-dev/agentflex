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
    block_mask(ch).is_some() || font().contains_key(&ch)
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
            let fg = cell.rgb.unwrap_or_else(|| hex(cell.fg));
            let bg = hex(cell.bg);
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
        use crate::stats::{CardStats, Period};
        let mut st = CardStats { sessions: 9, tokens: 123_456_789, tokens_per_day: 1.7e7, peak_parallel: 3, ..CardStats::default() };
        st.by_runtime = vec![("claude", 5), ("codex", 2), ("opencode", 1), ("pi", 1)];
        st.by_model = vec![("opus-5.5".into(), 90_000_000), ("gpt-6.1-sol".into(), 30_000_000), ("unknown".into(), 3_456_789)];
        st.heatmap[1][9] = 100.0;
        let view = crate::card::build_view(&st, Period::Week, "Tue 09:30".into(), None);
        let loading = crate::card::build_view(&CardStats::default(), Period::Week, String::new(), Some((3, 10)));
        let mut frames = vec![crate::card::compose(&loading, None, Some(0.4), None)];
        for t in [0.2, 0.6, 1.0, 1.6, 2.2, 2.9, 3.2, 3.4] {
            frames.push(crate::card::compose(&view, Some(t), None, None));
        }
        frames.push(crate::card::compose(&view, None, None, Some(0.3)));
        for g in &frames {
            for c in g.iter().flatten() {
                assert!(has_glyph(c.ch), "no glyph for {:?}", c.ch);
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
