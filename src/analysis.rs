//! Optional, compiler-owned editor protocol. See docs/ANALYSIS-PROTOCOL.md.
//! One bounded request per process; the client cancels by terminating that
//! process. Source overrides never modify disk or the resolved assets root.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::diagnostics::{Diagnostic, Diagnostics, Position};
use crate::output::{self, Format, Value};
use crate::project;
use crate::source::{normalise_display_path, SourceFile};
use crate::{compile_layout, CompileOptions, COMPILER_VERSION};

mod symbols;

pub const MAGIC: &[u8; 8] = b"ABANLZ01";
pub const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PATH_BYTES: usize = 32 * 1024;
pub const MAX_OVERLAYS: usize = 128;
pub const MAX_DIAGNOSTICS: usize = 100;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct Request {
    id: u32,
    overlays: HashMap<PathBuf, String>,
}

/// Decode the exact frame; unknown versions, trailing bytes, invalid UTF-8,
/// duplicate canonical targets, relative/ambiguous paths and limits fail closed.
pub fn read_request(reader: impl Read) -> Result<Request, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read analysis request: {error}"))?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("Analysis request exceeds 16 MiB.".into());
    }
    decode_request(&bytes)
}

fn decode_request(bytes: &[u8]) -> Result<Request, String> {
    let mut frame = Frame { bytes, offset: 0 };
    if frame.take(MAGIC.len())? != MAGIC {
        return Err("Unsupported analysis protocol; expected ABANLZ01.".into());
    }
    let id = frame.number()?;
    let count = frame.number()? as usize;
    if count > MAX_OVERLAYS {
        return Err("Analysis request exceeds 128 overlays.".into());
    }
    let mut overlays = HashMap::new();
    for _ in 0..count {
        let path_len = frame.number()? as usize;
        let text_len = frame.number()? as usize;
        if path_len == 0 || path_len > MAX_PATH_BYTES || text_len > MAX_TEXT_BYTES {
            return Err("Overlay path/text length exceeds protocol limits.".into());
        }
        let written =
            std::str::from_utf8(frame.take(path_len)?).map_err(|_| "Overlay path is not UTF-8.")?;
        let text = std::str::from_utf8(frame.take(text_len)?)
            .map_err(|_| "Overlay source is not UTF-8.")?;
        let origin = canonical_source(written)?;
        if overlays.insert(origin, text.to_string()).is_some() {
            return Err("Duplicate canonical overlay path.".into());
        }
    }
    if frame.offset != bytes.len() {
        return Err("Trailing bytes after analysis request.".into());
    }
    Ok(Request { id, overlays })
}

struct Frame<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Frame<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or("Analysis frame length overflow.")?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or("Truncated analysis request.")?;
        self.offset = end;
        Ok(value)
    }
    fn number(&mut self) -> Result<u32, String> {
        let value = self.take(4)?;
        Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
    }
}

fn canonical_source(written: &str) -> Result<PathBuf, String> {
    let path = Path::new(written);
    if written.contains('\0')
        || !path.is_absolute()
        || written
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(
            "Overlay paths must be absolute source paths without '.' or '..' components.".into(),
        );
    }
    match fs::canonicalize(path) {
        Ok(canonical) => {
            if !canonical.is_file() {
                return Err("Overlay target is not a regular source file.".into());
            }
            Ok(canonical)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or("Overlay target has no parent directory.")?;
            let parent = fs::canonicalize(parent)
                .map_err(|_| "An unsaved source requires an existing parent directory.")?;
            if !parent.is_dir() {
                return Err("Overlay parent is not a directory.".into());
            }
            let name = path.file_name().ok_or("Overlay target has no file name.")?;
            Ok(parent.join(name))
        }
        Err(error) => Err(format!("Cannot resolve overlay path: {error}")),
    }
}

pub fn capabilities() -> Result<String, String> {
    json(object(vec![
        ("protocol", text("abstract-analysis")),
        ("version", number(1)),
        ("compiler", text(COMPILER_VERSION)),
        ("positionEncoding", text("utf-16")),
        ("schemaBindings", number(1)),
        ("maxRequestBytes", number(MAX_REQUEST_BYTES)),
        ("maxTextBytes", number(MAX_TEXT_BYTES)),
        ("maxPathBytes", number(MAX_PATH_BYTES)),
        ("maxOverlays", number(MAX_OVERLAYS)),
        ("maxResponseBytes", number(MAX_RESPONSE_BYTES)),
    ]))
}

/// Runs the same discovery, parsing, schema, version, logic and asset phases as
/// `lint`. A syntactically valid request returns protocol JSON even when source
/// diagnostics exist; invalid transport/overlay contracts return an error.
pub fn analyze(root: &Path, request: Request) -> Result<String, String> {
    analyze_mode(root, request, false)
}

/// Optional complete schema bindings, requiring a valid compilation snapshot.
pub fn analyze_symbols(root: &Path, request: Request) -> Result<String, String> {
    analyze_mode(root, request, true)
}

fn analyze_mode(root: &Path, request: Request, bindings: bool) -> Result<String, String> {
    if !root.is_dir() {
        return Err("Analysis requires an existing project or data directory.".into());
    }
    let layout = match project::resolve_with_overlays(&[root.to_path_buf()], &request.overlays) {
        Ok(layout) => layout,
        Err(diagnostics) => {
            return response(
                request.id,
                &diagnostics,
                &[],
                &request.overlays,
                false,
                bindings.then(|| symbols::unavailable("Project discovery failed.")),
            )
        }
    };
    let origins: HashSet<PathBuf> = layout
        .sources
        .iter()
        .filter_map(SourceFile::origin)
        .map(Path::to_path_buf)
        .collect();
    if request
        .overlays
        .keys()
        .any(|origin| !origins.contains(origin))
    {
        return Err("An overlay does not belong to this project's discovered source set or an eligible new source.".into());
    }
    let diagnostics = compile_layout(&layout, CompileOptions::default())
        .err()
        .unwrap_or_default();
    response(
        request.id,
        &diagnostics,
        &layout.sources,
        &request.overlays,
        true,
        bindings.then(|| {
            if diagnostics.is_empty() {
                symbols::collect(&layout.sources, &request.overlays)
            } else {
                symbols::unavailable(
                    "Fix compiler diagnostics before requesting schema references or rename.",
                )
            }
        }),
    )
}

fn response(
    id: u32,
    diagnostics: &Diagnostics,
    sources: &[SourceFile],
    overlays: &HashMap<PathBuf, String>,
    analyzed: bool,
    bindings: Option<Value>,
) -> Result<String, String> {
    let mut truncated = diagnostics.len() > MAX_DIAGNOSTICS;
    let mut entries = Vec::new();
    let mut bytes = 0;
    for diagnostic in diagnostics.iter().take(MAX_DIAGNOSTICS) {
        let entry = diagnostic_value(diagnostic, sources, overlays, &mut truncated);
        let size = json(entry.clone())?.len();
        // Leave room for the response envelope and renderer whitespace.
        if bytes + size > MAX_RESPONSE_BYTES - 8192 {
            truncated = true;
            break;
        }
        bytes += size;
        entries.push(entry);
    }
    let mut fields = vec![
        ("protocol", text("abstract-analysis")),
        ("version", number(1)),
        ("requestId", number(id as usize)),
        ("compiler", text(COMPILER_VERSION)),
        ("positionEncoding", text("utf-16")),
        ("analyzed", Value::Bool(analyzed)),
        ("truncated", Value::Bool(truncated)),
        ("diagnostics", Value::List(entries)),
    ];
    if let Some(bindings) = bindings {
        fields.push(("bindings", bindings));
    }
    let rendered = json(object(fields))?;
    if rendered.len() > MAX_RESPONSE_BYTES {
        return Err("Analysis response exceeds protocol limit.".into());
    }
    Ok(rendered)
}

fn diagnostic_value(
    diagnostic: &Diagnostic,
    sources: &[SourceFile],
    overlays: &HashMap<PathBuf, String>,
    truncated: &mut bool,
) -> Value {
    let mut fields = vec![
        ("code", text(diagnostic.id.code())),
        ("severity", text("error")),
        (
            "message",
            text(shorten(&diagnostic.message, 16384, truncated)),
        ),
    ];
    fields.extend(location(
        diagnostic.file.as_deref(),
        diagnostic.position,
        sources,
        overlays,
    ));
    if diagnostic.notes.len() > 8 {
        *truncated = true;
    }
    let notes = diagnostic
        .notes
        .iter()
        .take(8)
        .map(|note| {
            let mut fields = vec![("message", text(shorten(&note.text, 4096, truncated)))];
            fields.extend(location(
                note.file.as_deref(),
                note.position,
                sources,
                overlays,
            ));
            object(fields)
        })
        .collect();
    fields.push(("notes", Value::List(notes)));
    object(fields)
}

fn location(
    file: Option<&str>,
    position: Option<Position>,
    sources: &[SourceFile],
    overlays: &HashMap<PathBuf, String>,
) -> Vec<(&'static str, Value)> {
    let Some(file) = file else {
        return Vec::new();
    };
    let mut fields = vec![("displayPath", text(file))];
    if let Some(source) = sources.iter().find(|source| source.path == file) {
        if let Some(origin) = source.origin() {
            let canonical = fs::canonicalize(origin).unwrap_or_else(|_| origin.to_path_buf());
            fields.push((
                "path",
                text(normalise_display_path(&canonical.to_string_lossy())),
            ));
        }
        if let Some(position) = position {
            // Disk BOMs are encoding metadata removed by editor decoders.
            // An overlay is exact TextDocument text: a typed BOM is a real
            // code unit in that buffer and must participate in its ranges.
            let preserve_bom = source
                .origin()
                .is_some_and(|origin| overlays.contains_key(origin));
            let (line, start, end) = utf16_range(source, position, preserve_bom);
            fields.push((
                "range",
                object(vec![
                    (
                        "start",
                        object(vec![("line", number(line)), ("character", number(start))]),
                    ),
                    (
                        "end",
                        object(vec![("line", number(line)), ("character", number(end))]),
                    ),
                ]),
            ));
        }
    }
    fields
}

fn utf16_range(
    source: &SourceFile,
    position: Position,
    preserve_bom: bool,
) -> (usize, usize, usize) {
    let line = position.line.max(1).min(source.line_count());
    let text = source.line_text(line).unwrap_or("");
    let mut chars = text.chars();
    let start = chars
        .by_ref()
        .take(position.col.saturating_sub(1) as usize)
        .map(char::len_utf16)
        .sum::<usize>()
        + usize::from(preserve_bom && line == 1 && source.has_leading_bom());
    (
        line as usize - 1,
        start,
        start + chars.next().map(char::len_utf16).unwrap_or(0),
    )
}

fn shorten<'a>(value: &'a str, max: usize, truncated: &mut bool) -> &'a str {
    if value.len() <= max {
        return value;
    }
    *truncated = true;
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}
fn object(fields: Vec<(&str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}
fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}
fn number(value: usize) -> Value {
    Value::Int(value as i64)
}
fn json(value: Value) -> Result<String, String> {
    output::render(&value, Format::Json).map_err(|_| "Cannot render analysis response.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_frames_never_panic_and_never_accept_trailing_data() {
        let mut valid = MAGIC.to_vec();
        valid.extend_from_slice(&5_u32.to_be_bytes());
        valid.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(decode_request(&valid).unwrap().id, 5);
        for end in 0..valid.len() {
            assert!(decode_request(&valid[..end]).is_err());
        }
        valid.push(0);
        assert!(decode_request(&valid).is_err());
        valid[0] = b'X';
        assert!(decode_request(&valid).is_err());
    }

    #[test]
    fn frame_count_and_declared_lengths_are_bounded_before_allocation() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&129_u32.to_be_bytes());
        assert!(decode_request(&bytes).unwrap_err().contains("128"));
        bytes[12..16].copy_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&u32::MAX.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        assert!(decode_request(&bytes).unwrap_err().contains("limits"));
        assert!(read_request(std::io::repeat(0))
            .unwrap_err()
            .contains("16 MiB"));
    }

    #[test]
    fn unicode_ranges_use_utf16_and_preserve_bom_crlf_and_end_of_line() {
        let source = SourceFile::new("data/a.ab", "\u{feff}a😀b\r\nx\n");
        assert_eq!(utf16_range(&source, Position::new(1, 2), true), (0, 2, 4));
        assert_eq!(utf16_range(&source, Position::new(1, 2), false), (0, 1, 3));
        assert_eq!(utf16_range(&source, Position::new(1, 3), true), (0, 4, 5));
        assert_eq!(utf16_range(&source, Position::new(2, 2), true), (1, 1, 1));
        assert_eq!(
            utf16_range(&source, Position::new(999, 999), true),
            (2, 0, 0)
        );
    }
}
