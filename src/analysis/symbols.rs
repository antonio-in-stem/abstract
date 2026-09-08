//! Compiler-owned schema identities. Validated grammar and the project table,
//! never spelling search, decide which tokens refer to a schema declaration.
use super::*;
use crate::lexer::{self, TokenKind};

const MAX_SOURCES: usize = 1024;
const MAX_OCCURRENCES: usize = 16384;

pub(super) fn unavailable(reason: &str) -> Value {
    object(vec![
        ("version", number(1)),
        ("complete", Value::Bool(false)),
        ("reason", text(reason)),
        ("sources", Value::List(vec![])),
        ("symbols", Value::List(vec![])),
    ])
}

pub(super) fn collect(sources: &[SourceFile], overlays: &HashMap<PathBuf, String>) -> Value {
    collect_checked(sources, overlays).unwrap_or_else(|reason| unavailable(&reason))
}

fn collect_checked(
    sources: &[SourceFile],
    overlays: &HashMap<PathBuf, String>,
) -> Result<Value, String> {
    if sources.len() > MAX_SOURCES {
        return Err("Schema bindings exceed 1024 sources.".into());
    }
    let (templates, _) = crate::parse_sources(sources).map_err(|_| "Sources do not parse.")?;
    let tables =
        crate::schema::build_tables(&templates).map_err(|_| "Schema identities are ambiguous.")?;
    let mut declarations = vec![None; tables.schemas.len()];
    let mut occurrences = vec![Vec::new(); tables.schemas.len()];
    let mut files = Vec::new();
    let mut count = 0;
    for source in sources {
        let written_origin = source
            .origin()
            .ok_or("Source has no filesystem identity.")?;
        // Saved sources keep the discovered alias in SourceFile::origin();
        // overlays already carry canonical paths. Both must expose one identity.
        let origin =
            fs::canonicalize(written_origin).unwrap_or_else(|_| written_origin.to_path_buf());
        let path = normalise_display_path(&origin.to_string_lossy());
        let preserve_bom = overlays.contains_key(&origin) && source.has_leading_bom();
        let editor_text = if preserve_bom {
            format!("\u{feff}{}", source.text)
        } else {
            source.text.clone()
        };
        let hash = crate::crypto::sha256(editor_text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        files.push(object(vec![("path", text(&path)), ("sha256", text(hash))]));
        let tokens = lexer::tokenize(source).map_err(|_| "Source cannot be tokenized.")?;
        for (index, token) in tokens.iter().enumerate() {
            let TokenKind::SchemaName(name) = &token.kind else {
                continue;
            };
            let schema_index = tables
                .schemas
                .iter()
                .position(|s| &s.name == name)
                .ok_or("Schema reference has no declaration.")?;
            count += 1;
            if count > MAX_OCCURRENCES {
                return Err("Schema bindings exceed 16384 occurrences.".into());
            }
            let declaration = index > 0 && tokens[index - 1].is_keyword("schema");
            if declaration {
                let schema = &tables.schemas[schema_index];
                if schema.at.file != source.path
                    || schema.at.position != tokens[index - 1].position
                    || declarations[schema_index].is_some()
                {
                    return Err("Schema declaration location is ambiguous.".into());
                }
            }
            let start = source.position_at(token.span.0);
            let end = source.position_at(token.span.1);
            let (start_line, start_col, _) = utf16_range(source, start, preserve_bom);
            let (end_line, end_col, _) = utf16_range(source, end, preserve_bom);
            let range = object(vec![
                (
                    "start",
                    object(vec![
                        ("line", number(start_line)),
                        ("character", number(start_col)),
                    ]),
                ),
                (
                    "end",
                    object(vec![
                        ("line", number(end_line)),
                        ("character", number(end_col)),
                    ]),
                ),
            ]);
            let location = object(vec![("path", text(&path)), ("range", range.clone())]);
            if declaration {
                declarations[schema_index] = Some(location);
            }
            occurrences[schema_index].push(object(vec![
                ("path", text(&path)),
                ("range", range),
                (
                    "role",
                    text(if declaration {
                        "declaration"
                    } else {
                        "reference"
                    }),
                ),
            ]));
        }
    }
    let mut symbols = Vec::new();
    for (index, schema) in tables.schemas.iter().enumerate() {
        symbols.push(object(vec![
            ("id", text(format!("schema:{index}"))),
            ("name", text(&schema.name)),
            ("kind", text("schema")),
            (
                "declaration",
                declarations[index]
                    .take()
                    .ok_or("Missing schema declaration span.")?,
            ),
            (
                "occurrences",
                Value::List(std::mem::take(&mut occurrences[index])),
            ),
        ]));
    }
    let value = object(vec![
        ("version", number(1)),
        ("complete", Value::Bool(true)),
        ("sources", Value::List(files)),
        ("symbols", Value::List(symbols)),
    ]);
    if json(value.clone())?.len() > MAX_RESPONSE_BYTES - 32768 {
        return Err("Schema bindings exceed response budget.".into());
    }
    Ok(value)
}
