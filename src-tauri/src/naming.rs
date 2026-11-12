//! Label -> hidden Windows user name (`claude-<slug>`, at most 20 chars).

use unicode_normalization::UnicodeNormalization;

const PREFIX: &str = "claude-";
/// `^claude-[a-z0-9-]{1,13}$` — 20 characters total, within Windows' 20-char SAM limit.
const MAX_SLUG: usize = 13;
const FALLBACK: &str = "akun";

pub fn is_valid_windows_user(name: &str) -> bool {
    match name.strip_prefix(PREFIX) {
        Some(rest) => {
            (1..=MAX_SLUG).contains(&rest.len())
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
        None => false,
    }
}

/// Picks `claude-<slug>` for `label`, adding `-2`, `-3`, … while `taken` says the name exists.
pub fn windows_user_for(label: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let slug = slugify(label);
    let first = format!("{PREFIX}{}", cut(&slug, MAX_SLUG));
    if !taken(&first) {
        return first;
    }
    (2u32..)
        .map(|n| {
            let suffix = format!("-{n}");
            format!("{PREFIX}{}{suffix}", cut(&slug, MAX_SLUG - suffix.len()))
        })
        .find(|candidate| !taken(candidate))
        .expect("an unused suffix always exists")
}

fn slugify(label: &str) -> String {
    let mut out = String::new();
    for c in label.nfkd() {
        let mapped: &str = match c {
            'a'..='z' | '0'..='9' => {
                out.push(c);
                continue;
            }
            'A'..='Z' => {
                out.push(c.to_ascii_lowercase());
                continue;
            }
            'ø' | 'Ø' => "o",
            'đ' | 'Đ' => "d",
            'ł' | 'Ł' => "l",
            'æ' | 'Æ' => "ae",
            'œ' | 'Œ' => "oe",
            'ß' => "ss",
            c if unicode_normalization::char::is_combining_mark(c) => continue,
            _ => "-",
        };
        out.push_str(mapped);
    }
    let collapsed = out
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        FALLBACK.to_string()
    } else {
        collapsed
    }
}

fn cut(slug: &str, max: usize) -> &str {
    slug[..slug.len().min(max)].trim_end_matches('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(windows_user_for("Kantor", &|_| false), "claude-kantor");
    }
    #[test]
    fn slug_unicode_and_emoji() {
        assert_eq!(windows_user_for("Ünïcødé 🚀", &|_| false), "claude-unicode");
    }
    #[test]
    fn slug_spaces_become_dashes() {
        assert_eq!(
            windows_user_for("Tim  Desain", &|_| false),
            "claude-tim-desain"
        );
    }
    #[test]
    fn slug_empty_fallback() {
        assert_eq!(windows_user_for("   ", &|_| false), "claude-akun");
        assert_eq!(windows_user_for("🚀🚀", &|_| false), "claude-akun");
    }
    #[test]
    fn slug_truncates() {
        let n = windows_user_for("sangat panjang sekali namanya", &|_| false);
        assert!(n.len() <= 20, "{n}");
        assert!(!n.ends_with('-'), "{n}");
    }
    #[test]
    fn slug_collision_suffix() {
        let taken = |n: &str| n == "claude-kantor" || n == "claude-kantor-2";
        assert_eq!(windows_user_for("Kantor", &taken), "claude-kantor-3");
    }
    #[test]
    fn slug_collision_on_long_name_stays_within_20() {
        let base = windows_user_for("sangat panjang sekali namanya", &|_| false);
        let n = windows_user_for("sangat panjang sekali namanya", &|x| x == base);
        assert!(n.len() <= 20 && n.ends_with("-2"), "{n}");
    }
    #[test]
    fn every_generated_name_is_valid() {
        let long = "x".repeat(50);
        for l in ["a", "Kantor", long.as_str(), "--", "Ünïcødé 🚀", "  "] {
            let n = windows_user_for(l, &|_| false);
            assert!(is_valid_windows_user(&n), "{l} -> {n}");
        }
    }
    #[test]
    fn validation_rejects_others() {
        for bad in [
            "Administrator",
            "claude-",
            "claude-UPPER",
            "claude-a b",
            "claude-12345678901234",
            "xclaude-a",
        ] {
            assert!(!is_valid_windows_user(bad), "{bad}");
        }
    }
}
