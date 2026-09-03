//! Abstract language compiler core.
//!
//! Abstract is a compact, schema-backed language for authoring validated
//! data. `.abt` files declare templates (schemas plus logic) and `.ab` files
//! declare instances. The compiler validates every instance against its
//! template and emits canonical RAW, JSON, or YAML data.
//!
//! The crate is dependency-free by design. Three companion modules extend
//! the core: [`media`] probes image headers without decoding pixels,
//! [`crypto`] implements the RFC 8439 AEAD used by bundles, and [`bundle`]
//! seals compiled data into tamper-evident `.abx` containers.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub mod bundle;
pub mod crypto;
pub mod media;

use media::{probe_image, ImageFormat};

// ---------------------------------------------------------------------------
// Public API types
// ---------------------------------------------------------------------------

/// Compiler behavior switches.
#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// Accept instance fields that are not declared in the schema instead of
    /// failing. Strict mode (the default) catches typos early and is the
    /// recommended setting for real projects.
    pub allow_unknown_fields: bool,
    /// Skip on-disk asset checks (`file(...)` existence, `image(...)` header
    /// probing, and file-path `exists` conditions). Useful when authoring
    /// data on a machine that does not have the binary assets checked out.
    pub skip_asset_checks: bool,
}

/// One in-memory source file handed to the compiler.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
    project_dir: Option<PathBuf>,
}

impl SourceFile {
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
            project_dir: None,
        }
    }
}

/// A compilation failure with a human-oriented message. Messages include the
/// source path, and the line number whenever the failing construct has one.
#[derive(Clone, Debug)]
pub struct CompileError {
    message: String,
}

impl CompileError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn with_path(path: &str, message: impl Into<String>) -> Self {
        Self::new(format!("{}: {}", path, message.into()))
    }

    pub fn at(path: &str, line: usize, message: impl Into<String>) -> Self {
        Self::new(format!("{}:{}: {}", path, line, message.into()))
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompileError {}

/// The result of compiling a project: one item per resolved instance.
#[derive(Clone, Debug)]
pub struct CompiledProject {
    pub items: Vec<RawValue>,
}

impl CompiledProject {
    pub fn to_raw_string(&self) -> String {
        let mut output = String::from("data: [\n");
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                output.push_str(",\n");
            }
            output.push_str(&format_raw_value_multiline(item, 1));
        }
        output.push_str("\n]\n");
        output
    }

    pub fn to_json_string(&self) -> String {
        let root = RawValue::Object(vec![(
            "data".to_string(),
            RawValue::Array(self.items.clone()),
        )]);
        let mut output = format_json_value(&root, 0);
        output.push('\n');
        output
    }

    pub fn to_yaml_string(&self) -> String {
        let root = RawValue::Object(vec![(
            "data".to_string(),
            RawValue::Array(self.items.clone()),
        )]);
        format_yaml_document(&root)
    }
}

/// A compiled value. Numbers keep their authored precision: integers stay
/// `Int`, decimal literals become `Float`, and `true`/`false` become `Bool`,
/// so JSON and YAML consumers receive native types instead of strings.
#[derive(Clone, Debug, PartialEq)]
pub enum RawValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<RawValue>),
    Object(Vec<(String, RawValue)>),
}

impl RawValue {
    fn object() -> Self {
        Self::Object(Vec::new())
    }

    fn as_object_mut(&mut self) -> Option<&mut Vec<(String, RawValue)>> {
        match self {
            Self::Object(fields) => Some(fields),
            _ => None,
        }
    }

    fn as_array_mut(&mut self) -> Option<&mut Vec<RawValue>> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "text",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Bool(_) => "bool",
            Self::Array(_) => "list",
            Self::Object(_) => "object",
        }
    }
}

// ---------------------------------------------------------------------------
// Compilation entry points
// ---------------------------------------------------------------------------

pub fn compile_project(
    path: &Path,
    options: CompileOptions,
) -> Result<CompiledProject, CompileError> {
    compile_paths(&[path.to_path_buf()], options)
}

pub fn compile_paths(
    paths: &[PathBuf],
    options: CompileOptions,
) -> Result<CompiledProject, CompileError> {
    let mut sources = Vec::new();
    let mut requested_ids = BTreeSet::<String>::new();
    let mut loaded_roots = HashSet::<PathBuf>::new();

    for path in paths {
        if path.is_file() {
            let root = source_parent(path);
            if path.extension().and_then(|value| value.to_str()) == Some("ab") {
                for instance in parse_instance_source(&read_source(path, root)?)? {
                    requested_ids.insert(instance_id(&instance));
                }
            }
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if loaded_roots.insert(canonical_root) {
                collect_sources(root, root, &mut sources)?;
            }
        } else {
            collect_sources(path, path, &mut sources)?;
        }
    }
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    if !requested_ids.is_empty() {
        return compile_sources_for_ids(sources, options, Some(&requested_ids));
    }
    compile_sources_for_ids(sources, options, None)
}

fn source_parent(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

pub fn compile_sources(
    sources: Vec<SourceFile>,
    options: CompileOptions,
) -> Result<CompiledProject, CompileError> {
    compile_sources_for_ids(sources, options, None)
}

fn compile_sources_for_ids(
    sources: Vec<SourceFile>,
    options: CompileOptions,
    requested_ids: Option<&BTreeSet<String>>,
) -> Result<CompiledProject, CompileError> {
    let mut schemas = HashMap::<String, Schema>::new();
    let mut logic_blocks = HashMap::<String, Vec<LogicStatement>>::new();
    let mut instances = Vec::<InstanceFile>::new();

    for source in sources {
        if source.path.ends_with(".abt") {
            for schema in parse_template_source(&source)? {
                if let Some(existing) = schemas.get(&schema.name) {
                    return Err(CompileError::with_path(
                        &source.path,
                        format!(
                            "Duplicate schema '{}'. It is already defined in '{}'; schema names must be unique across the project.",
                            schema.name, existing.defined_in
                        ),
                    ));
                }
                schemas.insert(schema.name.clone(), schema);
            }
            for block in parse_logic_source(&source)? {
                logic_blocks
                    .entry(block.schema)
                    .or_default()
                    .extend(block.statements);
            }
        } else if source.path.ends_with(".ab") {
            instances.extend(parse_instance_source(&source)?);
        }
    }

    let mut parsed_by_id = HashMap::<String, InstanceFile>::new();
    let mut order = Vec::<String>::new();
    for instance in instances {
        if !schemas.contains_key(&instance.template) {
            return Err(CompileError::at(
                &instance.path,
                instance.header_line,
                format!(
                    "Unknown template '{}'. Add a schema named '{}' in a .abt file, or change this instance header to an existing schema.",
                    instance.template, instance.template
                ),
            ));
        }
        let id = instance_id(&instance);
        if let Some(existing) = parsed_by_id.get(&id) {
            return Err(CompileError::at(
                &instance.path,
                instance.header_line,
                format!(
                    "Duplicate instance id '{}'. It is already declared in '{}' (line {}); every instance needs a unique id.",
                    id, existing.path, existing.header_line
                ),
            ));
        }
        order.push(id.clone());
        parsed_by_id.insert(id, instance);
    }

    let mut compiled_by_id = HashMap::<String, RawValue>::new();
    let mut resolving = HashSet::<String>::new();
    let mut items = Vec::new();

    let output_order = if let Some(requested_ids) = requested_ids {
        order
            .into_iter()
            .filter(|id| requested_ids.contains(id))
            .collect::<Vec<_>>()
    } else {
        order
    };

    for id in output_order {
        let item = resolve_instance(
            &id,
            &parsed_by_id,
            &schemas,
            &logic_blocks,
            &options,
            &mut compiled_by_id,
            &mut resolving,
        )?;
        items.push(item);
    }

    Ok(CompiledProject { items })
}

fn read_source(path: &Path, root: &Path) -> Result<SourceFile, CompileError> {
    let text = fs::read_to_string(path).map_err(|error| {
        let cwd = std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        CompileError::new(format!(
            "Failed to read source file '{}'.\n  Current directory: {}\n  Resolved path: {}\n  OS error: {error}\n  Fix: check the filename, or pass a full/relative path to the .ab/.abt file.",
            path.display(),
            cwd,
            std::env::current_dir()
                .map(|cwd| cwd.join(path).display().to_string())
                .unwrap_or_else(|_| path.display().to_string())
        ))
    })?;
    let relative = path.strip_prefix(root).unwrap_or(path);
    Ok(SourceFile {
        path: normalize_path(relative),
        text,
        project_dir: Some(project_dir_for_root(root)),
    })
}

fn project_dir_for_root(root: &Path) -> PathBuf {
    let resolved = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if resolved.file_name().and_then(|name| name.to_str()) == Some("data") {
        resolved.parent().unwrap_or(&resolved).to_path_buf()
    } else {
        resolved
    }
}

fn collect_sources(
    root: &Path,
    current: &Path,
    output: &mut Vec<SourceFile>,
) -> Result<(), CompileError> {
    for entry in fs::read_dir(current).map_err(|error| {
        CompileError::new(format!("failed to read {}: {error}", current.display()))
    })? {
        let entry = entry.map_err(|error| {
            CompileError::new(format!("failed to read directory entry: {error}"))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_sources(root, &path, output)?;
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension == "ab" || extension == "abt" {
            output.push(read_source(&path, root)?);
        }
    }
    output.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// String-aware scanning primitives
// ---------------------------------------------------------------------------

/// Tracks whether a scan position is inside a double-quoted string, honoring
/// backslash escapes such as `\"` and `\\`.
#[derive(Clone, Copy, Default)]
struct StringScanner {
    in_string: bool,
    escaped: bool,
}

impl StringScanner {
    /// Advances over `ch`; returns `true` when `ch` belongs to string content
    /// (including the delimiting quotes), meaning structural characters such
    /// as braces and commas must be ignored at this position.
    fn step(&mut self, ch: char) -> bool {
        if self.in_string {
            if self.escaped {
                self.escaped = false;
            } else if ch == '\\' {
                self.escaped = true;
            } else if ch == '"' {
                self.in_string = false;
            }
            true
        } else if ch == '"' {
            self.in_string = true;
            true
        } else {
            false
        }
    }
}

/// Removes a `//` comment from a line. Comments start at the beginning of a
/// line or after whitespace, so protocol strings such as `https://...` in
/// unquoted values survive. Quoted strings are never scanned for comments.
fn strip_comment(line: &str) -> String {
    let mut scanner = StringScanner::default();
    let mut previous: Option<char> = None;
    for (index, ch) in line.char_indices() {
        let in_string = scanner.step(ch);
        if !in_string
            && ch == '/'
            && line[index..].starts_with("//")
            && previous.is_none_or(|prev| prev.is_whitespace())
        {
            return line[..index].to_string();
        }
        previous = Some(ch);
    }
    line.to_string()
}

/// A cleaned source line that remembers where it came from.
#[derive(Clone, Debug)]
struct Line {
    number: usize,
    text: String,
}

fn cleaned_lines(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .map(|(index, line)| Line {
            number: index + 1,
            text: strip_comment(line).trim().to_string(),
        })
        .filter(|line| !line.text.is_empty())
        .collect()
}

/// Net brace depth change of a line, ignoring braces inside strings.
fn brace_delta(text: &str) -> isize {
    let mut scanner = StringScanner::default();
    let mut delta = 0isize;
    for ch in text.chars() {
        if scanner.step(ch) {
            continue;
        }
        match ch {
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn skip_block(lines: &[Line], mut index: usize) -> usize {
    let mut depth = brace_delta(&lines[index].text);
    index += 1;
    while index < lines.len() && depth > 0 {
        depth += brace_delta(&lines[index].text);
        index += 1;
    }
    index
}

/// Splits `text` on `delimiter` at nesting depth zero, ignoring delimiters
/// inside parentheses, braces, brackets, and strings.
fn split_top_level(text: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren = 0isize;
    let mut brace = 0isize;
    let mut bracket = 0isize;
    let mut scanner = StringScanner::default();
    for ch in text.chars() {
        let in_string = scanner.step(ch);
        if !in_string {
            match ch {
                '(' => paren += 1,
                ')' => paren = (paren - 1).max(0),
                '{' => brace += 1,
                '}' => brace = (brace - 1).max(0),
                '[' => bracket += 1,
                ']' => bracket = (bracket - 1).max(0),
                _ => {}
            }
            if ch == delimiter && paren == 0 && brace == 0 && bracket == 0 {
                parts.push(current.trim().to_string());
                current.clear();
                continue;
            }
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Splits at the first top-level occurrence of `needle` outside strings.
fn split_once_top_level<'a>(text: &'a str, needle: char) -> Option<(&'a str, &'a str)> {
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        if ch == needle {
            return Some((&text[..index], &text[index + ch.len_utf8()..]));
        }
    }
    None
}

fn normalize_identifier(text: impl AsRef<str>) -> String {
    let text = text.as_ref().trim().trim_matches('"').trim();
    text.to_ascii_lowercase().replace('-', "_")
}

/// Compatibility rename applied to output field names.
fn output_field_name(name: &str) -> String {
    match name {
        "lang" => "lang_values".to_string(),
        other => other.to_string(),
    }
}

/// Finds the closest declared name for a typo suggestion (edit distance <= 2).
fn closest_name<'a>(needle: &str, haystack: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for candidate in haystack {
        let distance = edit_distance(needle, candidate);
        if distance <= 2 && best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, name)| name.to_string())
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (row, left_ch) in left.iter().enumerate() {
        let mut current = vec![row + 1];
        for (column, right_ch) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_ch != right_ch);
            current.push(
                substitution
                    .min(previous[column + 1] + 1)
                    .min(current[column] + 1),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

// ---------------------------------------------------------------------------
// Schema model and template parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Schema {
    name: String,
    defined_in: String,
    fields: Vec<FieldSpec>,
}

#[derive(Clone, Debug)]
struct FieldSpec {
    name: String,
    list: bool,
    optional: bool,
    tag: bool,
    default: Option<RawValue>,
    ty: TypeSpec,
}

#[derive(Clone, Debug)]
enum TypeSpec {
    Text(Vec<RangeSpec>),
    Int(Vec<RangeSpec>),
    Float(Vec<FloatRangeSpec>),
    Bool,
    Enum(Vec<String>),
    File(Vec<String>),
    Image(Vec<ImageSpec>),
    Ref(String),
    Group(Vec<FieldSpec>),
}

#[derive(Clone, Debug)]
struct RangeSpec {
    min: i64,
    max: i64,
}

#[derive(Clone, Debug)]
struct FloatRangeSpec {
    min: f64,
    max: f64,
}

/// One `image(...)` alternative: a format plus optional exact dimensions.
/// `None` in a dimension means "any size" (written `*`).
#[derive(Clone, Debug)]
struct ImageSpec {
    format: ImageFormat,
    width: Option<u32>,
    height: Option<u32>,
}

impl ImageSpec {
    fn describe(&self) -> String {
        match (self.width, self.height) {
            (None, None) => self.format.name().to_string(),
            (width, height) => format!(
                "{} {}x{}",
                self.format.name(),
                width.map_or("*".to_string(), |value| value.to_string()),
                height.map_or("*".to_string(), |value| value.to_string())
            ),
        }
    }
}

fn parse_template_source(source: &SourceFile) -> Result<Vec<Schema>, CompileError> {
    let lines = cleaned_lines(&source.text);
    let mut schemas = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].text.as_str();
        if line.starts_with("logic ") {
            index = skip_block(&lines, index);
            continue;
        }
        if !line.starts_with("schema ") {
            index += 1;
            continue;
        }

        let number = lines[index].number;
        let name = line
            .trim_start_matches("schema ")
            .split('{')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            return Err(CompileError::at(
                &source.path,
                number,
                "schema is missing a name",
            ));
        }
        index += 1;
        let fields = parse_schema_fields(&source.path, &lines, &mut index)?;
        schemas.push(Schema {
            name,
            defined_in: source.path.clone(),
            fields,
        });
    }
    Ok(schemas)
}

fn parse_schema_fields(
    path: &str,
    lines: &[Line],
    index: &mut usize,
) -> Result<Vec<FieldSpec>, CompileError> {
    let mut fields: Vec<FieldSpec> = Vec::new();
    while *index < lines.len() {
        let number = lines[*index].number;
        let line = lines[*index].text.as_str();
        if line == "}" {
            *index += 1;
            return Ok(fields);
        }
        let field = if line.ends_with('{') {
            let header = line.trim_end_matches('{').trim();
            let (name, list, optional, tag) = parse_field_header(path, number, header)?;
            *index += 1;
            let children = parse_schema_fields(path, lines, index)?;
            FieldSpec {
                name,
                list,
                optional,
                tag,
                default: None,
                ty: TypeSpec::Group(children),
            }
        } else {
            let Some((left, right)) = line.split_once(':') else {
                return Err(CompileError::at(
                    path,
                    number,
                    format!("invalid schema line '{line}' (expected 'name: type' or 'name {{')"),
                ));
            };
            let (name, list, optional_from_left, tag_from_left) =
                parse_field_header(path, number, left)?;
            let (type_part, default, optional_from_right, tag_from_right) =
                parse_schema_type_tail(right);
            let field = FieldSpec {
                name,
                list,
                optional: optional_from_left || optional_from_right,
                tag: tag_from_left || tag_from_right,
                default,
                ty: parse_type_spec(path, number, &type_part)?,
            };
            *index += 1;
            field
        };
        if fields.iter().any(|existing| existing.name == field.name) {
            return Err(CompileError::at(
                path,
                number,
                format!("duplicate field '{}' in schema block", field.name),
            ));
        }
        fields.push(field);
    }
    Err(CompileError::with_path(path, "schema block was not closed"))
}

fn parse_field_header(
    path: &str,
    line: usize,
    header: &str,
) -> Result<(String, bool, bool, bool), CompileError> {
    let mut optional = false;
    let mut tag = false;
    let mut name: Option<String> = None;
    let mut list = false;
    for token in header.split_whitespace() {
        match token {
            "@optional" => optional = true,
            "@tag" => tag = true,
            other if other.starts_with('@') => {
                return Err(CompileError::at(
                    path,
                    line,
                    format!("unknown field modifier '{other}' (expected @optional or @tag)"),
                ));
            }
            other => {
                if name.is_some() {
                    return Err(CompileError::at(
                        path,
                        line,
                        format!("unexpected token '{other}' in field declaration"),
                    ));
                }
                let mut candidate = other.trim().to_string();
                if candidate.ends_with("[]") {
                    list = true;
                    candidate.truncate(candidate.len() - 2);
                }
                name = Some(normalize_identifier(&candidate));
            }
        }
    }
    let Some(name) = name else {
        return Err(CompileError::at(path, line, "field is missing a name"));
    };
    if name.is_empty() {
        return Err(CompileError::at(path, line, "field is missing a name"));
    }
    Ok((name, list, optional, tag))
}

/// Splits a schema type tail into the type expression, an optional default
/// value, and trailing modifiers. String content is preserved verbatim.
fn parse_schema_type_tail(tail: &str) -> (String, Option<RawValue>, bool, bool) {
    let mut remaining = tail.trim().to_string();
    let mut optional = false;
    let mut tag = false;
    for modifier in ["@optional", "@tag"] {
        if let Some(found) = find_top_level_token(&remaining, modifier) {
            remaining.replace_range(found..found + modifier.len(), "");
            if modifier == "@optional" {
                optional = true;
            } else {
                tag = true;
            }
        }
    }
    let remaining = remaining.trim();
    if let Some((type_part, default_part)) = split_once_top_level(remaining, '=') {
        (
            type_part.trim().to_string(),
            Some(parse_scalar(default_part.trim())),
            optional,
            tag,
        )
    } else {
        (remaining.to_string(), None, optional, tag)
    }
}

/// Finds a whitespace-delimited token outside strings; returns its offset.
fn find_top_level_token(text: &str, token: &str) -> Option<usize> {
    let mut scanner = StringScanner::default();
    let mut previous: Option<char> = None;
    for (index, ch) in text.char_indices() {
        let in_string = scanner.step(ch);
        if !in_string
            && text[index..].starts_with(token)
            && previous.is_none_or(|prev| prev.is_whitespace())
        {
            let after = index + token.len();
            if text[after..]
                .chars()
                .next()
                .is_none_or(|next| next.is_whitespace())
            {
                return Some(index);
            }
        }
        previous = Some(ch);
    }
    None
}

fn parse_type_spec(path: &str, line: usize, text: &str) -> Result<TypeSpec, CompileError> {
    let text = text.trim();
    match text {
        "text" => return Ok(TypeSpec::Text(Vec::new())),
        "int" => return Ok(TypeSpec::Int(Vec::new())),
        "float" => return Ok(TypeSpec::Float(Vec::new())),
        "bool" => return Ok(TypeSpec::Bool),
        _ => {}
    }
    if let Some(inner) = call_inner(text, "text") {
        return Ok(TypeSpec::Text(parse_ranges(path, line, inner)?));
    }
    if let Some(inner) = call_inner(text, "int") {
        return Ok(TypeSpec::Int(parse_ranges(path, line, inner)?));
    }
    if let Some(inner) = call_inner(text, "float") {
        return Ok(TypeSpec::Float(parse_float_ranges(path, line, inner)?));
    }
    if let Some(inner) = call_inner(text, "enum") {
        let values: Vec<String> = split_top_level(inner, ',')
            .into_iter()
            .map(normalize_identifier)
            .filter(|value| !value.is_empty())
            .collect();
        if values.is_empty() {
            return Err(CompileError::at(
                path,
                line,
                "enum(...) needs at least one allowed value",
            ));
        }
        return Ok(TypeSpec::Enum(values));
    }
    if let Some(inner) = call_inner(text, "file") {
        let extensions: Vec<String> = split_top_level(inner, ',')
            .into_iter()
            .map(|item| item.trim().trim_start_matches('.').to_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        if extensions.is_empty() {
            return Err(CompileError::at(
                path,
                line,
                "file(...) needs at least one allowed extension",
            ));
        }
        return Ok(TypeSpec::File(extensions));
    }
    if let Some(inner) = call_inner(text, "image") {
        return Ok(TypeSpec::Image(parse_image_specs(path, line, inner)?));
    }
    if text.starts_with("$(") && text.ends_with(')') {
        return Ok(TypeSpec::Ref(text[2..text.len() - 1].trim().to_string()));
    }
    Err(CompileError::at(
        path,
        line,
        format!(
            "unsupported schema type '{text}' (expected text, int, float, bool, enum(...), file(...), image(...), or $(Schema))"
        ),
    ))
}

fn call_inner<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}(");
    text.strip_prefix(&prefix)?.strip_suffix(')')
}

fn parse_ranges(path: &str, line: usize, text: &str) -> Result<Vec<RangeSpec>, CompileError> {
    split_top_level(text, ',')
        .into_iter()
        .map(|part| {
            let part = part.trim();
            if let Some((min, max)) = part.split_once("..") {
                Ok(RangeSpec {
                    min: min.trim().parse().map_err(|_| {
                        CompileError::at(path, line, format!("invalid range '{part}'"))
                    })?,
                    max: max.trim().parse().map_err(|_| {
                        CompileError::at(path, line, format!("invalid range '{part}'"))
                    })?,
                })
            } else {
                let value = part.parse().map_err(|_| {
                    CompileError::at(path, line, format!("invalid range value '{part}'"))
                })?;
                Ok(RangeSpec {
                    min: value,
                    max: value,
                })
            }
        })
        .collect()
}

fn parse_float_ranges(
    path: &str,
    line: usize,
    text: &str,
) -> Result<Vec<FloatRangeSpec>, CompileError> {
    split_top_level(text, ',')
        .into_iter()
        .map(|part| {
            let part = part.trim();
            let parse = |value: &str| -> Result<f64, CompileError> {
                let parsed: f64 = value.trim().parse().map_err(|_| {
                    CompileError::at(path, line, format!("invalid float range '{part}'"))
                })?;
                if !parsed.is_finite() {
                    return Err(CompileError::at(
                        path,
                        line,
                        format!("invalid float range '{part}'"),
                    ));
                }
                Ok(parsed)
            };
            if let Some((min, max)) = part.split_once("..") {
                Ok(FloatRangeSpec {
                    min: parse(min)?,
                    max: parse(max)?,
                })
            } else {
                let value = parse(part)?;
                Ok(FloatRangeSpec {
                    min: value,
                    max: value,
                })
            }
        })
        .collect()
}

fn parse_image_specs(
    path: &str,
    line: usize,
    text: &str,
) -> Result<Vec<ImageSpec>, CompileError> {
    let mut specs = Vec::new();
    for piece in split_top_level(text, ',') {
        let mut parts = piece.split_whitespace();
        let Some(extension) = parts.next() else {
            continue;
        };
        let extension = extension.trim_start_matches('.');
        let Some(format) = ImageFormat::from_extension(extension) else {
            return Err(CompileError::at(
                path,
                line,
                format!(
                    "unsupported image format '{extension}' (expected png, jpg, gif, bmp, or webp)"
                ),
            ));
        };
        let mut spec = ImageSpec {
            format,
            width: None,
            height: None,
        };
        if let Some(dimensions) = parts.next() {
            let lowered = dimensions.to_ascii_lowercase();
            let Some((width, height)) = lowered.split_once('x') else {
                return Err(CompileError::at(
                    path,
                    line,
                    format!("invalid image dimensions '{dimensions}' (expected WIDTHxHEIGHT, for example 128x128 or 128x*)"),
                ));
            };
            spec.width = parse_image_dimension(path, line, width)?;
            spec.height = parse_image_dimension(path, line, height)?;
        }
        if let Some(extra) = parts.next() {
            return Err(CompileError::at(
                path,
                line,
                format!("unexpected token '{extra}' in image(...) specification"),
            ));
        }
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err(CompileError::at(
            path,
            line,
            "image(...) needs at least one format, for example image(png) or image(png 128x128)",
        ));
    }
    Ok(specs)
}

fn parse_image_dimension(
    path: &str,
    line: usize,
    text: &str,
) -> Result<Option<u32>, CompileError> {
    let text = text.trim();
    if text == "*" {
        return Ok(None);
    }
    text.parse::<u32>().map(Some).map_err(|_| {
        CompileError::at(
            path,
            line,
            format!("invalid image dimension '{text}' (expected a number or *)"),
        )
    })
}

// ---------------------------------------------------------------------------
// Logic model and parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct LogicBlock {
    schema: String,
    statements: Vec<LogicStatement>,
}

#[derive(Clone, Debug)]
enum LogicStatement {
    If {
        condition: String,
        then_branch: Vec<LogicStatement>,
        else_branch: Vec<LogicStatement>,
    },
    Derive {
        path: Vec<String>,
        value: RawValue,
        only_if_missing: bool,
    },
    Require {
        condition: String,
        message: String,
    },
    For {
        var: String,
        path: String,
        statements: Vec<LogicStatement>,
    },
}

fn parse_logic_source(source: &SourceFile) -> Result<Vec<LogicBlock>, CompileError> {
    let lines = split_close_else_lines(cleaned_lines(&source.text));
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].text.as_str();
        if !line.starts_with("logic ") {
            index += 1;
            continue;
        }
        let number = lines[index].number;
        let schema = line
            .trim_start_matches("logic ")
            .split('{')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if schema.is_empty() {
            return Err(CompileError::at(
                &source.path,
                number,
                "logic block is missing a schema name",
            ));
        }
        index += 1;
        let statements = parse_logic_statements(&source.path, &lines, &mut index)?;
        blocks.push(LogicBlock { schema, statements });
    }
    Ok(blocks)
}

/// Rewrites `} else ...` lines into a `}` line followed by the `else ...`
/// remainder, so both close-brace styles parse identically.
fn split_close_else_lines(lines: Vec<Line>) -> Vec<Line> {
    let mut output = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some(rest) = line.text.strip_prefix('}') {
            let rest = rest.trim();
            if rest.starts_with("else") {
                output.push(Line {
                    number: line.number,
                    text: "}".to_string(),
                });
                output.push(Line {
                    number: line.number,
                    text: rest.to_string(),
                });
                continue;
            }
        }
        output.push(line);
    }
    output
}

fn parse_logic_statements(
    path: &str,
    lines: &[Line],
    index: &mut usize,
) -> Result<Vec<LogicStatement>, CompileError> {
    let mut statements = Vec::new();
    while *index < lines.len() {
        let number = lines[*index].number;
        let line = lines[*index].text.clone();
        let line = line.as_str();
        if line == "}" {
            *index += 1;
            return Ok(statements);
        }
        if let Some(condition) = line
            .strip_prefix("if ")
            .and_then(|rest| rest.strip_suffix('{'))
        {
            *index += 1;
            statements.push(parse_if_chain(
                path,
                lines,
                index,
                condition.trim().to_string(),
            )?);
            continue;
        }
        if line.starts_with("for ") && line.ends_with('{') {
            let header = line.trim_start_matches("for ").trim_end_matches('{').trim();
            let Some((var, target)) = header.split_once(" in ") else {
                return Err(CompileError::at(
                    path,
                    number,
                    format!("invalid logic loop '{line}' (expected 'for item in .list {{')"),
                ));
            };
            *index += 1;
            statements.push(LogicStatement::For {
                var: normalize_identifier(var),
                path: target.trim().to_string(),
                statements: parse_logic_statements(path, lines, index)?,
            });
            continue;
        }
        if line.starts_with("require ") {
            let mut require_line = line.to_string();
            if !require_line.contains(" else throw ") && *index + 1 < lines.len() {
                let next = lines[*index + 1].text.as_str();
                if next.starts_with("else throw ") {
                    require_line.push(' ');
                    require_line.push_str(next);
                    *index += 1;
                }
            }
            statements.push(parse_logic_require(path, number, &require_line)?);
            *index += 1;
            continue;
        }
        if line.starts_with("derive? ") || line.starts_with("derive ") {
            statements.push(parse_logic_derive(path, number, line)?);
            *index += 1;
            continue;
        }
        return Err(CompileError::at(
            path,
            number,
            format!(
                "invalid logic statement '{line}' (expected if, for, require, derive, derive?, or '}}')"
            ),
        ));
    }
    Err(CompileError::with_path(path, "logic block was not closed"))
}

fn parse_if_chain(
    path: &str,
    lines: &[Line],
    index: &mut usize,
    condition: String,
) -> Result<LogicStatement, CompileError> {
    let then_branch = parse_logic_statements(path, lines, index)?;
    let mut else_branch = Vec::new();
    if *index < lines.len() {
        let number = lines[*index].number;
        let line = lines[*index].text.clone();
        if let Some(rest) = line.strip_prefix("else") {
            let rest = rest.trim();
            if let Some(nested) = rest
                .strip_prefix("if ")
                .and_then(|value| value.strip_suffix('{'))
            {
                *index += 1;
                else_branch.push(parse_if_chain(
                    path,
                    lines,
                    index,
                    nested.trim().to_string(),
                )?);
            } else if rest == "{" {
                *index += 1;
                else_branch = parse_logic_statements(path, lines, index)?;
            } else {
                return Err(CompileError::at(
                    path,
                    number,
                    format!("invalid else clause '{line}' (expected 'else {{' or 'else if ... {{')"),
                ));
            }
        }
    }
    Ok(LogicStatement::If {
        condition,
        then_branch,
        else_branch,
    })
}

fn parse_logic_require(
    path: &str,
    line_number: usize,
    line: &str,
) -> Result<LogicStatement, CompileError> {
    let tail = line.trim_start_matches("require ").trim();
    let Some((condition, message)) = tail.split_once(" else throw ") else {
        return Err(CompileError::at(
            path,
            line_number,
            format!("logic require is missing 'else throw': {line}"),
        ));
    };
    let RawValue::String(message) = parse_scalar(message.trim()) else {
        return Err(CompileError::at(
            path,
            line_number,
            "logic throw message must be text",
        ));
    };
    Ok(LogicStatement::Require {
        condition: condition.trim().to_string(),
        message,
    })
}

fn parse_logic_derive(
    path: &str,
    line_number: usize,
    line: &str,
) -> Result<LogicStatement, CompileError> {
    let (tail, only_if_missing) = if let Some(rest) = line.strip_prefix("derive? ") {
        (rest.trim(), true)
    } else {
        (line.trim_start_matches("derive ").trim(), false)
    };
    let Some((target, value)) = split_once_top_level(tail, '=') else {
        return Err(CompileError::at(
            path,
            line_number,
            format!("logic derive is missing '=': {line}"),
        ));
    };
    let target = target.trim();
    if !target.starts_with('.') {
        return Err(CompileError::at(
            path,
            line_number,
            format!("logic derive target must start with '.': {line}"),
        ));
    }
    let target_path = target
        .trim_start_matches('.')
        .split('.')
        .filter(|segment| !segment.trim().is_empty())
        .map(normalize_identifier)
        .collect::<Vec<_>>();
    if target_path.is_empty() {
        return Err(CompileError::at(
            path,
            line_number,
            format!("logic derive target is empty: {line}"),
        ));
    }
    Ok(LogicStatement::Derive {
        path: target_path,
        value: parse_value(value.trim()),
        only_if_missing,
    })
}

// ---------------------------------------------------------------------------
// Instance model and parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct InstanceFile {
    path: String,
    project_dir: Option<PathBuf>,
    file_stem: String,
    template: String,
    header_line: usize,
    clones: Vec<CloneRef>,
    assignments: Vec<Assignment>,
}

/// `&id.*` clones a whole instance; `&id.field.path` clones one subtree.
#[derive(Clone, Debug)]
struct CloneRef {
    line: usize,
    id: String,
    path: Vec<String>,
}

#[derive(Clone, Debug)]
enum Assignment {
    Path {
        path: Vec<String>,
        value: RawValue,
    },
    MultiPath {
        prefix: String,
        keys: Vec<String>,
        value: RawValue,
    },
}

#[derive(Clone, Debug)]
struct Statement {
    line: usize,
    text: String,
}

fn parse_instance_source(source: &SourceFile) -> Result<Vec<InstanceFile>, CompileError> {
    let statements = instance_statements(&source.path, &source.text)?;
    let mut output = Vec::new();
    let mut pending_clones = Vec::new();
    let mut current: Option<InstanceFile> = None;

    for statement in statements {
        if statement.text.starts_with('&') {
            pending_clones.push(parse_clone_ref(&source.path, &statement)?);
            continue;
        }
        if let Some((template, tail)) = find_instance_header(&statement.text) {
            if let Some(instance) = current.take() {
                output.push(instance);
            }
            let mut instance = InstanceFile {
                path: source.path.clone(),
                project_dir: source.project_dir.clone(),
                file_stem: file_stem(&source.path),
                template,
                header_line: statement.line,
                clones: std::mem::take(&mut pending_clones),
                assignments: Vec::new(),
            };
            for tag in parse_at_tags(&source.path, statement.line, &tail)? {
                instance.assignments.push(tag);
            }
            current = Some(instance);
            continue;
        }

        let Some(instance) = current.as_mut() else {
            return Err(CompileError::at(
                &source.path,
                statement.line,
                format!(
                    "statement '{}' appears before any instance header (expected 'Template :: ...' first)",
                    statement.text
                ),
            ));
        };
        instance
            .assignments
            .extend(parse_assignment(&source.path, &statement)?);
    }

    if let Some(instance) = current.take() {
        output.push(instance);
    }
    Ok(output)
}

/// Splits an instance file into logical statements. A statement continues
/// across lines while brackets remain open or while it ends with a trailing
/// comma, so multi-line arrays and tuple tables read naturally.
fn instance_statements(path: &str, text: &str) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut current_line = 0usize;
    let mut depth = 0isize;

    for (index, raw_line) in text.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if depth == 0 && current.is_empty() && line.starts_with("data:") {
            break;
        }
        if current.is_empty() {
            current_line = number;
        } else {
            current.push(' ');
        }
        current.push_str(&line);

        let (delta, ends_inside_string) = structure_delta(&line);
        if ends_inside_string {
            return Err(CompileError::at(
                path,
                number,
                "unterminated string literal (strings must close on the same line)",
            ));
        }
        depth = (depth + delta).max(0);
        if depth > 0 {
            continue;
        }
        if current.trim_end().ends_with(',') {
            continue;
        }
        statements.push(Statement {
            line: current_line,
            text: current.trim().trim_end_matches(',').trim().to_string(),
        });
        current.clear();
    }
    if !current.trim().is_empty() {
        statements.push(Statement {
            line: current_line,
            text: current.trim().trim_end_matches(',').trim().to_string(),
        });
    }
    Ok(statements)
}

/// Net bracket depth change of a line plus whether it ends inside a string.
fn structure_delta(text: &str) -> (isize, bool) {
    let mut scanner = StringScanner::default();
    let mut delta = 0isize;
    for ch in text.chars() {
        if scanner.step(ch) {
            continue;
        }
        match ch {
            '(' | '[' | '{' => delta += 1,
            ')' | ']' | '}' => delta -= 1,
            _ => {}
        }
    }
    (delta, scanner.in_string)
}

/// Detects `Template :: tail` headers. The `::` must sit outside strings and
/// the left side must be a plain template name, so values containing `::`
/// (URLs, namespaced ids) are never mistaken for headers.
fn find_instance_header(text: &str) -> Option<(String, String)> {
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        if ch == ':' && text[index..].starts_with("::") {
            let left = text[..index].trim();
            if is_template_name(left) {
                return Some((left.to_string(), text[index + 2..].to_string()));
            }
            return None;
        }
    }
    None
}

fn is_template_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && text.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn parse_clone_ref(path: &str, statement: &Statement) -> Result<CloneRef, CompileError> {
    let reference = statement.text.trim().trim_start_matches('&').trim();
    let (id_part, clone_path) = if let Some(prefix) = reference.strip_suffix(".*") {
        (prefix.trim(), Vec::new())
    } else if let Some((id, rest)) = reference.split_once('.') {
        (
            id.trim(),
            rest.split('.')
                .filter(|segment| !segment.trim().is_empty())
                .map(normalize_identifier)
                .collect(),
        )
    } else {
        (reference, Vec::new())
    };
    if id_part.is_empty() {
        return Err(CompileError::at(
            path,
            statement.line,
            format!(
                "invalid clone reference '{}' (expected '&instance_id.*' or '&instance_id.field')",
                statement.text
            ),
        ));
    }
    Ok(CloneRef {
        line: statement.line,
        id: id_part.to_string(),
        path: clone_path,
    })
}

fn file_stem(path: &str) -> String {
    PathBuf::from(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("instance")
        .to_string()
}

/// Parses header tags: `@field.value` assigns a scalar, a bare `@field`
/// assigns `true` (handy for boolean flags).
fn parse_at_tags(
    path: &str,
    line: usize,
    text: &str,
) -> Result<Vec<Assignment>, CompileError> {
    let mut assignments = Vec::new();
    for piece in header_tag_pieces(text) {
        let Some(tag) = piece.strip_prefix('@') else {
            return Err(CompileError::at(
                path,
                line,
                format!(
                    "unexpected token '{piece}' in instance header (expected tags such as @field.value; quote values that contain '@')"
                ),
            ));
        };
        match tag.split_once('.') {
            Some((name, value)) => assignments.push(Assignment::Path {
                path: vec![normalize_identifier(name)],
                value: parse_scalar(value),
            }),
            None => {
                let name = normalize_identifier(tag);
                if name.is_empty() {
                    return Err(CompileError::at(
                        path,
                        line,
                        "instance header contains an empty @tag",
                    ));
                }
                assignments.push(Assignment::Path {
                    path: vec![name],
                    value: RawValue::Bool(true),
                });
            }
        }
    }
    Ok(assignments)
}

fn header_tag_pieces(text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut paren = 0isize;
    let mut brace = 0isize;
    let mut bracket = 0isize;
    let mut scanner = StringScanner::default();
    let mut seen_tag = false;
    for ch in text.chars() {
        let in_string = scanner.step(ch);
        if !in_string {
            match ch {
                '(' => paren += 1,
                ')' => paren = (paren - 1).max(0),
                '{' => brace += 1,
                '}' => brace = (brace - 1).max(0),
                '[' => bracket += 1,
                ']' => bracket = (bracket - 1).max(0),
                '@' if seen_tag && paren == 0 && brace == 0 && bracket == 0 => {
                    if !current.trim().trim_end_matches(',').is_empty() {
                        pieces.push(current.trim().trim_end_matches(',').trim().to_string());
                    }
                    current.clear();
                }
                ',' if paren == 0 && brace == 0 && bracket == 0 => {
                    if !current.trim().is_empty() {
                        pieces.push(current.trim().to_string());
                    }
                    current.clear();
                    continue;
                }
                _ => {}
            }
            if ch == '@' {
                seen_tag = true;
            }
        }
        current.push(ch);
    }
    if !current.trim().trim_end_matches(',').is_empty() {
        pieces.push(current.trim().trim_end_matches(',').trim().to_string());
    }
    pieces
}

fn parse_assignment(
    path: &str,
    statement: &Statement,
) -> Result<Vec<Assignment>, CompileError> {
    let Some((left, right)) = statement.text.split_once(':') else {
        return Err(CompileError::at(
            path,
            statement.line,
            format!(
                "invalid statement '{}' (expected 'field: value', 'Template :: ...', or '&clone.*')",
                statement.text
            ),
        ));
    };
    let left = left.trim();
    let right = right.trim();

    // Tuple multi-assignment: (a, b): (1, 2)
    if left.starts_with('(') && left.ends_with(')') {
        let keys = split_top_level(&left[1..left.len() - 1], ',');
        let tuple = parse_tuple(right);
        if keys.len() != tuple.len() {
            return Err(CompileError::at(
                path,
                statement.line,
                format!(
                    "tuple assignment mismatch: {} field(s) on the left, {} value(s) on the right",
                    keys.len(),
                    tuple.len()
                ),
            ));
        }
        let mut output = Vec::new();
        for (key, value) in keys.into_iter().zip(tuple) {
            output.push(Assignment::Path {
                path: vec![normalize_identifier(&key)],
                value,
            });
        }
        return Ok(output);
    }

    // Tuple arrays: field(col_a, col_b): (1, 2), (3, 4)
    if let Some((field, tuple_columns)) = parse_tuple_array_left(left) {
        let tuples = parse_tuple_list(right);
        let mut entries = Vec::new();
        for tuple in tuples {
            if tuple.len() != tuple_columns.len() {
                return Err(CompileError::at(
                    path,
                    statement.line,
                    format!(
                        "tuple arity mismatch in '{}': declared {} column(s) ({}), found a tuple with {} value(s)",
                        field,
                        tuple_columns.len(),
                        tuple_columns.join(", "),
                        tuple.len()
                    ),
                ));
            }
            let mut object = RawValue::object();
            for (column, value) in tuple_columns.iter().zip(tuple) {
                set_object_field(&mut object, &normalize_identifier(column), value);
            }
            entries.push(object);
        }
        return Ok(vec![Assignment::Path {
            path: vec![output_field_name(&normalize_identifier(&field))],
            value: RawValue::Array(entries),
        }]);
    }

    // Multi-path: slots.{3, 4}: value
    if left.contains(".{") {
        let (prefix, keys) = parse_multi_path_left(path, statement.line, left)?;
        return Ok(vec![Assignment::MultiPath {
            prefix,
            keys,
            value: parse_value(right),
        }]);
    }

    let segments: Vec<String> = left.split('.').map(normalize_identifier).collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(CompileError::at(
            path,
            statement.line,
            format!("invalid assignment path '{left}' (empty path segment)"),
        ));
    }
    Ok(vec![Assignment::Path {
        path: segments,
        value: parse_value(right),
    }])
}

fn parse_tuple_array_left(left: &str) -> Option<(String, Vec<String>)> {
    let open = left.find('(')?;
    let close = left.rfind(')')?;
    if close < open {
        return None;
    }
    let field = left[..open].trim().to_string();
    if field.is_empty() {
        return None;
    }
    let columns = split_top_level(&left[open + 1..close], ',');
    Some((field, columns))
}

fn parse_multi_path_left(
    path: &str,
    line: usize,
    left: &str,
) -> Result<(String, Vec<String>), CompileError> {
    let Some(open) = left.find(".{") else {
        return Err(CompileError::at(
            path,
            line,
            format!("invalid multi path '{left}'"),
        ));
    };
    let Some(close) = left.rfind('}') else {
        return Err(CompileError::at(
            path,
            line,
            format!("invalid multi path '{left}' (missing '}}')"),
        ));
    };
    let prefix = normalize_identifier(&left[..open]);
    let keys: Vec<String> = split_top_level(&left[open + 2..close], ',')
        .into_iter()
        .map(normalize_identifier)
        .collect();
    if keys.is_empty() {
        return Err(CompileError::at(
            path,
            line,
            format!("invalid multi path '{left}' (no keys inside '{{...}}')"),
        ));
    }
    Ok((prefix, keys))
}

// ---------------------------------------------------------------------------
// Value parsing
// ---------------------------------------------------------------------------

fn parse_value(text: &str) -> RawValue {
    let text = text.trim();
    if text.starts_with('[') && text.ends_with(']') {
        return RawValue::Array(
            split_top_level(&text[1..text.len() - 1], ',')
                .into_iter()
                .flat_map(parse_value_piece)
                .collect(),
        );
    }
    let pieces = split_top_level(text, ',');
    if pieces.len() > 1 {
        return RawValue::Array(pieces.into_iter().flat_map(parse_value_piece).collect());
    }
    let values = parse_value_piece(text.to_string());
    if values.len() == 1 {
        values.into_iter().next().unwrap()
    } else {
        RawValue::Array(values)
    }
}

fn parse_value_piece(piece: String) -> Vec<RawValue> {
    let piece = piece.trim().to_string();
    if piece.starts_with('"') {
        // Quoted values are always literal text: never tags, never patterns.
        return vec![parse_scalar(&piece)];
    }
    if piece.starts_with('#') {
        return vec![parse_tag_object(&piece)];
    }
    if looks_like_brace_file_pattern(&piece) {
        return expand_file_pattern(&piece)
            .into_iter()
            .map(RawValue::String)
            .collect();
    }
    vec![parse_scalar(&piece)]
}

fn parse_scalar(text: &str) -> RawValue {
    let text = text.trim().trim_end_matches(',').trim();
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        return RawValue::String(unescape_string(&text[1..text.len() - 1]));
    }
    if let Ok(value) = text.parse::<i64>() {
        return RawValue::Int(value);
    }
    match text {
        "true" => return RawValue::Bool(true),
        "false" => return RawValue::Bool(false),
        _ => {}
    }
    if is_float_literal(text) {
        if let Ok(value) = text.parse::<f64>() {
            if value.is_finite() {
                return RawValue::Float(value);
            }
        }
    }
    RawValue::String(text.to_string())
}

/// Decodes the escape sequences supported inside quoted strings.
fn unescape_string(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => output.push('"'),
            Some('\\') => output.push('\\'),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

/// Accepts decimal literals such as `1.5`, `-0.25`, `.5`, or `2.5e3`.
/// Names like `1.2.3` or `infinity` stay text.
fn is_float_literal(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    let Some((integer, fraction)) = body.split_once('.') else {
        return false;
    };
    let (fraction, exponent) = match fraction.split_once(['e', 'E']) {
        Some((fraction, exponent)) => (fraction, Some(exponent)),
        None => (fraction, None),
    };
    if !integer.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if integer.is_empty() && fraction.is_empty() {
        return false;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }
    }
    true
}

fn parse_tag_object(text: &str) -> RawValue {
    let text = text.trim().trim_start_matches('#');
    let (tag, args) = if let Some(open) = text.find('(') {
        let close = text.rfind(')').unwrap_or(text.len());
        (&text[..open], Some(&text[open + 1..close]))
    } else {
        (text, None)
    };
    let mut object = RawValue::object();
    set_object_field(
        &mut object,
        "id",
        RawValue::String(normalize_identifier(tag)),
    );
    if let Some(args) = args {
        for arg in split_top_level(args, ',') {
            if let Some((key, value)) = split_once_top_level(&arg, ':') {
                set_object_field(&mut object, &normalize_identifier(key), parse_value(value));
            } else {
                let flag = normalize_identifier(&arg);
                if !flag.is_empty() {
                    set_object_field(&mut object, &flag, RawValue::Bool(true));
                }
            }
        }
    }
    object
}

/// Extracts the `(...)` groups of a tuple list, ignoring parentheses that
/// appear inside quoted strings.
fn parse_tuple_list(text: &str) -> Vec<Vec<RawValue>> {
    let mut tuples = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        if ch == '(' {
            if depth == 0 {
                start = Some(index + 1);
            }
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if let Some(start) = start.take() {
                    tuples.push(parse_tuple(&text[start..index]));
                }
            }
        }
    }
    tuples
}

fn parse_tuple(text: &str) -> Vec<RawValue> {
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    split_top_level(inner, ',')
        .into_iter()
        .map(|piece| parse_scalar(&piece))
        .collect()
}

/// A brace pattern such as `./textures/{sword,axe}.png` expands into one
/// value per alternative. Quoted values never expand, and pieces with spaces
/// outside the braces are treated as plain text.
fn looks_like_brace_file_pattern(text: &str) -> bool {
    if !(text.contains('{') && text.contains('}') && text.contains('.')) {
        return false;
    }
    if text.contains('"') {
        return false;
    }
    let mut depth = 0isize;
    for ch in text.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = (depth - 1).max(0),
            ch if ch.is_whitespace() && depth == 0 => return false,
            _ => {}
        }
    }
    true
}

fn expand_file_pattern(text: &str) -> Vec<String> {
    let Some(open) = text.find('{') else {
        return vec![text.to_string()];
    };
    let Some(close) = text.rfind('}') else {
        return vec![text.to_string()];
    };
    let prefix = &text[..open];
    let suffix = &text[close + 1..];
    split_top_level(&text[open + 1..close], ',')
        .into_iter()
        .map(|part| format!("{prefix}{}{suffix}", part.trim()))
        .collect()
}

// ---------------------------------------------------------------------------
// Instance resolution
// ---------------------------------------------------------------------------

fn instance_id(instance: &InstanceFile) -> String {
    for assignment in &instance.assignments {
        if let Assignment::Path { path, value } = assignment {
            if path == &["id".to_string()] {
                if let RawValue::String(value) = value {
                    return value.clone();
                }
            }
        }
    }
    instance.file_stem.clone()
}

#[allow(clippy::too_many_arguments)]
fn resolve_instance(
    id: &str,
    instances: &HashMap<String, InstanceFile>,
    schemas: &HashMap<String, Schema>,
    logic_blocks: &HashMap<String, Vec<LogicStatement>>,
    options: &CompileOptions,
    compiled: &mut HashMap<String, RawValue>,
    resolving: &mut HashSet<String>,
) -> Result<RawValue, CompileError> {
    if let Some(value) = compiled.get(id) {
        return Ok(value.clone());
    }
    if !resolving.insert(id.to_string()) {
        return Err(CompileError::new(format!(
            "clone cycle detected for '{id}'"
        )));
    }

    let instance = instances.get(id).ok_or_else(|| {
        CompileError::new(format!(
            "Unknown clone target '{id}'. Clone references such as '&{id}.*' resolve by instance id; make sure an .ab file with @id.{id} is in the same project or directory context."
        ))
    })?;
    let mut object = build_instance_object(instance, instances, resolving)?;
    interpolate_object_variables(&mut object);

    let schema = schemas.get(&instance.template).ok_or_else(|| {
        CompileError::at(
            &instance.path,
            instance.header_line,
            format!(
                "Unknown template '{}'. Add a schema named '{}' in a .abt file, or change this instance header to an existing schema.",
                instance.template, instance.template
            ),
        )
    })?;
    // Without a project directory every on-disk check is skipped, so
    // `skip_asset_checks` simply compiles as if the sources were in memory.
    let project_dir = if options.skip_asset_checks {
        None
    } else {
        instance.project_dir.as_deref()
    };
    validate_schema(schema, &mut object, schemas, options, &instance.path, project_dir, true)?;
    evaluate_schema_logic(
        &schema.name,
        &mut object,
        logic_blocks,
        &instance.path,
        project_dir,
    )?;
    validate_schema(schema, &mut object, schemas, options, &instance.path, project_dir, true)?;
    reorder_instance_object(&mut object, schema);

    resolving.remove(id);
    compiled.insert(id.to_string(), object.clone());
    Ok(object)
}

fn build_instance_object(
    instance: &InstanceFile,
    instances: &HashMap<String, InstanceFile>,
    resolving: &mut HashSet<String>,
) -> Result<RawValue, CompileError> {
    let mut object = RawValue::object();
    for clone in &instance.clones {
        if !resolving.insert(clone.id.clone()) {
            return Err(CompileError::at(
                &instance.path,
                clone.line,
                format!("clone cycle detected for '{}'", clone.id),
            ));
        }
        let clone_instance = instances.get(&clone.id).ok_or_else(|| {
            CompileError::at(
                &instance.path,
                clone.line,
                format!(
                    "Unknown clone target '{}'. Clone references such as '&{}.*' resolve by instance id; make sure the source .ab is available next to this file or inside the compiled project.",
                    clone.id, clone.id
                ),
            )
        })?;
        let clone_object = build_instance_object(clone_instance, instances, resolving)?;
        resolving.remove(&clone.id);

        if clone.path.is_empty() {
            merge_into(&mut object, clone_object);
        } else {
            let Some(subtree) = get_path_value(&clone_object, &clone.path) else {
                return Err(CompileError::at(
                    &instance.path,
                    clone.line,
                    format!(
                        "clone path '&{}.{}' not found on the source instance",
                        clone.id,
                        clone.path.join(".")
                    ),
                ));
            };
            set_path(&mut object, &clone.path, subtree.clone());
        }
    }

    set_object_field(
        &mut object,
        "template",
        RawValue::String(instance.template.clone()),
    );
    set_object_field(
        &mut object,
        "id",
        RawValue::String(instance.file_stem.clone()),
    );
    for assignment in &instance.assignments {
        apply_assignment(&mut object, assignment.clone());
    }
    Ok(object)
}

/// Deep-merges `incoming` into `target`: objects merge key by key, and every
/// other value type replaces the previous one. Later clones win on conflict.
fn merge_into(target: &mut RawValue, incoming: RawValue) {
    match (&mut *target, incoming) {
        (RawValue::Object(target_fields), RawValue::Object(incoming_fields)) => {
            for (key, incoming_value) in incoming_fields {
                if let Some((_, existing)) = target_fields
                    .iter_mut()
                    .find(|(existing_key, _)| existing_key == &key)
                {
                    merge_into(existing, incoming_value);
                } else {
                    target_fields.push((key, incoming_value));
                }
            }
        }
        (target_slot, incoming_value) => *target_slot = incoming_value,
    }
}

fn apply_assignment(object: &mut RawValue, assignment: Assignment) {
    match assignment {
        Assignment::Path { path, value } => set_path(object, &path, value),
        Assignment::MultiPath {
            prefix,
            keys,
            value,
        } => {
            for key in keys {
                set_path(object, &[prefix.clone(), key], value.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Variable interpolation
// ---------------------------------------------------------------------------

fn interpolate_object_variables(object: &mut RawValue) {
    let snapshot = object.clone();
    interpolate_value_variables(object, &snapshot);
}

fn interpolate_value_variables(value: &mut RawValue, root: &RawValue) {
    match value {
        RawValue::String(text) => {
            *text = interpolate_root_text(text, root);
        }
        RawValue::Array(items) => {
            for item in items {
                interpolate_value_variables(item, root);
            }
        }
        RawValue::Object(fields) => {
            for (_, value) in fields {
                interpolate_value_variables(value, root);
            }
        }
        RawValue::Int(_) | RawValue::Float(_) | RawValue::Bool(_) => {}
    }
}

/// Replaces `$field` and `${field}` with root scalar values. Longer names are
/// replaced first so `$item` never clobbers `$item_size`, and `$$` escapes a
/// literal dollar sign.
fn interpolate_root_text(text: &str, root: &RawValue) -> String {
    if !text.contains('$') {
        return text.to_string();
    }
    let mut variables = root_scalar_variables(root);
    variables.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    const SENTINEL: char = '\u{1}';
    let mut output = text.replace("$$", &SENTINEL.to_string());
    for (key, value) in &variables {
        output = output.replace(&format!("${{{key}}}"), value);
        output = output.replace(&format!("${key}"), value);
    }
    output.replace(SENTINEL, "$")
}

fn root_scalar_variables(root: &RawValue) -> Vec<(String, String)> {
    let RawValue::Object(fields) = root else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|(key, value)| scalar_to_text(value).map(|text| (key.clone(), text)))
        .collect()
}

fn scalar_to_text(value: &RawValue) -> Option<String> {
    match value {
        RawValue::String(text) => Some(text.clone()),
        RawValue::Int(number) => Some(number.to_string()),
        RawValue::Float(number) => Some(number.to_string()),
        RawValue::Bool(flag) => Some(flag.to_string()),
        RawValue::Array(_) | RawValue::Object(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn set_path(object: &mut RawValue, path: &[String], value: RawValue) {
    if path.is_empty() {
        return;
    }
    if path.len() == 1 {
        set_object_field(object, &output_field_name(&path[0]), value);
        return;
    }
    let current_key = output_field_name(&path[0]);
    let child = get_or_insert_object_field(object, &current_key);
    set_path(child, &path[1..], value);
}

fn get_path_value<'a>(object: &'a RawValue, path: &[String]) -> Option<&'a RawValue> {
    let mut current = object;
    for segment in path {
        current = get_object_field(current, &output_field_name(segment))?;
    }
    Some(current)
}

fn set_object_field(object: &mut RawValue, key: &str, value: RawValue) {
    let Some(fields) = object.as_object_mut() else {
        return;
    };
    if let Some((_, existing)) = fields
        .iter_mut()
        .find(|(existing_key, _)| existing_key == key)
    {
        *existing = value;
        return;
    }
    fields.push((key.to_string(), value));
}

fn get_or_insert_object_field<'a>(object: &'a mut RawValue, key: &str) -> &'a mut RawValue {
    let fields = object
        .as_object_mut()
        .expect("path target must be an object");
    if let Some(index) = fields
        .iter()
        .position(|(existing_key, _)| existing_key == key)
    {
        return &mut fields[index].1;
    }
    fields.push((key.to_string(), RawValue::object()));
    &mut fields.last_mut().unwrap().1
}

fn get_object_field<'a>(object: &'a RawValue, key: &str) -> Option<&'a RawValue> {
    match object {
        RawValue::Object(fields) => fields
            .iter()
            .find(|(existing_key, _)| existing_key == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

fn get_object_field_mut<'a>(object: &'a mut RawValue, key: &str) -> Option<&'a mut RawValue> {
    match object {
        RawValue::Object(fields) => fields
            .iter_mut()
            .find(|(existing_key, _)| existing_key == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

fn remove_object_field(object: &mut RawValue, key: &str) {
    let Some(fields) = object.as_object_mut() else {
        return;
    };
    if let Some(index) = fields.iter().position(|(field_key, _)| field_key == key) {
        fields.remove(index);
    }
}

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn validate_schema(
    schema: &Schema,
    object: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    options: &CompileOptions,
    path: &str,
    project_dir: Option<&Path>,
    is_root: bool,
) -> Result<(), CompileError> {
    normalize_tag_object(&schema.fields, object);
    validate_fields(
        &schema.fields,
        object,
        schemas,
        options,
        path,
        &schema.name,
        project_dir,
        is_root,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_fields(
    fields: &[FieldSpec],
    object: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    options: &CompileOptions,
    path: &str,
    context: &str,
    project_dir: Option<&Path>,
    is_root: bool,
) -> Result<(), CompileError> {
    // Reject typos first: a misspelled field otherwise surfaces as a
    // confusing "missing required field" error.
    if !options.allow_unknown_fields {
        reject_unknown_fields(fields, object, path, context, is_root)?;
    }

    for field in fields {
        let output_name = output_field_name(&field.name);
        let present = get_object_field(object, &output_name).is_some();
        if !present {
            if let Some(default) = &field.default {
                set_object_field(object, &output_name, default.clone());
            } else if field.list && field.optional {
                set_object_field(object, &output_name, RawValue::Array(Vec::new()));
            } else if !field.optional {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Missing required field {context}.{}. Add '{}: ...' to the instance, provide it as a header tag when appropriate, or mark the schema field @optional / give it a default in the .abt file.",
                        field.name, field.name
                    ),
                ));
            }
        }

        if let Some(value) = get_object_field_mut(object, &output_name) {
            expand_wildcards_for_field(field, value);
            validate_value(field, value, schemas, options, path, context, project_dir)?;
        }
    }
    Ok(())
}

/// Rejects instance fields that the schema does not declare. This is what
/// turns a silent typo into an actionable error.
fn reject_unknown_fields(
    fields: &[FieldSpec],
    object: &RawValue,
    path: &str,
    context: &str,
    is_root: bool,
) -> Result<(), CompileError> {
    let RawValue::Object(entries) = object else {
        return Ok(());
    };
    let mut allowed: Vec<String> = fields
        .iter()
        .map(|field| output_field_name(&field.name))
        .collect();
    if is_root {
        allowed.push("template".to_string());
        allowed.push("id".to_string());
    }
    for (key, _) in entries {
        if allowed.iter().any(|name| name == key) {
            continue;
        }
        if key == "id" && !is_root {
            return Err(CompileError::with_path(
                path,
                format!(
                    "Unexpected tag value at {context}: this group has no @tag field, so shorthand like '#value' cannot be used here. Declare a field with @tag in the schema, or assign fields explicitly."
                ),
            ));
        }
        let suggestion = closest_name(key, allowed.iter().map(String::as_str))
            .map(|name| format!(" Did you mean '{name}'?"))
            .unwrap_or_default();
        return Err(CompileError::with_path(
            path,
            format!(
                "Unknown field '{key}' at {context}.{suggestion} Declared fields: {}.",
                if allowed.is_empty() {
                    "(none)".to_string()
                } else {
                    allowed.join(", ")
                }
            ),
        ));
    }
    Ok(())
}

/// Expands `prefix*` tag values inside tagged group lists against the tag
/// field's enum vocabulary. `es_*` clones the entry for `es_es`, `es_mx`, ...
fn expand_wildcards_for_field(field: &FieldSpec, value: &mut RawValue) {
    let TypeSpec::Group(children) = &field.ty else {
        return;
    };
    let Some(tag_field) = children.iter().find(|child| child.tag) else {
        return;
    };
    let TypeSpec::Enum(allowed) = &tag_field.ty else {
        return;
    };
    let Some(items) = value.as_array_mut() else {
        return;
    };

    let mut expanded = Vec::new();
    for item in std::mem::take(items) {
        let Some(RawValue::String(tag_value)) =
            get_object_field(&item, &output_field_name(&tag_field.name)).cloned()
        else {
            expanded.push(item);
            continue;
        };
        if let Some(prefix) = tag_value.strip_suffix('*') {
            for allowed_value in allowed
                .iter()
                .filter(|candidate| candidate.starts_with(prefix))
            {
                let mut clone = item.clone();
                set_object_field(
                    &mut clone,
                    &output_field_name(&tag_field.name),
                    RawValue::String(allowed_value.clone()),
                );
                expanded.push(clone);
            }
        } else {
            expanded.push(item);
        }
    }
    *items = expanded;
}

/// Expands `prefix*` entries in plain enum list fields: `flags: hat_*`.
/// A single wildcard value expands into the whole matching set.
fn expand_enum_list_wildcards(allowed: &[String], value: &mut RawValue) {
    if let RawValue::String(text) = value {
        if text.ends_with('*') {
            *value = RawValue::Array(vec![RawValue::String(text.clone())]);
        }
    }
    let Some(items) = value.as_array_mut() else {
        return;
    };
    let mut expanded = Vec::new();
    for item in std::mem::take(items) {
        match &item {
            RawValue::String(text) => {
                if let Some(prefix) = text.strip_suffix('*') {
                    let prefix = normalize_identifier(prefix);
                    for allowed_value in allowed
                        .iter()
                        .filter(|candidate| candidate.starts_with(&prefix))
                    {
                        expanded.push(RawValue::String(allowed_value.clone()));
                    }
                } else {
                    expanded.push(item);
                }
            }
            _ => expanded.push(item),
        }
    }
    *items = expanded;
}

fn validate_value(
    field: &FieldSpec,
    value: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    options: &CompileOptions,
    path: &str,
    context: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    if field.list {
        if let TypeSpec::Enum(allowed) = &field.ty {
            expand_enum_list_wildcards(allowed, value);
        }
        match value {
            RawValue::Array(items) => {
                for item in items {
                    validate_single_value(field, item, schemas, options, path, context, project_dir)?;
                }
            }
            _ => {
                let mut single = value.clone();
                validate_single_value(
                    field,
                    &mut single,
                    schemas,
                    options,
                    path,
                    context,
                    project_dir,
                )?;
                *value = RawValue::Array(vec![single]);
            }
        }
    } else {
        validate_single_value(field, value, schemas, options, path, context, project_dir)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_single_value(
    field: &FieldSpec,
    value: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    options: &CompileOptions,
    path: &str,
    context: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    match &field.ty {
        TypeSpec::Text(ranges) => {
            let RawValue::String(text) = value else {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected text, but received a {} value. Wrap the value in quotes to keep it as text.",
                        field.name,
                        value.type_name()
                    ),
                ));
            };
            validate_ranges(
                text.chars().count() as i64,
                ranges,
                path,
                &format!("{context}.{}", field.name),
                "length",
            )
        }
        TypeSpec::Int(ranges) => {
            let number = match value {
                RawValue::Int(number) => *number,
                RawValue::String(text) => text.parse().map_err(|_| {
                    CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected int, but '{}' is not a valid integer.",
                            field.name, text
                        ),
                    )
                })?,
                _ => {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected int, but received a {} value.",
                            field.name,
                            value.type_name()
                        ),
                    ))
                }
            };
            *value = RawValue::Int(number);
            validate_ranges(
                number,
                ranges,
                path,
                &format!("{context}.{}", field.name),
                "value",
            )
        }
        TypeSpec::Float(ranges) => {
            let number = match value {
                RawValue::Float(number) => *number,
                RawValue::Int(number) => *number as f64,
                RawValue::String(text) => {
                    let parsed: Result<f64, _> = text.parse();
                    match parsed {
                        Ok(parsed) if parsed.is_finite() && text.chars().next().is_some_and(|ch| ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.') => parsed,
                        _ => {
                            return Err(CompileError::with_path(
                                path,
                                format!(
                                    "Type mismatch at {context}.{}: expected float, but '{}' is not a valid number.",
                                    field.name, text
                                ),
                            ))
                        }
                    }
                }
                _ => {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected float, but received a {} value.",
                            field.name,
                            value.type_name()
                        ),
                    ))
                }
            };
            *value = RawValue::Float(number);
            validate_float_ranges(number, ranges, path, &format!("{context}.{}", field.name))
        }
        TypeSpec::Bool => {
            let flag = match value {
                RawValue::Bool(flag) => *flag,
                RawValue::String(text) => match text.trim() {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(CompileError::with_path(
                            path,
                            format!(
                                "Type mismatch at {context}.{}: expected bool (true or false), but received '{other}'.",
                                field.name
                            ),
                        ))
                    }
                },
                _ => {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected bool (true or false), but received a {} value.",
                            field.name,
                            value.type_name()
                        ),
                    ))
                }
            };
            *value = RawValue::Bool(flag);
            Ok(())
        }
        TypeSpec::Enum(allowed) => {
            let text = match value {
                RawValue::String(text) => text.clone(),
                RawValue::Int(number) => number.to_string(),
                RawValue::Bool(flag) => flag.to_string(),
                _ => {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected enum(...), but received a {} value.",
                            field.name,
                            value.type_name()
                        ),
                    ))
                }
            };
            let normalized = normalize_identifier(&text);
            if allowed.contains(&normalized) {
                *value = RawValue::String(normalized);
                Ok(())
            } else {
                let suggestion = closest_name(&normalized, allowed.iter().map(String::as_str))
                    .map(|name| format!(" Did you mean '{name}'?"))
                    .unwrap_or_default();
                Err(CompileError::with_path(
                    path,
                    format!(
                        "Enum mismatch at {context}.{}: received '{}', expected one of: {}.{suggestion}",
                        field.name,
                        text,
                        allowed.join(", ")
                    ),
                ))
            }
        }
        TypeSpec::File(extensions) => {
            let RawValue::String(text) = value else {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected file(...), but received a {} value.",
                        field.name,
                        value.type_name()
                    ),
                ));
            };
            let ext = Path::new(text)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if extensions.contains(&ext) {
                validate_file_exists(text, project_dir, path, context, &field.name)
            } else {
                Err(CompileError::with_path(
                    path,
                    format!(
                        "File type mismatch at {context}.{}: '{text}' has extension '{ext}', but this schema allows only: {}.",
                        field.name,
                        extensions.join(", ")
                    ),
                ))
            }
        }
        TypeSpec::Image(specs) => {
            let RawValue::String(text) = value else {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected image(...), but received a {} value.",
                        field.name,
                        value.type_name()
                    ),
                ));
            };
            validate_image_value(field, specs, text, project_dir, path, context)
        }
        TypeSpec::Ref(schema_name) => {
            let schema = schemas.get(schema_name).ok_or_else(|| {
                CompileError::with_path(path, format!("unknown schema reference '{schema_name}'"))
            })?;
            if !matches!(value, RawValue::Object(_)) {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected an object shaped by schema '{schema_name}', but received a {} value.",
                        field.name,
                        value.type_name()
                    ),
                ));
            }
            validate_schema(schema, value, schemas, options, path, project_dir, false)?;
            reorder_object_by_fields(value, &schema.fields);
            Ok(())
        }
        TypeSpec::Group(children) => {
            if !matches!(value, RawValue::Object(_)) {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected a group object, but received a {} value.",
                        field.name,
                        value.type_name()
                    ),
                ));
            }
            normalize_tag_object(children, value);
            validate_fields(
                children,
                value,
                schemas,
                options,
                path,
                &format!("{context}.{}", field.name),
                project_dir,
                false,
            )?;
            reorder_object_by_fields(value, children);
            Ok(())
        }
    }
}

/// Validates an `image(...)` value: allowed extension, file existence, real
/// content signature, and declared dimensions. Only header bytes are read.
fn validate_image_value(
    field: &FieldSpec,
    specs: &[ImageSpec],
    text: &str,
    project_dir: Option<&Path>,
    path: &str,
    context: &str,
) -> Result<(), CompileError> {
    let ext = Path::new(text)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let Some(format) = ImageFormat::from_extension(&ext) else {
        return Err(CompileError::with_path(
            path,
            format!(
                "Image type mismatch at {context}.{}: '{text}' has extension '{ext}', but this schema allows only: {}.",
                field.name,
                describe_image_specs(specs)
            ),
        ));
    };
    if !specs.iter().any(|spec| spec.format == format) {
        return Err(CompileError::with_path(
            path,
            format!(
                "Image type mismatch at {context}.{}: '{text}' is {format}, but this schema allows only: {}.",
                field.name,
                describe_image_specs(specs)
            ),
        ));
    }

    // In-memory compilation has no disk to probe; existence and content
    // checks apply when compiling from a real project directory.
    let Some(resolved) = resolve_asset_file(text, project_dir) else {
        return Ok(());
    };
    if !resolved.is_file() {
        return Err(CompileError::with_path(
            path,
            format!(
                "File not found at {context}.{}: '{text}' resolved to '{}'. The schema declares image(...), so the file must exist.",
                field.name,
                resolved.display()
            ),
        ));
    }
    let info = probe_image(&resolved).map_err(|error| {
        CompileError::with_path(
            path,
            format!(
                "Invalid image at {context}.{}: '{text}': {error}",
                field.name
            ),
        )
    })?;
    if info.format != format {
        return Err(CompileError::with_path(
            path,
            format!(
                "Image content mismatch at {context}.{}: '{text}' is named .{ext} but its header is {} data. Rename the file or re-export it.",
                field.name,
                info.format
            ),
        ));
    }
    let matching: Vec<&ImageSpec> = specs.iter().filter(|spec| spec.format == format).collect();
    let accepted = matching.iter().any(|spec| {
        spec.width.is_none_or(|width| info.width == width)
            && spec.height.is_none_or(|height| info.height == height)
    });
    if !accepted {
        return Err(CompileError::with_path(
            path,
            format!(
                "Image size mismatch at {context}.{}: '{text}' is {}x{}, but this schema allows only: {}.",
                field.name,
                info.width,
                info.height,
                describe_image_specs(&matching.into_iter().cloned().collect::<Vec<_>>())
            ),
        ));
    }
    Ok(())
}

fn describe_image_specs(specs: &[ImageSpec]) -> String {
    specs
        .iter()
        .map(ImageSpec::describe)
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_tag_object(children: &[FieldSpec], value: &mut RawValue) {
    let Some(tag_field) = children.iter().find(|child| child.tag) else {
        return;
    };
    let tag_key = output_field_name(&tag_field.name);
    if get_object_field(value, &tag_key).is_some() {
        return;
    }
    let Some(RawValue::String(tag_value)) = get_object_field(value, "id").cloned() else {
        return;
    };
    set_object_field(value, &tag_key, RawValue::String(tag_value));
    if tag_key != "id" {
        remove_object_field(value, "id");
    }
}

fn validate_ranges(
    value: i64,
    ranges: &[RangeSpec],
    path: &str,
    field: &str,
    kind: &str,
) -> Result<(), CompileError> {
    if ranges.is_empty()
        || ranges
            .iter()
            .any(|range| value >= range.min && value <= range.max)
    {
        return Ok(());
    }
    let printable = ranges
        .iter()
        .map(|range| {
            if range.min == range.max {
                range.min.to_string()
            } else {
                format!("{}..{}", range.min, range.max)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Err(CompileError::with_path(
        path,
        format!(
            "Range mismatch at {field}: received {kind} {value}, expected one of: {printable}."
        ),
    ))
}

fn validate_float_ranges(
    value: f64,
    ranges: &[FloatRangeSpec],
    path: &str,
    field: &str,
) -> Result<(), CompileError> {
    if ranges.is_empty()
        || ranges
            .iter()
            .any(|range| value >= range.min && value <= range.max)
    {
        return Ok(());
    }
    let printable = ranges
        .iter()
        .map(|range| {
            if range.min == range.max {
                range.min.to_string()
            } else {
                format!("{}..{}", range.min, range.max)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Err(CompileError::with_path(
        path,
        format!("Range mismatch at {field}: received {value}, expected one of: {printable}."),
    ))
}

fn validate_file_exists(
    text: &str,
    project_dir: Option<&Path>,
    path: &str,
    context: &str,
    field_name: &str,
) -> Result<(), CompileError> {
    let Some(resolved) = resolve_asset_file(text, project_dir) else {
        return Ok(());
    };
    if resolved.is_file() {
        return Ok(());
    }
    Err(CompileError::with_path(
        path,
        format!(
            "File not found at {context}.{field_name}: '{text}' resolved to '{}'. The schema declares file(...), so the file must exist.",
            resolved.display()
        ),
    ))
}

fn looks_like_file_path(text: &str) -> bool {
    Path::new(text)
        .extension()
        .and_then(|value| value.to_str())
        .is_some()
}

fn resolve_asset_file(text: &str, project_dir: Option<&Path>) -> Option<PathBuf> {
    let raw = Path::new(text);
    if raw.is_absolute() {
        return Some(raw.to_path_buf());
    }
    let project_dir = project_dir?;
    let clean = text.trim_start_matches("./").trim_start_matches(".\\");
    let path = if clean.starts_with("assets/") || clean.starts_with("assets\\") {
        project_dir.join(clean)
    } else {
        project_dir.join("assets").join(clean)
    };
    Some(path)
}

// ---------------------------------------------------------------------------
// Logic evaluation
// ---------------------------------------------------------------------------

fn evaluate_schema_logic(
    schema_name: &str,
    object: &mut RawValue,
    logic_blocks: &HashMap<String, Vec<LogicStatement>>,
    path: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    let Some(statements) = logic_blocks.get(schema_name) else {
        return Ok(());
    };
    let mut vars = HashMap::new();
    evaluate_logic_statements(
        statements,
        object,
        &mut vars,
        path,
        schema_name,
        project_dir,
    )
}

fn evaluate_logic_statements(
    statements: &[LogicStatement],
    root: &mut RawValue,
    vars: &mut HashMap<String, RawValue>,
    path: &str,
    schema_name: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    for statement in statements {
        match statement {
            LogicStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let branch = if eval_logic_condition(condition, root, vars, project_dir) {
                    then_branch
                } else {
                    else_branch
                };
                evaluate_logic_statements(branch, root, vars, path, schema_name, project_dir)?;
            }
            LogicStatement::Derive {
                path: target,
                value,
                only_if_missing,
            } => {
                if *only_if_missing && get_path_value(root, target).is_some() {
                    continue;
                }
                let mut value = value.clone();
                interpolate_derived_value(&mut value, root, vars);
                set_path(root, target, value);
            }
            LogicStatement::Require { condition, message } => {
                if !eval_logic_condition(condition, root, vars, project_dir) {
                    let message = interpolate_logic_text(message, vars);
                    let message = interpolate_root_text(&message, root);
                    return Err(CompileError::with_path(
                        path,
                        format!("{schema_name} logic: {message}"),
                    ));
                }
            }
            LogicStatement::For {
                var,
                path: target,
                statements,
            } => {
                let previous = vars.get(var).cloned();
                let iterable_values = resolve_logic_iterable(target, root, vars)
                    .into_iter()
                    .flat_map(|value| match value {
                        RawValue::Array(items) => items,
                        value => vec![value],
                    });
                for value in iterable_values {
                    vars.insert(var.clone(), value);
                    evaluate_logic_statements(
                        statements,
                        root,
                        vars,
                        path,
                        schema_name,
                        project_dir,
                    )?;
                }
                if let Some(previous) = previous {
                    vars.insert(var.clone(), previous);
                } else {
                    vars.remove(var);
                }
            }
        }
    }
    Ok(())
}

/// Interpolates a derive value. A value that is exactly one variable keeps
/// its native type (`derive .count = $n` stays an int); mixed text uses
/// string interpolation with loop variables and root scalars.
fn interpolate_derived_value(
    value: &mut RawValue,
    root: &RawValue,
    vars: &HashMap<String, RawValue>,
) {
    match value {
        RawValue::String(text) => {
            let trimmed = text.trim();
            if let Some(variable) = vars.get(trimmed) {
                *value = variable.clone();
                return;
            }
            if let Some(key) = trimmed.strip_prefix('$') {
                if let Some(variable) = get_object_field(root, &normalize_identifier(key)) {
                    *value = variable.clone();
                    return;
                }
            }
            let interpolated = interpolate_logic_text(text, vars);
            *text = interpolate_root_text(&interpolated, root);
        }
        RawValue::Array(items) => {
            for item in items {
                interpolate_derived_value(item, root, vars);
            }
        }
        RawValue::Object(fields) => {
            for (_, field_value) in fields {
                interpolate_derived_value(field_value, root, vars);
            }
        }
        RawValue::Int(_) | RawValue::Float(_) | RawValue::Bool(_) => {}
    }
}

fn eval_logic_condition(
    condition: &str,
    root: &RawValue,
    vars: &HashMap<String, RawValue>,
    project_dir: Option<&Path>,
) -> bool {
    let condition = strip_wrapping_parens(condition.trim());
    if let Some(parts) = split_logic_operator(condition, " or ") {
        return parts
            .into_iter()
            .any(|part| eval_logic_condition(&part, root, vars, project_dir));
    }
    if let Some(parts) = split_logic_operator(condition, " || ") {
        return parts
            .into_iter()
            .any(|part| eval_logic_condition(&part, root, vars, project_dir));
    }
    if let Some(parts) = split_logic_operator(condition, " and ") {
        return parts
            .into_iter()
            .all(|part| eval_logic_condition(&part, root, vars, project_dir));
    }
    if let Some(parts) = split_logic_operator(condition, " && ") {
        return parts
            .into_iter()
            .all(|part| eval_logic_condition(&part, root, vars, project_dir));
    }
    if let Some(rest) = condition.strip_prefix("not ") {
        return !eval_logic_condition(rest, root, vars, project_dir);
    }
    if let Some(rest) = condition.strip_prefix('!') {
        if !rest.starts_with('=') {
            return !eval_logic_condition(rest, root, vars, project_dir);
        }
    }
    if let Some(inner) = function_argument(condition, "length") {
        return logic_length(inner, root, vars) > 0;
    }
    if let Some(operand) = condition.strip_suffix(" exists") {
        return logic_exists(
            &resolve_logic_values(operand.trim(), root, vars),
            project_dir,
        );
    }
    if let Some((left, right)) = split_once_logic(condition, " contains ") {
        let left_values = resolve_logic_values(&left, root, vars);
        let right_values = logic_operand_values(&right, root, vars);
        return left_values
            .iter()
            .any(|left| right_values.iter().any(|right| logic_contains(left, right)));
    }
    for operator in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some((left, right)) = split_once_logic(condition, operator) {
            let left_values = logic_operand_values(&left, root, vars);
            let right_values = logic_operand_values(&right, root, vars);
            let matched = left_values.iter().any(|left| {
                right_values
                    .iter()
                    .any(|right| compare_logic_values(left, right, operator))
            });
            return matched;
        }
    }
    logic_operand_values(condition, root, vars)
        .iter()
        .any(logic_truthy)
}

fn strip_wrapping_parens(text: &str) -> &str {
    let mut output = text;
    while output.starts_with('(') && output.ends_with(')') && balanced_outer_parens(output) {
        output = output[1..output.len() - 1].trim();
    }
    output
}

fn balanced_outer_parens(text: &str) -> bool {
    let mut depth = 0isize;
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 && index + ch.len_utf8() < text.len() {
                return false;
            }
        }
    }
    depth == 0
}

fn split_logic_operator(text: &str, operator: &str) -> Option<Vec<String>> {
    let parts = split_logic_by(text, operator);
    if parts.len() > 1 {
        Some(parts)
    } else {
        None
    }
}

fn split_once_logic(text: &str, operator: &str) -> Option<(String, String)> {
    let mut depth = 0isize;
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && text[index..].starts_with(operator) {
            return Some((
                text[..index].trim().to_string(),
                text[index + operator.len()..].trim().to_string(),
            ));
        }
    }
    None
}

fn split_logic_by(text: &str, operator: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0isize;
    let mut scanner = StringScanner::default();
    for (index, ch) in text.char_indices() {
        if scanner.step(ch) {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && text[index..].starts_with(operator) && index >= start {
            parts.push(text[start..index].trim().to_string());
            start = index + operator.len();
        }
    }
    parts.push(text[start..].trim().to_string());
    parts.retain(|part| !part.is_empty());
    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

fn logic_operand_values(
    text: &str,
    root: &RawValue,
    vars: &HashMap<String, RawValue>,
) -> Vec<RawValue> {
    let text = text.trim();
    if let Some(inner) = function_argument(text, "length") {
        return vec![RawValue::Int(logic_length(inner, root, vars) as i64)];
    }
    if let Some(value) = resolve_logic_variable(text, vars) {
        return vec![value];
    }
    if text.starts_with('.') || looks_like_logic_var_path(text, vars) {
        let values = resolve_logic_values(text, root, vars);
        if !values.is_empty() {
            return values;
        }
    }
    let value = parse_scalar(text);
    if let RawValue::String(text) = &value {
        if let Some(value) = resolve_logic_variable(text, vars) {
            return vec![value];
        }
    }
    vec![value]
}

fn resolve_logic_iterable(
    text: &str,
    root: &RawValue,
    vars: &HashMap<String, RawValue>,
) -> Vec<RawValue> {
    let text = text.trim();
    if text.starts_with('[') && text.ends_with(']') {
        return vec![parse_value(text)];
    }
    if let Some(value) = resolve_logic_variable(text, vars) {
        return vec![value];
    }
    let values = resolve_logic_values(text, root, vars);
    if values.is_empty() {
        vec![parse_value(text)]
    } else {
        values
    }
}

fn function_argument<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let text = text.trim();
    let prefix = format!("{name}(");
    if text.starts_with(&prefix) && text.ends_with(')') {
        Some(text[prefix.len()..text.len() - 1].trim())
    } else {
        None
    }
}

fn logic_length(text: &str, root: &RawValue, vars: &HashMap<String, RawValue>) -> usize {
    let values = resolve_logic_values(text, root, vars);
    if values.len() == 1 {
        if let RawValue::Array(items) = &values[0] {
            return items.len();
        }
    }
    values.len()
}

fn resolve_logic_variable(text: &str, vars: &HashMap<String, RawValue>) -> Option<RawValue> {
    let key = text.trim().trim_matches('"');
    vars.get(key)
        .cloned()
        .or_else(|| vars.get(&normalize_identifier(key)).cloned())
}

fn looks_like_logic_var_path(text: &str, vars: &HashMap<String, RawValue>) -> bool {
    let head = text.split('.').next().unwrap_or_default();
    vars.contains_key(&normalize_identifier(head))
}

fn resolve_logic_values(
    path: &str,
    root: &RawValue,
    vars: &HashMap<String, RawValue>,
) -> Vec<RawValue> {
    let path = path.trim();
    let trimmed = path.trim_start_matches('.');
    if trimmed.is_empty() {
        return vec![root.clone()];
    }
    let raw_segments = trimmed.split('.').collect::<Vec<_>>();
    let values = if !path.starts_with('.') {
        let first = raw_segments.first().copied().unwrap_or_default();
        if let Some(value) = resolve_logic_variable(first, vars) {
            vec![value.clone()]
        } else {
            vec![root.clone()]
        }
    } else {
        vec![root.clone()]
    };
    let segments =
        if !path.starts_with('.') && resolve_logic_variable(raw_segments[0], vars).is_some() {
            raw_segments[1..]
                .iter()
                .flat_map(|segment| expand_logic_segment(segment, vars))
                .collect::<Vec<_>>()
        } else {
            raw_segments
                .iter()
                .flat_map(|segment| expand_logic_segment(segment, vars))
                .collect::<Vec<_>>()
        };
    let mut values = values;
    for segment in segments {
        values = project_logic_segment(&values, &segment);
    }
    values
}

fn project_logic_segment(values: &[RawValue], segment: &str) -> Vec<RawValue> {
    let mut output = Vec::new();
    let (field, index) = split_indexed_segment(segment);
    let key = output_field_name(&field);
    for value in values {
        match value {
            RawValue::Object(_) => {
                if let Some(child) = get_object_field(value, &key) {
                    push_indexed_logic_value(child, index, &mut output);
                }
            }
            RawValue::Array(items) => {
                if field.is_empty() {
                    if let Some(index) = index {
                        if let Some(item) = items.get(index) {
                            output.push(item.clone());
                        }
                    } else {
                        output.extend(items.clone());
                    }
                } else {
                    output.extend(project_logic_segment(items, segment));
                }
            }
            _ => {}
        }
    }
    output
}

fn expand_logic_segment(segment: &str, vars: &HashMap<String, RawValue>) -> Vec<String> {
    let segment = segment.trim();
    if let Some((field, index)) = raw_indexed_segment(segment) {
        let field = resolve_segment_variable(field, vars);
        return vec![format!("{field}[{index}]")];
    }
    vec![resolve_segment_variable(segment, vars)]
}

fn resolve_segment_variable(segment: &str, vars: &HashMap<String, RawValue>) -> String {
    if let Some(value) = resolve_logic_variable(segment, vars) {
        return logic_value_to_path_segment(&value);
    }
    normalize_identifier(segment)
}

fn logic_value_to_path_segment(value: &RawValue) -> String {
    match value {
        RawValue::String(text) => normalize_identifier(text),
        RawValue::Int(number) => number.to_string(),
        RawValue::Float(number) => number.to_string(),
        RawValue::Bool(flag) => flag.to_string(),
        RawValue::Array(_) | RawValue::Object(_) => String::new(),
    }
}

fn split_indexed_segment(segment: &str) -> (String, Option<usize>) {
    if let Some((field, index)) = raw_indexed_segment(segment) {
        (normalize_identifier(field), index.parse::<usize>().ok())
    } else {
        (normalize_identifier(segment), None)
    }
}

fn raw_indexed_segment(segment: &str) -> Option<(&str, &str)> {
    let open = segment.find('[')?;
    let close = segment.rfind(']')?;
    Some((&segment[..open], &segment[open + 1..close]))
}

fn push_indexed_logic_value(value: &RawValue, index: Option<usize>, output: &mut Vec<RawValue>) {
    if let Some(index) = index {
        if let RawValue::Array(items) = value {
            if let Some(item) = items.get(index) {
                output.push(item.clone());
            }
        }
    } else {
        output.push(value.clone());
    }
}

fn logic_contains(left: &RawValue, right: &RawValue) -> bool {
    match left {
        RawValue::Array(items) => items.iter().any(|item| logic_values_equal(item, right)),
        _ => logic_values_equal(left, right),
    }
}

fn compare_logic_values(left: &RawValue, right: &RawValue, operator: &str) -> bool {
    match operator {
        "==" => logic_values_equal(left, right),
        "!=" => !logic_values_equal(left, right),
        ">" | ">=" | "<" | "<=" => {
            let Some(left) = logic_number(left) else {
                return false;
            };
            let Some(right) = logic_number(right) else {
                return false;
            };
            match operator {
                ">" => left > right,
                ">=" => left >= right,
                "<" => left < right,
                "<=" => left <= right,
                _ => false,
            }
        }
        _ => false,
    }
}

fn logic_values_equal(left: &RawValue, right: &RawValue) -> bool {
    match (left, right) {
        (RawValue::String(left), RawValue::String(right)) => {
            normalize_identifier(left) == normalize_identifier(right)
        }
        (RawValue::Int(left), RawValue::Int(right)) => left == right,
        (RawValue::Float(left), RawValue::Float(right)) => left == right,
        (RawValue::Int(left), RawValue::Float(right)) => (*left as f64) == *right,
        (RawValue::Float(left), RawValue::Int(right)) => *left == (*right as f64),
        (RawValue::Bool(left), RawValue::Bool(right)) => left == right,
        (RawValue::Bool(left), RawValue::String(right)) => {
            normalize_identifier(right) == left.to_string()
        }
        (RawValue::String(left), RawValue::Bool(right)) => {
            normalize_identifier(left) == right.to_string()
        }
        (RawValue::Int(left), RawValue::String(right)) => right.parse::<i64>() == Ok(*left),
        (RawValue::String(left), RawValue::Int(right)) => left.parse::<i64>() == Ok(*right),
        (RawValue::Float(left), RawValue::String(right)) => right.parse::<f64>() == Ok(*left),
        (RawValue::String(left), RawValue::Float(right)) => left.parse::<f64>() == Ok(*right),
        _ => left == right,
    }
}

fn logic_number(value: &RawValue) -> Option<f64> {
    match value {
        RawValue::Int(value) => Some(*value as f64),
        RawValue::Float(value) => Some(*value),
        RawValue::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn logic_truthy(value: &RawValue) -> bool {
    match value {
        RawValue::String(value) => !value.is_empty(),
        RawValue::Int(value) => *value != 0,
        RawValue::Float(value) => *value != 0.0,
        RawValue::Bool(value) => *value,
        RawValue::Array(value) => !value.is_empty(),
        RawValue::Object(value) => !value.is_empty(),
    }
}

fn logic_exists(values: &[RawValue], project_dir: Option<&Path>) -> bool {
    values.iter().any(|value| match value {
        RawValue::String(text) if looks_like_file_path(text) => project_dir
            .and_then(|_| resolve_asset_file(text, project_dir))
            .map(|path| path.is_file())
            .unwrap_or_else(|| logic_truthy(value)),
        RawValue::Array(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| logic_exists(std::slice::from_ref(item), project_dir))
        }
        value => logic_truthy(value),
    })
}

fn interpolate_logic_text(text: &str, vars: &HashMap<String, RawValue>) -> String {
    let mut output = text.to_string();
    let mut keys: Vec<&String> = vars.keys().filter(|key| key.starts_with('$')).collect();
    keys.sort_by(|left, right| right.len().cmp(&left.len()));
    for key in keys {
        if let Some(value) = vars.get(key) {
            output = output.replace(key.as_str(), &logic_value_to_message(value));
        }
    }
    output
}

fn logic_value_to_message(value: &RawValue) -> String {
    match value {
        RawValue::String(text) => text.clone(),
        RawValue::Int(number) => number.to_string(),
        RawValue::Float(number) => number.to_string(),
        RawValue::Bool(flag) => flag.to_string(),
        RawValue::Array(_) | RawValue::Object(_) => format_raw_value_inline(value),
    }
}

// ---------------------------------------------------------------------------
// Output ordering
// ---------------------------------------------------------------------------

fn reorder_instance_object(object: &mut RawValue, schema: &Schema) {
    let RawValue::Object(fields) = object else {
        return;
    };
    let mut old = std::mem::take(fields);
    let mut reordered = Vec::new();
    move_field(&mut old, &mut reordered, "template");
    move_field(&mut old, &mut reordered, "id");
    for field in &schema.fields {
        move_field(&mut old, &mut reordered, &output_field_name(&field.name));
    }
    reordered.extend(old);
    *fields = reordered;
}

fn reorder_object_by_fields(object: &mut RawValue, specs: &[FieldSpec]) {
    let RawValue::Object(fields) = object else {
        return;
    };
    let mut old = std::mem::take(fields);
    let mut reordered = Vec::new();
    for spec in specs {
        move_field(&mut old, &mut reordered, &output_field_name(&spec.name));
    }
    reordered.extend(old);
    *fields = reordered;
}

fn move_field(
    source: &mut Vec<(String, RawValue)>,
    target: &mut Vec<(String, RawValue)>,
    name: &str,
) {
    if let Some(index) = source.iter().position(|(key, _)| key == name) {
        target.push(source.remove(index));
    }
}

// ---------------------------------------------------------------------------
// Output formatting: RAW, JSON, YAML
// ---------------------------------------------------------------------------

fn format_raw_value_multiline(value: &RawValue, indent: usize) -> String {
    match value {
        RawValue::Object(fields) => {
            let pad = "    ".repeat(indent);
            let inner_pad = "    ".repeat(indent + 1);
            let mut output = String::from("{\n");
            for (index, (key, value)) in fields.iter().enumerate() {
                output.push_str(&inner_pad);
                output.push_str(key);
                output.push_str(": ");
                if matches!(value, RawValue::Object(_)) {
                    output.push_str(&format_raw_value_multiline(value, indent + 1));
                } else {
                    output.push_str(&format_raw_value_inline(value));
                }
                if index + 1 < fields.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str(&pad);
            output.push('}');
            output
        }
        _ => format_raw_value_inline(value),
    }
}

fn format_raw_value_inline(value: &RawValue) -> String {
    match value {
        RawValue::String(text) => format!("\"{}\"", escape_raw_string(text)),
        RawValue::Int(number) => number.to_string(),
        RawValue::Float(number) => format_float(*number),
        RawValue::Bool(flag) => flag.to_string(),
        RawValue::Array(items) => {
            let joined = items
                .iter()
                .map(format_raw_value_inline)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{joined}]")
        }
        RawValue::Object(fields) => {
            let joined = fields
                .iter()
                .map(|(key, value)| format!("{key}: {}", format_raw_value_inline(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{joined}}}")
        }
    }
}

fn format_float(value: f64) -> String {
    let formatted = value.to_string();
    if formatted.contains('.') || formatted.contains('e') || formatted.contains("inf") {
        formatted
    } else {
        format!("{formatted}.0")
    }
}

fn escape_raw_string(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch => output.push(ch),
        }
    }
    output
}

fn format_json_value(value: &RawValue, indent: usize) -> String {
    match value {
        RawValue::String(text) => format!("\"{}\"", escape_json_string(text)),
        RawValue::Int(number) => number.to_string(),
        RawValue::Float(number) => format_float(*number),
        RawValue::Bool(flag) => flag.to_string(),
        RawValue::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let pad = "  ".repeat(indent);
            let inner_pad = "  ".repeat(indent + 1);
            let mut output = String::from("[\n");
            for (index, item) in items.iter().enumerate() {
                output.push_str(&inner_pad);
                output.push_str(&format_json_value(item, indent + 1));
                if index + 1 < items.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str(&pad);
            output.push(']');
            output
        }
        RawValue::Object(fields) => {
            if fields.is_empty() {
                return "{}".to_string();
            }
            let pad = "  ".repeat(indent);
            let inner_pad = "  ".repeat(indent + 1);
            let mut output = String::from("{\n");
            for (index, (key, value)) in fields.iter().enumerate() {
                output.push_str(&inner_pad);
                output.push('"');
                output.push_str(&escape_json_string(key));
                output.push_str("\": ");
                output.push_str(&format_json_value(value, indent + 1));
                if index + 1 < fields.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str(&pad);
            output.push('}');
            output
        }
    }
}

fn escape_json_string(text: &str) -> String {
    let mut output = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() => output.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => output.push(ch),
        }
    }
    output
}

fn format_yaml_document(value: &RawValue) -> String {
    let mut output = format_yaml_value(value, 0);
    output.push('\n');
    output
}

fn format_yaml_value(value: &RawValue, indent: usize) -> String {
    match value {
        RawValue::String(text) => format!("\"{}\"", escape_yaml_string(text)),
        RawValue::Int(number) => number.to_string(),
        RawValue::Float(number) => format_float(*number),
        RawValue::Bool(flag) => flag.to_string(),
        RawValue::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let pad = "  ".repeat(indent);
            let mut output = String::new();
            for item in items {
                match item {
                    RawValue::Object(fields) if !fields.is_empty() => {
                        output.push_str(&pad);
                        output.push_str("- ");
                        output.push_str(&format_yaml_object_fields(fields, indent + 1, true));
                    }
                    _ => {
                        output.push_str(&pad);
                        output.push_str("- ");
                        output.push_str(&format_yaml_value(item, indent + 1));
                        output.push('\n');
                    }
                }
            }
            output.trim_end_matches('\n').to_string()
        }
        RawValue::Object(fields) => format_yaml_object_fields(fields, indent, false)
            .trim_end_matches('\n')
            .to_string(),
    }
}

fn format_yaml_object_fields(
    fields: &[(String, RawValue)],
    indent: usize,
    first_field_after_dash: bool,
) -> String {
    let mut output = String::new();
    let pad = "  ".repeat(indent);
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 || !first_field_after_dash {
            output.push_str(&pad);
        }
        output.push_str(key);
        match value {
            RawValue::Array(items) if items.is_empty() => output.push_str(": []\n"),
            RawValue::Object(fields) if fields.is_empty() => output.push_str(": {}\n"),
            RawValue::Array(_) | RawValue::Object(_) => {
                output.push_str(":\n");
                output.push_str(&format_yaml_value(value, indent + 1));
                output.push('\n');
            }
            _ => {
                output.push_str(": ");
                output.push_str(&format_yaml_value(value, indent));
                output.push('\n');
            }
        }
    }
    output
}

fn escape_yaml_string(text: &str) -> String {
    escape_json_string(text)
}

// ---------------------------------------------------------------------------
// Tooling entry points
// ---------------------------------------------------------------------------

pub fn lint_project(path: &Path) -> Result<(), CompileError> {
    compile_project(path, CompileOptions::default()).map(|_| ())
}

pub fn known_template_names(path: &Path) -> Result<Vec<String>, CompileError> {
    let mut sources = Vec::new();
    if path.is_file() {
        sources.push(read_source(
            path,
            path.parent().unwrap_or_else(|| Path::new("")),
        )?);
    } else {
        collect_sources(path, path, &mut sources)?;
    }
    let mut names = BTreeSet::new();
    for source in sources {
        if source.path.ends_with(".abt") {
            for schema in parse_template_source(&source)? {
                names.insert(schema.name);
            }
        }
    }
    Ok(names.into_iter().collect())
}
