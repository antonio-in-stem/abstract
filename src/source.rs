//! Source files, byte-order marks, line terminators and positions.
//!
//! SPEC §2.2: source files are UTF-8; a BOM at offset 0 is removed before
//! lexing and has no other effect; `LF` and `CRLF` are each one line
//! terminator; line and column numbers are 1-based and a column counts
//! Unicode scalar values, not bytes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::{Diagnostic, ErrorId, Position};

/// The UTF-8 byte-order mark.
pub const BOM: &str = "\u{feff}";

/// One source file handed to the compiler.
///
/// `path` is the project-relative path used in diagnostics, always with `/`
/// separators (SPEC §9.8). `text` is the file's contents with a leading BOM
/// already removed.
#[derive(Clone, Debug)]
pub struct SourceFile {
    /// Project-relative path, `/`-separated, as it appears in diagnostics.
    pub path: String,
    /// File contents, BOM stripped.
    pub text: String,
    /// Absolute path on disk, when the file was read from one.
    origin: Option<PathBuf>,
    /// Byte offsets at which each line starts, including line 1 at offset 0.
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// Builds an in-memory source file. A leading BOM is stripped.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        let text: String = text.into();
        let text = strip_bom(&text).to_string();
        let line_starts = line_starts(&text);
        Self {
            path: normalise_display_path(&path.into()),
            text,
            origin: None,
            line_starts,
        }
    }

    /// Reads a file from disk. `display_path` is the project-relative path
    /// used in diagnostics.
    ///
    /// A file that cannot be read is E101; a file that is not valid UTF-8 is
    /// E102, naming the offset of the first bad byte.
    pub fn read(origin: &Path, display_path: impl Into<String>) -> Result<Self, Diagnostic> {
        let display_path = normalise_display_path(&display_path.into());
        let bytes = fs::read(origin).map_err(|error| {
            Diagnostic::new(
                ErrorId::E101,
                format!("Cannot read source file '{display_path}': {error}."),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|error| {
            let offset = error.utf8_error().valid_up_to();
            Diagnostic::new(
                ErrorId::E102,
                format!(
                    "Source file '{display_path}' is not valid UTF-8 (first bad byte at offset {offset})."
                ),
            )
        })?;
        let mut file = SourceFile::new(display_path, text);
        file.origin = Some(origin.to_path_buf());
        Ok(file)
    }

    /// The absolute path this file was read from, when there is one.
    pub fn origin(&self) -> Option<&Path> {
        self.origin.as_deref()
    }

    /// Records where this file lives on disk.
    pub fn set_origin(&mut self, origin: impl Into<PathBuf>) {
        self.origin = Some(origin.into());
    }

    /// The stem of the file name, with the final `.ab` or `.abt` extension
    /// removed case-insensitively (SPEC §5.3). It is the default instance id.
    pub fn stem(&self) -> &str {
        let name = match self.path.rfind('/') {
            Some(index) => &self.path[index + 1..],
            None => &self.path,
        };
        for extension in [".abt", ".ab"] {
            if name.len() > extension.len() {
                let tail = &name[name.len() - extension.len()..];
                if tail.eq_ignore_ascii_case(extension) {
                    return &name[..name.len() - extension.len()];
                }
            }
        }
        name
    }

    /// True when the file name ends with `.abt`, compared case-insensitively.
    pub fn is_template(&self) -> bool {
        has_extension(&self.path, "abt")
    }

    /// True when the file name ends with `.ab`, compared case-insensitively.
    pub fn is_instance(&self) -> bool {
        has_extension(&self.path, "ab")
    }

    /// The number of lines, counting a final unterminated line.
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// The 1-based position of a byte offset. An offset past the end of the
    /// text maps to the end of the last line; the function never panics and
    /// never indexes outside the text.
    pub fn position_at(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line_index).copied().unwrap_or(0);
        // Column counts Unicode scalar values, so slice on a char boundary.
        let head = self.text.get(line_start..offset).unwrap_or("");
        Position::new(line_index as u32 + 1, head.chars().count() as u32 + 1)
    }

    /// The byte offset at which a 1-based line starts, when the line exists.
    pub fn line_offset(&self, line: u32) -> Option<usize> {
        if line == 0 {
            return None;
        }
        self.line_starts.get(line as usize - 1).copied()
    }

    /// The text of a 1-based line, without its terminator.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let start = self.line_offset(line)?;
        let end = self
            .line_offset(line + 1)
            .unwrap_or(self.text.len())
            .min(self.text.len());
        let slice = self.text.get(start..end)?;
        Some(slice.trim_end_matches('\n').trim_end_matches('\r'))
    }

    /// The byte offset of the first lone `CR` — a `CR` not followed by `LF` —
    /// which SPEC §2.2 makes E210.
    pub fn first_lone_carriage_return(&self) -> Option<usize> {
        let bytes = self.text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
                return Some(index);
            }
            index += 1;
        }
        None
    }
}

/// Removes one leading UTF-8 byte-order mark. A BOM anywhere else is an
/// ordinary character (SPEC §2.2) and is left alone.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix(BOM).unwrap_or(text)
}

/// Rewrites a path for display: `\` becomes `/`, and a Windows extended-length
/// prefix is removed. Diagnostics never print a platform-canonical path
/// (SPEC §5.9, §9.8).
pub fn normalise_display_path(path: &str) -> String {
    let trimmed = path
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| path.strip_prefix(r"\\?\").unwrap_or(path).to_string());
    trimmed.replace('\\', "/")
}

/// True when `path` ends with `.<extension>`, compared case-insensitively.
pub fn has_extension(path: &str, extension: &str) -> bool {
    let name = match path.rfind(['/', '\\']) {
        Some(index) => &path[index + 1..],
        None => path,
    };
    match name.rfind('.') {
        Some(index) => name[index + 1..].eq_ignore_ascii_case(extension),
        None => false,
    }
}

/// Byte offsets at which each line starts. `LF` and `CRLF` both end a line;
/// a lone `CR` does not, because it is not a line terminator (SPEC §2.2).
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            starts.push(index + 1);
        }
        index += 1;
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_bom_is_removed_and_a_later_one_is_kept() {
        let file = SourceFile::new("a.ab", "\u{feff}Thing :: @id.x");
        assert_eq!(file.text, "Thing :: @id.x");
        let inner = SourceFile::new("a.ab", "x: \u{feff}y");
        assert_eq!(inner.text, "x: \u{feff}y");
    }

    #[test]
    fn positions_are_one_based_and_count_scalar_values() {
        let file = SourceFile::new("a.ab", "abc\r\nñé x\nlast");
        assert_eq!(file.position_at(0), Position::new(1, 1));
        assert_eq!(file.position_at(2), Position::new(1, 3));
        // Offset 5 is the start of line 2; 'ñ' is two bytes, so offset 7 is column 2.
        assert_eq!(file.position_at(5), Position::new(2, 1));
        assert_eq!(file.position_at(7), Position::new(2, 2));
        assert_eq!(file.line_count(), 3);
        assert_eq!(file.line_text(1), Some("abc"));
        assert_eq!(file.line_text(2), Some("ñé x"));
        assert_eq!(file.line_text(3), Some("last"));
        assert_eq!(file.line_text(4), None);
    }

    #[test]
    fn an_offset_past_the_end_never_panics() {
        let file = SourceFile::new("a.ab", "ab");
        assert_eq!(file.position_at(999), Position::new(1, 3));
    }

    #[test]
    fn a_lone_carriage_return_is_found_and_crlf_is_not() {
        assert_eq!(
            SourceFile::new("a.ab", "a\r\nb").first_lone_carriage_return(),
            None
        );
        assert_eq!(
            SourceFile::new("a.ab", "a\rb").first_lone_carriage_return(),
            Some(1)
        );
    }

    #[test]
    fn stems_and_extensions_are_case_insensitive() {
        let file = SourceFile::new("data/items/Frost.AB", "");
        assert_eq!(file.stem(), "Frost");
        assert!(file.is_instance());
        assert!(!file.is_template());
        let template = SourceFile::new("data/templates/Item.ABT", "");
        assert_eq!(template.stem(), "Item");
        assert!(template.is_template());
        assert!(!template.is_instance());
        let dotted = SourceFile::new("data/items/frost.v2.ab", "");
        assert_eq!(dotted.stem(), "frost.v2");
    }

    #[test]
    fn display_paths_lose_backslashes_and_verbatim_prefixes() {
        assert_eq!(
            normalise_display_path(r"\\?\C:\pack\data\a.ab"),
            "C:/pack/data/a.ab"
        );
        assert_eq!(
            normalise_display_path(r"data\items\a.ab"),
            "data/items/a.ab"
        );
    }
}
