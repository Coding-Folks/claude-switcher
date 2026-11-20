//! How much each account used Claude Code lately, and whether it hit its usage limit, read from the
//! shared transcripts in `~/.claude/projects` (nothing is asked from Anthropic, no login token is
//! touched). Each assistant message carries a `timestamp` and `message.usage`; an API error carries
//! `isApiErrorMessage` and `error`. Sums are estimates: Chat is not on this computer, and cache reads
//! count for a tenth.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Cache reads cost far less of the usage limit than new tokens: they count for this much.
pub const CACHE_READ_WEIGHT: f64 = 0.1;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;
/// Claude's usage window.
pub const WINDOW: i64 = 5 * HOUR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub at_ms: i64,
    pub tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitHit {
    pub at_ms: i64,
    pub until_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    Usage(Event),
    Limit(LimitHit),
}

/// This computer's offset from UTC now, in minutes (for "resets 7pm").
fn local_offset_min() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// One transcript line: a message's token use, a usage-limit error, or nothing.
pub fn parse_line(line: &str) -> Option<Parsed> {
    let v: Value = serde_json::from_str(line).ok()?;
    let at = chrono::DateTime::parse_from_rfc3339(v.get("timestamp")?.as_str()?)
        .ok()?
        .timestamp_millis();
    if v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
        let error = v
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        let content = v.pointer("/message/content");
        let text = match content {
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" "),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let lower = text.to_lowercase();
        let is_limit =
            error.contains("rate_limit") || (lower.contains("limit") && lower.contains("reset"));
        return is_limit.then(|| {
            Parsed::Limit(LimitHit {
                at_ms: at,
                until_ms: reset_time(&text, at, local_offset_min()),
            })
        });
    }
    let u = v.pointer("/message/usage")?;
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    let tokens = n("input_tokens")
        + n("cache_creation_input_tokens")
        + n("output_tokens")
        + (n("cache_read_input_tokens") as f64 * CACHE_READ_WEIGHT).round() as u64;
    Some(Parsed::Usage(Event { at_ms: at, tokens }))
}

/// When a limit lifts, from its message: "…|<epoch seconds>", "resets 7pm", "resets 7:30 pm",
/// "resets at 19:00" (a clock time already past means tomorrow); else five hours after the error.
pub fn reset_time(text: &str, at_ms: i64, tz_offset_min: i32) -> i64 {
    if let Some(i) = text.rfind('|') {
        let digits: String = text[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() >= 10 {
            if let Ok(s) = digits.parse::<i64>() {
                return s * 1000;
            }
        }
    }
    let lower = text.to_lowercase();
    if let Some(i) = lower.find("reset") {
        let rest = lower[i + 5..].trim_start_matches('s').trim_start();
        let rest = rest.strip_prefix("at").unwrap_or(rest).trim_start();
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(mut h) = num.parse::<i64>() {
            let mut tail = &rest[num.len()..];
            let mut m = 0;
            if let Some(after) = tail.strip_prefix(':') {
                let mm: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                m = mm.parse().unwrap_or(0);
                tail = &after[mm.len()..];
            }
            let tail = tail.trim_start();
            if tail.starts_with("pm") && h < 12 {
                h += 12;
            } else if tail.starts_with("am") && h == 12 {
                h = 0;
            }
            if h < 24 && m < 60 {
                let off = tz_offset_min as i64 * 60_000;
                let local = at_ms + off;
                let mut cand = local - local.rem_euclid(DAY) + h * HOUR + m * 60_000;
                if cand <= local {
                    cand += DAY;
                }
                return cand - off;
            }
        }
    }
    at_ms + WINDOW
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct FileState {
    offset: u64,
    events: Vec<Event>,
    limits: Vec<LimitHit>,
}

/// Token use and limits over a window, for one account's transcripts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub five_h: u64,
    pub seven_d: u64,
    pub limited_until: Option<i64>,
}

/// Reads transcripts incrementally (each file from where it stopped), keeping the last week.
pub struct Tracker {
    cache: PathBuf,
    files: HashMap<String, FileState>,
}

impl Tracker {
    pub fn new(cache: PathBuf) -> Self {
        let files = std::fs::read_to_string(&cache)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { cache, files }
    }

    /// Reads what was added to each file since the last scan. A last line without its newline is
    /// being written: it is left for the next scan.
    pub fn scan(&mut self, files: &[PathBuf]) {
        use std::io::{Read, Seek, SeekFrom};
        for path in files {
            let key = path.to_string_lossy().into_owned();
            let Ok(mut f) = std::fs::File::open(path) else {
                continue;
            };
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            let st = self.files.entry(key).or_default();
            if len < st.offset {
                *st = FileState::default(); // rewritten: start again
            }
            if len == st.offset || f.seek(SeekFrom::Start(st.offset)).is_err() {
                continue;
            }
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_err() {
                continue;
            }
            let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
                continue;
            };
            for line in String::from_utf8_lossy(&buf[..end]).lines() {
                match parse_line(line) {
                    Some(Parsed::Usage(e)) => st.events.push(e),
                    Some(Parsed::Limit(l)) => st.limits.push(l),
                    None => {}
                }
            }
            st.offset += end as u64 + 1;
            // Keep a week (and a day to spare) after the newest thing seen in this file.
            let newest = st
                .events
                .iter()
                .map(|e| e.at_ms)
                .chain(st.limits.iter().map(|l| l.at_ms))
                .max();
            if let Some(newest) = newest {
                st.events.retain(|e| e.at_ms >= newest - 8 * DAY);
                st.limits.retain(|l| l.at_ms >= newest - 8 * DAY);
            }
        }
    }

    pub fn totals(&self, files: &[PathBuf], now_ms: i64) -> Totals {
        let mut t = Totals::default();
        for path in files {
            let Some(st) = self.files.get(&*path.to_string_lossy()) else {
                continue;
            };
            for e in &st.events {
                if e.at_ms <= now_ms && e.at_ms > now_ms - 7 * DAY {
                    t.seven_d += e.tokens;
                    if e.at_ms > now_ms - WINDOW {
                        t.five_h += e.tokens;
                    }
                }
            }
            for l in &st.limits {
                if l.until_ms > now_ms && l.at_ms <= now_ms {
                    t.limited_until =
                        Some(t.limited_until.map_or(l.until_ms, |u| u.max(l.until_ms)));
                }
            }
        }
        t
    }

    /// Writes the offsets and the kept events, so a restart does not read everything again.
    pub fn save(&self) {
        if let Some(dir) = self.cache.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&self.files) {
            let tmp = self.cache.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.cache);
            }
        }
    }
}

/// The transcripts of these Claude Code sessions: `projects/*/<id>.jsonl` and their subagents.
pub fn transcript_files(claude_dir: &Path, cli_ids: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(claude_dir.join("projects")) else {
        return out;
    };
    for p in projects.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        for id in cli_ids {
            let main = p.join(format!("{id}.jsonl"));
            if main.is_file() {
                out.push(main);
            }
            if let Ok(subs) = std::fs::read_dir(p.join(id).join("subagents")) {
                out.extend(
                    subs.flatten()
                        .map(|e| e.path())
                        .filter(|f| f.extension().is_some_and(|x| x == "jsonl")),
                );
            }
        }
    }
    out
}

/// The limit learned from the token use at the last hits (their mean).
pub fn learned(samples: &[u64]) -> Option<u64> {
    (!samples.is_empty()).then(|| samples.iter().sum::<u64>() / samples.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const H: i64 = 3_600_000;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn iso(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339()
    }
    fn usage_line(ms: i64, input: u64, create: u64, read: u64, out: u64) -> String {
        serde_json::json!({"type": "assistant", "timestamp": iso(ms), "message": {"role": "assistant",
            "usage": {"input_tokens": input, "cache_creation_input_tokens": create,
                      "cache_read_input_tokens": read, "output_tokens": out}}})
        .to_string()
    }
    fn error_line(ms: i64, error: &str, text: &str) -> String {
        serde_json::json!({"type": "assistant", "timestamp": iso(ms), "isApiErrorMessage": true,
            "error": error, "message": {"content": [{"type": "text", "text": text}]}})
        .to_string()
    }
    fn append(p: &Path, s: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    #[test]
    fn usage_counts_input_cache_creation_output_and_a_tenth_of_cache_reads() {
        let Some(Parsed::Usage(e)) = parse_line(&usage_line(1_000, 2, 1000, 10_000, 300)) else {
            panic!("usage expected")
        };
        assert_eq!((e.at_ms, e.tokens), (1_000, 2302));
    }

    #[test]
    fn lines_without_usage_or_timestamp_are_ignored() {
        assert!(parse_line(r#"{"type":"user","timestamp":"2026-11-20T01:00:00Z"}"#).is_none());
        assert!(parse_line(r#"{"message":{"usage":{"output_tokens":5}}}"#).is_none());
        assert!(parse_line("{ half a line").is_none());
    }

    #[test]
    fn a_rate_limit_error_is_a_limit_hit() {
        let at = 1_795_150_000_000;
        let l = error_line(
            at,
            "rate_limit",
            "Claude usage limit reached. Your limit resets at 19:00.",
        );
        assert!(matches!(parse_line(&l), Some(Parsed::Limit(_))));
        let l = error_line(at, "", "You've hit your limit · resets 7pm");
        assert!(matches!(parse_line(&l), Some(Parsed::Limit(_))));
    }

    #[test]
    fn an_auth_error_is_not() {
        let l = error_line(
            1,
            "authentication_failed",
            "Login expired · Please run /login",
        );
        assert!(parse_line(&l).is_none());
    }

    #[test]
    fn reset_time_reads_12h_24h_and_epoch_forms() {
        // 2026-11-20 15:00 local, local = UTC+7 (420 min)
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-20T15:00:00+07:00")
            .unwrap()
            .timestamp_millis();
        let local = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .timestamp_millis()
        };
        assert_eq!(
            reset_time("resets 7pm", at, 420),
            local("2026-11-20T19:00:00+07:00")
        );
        assert_eq!(
            reset_time("resets 7:30 pm", at, 420),
            local("2026-11-20T19:30:00+07:00")
        );
        assert_eq!(
            reset_time("Your limit resets at 19:00.", at, 420),
            local("2026-11-20T19:00:00+07:00")
        );
        assert_eq!(
            reset_time("limit reached|1795150800", at, 420),
            1_795_150_800_000
        );
        assert_eq!(reset_time("limit reached", at, 420), at + 5 * H);
        // a clock time already past today means tomorrow
        assert_eq!(
            reset_time("resets 9am", at, 420),
            local("2026-11-21T09:00:00+07:00")
        );
    }

    #[test]
    fn totals_sum_5h_and_7d_windows() {
        let dir = tmp();
        let f = dir.join("s.jsonl");
        let now = 100 * 24 * H;
        for (ago, t) in [(H, 100), (4 * H, 200), (6 * H, 400), (8 * 24 * H, 800)] {
            append(&f, &(usage_line(now - ago, 0, 0, 0, t) + "\n"));
        }
        let mut tr = Tracker::new(dir.join("cache.json"));
        tr.scan(std::slice::from_ref(&f));
        let t = tr.totals(std::slice::from_ref(&f), now);
        assert_eq!((t.five_h, t.seven_d), (300, 700));
        assert_eq!(t.limited_until, None);
    }

    #[test]
    fn a_limit_in_the_past_is_not_current() {
        let dir = tmp();
        let f = dir.join("s.jsonl");
        let now = 100 * 24 * H;
        append(
            &f,
            &(error_line(now - 6 * H, "rate_limit", "limit reached") + "\n"),
        ); // reset at now - 1 h
        append(
            &f,
            &(error_line(now - H, "rate_limit", "limit reached") + "\n"),
        ); // reset at now + 4 h
        let mut tr = Tracker::new(dir.join("cache.json"));
        tr.scan(std::slice::from_ref(&f));
        assert_eq!(
            tr.totals(std::slice::from_ref(&f), now).limited_until,
            Some(now + 4 * H)
        );
        assert_eq!(
            tr.totals(std::slice::from_ref(&f), now + 5 * H)
                .limited_until,
            None
        );
    }

    #[test]
    fn scan_is_incremental_and_waits_for_a_partial_last_line() {
        let dir = tmp();
        let f = dir.join("s.jsonl");
        let now = 10 * H;
        append(&f, &(usage_line(now - H, 0, 0, 0, 10) + "\n"));
        let half = usage_line(now - H, 0, 0, 0, 5);
        let (a, b) = half.split_at(half.len() / 2);
        append(&f, a);
        let mut tr = Tracker::new(dir.join("cache.json"));
        tr.scan(std::slice::from_ref(&f));
        assert_eq!(tr.totals(std::slice::from_ref(&f), now).five_h, 10);
        append(&f, &format!("{b}\n"));
        tr.scan(std::slice::from_ref(&f));
        tr.scan(std::slice::from_ref(&f)); // nothing new: counted once
        assert_eq!(tr.totals(std::slice::from_ref(&f), now).five_h, 15);
    }

    #[test]
    fn cache_survives_a_restart() {
        let dir = tmp();
        let f = dir.join("s.jsonl");
        let now = 10 * H;
        append(&f, &(usage_line(now - H, 0, 0, 0, 10) + "\n"));
        let mut tr = Tracker::new(dir.join("cache.json"));
        tr.scan(std::slice::from_ref(&f));
        tr.save();
        let mut again = Tracker::new(dir.join("cache.json"));
        again.scan(std::slice::from_ref(&f)); // must not count the same line twice
        assert_eq!(again.totals(std::slice::from_ref(&f), now).five_h, 10);
    }

    #[test]
    fn transcript_files_include_subagents() {
        let claude = tmp();
        let p = claude.join("projects").join("E--x");
        std::fs::create_dir_all(p.join("abc").join("subagents")).unwrap();
        append(&p.join("abc.jsonl"), "");
        append(&p.join("abc").join("subagents").join("agent-1.jsonl"), "");
        append(&p.join("other.jsonl"), "");
        let mut got = transcript_files(&claude, &["abc".to_string()]);
        got.sort();
        assert_eq!(got.len(), 2, "{got:?}");
    }

    #[test]
    fn learned_is_the_mean() {
        assert_eq!(learned(&[]), None);
        assert_eq!(learned(&[100, 200, 300]), Some(200));
    }
}
