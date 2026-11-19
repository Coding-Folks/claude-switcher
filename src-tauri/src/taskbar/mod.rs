//! An own taskbar button per account: colour, recoloured Claude icon, Start Menu shortcut, and the
//! taskbar identity stamped on that account's Claude windows. See docs/design/taskbar-per-account.md.

pub mod icon;

/// Taskbar colours for non-Main accounts, in the order they are given out.
pub const PALETTE: [&str; 8] = [
    "#2F6FDE", // blue
    "#1E9E6A", // green
    "#8B5CF6", // violet
    "#D9468C", // pink
    "#0E9AA7", // teal
    "#C9A227", // gold
    "#6B7280", // slate
    "#B91C1C", // red
];

/// The colour for the `n`-th coloured account: the first palette entry nobody uses yet, or the palette
/// again in order once all are taken.
pub fn next_color(used: &[&str], n: usize) -> &'static str {
    PALETTE
        .iter()
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
        .copied()
        .unwrap_or(PALETTE[n % PALETTE.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_color_takes_the_first_unused_palette_entry() {
        assert_eq!(next_color(&[], 0), "#2F6FDE");
        assert_eq!(next_color(&["#2F6FDE"], 1), "#1E9E6A");
        assert_eq!(next_color(&["#1E9E6A"], 1), "#2F6FDE");
    }

    #[test]
    fn next_color_repeats_in_order_when_the_palette_is_used_up() {
        assert_eq!(next_color(&PALETTE, 8), "#2F6FDE");
        assert_eq!(next_color(&PALETTE, 9), "#1E9E6A");
    }
}
