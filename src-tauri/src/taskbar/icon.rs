//! The account's icon: Claude's own logo from the installed package, its orange plate recoloured to
//! the account's colour (the white mark stays), written as a multi-size `.ico`.

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// Claude's plate colour in the logo (it has a slight gradient around this).
const PLATE: [f32; 3] = [218.0, 112.0, 76.0];
/// Chroma (max − min channel) from which a pixel counts as all plate; below it, partly the white mark.
const PLATE_CHROMA: f32 = 114.0;
/// The sizes Windows asks for on the taskbar, Start Menu and Alt+Tab.
const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// Recolours RGBA pixels in place: each pixel is read as a mix of the white mark and the plate (by how
/// colourful it is), and its plate part is moved from Claude's orange to `color`. Alpha is untouched.
pub fn recolor(rgba: &mut [u8], color: [u8; 3]) {
    for px in rgba.chunks_exact_mut(4) {
        let max = px[0].max(px[1]).max(px[2]) as f32;
        let min = px[0].min(px[1]).min(px[2]) as f32;
        let w = ((max - min) / PLATE_CHROMA).clamp(0.0, 1.0);
        for c in 0..3 {
            let v = px[c] as f32 + w * (color[c] as f32 - PLATE[c]);
            px[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// `#RRGGBB` to bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// `<id>-<hash>.ico`: a new name whenever the colour or Claude's logo changes, because Explorer
/// caches taskbar icons by path.
pub fn file_name(id: &str, color: &str, source: &[u8]) -> String {
    let mut h: u32 = 0x811c_9dc5; // FNV-1a
    for b in color.bytes().chain([0]).chain(source.iter().copied()) {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{id}-{h:08x}.ico")
}

fn source_png(install_dir: &Path, size: u32) -> PathBuf {
    install_dir.join("assets").join(format!(
        "Square44x44Logo.targetsize-{size}_altform-unplated.png"
    ))
}

/// The account's icon in `dir`, written when missing. Fails when Claude's logo is not found.
pub fn ensure(dir: &Path, install_dir: &Path, id: &str, color: &str) -> AppResult<PathBuf> {
    let rgb = parse_hex(color).ok_or_else(|| AppError::io(format!("not a colour: {color}")))?;
    let sources: Vec<Vec<u8>> = SIZES
        .iter()
        .filter_map(|&n| std::fs::read(source_png(install_dir, n)).ok())
        .collect();
    if sources.is_empty() {
        return Err(AppError::io(format!(
            "Claude's logo not found in {}",
            install_dir.display()
        )));
    }
    let path = dir.join(file_name(id, color, &sources.concat()));
    if path.exists() {
        return Ok(path);
    }
    let mut ico = ico::IconDir::new(ico::ResourceType::Icon);
    for png in &sources {
        let mut img = ico::IconImage::read_png(std::io::Cursor::new(png))?;
        recolor(img.rgba_data_mut(), rgb);
        ico.add_entry(ico::IconDirEntry::encode(&img)?);
    }
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("ico.tmp");
    ico.write(std::fs::File::create(&tmp)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: [u8; 3] = [0x2F, 0x6F, 0xDE];

    fn one(px: [u8; 4], color: [u8; 3]) -> [u8; 4] {
        let mut v = px.to_vec();
        recolor(&mut v, color);
        [v[0], v[1], v[2], v[3]]
    }

    #[test]
    fn the_plate_takes_the_account_colour() {
        assert_eq!(one([218, 112, 76, 255], BLUE), [47, 111, 222, 255]);
    }

    #[test]
    fn the_plate_gradient_is_kept() {
        // 1 darker red, 6 more green, 9 more blue than the reference plate: the same offsets on blue.
        assert_eq!(one([217, 118, 85, 255], BLUE), [46, 117, 231, 255]);
    }

    #[test]
    fn the_white_mark_stays_white() {
        let out = one([254, 252, 251, 255], BLUE);
        for (o, i) in out.iter().zip([254u8, 252, 251]) {
            assert!(o.abs_diff(i) <= 6, "{out:?}");
        }
    }

    #[test]
    fn an_edge_between_mark_and_plate_lands_between_white_and_the_colour() {
        // Halfway between white and the plate.
        let out = one([236, 183, 165, 255], BLUE);
        assert!(out[2] > out[0], "bluish, not orange: {out:?}");
        assert!(out[0] > 47 && out[0] < 236, "{out:?}");
    }

    #[test]
    fn alpha_is_unchanged() {
        assert_eq!(one([218, 112, 76, 90], BLUE)[3], 90);
        assert_eq!(one([0, 0, 0, 0], BLUE), [0, 0, 0, 0]);
    }

    #[test]
    fn hex_colours_parse() {
        assert_eq!(parse_hex("#2f6fde"), Some(BLUE));
        assert_eq!(parse_hex("#2F6FDE"), Some(BLUE));
        assert_eq!(parse_hex("blue"), None);
        assert_eq!(parse_hex("#2F6FD"), None);
    }

    #[test]
    fn file_name_changes_with_colour_and_source_only() {
        let a = file_name("id1", "#2F6FDE", b"png");
        assert_eq!(a, file_name("id1", "#2F6FDE", b"png"));
        assert_ne!(a, file_name("id1", "#1E9E6A", b"png"));
        assert_ne!(a, file_name("id1", "#2F6FDE", b"png2"));
        assert!(
            a.starts_with("id1-") && a.ends_with(".ico") && a.len() == "id1-".len() + 8 + 4,
            "{a}"
        );
    }

    #[test]
    #[ignore] // writes a real icon from the installed Claude to %TEMP% for a look
    fn writes_a_real_icon() {
        let pkg = crate::claude_pkg::detect().unwrap();
        let dir = std::env::temp_dir().join("cs-icon-look");
        for c in crate::taskbar::PALETTE {
            println!(
                "{}",
                ensure(&dir, &pkg.install_dir, &c[1..], c)
                    .unwrap()
                    .display()
            );
        }
    }

    #[test]
    fn ensure_without_claude_installed_is_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("cs-icon-{}", uuid::Uuid::new_v4()));
        assert!(ensure(&dir, &dir.join("no-claude"), "id1", "#2F6FDE").is_err());
    }
}
