//! Label -> hidden Windows user name (`claude-<slug>`, at most 20 chars).

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
        assert_eq!(windows_user_for("Tim  Desain", &|_| false), "claude-tim-desain");
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
        for bad in ["Administrator", "claude-", "claude-UPPER", "claude-a b", "claude-12345678901234", "xclaude-a"] {
            assert!(!is_valid_windows_user(bad), "{bad}");
        }
    }
}
