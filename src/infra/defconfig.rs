//! Assignment-file loader (`*_defconfig` / `.config`).
//!
//! Pipeline step: [`DefconfigLoader::load`] produces a domain [`AssignmentSet`].

use crate::domain::{Assignment, AssignmentSet, Limits, strip_config_prefix, validate_symbol_name};
use crate::error::Error;
use std::fs;
use std::path::Path;

/// Loads Linux-style assignment files into the domain model.
pub struct DefconfigLoader;

impl DefconfigLoader {
    pub fn new() -> Self {
        Self
    }

    /// Load a Linux-style `*_defconfig` or `.config` assignment file.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if the file cannot be read; [`Error::Usage`] for a
    /// malformed line; [`Error::Domain`] if a string value exceeds [`Limits`].
    pub fn load(&self, path: &Path, limits: Limits) -> Result<AssignmentSet, Error> {
        check_file_size(path, limits)?;
        let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        self.parse_text(&text, limits)
    }

    /// Parse assignment text without touching the filesystem.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`] except [`Error::Io`].
    pub fn parse_text(&self, text: &str, limits: Limits) -> Result<AssignmentSet, Error> {
        let mut set = AssignmentSet::new();
        for (idx, line) in text.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            if let Some(assignment) = self.parse_line(line, line_no, limits)? {
                set.push(assignment, limits)?;
            }
        }
        Ok(set)
    }

    fn parse_line(
        &self,
        line: &str,
        line_no: u32,
        limits: Limits,
    ) -> Result<Option<Assignment>, Error> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if let Some(name) = Self::parse_is_not_set(trimmed) {
            validate_symbol_name(name, limits)?;
            return Ok(Some(Assignment {
                name: name.to_string(),
                raw: "n".to_string(),
                line: line_no,
            }));
        }
        if trimmed.starts_with('#') {
            return Ok(None);
        }
        let (lhs, rhs) = trimmed.split_once('=').ok_or_else(|| {
            Error::Usage(format!(
                "malformed assignment on line {line_no}: expected `CONFIG_NAME=value`. Fix that line in the defconfig"
            ))
        })?;
        let ident = lhs.trim();
        if ident.is_empty() {
            return Err(Error::Usage(format!(
                "empty symbol name on line {line_no}. Use `CONFIG_NAME=value`"
            )));
        }
        let name = strip_config_prefix(ident);
        validate_symbol_name(name, limits)?;
        let raw = Self::strip_inline_comment(rhs.trim()).to_string();
        if raw.len() > limits.max_string_value_len {
            return Err(crate::domain::DomainError::string_value_too_long(name, limits).into());
        }
        Ok(Some(Assignment {
            name: name.to_string(),
            raw,
            line: line_no,
        }))
    }

    fn parse_is_not_set(line: &str) -> Option<&str> {
        let rest = line.strip_prefix('#')?.trim();
        let ident = rest.strip_suffix("is not set")?.trim();
        Some(strip_config_prefix(ident))
    }

    fn strip_inline_comment(rhs: &str) -> &str {
        if rhs.starts_with('"') {
            return rhs;
        }
        match rhs.split_once('#') {
            Some((value, _)) => value.trim(),
            None => rhs,
        }
    }
}

impl Default for DefconfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Load a Linux-style `*_defconfig` or `.config` assignment file.
///
/// Alias for [`DefconfigLoader::load`].
///
/// # Errors
///
/// Same as [`DefconfigLoader::load`].
pub fn load_defconfig(path: &Path, limits: Limits) -> Result<AssignmentSet, Error> {
    DefconfigLoader::new().load(path, limits)
}

/// Parse assignment text without a file.
///
/// Alias for [`DefconfigLoader::parse_text`].
///
/// # Errors
///
/// Same as [`DefconfigLoader::parse_text`].
pub fn parse_defconfig_text(text: &str, limits: Limits) -> Result<AssignmentSet, Error> {
    DefconfigLoader::new().parse_text(text, limits)
}

pub(crate) fn check_file_size(path: &Path, limits: Limits) -> Result<(), Error> {
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let size = meta.len();
    if size > limits.max_file_bytes {
        return Err(crate::domain::DomainError::FileTooLarge {
            path: path.display().to_string(),
            size,
            max: limits.max_file_bytes,
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::DefconfigLoader;
    use crate::domain::Limits;

    #[test]
    fn parses_y_n_int_hex_string_and_not_set() {
        let text = r#"
# comment
CONFIG_FOO=y
CONFIG_BAR=n # inline
# CONFIG_BAZ is not set
CONFIG_SIZE=256
CONFIG_ADDR=0x20
CONFIG_NAME="board#1"
"#;
        let set = DefconfigLoader::new()
            .parse_text(text, Limits::default())
            .unwrap();
        assert_eq!(set.get("FOO").unwrap().raw, "y");
        assert_eq!(set.get("BAR").unwrap().raw, "n");
        assert_eq!(set.get("BAZ").unwrap().raw, "n");
        assert_eq!(set.get("SIZE").unwrap().raw, "256");
        assert_eq!(set.get("ADDR").unwrap().raw, "0x20");
        assert_eq!(set.get("NAME").unwrap().raw, "\"board#1\"");
    }

    #[test]
    fn rejects_malformed_line() {
        let err = DefconfigLoader::new()
            .parse_text("NOT_AN_ASSIGNMENT", Limits::default())
            .unwrap_err();
        assert!(err.to_string().contains("malformed"));
    }
}
