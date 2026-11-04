//! 1200×630 PNG of a card grid: 16×32 cells from an embedded Spleen subset
//! (BSD-2-Clause, `assets/SPLEEN-LICENSE.txt`), block elements drawn
//! geometrically, bold as a one-pixel overstrike, zlib via `flate2`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::card::{Grid, HEIGHT, WIDTH};

pub const IMAGE_W: usize = 1200;
pub const IMAGE_H: usize = 630;
const CELL_W: usize = 16;
const CELL_H: usize = 32;
const PAGE: (u8, u8, u8) = (0x0C, 0x0A, 0x09);

const FONT_JSON: &str = include_str!("../assets/spleen-16x32-subset.json");

fn font() -> &'static HashMap<char, [u16; CELL_H]> {
    static FONT: OnceLock<HashMap<char, [u16; CELL_H]>> = OnceLock::new();
    FONT.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(FONT_JSON).expect("embedded font");
        let mut out = HashMap::new();
        if let Some(glyphs) = v["glyphs"].as_object() {
            for (k, hex) in glyphs {
                let (Some(ch), Some(hex)) = (k.chars().next(), hex.as_str()) else { continue };
                let mut rows = [0u16; CELL_H];
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

fn mask(ch: char, bold: bool) -> [u16; CELL_H] {
    if let Some(m) = block_mask(ch) {
        return m;
    }
    let mut rows = font().get(&ch).or_else(|| font().get(&'?')).copied().unwrap_or([0; CELL_H]);
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
    for px in img.chunks_exact_mut(3) {
        px.copy_from_slice(&[PAGE.0, PAGE.1, PAGE.2]);
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
    fn png_has_signature_size_and_valid_crc() {
        let grid = vec![vec![crate::card::Cell::default(); WIDTH]; HEIGHT];
        let data = encode(IMAGE_W, IMAGE_H, &render_rgb(&grid));
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(data[16..20].try_into().unwrap()), IMAGE_W as u32);
        assert_eq!(u32::from_be_bytes(data[20..24].try_into().unwrap()), IMAGE_H as u32);
    }
}
