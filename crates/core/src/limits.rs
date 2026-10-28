//! Caps on input sizes. Every value that reaches storage passes through these checks first, so a
//! hostile or broken client cannot grow a record without bound (AGENTS.md › Never crash).

use crate::CoreError;

/// Names, codes, units, ids: anything that fits on one line.
pub const MAX_SHORT_TEXT: usize = 256;
/// Free text such as a reason or a short note.
pub const MAX_LONG_TEXT: usize = 16 * 1024;
/// The body of a clinical note.
pub const MAX_NOTE_TEXT: usize = 64 * 1024;
/// Repeated items in one value (given names, address lines).
pub const MAX_ITEMS: usize = 16;

/// Checks a length cap (in bytes) and rejects control characters other than tab and newline.
pub fn check_text(field: &'static str, value: &str, max: usize) -> Result<(), CoreError> {
    if value.len() > max {
        return Err(CoreError::Invalid { field, reason: format!("longer than {max} bytes") });
    }
    if value.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(CoreError::Invalid { field, reason: "contains control characters".into() });
    }
    Ok(())
}

/// [`check_text`] plus "not blank".
pub fn check_required(field: &'static str, value: &str, max: usize) -> Result<(), CoreError> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid { field, reason: "must not be empty".into() });
    }
    check_text(field, value, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_checks() {
        assert!(check_text("f", "ok\twith tab\nand newline", 64).is_ok());
        assert!(check_text("f", "bell\u{7}", 64).is_err());
        assert!(check_text("f", "toolong", 3).is_err());
        assert!(check_required("f", "   ", 64).is_err());
        assert!(check_required("f", "x", 64).is_ok());
    }
}
