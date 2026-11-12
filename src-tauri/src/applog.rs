//! `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log`, rotated at 1 MB, 5 files kept.
//! Field values whose key mentions a password or secret are never written.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MAX_BYTES: u64 = 1 << 20;
const KEEP: usize = 5;

pub fn format_line(level: &str, msg: &str, fields: &[(&str, &str)]) -> String {
    let mut line = format!(
        "{} {level} {msg}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    for (k, v) in fields {
        let lk = k.to_ascii_lowercase();
        let v = if lk.contains("password") || lk.contains("secret") {
            "[redacted]"
        } else {
            v
        };
        line.push_str(&format!(" {k}={v}"));
    }
    line
}

pub struct Logger {
    path: PathBuf,
    max: u64,
    lock: Mutex<()>,
}

impl Logger {
    pub fn new(path: impl Into<PathBuf>, max: u64) -> Self {
        Self {
            path: path.into(),
            max,
            lock: Mutex::new(()),
        }
    }

    fn rotated(&self, n: usize) -> PathBuf {
        let mut s = self.path.as_os_str().to_owned();
        s.push(format!(".{n}"));
        PathBuf::from(s)
    }

    fn rotate(&self) {
        let _ = std::fs::remove_file(self.rotated(KEEP - 1));
        for n in (1..KEEP - 1).rev() {
            let _ = std::fs::rename(self.rotated(n), self.rotated(n + 1));
        }
        let _ = std::fs::rename(&self.path, self.rotated(1));
    }

    pub fn write(&self, level: &str, msg: &str, fields: &[(&str, &str)]) {
        let _g = self.lock.lock().unwrap();
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(&self.path)
            .map(|m| m.len() >= self.max)
            .unwrap_or(false)
        {
            self.rotate();
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{}", format_line(level, msg, fields));
        }
    }
}

static GLOBAL: OnceLock<Logger> = OnceLock::new();

pub fn init(dir: &Path) {
    let _ = GLOBAL.set(Logger::new(dir.join("app.log"), MAX_BYTES));
}

pub fn info(msg: &str, fields: &[(&str, &str)]) {
    if let Some(l) = GLOBAL.get() {
        l.write("INFO", msg, fields);
    }
}

pub fn error(msg: &str, fields: &[(&str, &str)]) {
    if let Some(l) = GLOBAL.get() {
        l.write("ERROR", msg, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secrets() {
        let line = format_line(
            "INFO",
            "add",
            &[
                ("user", "claude-kantor"),
                ("password", "hunter2"),
                ("Secret_File", "C:\\s"),
                ("new_password", "x"),
            ],
        );
        assert!(line.contains("user=claude-kantor"), "{line}");
        assert!(
            !line.contains("hunter2") && !line.contains("C:\\s") && !line.contains("=x"),
            "{line}"
        );
        assert!(line.contains("password=[redacted]"), "{line}");
    }

    #[test]
    fn rotates_at_limit_and_keeps_five() {
        let dir = std::env::temp_dir().join(format!("cs-log-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = Logger::new(dir.join("app.log"), 100);
        for i in 0..40 {
            log.write("INFO", &format!("event number {i:03}"), &[]);
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "app.log",
                "app.log.1",
                "app.log.2",
                "app.log.3",
                "app.log.4"
            ]
        );
        assert!(std::fs::metadata(dir.join("app.log")).unwrap().len() <= 100 + 80);
        assert!(std::fs::read_to_string(dir.join("app.log"))
            .unwrap()
            .contains("event number 039"));
    }
}
