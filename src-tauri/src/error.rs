//! Error type shared by every module and serialized to the UI as
//! `{ code, message_id, detail }` (message_id keys into ui/strings.js).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    UacCancelled,
    ClaudeMissing,
    LogonFailed,
    JunctionBroken,
    LowMemory,
    StoreCorrupt,
    NotFound,
    OpenTimeout,
    Invalid,
    Io,
    Win32(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message_id: &'static str,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message_id: &'static str) -> Self {
        Self { code, message_id, detail: None }
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    pub fn io(e: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Io, "err.generic").with_detail(e.to_string())
    }
    pub fn win32(code: u32, what: &str) -> Self {
        Self::new(ErrorCode::Win32(code), "err.generic").with_detail(format!("{what} (Win32 {code})"))
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {}", self.code, self.detail.as_deref().unwrap_or(self.message_id))
    }
}
impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::io(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_ui_shape() {
        let e = AppError::new(ErrorCode::UacCancelled, "err.uac_cancelled");
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"code":"UacCancelled","message_id":"err.uac_cancelled","detail":null}"#
        );
    }

    #[test]
    fn win32_code_carries_number() {
        let e = AppError::new(ErrorCode::Win32(5), "err.win32").with_detail("access denied");
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"]["Win32"], 5);
        assert_eq!(v["detail"], "access denied");
    }
}
