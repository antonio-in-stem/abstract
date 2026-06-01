use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    pub allow_schema_free_meta: bool,
}

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
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompileError {}

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

#[derive(Clone, Debug, PartialEq)]
pub enum RawValue {
    String(String),
    Int(i64),
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
}

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
    _options: CompileOptions,
    requested_ids: Option<&BTreeSet<String>>,
) -> Result<CompiledProject, CompileError> {
    let mut schemas = HashMap::<String, Schema>::new();
    let mut logic_blocks = HashMap::<String, Vec<LogicStatement>>::new();
    let mut instances = Vec::<InstanceFile>::new();

    for source in sources {
        if source.path.ends_with(".abt") {
            for schema in parse_template_source(&source)? {
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
            return Err(CompileError::with_path(
                &instance.path,
                format!(
                    "Unknown template '{}'. Add a schema named '{}' in a .abt file, or change this instance header to an existing schema.",
                    instance.template, instance.template
                ),
            ));
        }
        let id = instance_id(&instance);
        if !parsed_by_id.contains_key(&id) {
            order.push(id.clone());
        }
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

#[derive(Clone, Debug)]
struct Schema {
    name: String,
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
    Enum(Vec<String>),
    File(Vec<String>),
    Ref(String),
    Group(Vec<FieldSpec>),
}

#[derive(Clone, Debug)]
struct RangeSpec {
    min: i64,
    max: i64,
}

#[derive(Clone, Debug)]
struct InstanceFile {
    path: String,
    project_dir: Option<PathBuf>,
    file_stem: String,
    template: String,
    clones: Vec<String>,
    assignments: Vec<Assignment>,
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
struct LogicBlock {
    schema: String,
    statements: Vec<LogicStatement>,
}

#[derive(Clone, Debug)]
enum LogicStatement {
    If {
        condition: String,
        statements: Vec<LogicStatement>,
    },
    Derive {
        path: Vec<String>,
        value: RawValue,
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

fn parse_template_source(source: &SourceFile) -> Result<Vec<Schema>, CompileError> {
    let lines = cleaned_lines(&source.text);
    let mut schemas = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if line.starts_with("logic ") {
            index = skip_block(&lines, index);
            continue;
        }
        if !line.starts_with("schema ") {
            index += 1;
            continue;
        }

        let name = line
            .trim_start_matches("schema ")
            .split('{')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            return Err(CompileError::with_path(
                &source.path,
                "schema is missing a name",
            ));
        }
        index += 1;
        let fields = parse_schema_fields(&source.path, &lines, &mut index)?;
        schemas.push(Schema { name, fields });
    }
    Ok(schemas)
}

fn parse_logic_source(source: &SourceFile) -> Result<Vec<LogicBlock>, CompileError> {
    let lines = cleaned_lines(&source.text);
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim();
        if !line.starts_with("logic ") {
            index += 1;
            continue;
        }
        let schema = line
            .trim_start_matches("logic ")
            .split('{')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if schema.is_empty() {
            return Err(CompileError::with_path(
                &source.path,
                "logic block is missing a schema name",
            ));
        }
        index += 1;
        let statements = parse_logic_statements(&source.path, &lines, &mut index)?;
        blocks.push(LogicBlock { schema, statements });
    }
    Ok(blocks)
}

fn parse_logic_statements(
    path: &str,
    lines: &[String],
    index: &mut usize,
) -> Result<Vec<LogicStatement>, CompileError> {
    let mut statements = Vec::new();
    while *index < lines.len() {
        let line = lines[*index].trim();
        if line == "}" {
            *index += 1;
            return Ok(statements);
        }
        if line.starts_with("if ") && line.ends_with('{') {
            let condition = line
                .trim_start_matches("if ")
                .trim_end_matches('{')
                .trim()
                .to_string();
            *index += 1;
            statements.push(LogicStatement::If {
                condition,
                statements: parse_logic_statements(path, lines, index)?,
            });
            continue;
        }
        if line.starts_with("for ") && line.ends_with('{') {
            let header = line.trim_start_matches("for ").trim_end_matches('{').trim();
            let Some((var, target)) = header.split_once(" in ") else {
                return Err(CompileError::with_path(
                    path,
                    format!("invalid logic loop '{line}'"),
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
                let next = lines[*index + 1].trim();
                if next.starts_with("else throw ") {
                    require_line.push(' ');
                    require_line.push_str(next);
                    *index += 1;
                }
            }
            statements.push(parse_logic_require(path, &require_line)?);
        } else if line.starts_with("derive ") {
            statements.push(parse_logic_derive(path, line)?);
        }
        *index += 1;
    }
    Err(CompileError::with_path(path, "logic block was not closed"))
}

fn parse_logic_require(path: &str, line: &str) -> Result<LogicStatement, CompileError> {
    let tail = line.trim_start_matches("require ").trim();
    let Some((condition, message)) = tail.split_once(" else throw ") else {
        return Err(CompileError::with_path(
            path,
            format!("logic require is missing 'else throw': {line}"),
        ));
    };
    let RawValue::String(message) = parse_scalar(message.trim()) else {
        return Err(CompileError::with_path(
            path,
            "logic throw message must be text",
        ));
    };
    Ok(LogicStatement::Require {
        condition: condition.trim().to_string(),
        message,
    })
}

fn parse_logic_derive(path: &str, line: &str) -> Result<LogicStatement, CompileError> {
    let tail = line.trim_start_matches("derive ").trim();
    let Some((target, value)) = tail.split_once('=') else {
        return Err(CompileError::with_path(
            path,
            format!("logic derive is missing '=': {line}"),
        ));
    };
    let target = target.trim();
    if !target.starts_with('.') {
        return Err(CompileError::with_path(
            path,
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
        return Err(CompileError::with_path(
            path,
            format!("logic derive target is empty: {line}"),
        ));
    }
    Ok(LogicStatement::Derive {
        path: target_path,
        value: parse_value(value.trim()),
    })
}

fn cleaned_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(strip_comment)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn strip_comment(line: &str) -> String {
    let mut in_string = false;
    let mut previous = '\0';
    for (index, ch) in line.char_indices() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string && ch == '/' && line[index..].starts_with("//") {
            return line[..index].to_string();
        }
        previous = ch;
    }
    line.to_string()
}

fn skip_block(lines: &[String], mut index: usize) -> usize {
    let mut depth =
        count_char(&lines[index], '{') as isize - count_char(&lines[index], '}') as isize;
    index += 1;
    while index < lines.len() && depth > 0 {
        depth += count_char(&lines[index], '{') as isize;
        depth -= count_char(&lines[index], '}') as isize;
        index += 1;
    }
    index
}

fn count_char(text: &str, needle: char) -> usize {
    text.chars().filter(|ch| *ch == needle).count()
}

fn parse_schema_fields(
    path: &str,
    lines: &[String],
    index: &mut usize,
) -> Result<Vec<FieldSpec>, CompileError> {
    let mut fields = Vec::new();
    while *index < lines.len() {
        let line = lines[*index].trim();
        if line == "}" {
            *index += 1;
            return Ok(fields);
        }
        if line.ends_with('{') {
            let header = line.trim_end_matches('{').trim();
            let (name, list, optional, tag) = parse_field_header(header);
            *index += 1;
            let children = parse_schema_fields(path, lines, index)?;
            fields.push(FieldSpec {
                name,
                list,
                optional,
                tag,
                default: None,
                ty: TypeSpec::Group(children),
            });
            continue;
        }

        let Some((left, right)) = line.split_once(':') else {
            return Err(CompileError::with_path(
                path,
                format!("invalid schema line '{line}'"),
            ));
        };
        let (name, list, optional_from_left, tag_from_left) = parse_field_header(left);
        let (type_part, default, optional_from_right, tag_from_right) =
            parse_schema_type_tail(right)?;
        fields.push(FieldSpec {
            name,
            list,
            optional: optional_from_left || optional_from_right,
            tag: tag_from_left || tag_from_right,
            default,
            ty: parse_type_spec(&type_part)?,
        });
        *index += 1;
    }
    Err(CompileError::with_path(path, "schema block was not closed"))
}

fn parse_field_header(header: &str) -> (String, bool, bool, bool) {
    let mut optional = false;
    let mut tag = false;
    let mut name = String::new();
    let mut list = false;
    for token in header.split_whitespace() {
        match token {
            "@optional" => optional = true,
            "@tag" => tag = true,
            other => {
                name = other.trim().to_string();
                if name.ends_with("[]") {
                    list = true;
                    name.truncate(name.len() - 2);
                }
            }
        }
    }
    (name, list, optional, tag)
}

fn parse_schema_type_tail(
    tail: &str,
) -> Result<(String, Option<RawValue>, bool, bool), CompileError> {
    let mut optional = false;
    let mut tag = false;
    let mut pieces = Vec::new();
    for token in tail.split_whitespace() {
        match token {
            "@optional" => optional = true,
            "@tag" => tag = true,
            other => pieces.push(other),
        }
    }
    let joined = pieces.join(" ");
    let (type_part, default) = if let Some((left, right)) = joined.split_once('=') {
        (left.trim().to_string(), Some(parse_scalar(right.trim())))
    } else {
        (joined.trim().to_string(), None)
    };
    Ok((type_part, default, optional, tag))
}

fn parse_type_spec(text: &str) -> Result<TypeSpec, CompileError> {
    let text = text.trim();
    if let Some(inner) = call_inner(text, "text") {
        return Ok(TypeSpec::Text(parse_ranges(inner)?));
    }
    if let Some(inner) = call_inner(text, "int") {
        return Ok(TypeSpec::Int(parse_ranges(inner)?));
    }
    if let Some(inner) = call_inner(text, "enum") {
        return Ok(TypeSpec::Enum(
            split_top_level(inner, ',')
                .into_iter()
                .map(normalize_identifier)
                .collect(),
        ));
    }
    if let Some(inner) = call_inner(text, "file") {
        return Ok(TypeSpec::File(
            split_top_level(inner, ',')
                .into_iter()
                .map(|item| item.trim().trim_start_matches('.').to_lowercase())
                .collect(),
        ));
    }
    if text.starts_with("$(") && text.ends_with(')') {
        return Ok(TypeSpec::Ref(text[2..text.len() - 1].trim().to_string()));
    }
    Err(CompileError::new(format!(
        "unsupported schema type '{text}'"
    )))
}

fn call_inner<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}(");
    text.strip_prefix(&prefix)?.strip_suffix(')')
}

fn parse_ranges(text: &str) -> Result<Vec<RangeSpec>, CompileError> {
    split_top_level(text, ',')
        .into_iter()
        .map(|part| {
            let part = part.trim();
            if let Some((min, max)) = part.split_once("..") {
                Ok(RangeSpec {
                    min: min
                        .trim()
                        .parse()
                        .map_err(|_| CompileError::new(format!("invalid range '{part}'")))?,
                    max: max
                        .trim()
                        .parse()
                        .map_err(|_| CompileError::new(format!("invalid range '{part}'")))?,
                })
            } else {
                let value = part
                    .parse()
                    .map_err(|_| CompileError::new(format!("invalid range value '{part}'")))?;
                Ok(RangeSpec {
                    min: value,
                    max: value,
                })
            }
        })
        .collect()
}

fn parse_instance_source(source: &SourceFile) -> Result<Vec<InstanceFile>, CompileError> {
    let statements = instance_statements(&source.text);
    let mut output = Vec::new();
    let mut pending_clones = Vec::new();
    let mut current: Option<InstanceFile> = None;

    for statement in statements {
        if statement.starts_with('&') {
            pending_clones.push(parse_clone_ref(&statement));
            continue;
        }
        if statement.contains("::") {
            if let Some(instance) = current.take() {
                output.push(instance);
            }
            let (template, tail) = statement.split_once("::").unwrap();
            let mut instance = InstanceFile {
                path: source.path.clone(),
                project_dir: source.project_dir.clone(),
                file_stem: file_stem(&source.path),
                template: template.trim().to_string(),
                clones: std::mem::take(&mut pending_clones),
                assignments: Vec::new(),
            };
            for tag in parse_at_tags(tail) {
                instance.assignments.push(tag);
            }
            current = Some(instance);
            continue;
        }

        let Some(instance) = current.as_mut() else {
            continue;
        };
        instance.assignments.extend(parse_assignment(&statement)?);
    }

    if let Some(instance) = current.take() {
        output.push(instance);
    }
    Ok(output)
}

fn instance_statements(text: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("data:") {
            break;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&line);
        if current.trim_end().ends_with(',') {
            continue;
        }
        statements.push(current.trim().trim_end_matches(',').trim().to_string());
        current.clear();
    }
    if !current.trim().is_empty() {
        statements.push(current.trim().trim_end_matches(',').trim().to_string());
    }
    statements
}

fn parse_clone_ref(statement: &str) -> String {
    statement
        .trim()
        .trim_start_matches('&')
        .trim_end_matches(".*")
        .trim()
        .to_string()
}

fn file_stem(path: &str) -> String {
    PathBuf::from(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("instance")
        .to_string()
}

fn parse_at_tags(text: &str) -> Vec<Assignment> {
    header_tag_pieces(text)
        .into_iter()
        .filter_map(|piece| {
            let piece = piece.strip_prefix('@')?;
            let (name, value) = piece.split_once('.')?;
            Some(Assignment::Path {
                path: vec![normalize_identifier(name)],
                value: parse_scalar(value),
            })
        })
        .collect()
}

fn header_tag_pieces(text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut paren = 0isize;
    let mut brace = 0isize;
    let mut bracket = 0isize;
    let mut in_string = false;
    let mut previous = '\0';
    let mut seen_tag = false;
    for ch in text.chars() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            match ch {
                '(' => paren += 1,
                ')' => paren -= 1,
                '{' => brace += 1,
                '}' => brace -= 1,
                '[' => bracket += 1,
                ']' => bracket -= 1,
                '@' if seen_tag && paren == 0 && brace == 0 && bracket == 0 => {
                    if !current.trim().trim_end_matches(',').is_empty() {
                        pieces.push(current.trim().trim_end_matches(',').to_string());
                    }
                    current.clear();
                }
                ',' if paren == 0 && brace == 0 && bracket == 0 => {
                    if !current.trim().is_empty() {
                        pieces.push(current.trim().to_string());
                    }
                    current.clear();
                    previous = ch;
                    continue;
                }
                _ => {}
            }
        }
        if ch == '@' {
            seen_tag = true;
        }
        current.push(ch);
        previous = ch;
    }
    if !current.trim().trim_end_matches(',').is_empty() {
        pieces.push(current.trim().trim_end_matches(',').to_string());
    }
    pieces
}

fn parse_assignment(statement: &str) -> Result<Vec<Assignment>, CompileError> {
    let Some((left, right)) = statement.split_once(':') else {
        return Ok(Vec::new());
    };
    let left = left.trim();
    let right = right.trim();

    if left.starts_with('(') && left.ends_with(')') {
        let keys = split_top_level(&left[1..left.len() - 1], ',');
        let tuple = parse_tuple(right)?;
        let mut output = Vec::new();
        for (key, value) in keys.into_iter().zip(tuple) {
            output.push(Assignment::Path {
                path: vec![normalize_identifier(&key)],
                value,
            });
        }
        return Ok(output);
    }

    if let Some((field, tuple_columns)) = parse_tuple_array_left(left) {
        let tuples = parse_tuple_list(right)?;
        let mut entries = Vec::new();
        for tuple in tuples {
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

    if left.contains(".{") {
        let (prefix, keys) = parse_multi_path_left(left)?;
        return Ok(vec![Assignment::MultiPath {
            prefix,
            keys,
            value: parse_value(right),
        }]);
    }

    Ok(vec![Assignment::Path {
        path: left.split('.').map(normalize_identifier).collect(),
        value: parse_value(right),
    }])
}

fn parse_tuple_array_left(left: &str) -> Option<(String, Vec<String>)> {
    let open = left.find('(')?;
    let close = left.rfind(')')?;
    let field = left[..open].trim().to_string();
    let columns = split_top_level(&left[open + 1..close], ',');
    Some((field, columns))
}

fn parse_multi_path_left(left: &str) -> Result<(String, Vec<String>), CompileError> {
    let Some(open) = left.find(".{") else {
        return Err(CompileError::new(format!("invalid multi path '{left}'")));
    };
    let Some(close) = left.rfind('}') else {
        return Err(CompileError::new(format!("invalid multi path '{left}'")));
    };
    let prefix = normalize_identifier(&left[..open]);
    let keys = split_top_level(&left[open + 2..close], ',')
        .into_iter()
        .map(normalize_identifier)
        .collect();
    Ok((prefix, keys))
}

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
    let text = text.trim().trim_end_matches(',');
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        return RawValue::String(text[1..text.len() - 1].replace("\\\"", "\""));
    }
    if let Ok(value) = text.parse::<i64>() {
        return RawValue::Int(value);
    }
    RawValue::String(text.to_string())
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
            if let Some((key, value)) = arg.split_once(':') {
                set_object_field(&mut object, &normalize_identifier(key), parse_value(value));
            }
        }
    }
    object
}

fn parse_tuple_list(text: &str) -> Result<Vec<Vec<RawValue>>, CompileError> {
    let mut tuples = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch == '(' {
            if depth == 0 {
                start = Some(index + 1);
            }
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let Some(start) = start.take() else {
                    continue;
                };
                tuples.push(parse_tuple(&text[start..index])?);
            }
        }
    }
    Ok(tuples)
}

fn parse_tuple(text: &str) -> Result<Vec<RawValue>, CompileError> {
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    Ok(split_top_level(inner, ',')
        .into_iter()
        .map(|piece| parse_scalar(&piece))
        .collect())
}

fn looks_like_brace_file_pattern(text: &str) -> bool {
    text.contains('{') && text.contains('}') && text.contains('.')
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

fn split_top_level(text: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren = 0isize;
    let mut brace = 0isize;
    let mut bracket = 0isize;
    let mut in_string = false;
    let mut previous = '\0';
    for ch in text.chars() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            match ch {
                '(' => paren += 1,
                ')' => paren -= 1,
                '{' => brace += 1,
                '}' => brace -= 1,
                '[' => bracket += 1,
                ']' => bracket -= 1,
                _ => {}
            }
            if ch == delimiter && paren == 0 && brace == 0 && bracket == 0 {
                parts.push(current.trim().to_string());
                current.clear();
                previous = ch;
                continue;
            }
        }
        current.push(ch);
        previous = ch;
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn normalize_identifier(text: impl AsRef<str>) -> String {
    let text = text.as_ref().trim().trim_matches('"').trim();
    text.to_ascii_lowercase().replace('-', "_")
}

fn output_field_name(name: &str) -> String {
    match name {
        "lang" => "lang_values".to_string(),
        other => other.to_string(),
    }
}

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

fn resolve_instance(
    id: &str,
    instances: &HashMap<String, InstanceFile>,
    schemas: &HashMap<String, Schema>,
    logic_blocks: &HashMap<String, Vec<LogicStatement>>,
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

    let instance = instances
        .get(id)
        .ok_or_else(|| {
            CompileError::new(format!(
                "Unknown clone target '{id}'. Clone references such as '&{id}.*' resolve by instance id; make sure an .ab file with @id.{id} is in the same project or directory context."
            ))
        })?;
    let mut object = build_instance_object(instance, instances, resolving)?;
    interpolate_object_variables(&mut object);

    let schema = schemas.get(&instance.template).ok_or_else(|| {
        CompileError::with_path(
            &instance.path,
            format!(
                "Unknown template '{}'. Add a schema named '{}' in a .abt file, or change this instance header to an existing schema.",
                instance.template, instance.template
            ),
        )
    })?;
    validate_schema(
        schema,
        &mut object,
        schemas,
        &instance.path,
        instance.project_dir.as_deref(),
    )?;
    evaluate_schema_logic(
        &schema.name,
        &mut object,
        logic_blocks,
        &instance.path,
        instance.project_dir.as_deref(),
    )?;
    validate_schema(
        schema,
        &mut object,
        schemas,
        &instance.path,
        instance.project_dir.as_deref(),
    )?;
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
        if !resolving.insert(clone.clone()) {
            return Err(CompileError::new(format!(
                "clone cycle detected for '{clone}'"
            )));
        }
        let clone_instance = instances
            .get(clone)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "Unknown clone target '{clone}'. Clone references such as '&{clone}.*' resolve by instance id; make sure the source .ab is available next to this file or inside the compiled project."
                ))
            })?;
        object = build_instance_object(clone_instance, instances, resolving)?;
        resolving.remove(clone);
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
        RawValue::Int(_) => {}
    }
}

fn interpolate_root_text(text: &str, root: &RawValue) -> String {
    let mut output = text.to_string();
    for (key, value) in root_scalar_variables(root) {
        output = output.replace(&format!("${key}"), &value);
    }
    output
}

fn root_scalar_variables(root: &RawValue) -> Vec<(String, String)> {
    let RawValue::Object(fields) = root else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|(key, value)| match value {
            RawValue::String(text) => Some((key.clone(), text.clone())),
            RawValue::Int(number) => Some((key.clone(), number.to_string())),
            RawValue::Array(_) | RawValue::Object(_) => None,
        })
        .collect()
}

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

fn validate_schema(
    schema: &Schema,
    object: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    path: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    normalize_tag_object(&schema.fields, object);
    validate_fields(
        &schema.fields,
        object,
        schemas,
        path,
        &schema.name,
        project_dir,
    )
}

fn validate_fields(
    fields: &[FieldSpec],
    object: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    path: &str,
    context: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
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
            validate_value(field, value, schemas, path, context, project_dir)?;
        }
    }
    Ok(())
}

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
        if let Some(prefix) = tag_value.strip_suffix("*") {
            let prefix = prefix.trim_end_matches('_').to_string() + "_";
            for allowed_value in allowed
                .iter()
                .filter(|candidate| candidate.starts_with(&prefix))
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

fn validate_value(
    field: &FieldSpec,
    value: &mut RawValue,
    schemas: &HashMap<String, Schema>,
    path: &str,
    context: &str,
    project_dir: Option<&Path>,
) -> Result<(), CompileError> {
    if field.list {
        match value {
            RawValue::Array(items) => {
                for item in items {
                    validate_single_value(field, item, schemas, path, context, project_dir)?;
                }
            }
            _ => {
                let mut single = value.clone();
                validate_single_value(field, &mut single, schemas, path, context, project_dir)?;
                *value = RawValue::Array(vec![single]);
            }
        }
    } else {
        validate_single_value(field, value, schemas, path, context, project_dir)?;
    }
    Ok(())
}

fn validate_single_value(
    field: &FieldSpec,
    value: &mut RawValue,
    schemas: &HashMap<String, Schema>,
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
                        "Type mismatch at {context}.{}: expected text(...), but received a non-text value.",
                        field.name
                    ),
                ));
            };
            validate_ranges(
                text.chars().count() as i64,
                ranges,
                path,
                &format!("{context}.{}", field.name),
            )
        }
        TypeSpec::Int(ranges) => {
            let number = match value {
                RawValue::Int(number) => *number,
                RawValue::String(text) => text.parse().map_err(|_| {
                    CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected int(...), but '{}' is not a valid integer.",
                            field.name, text
                        ),
                    )
                })?,
                _ => {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "Type mismatch at {context}.{}: expected int(...), but received a non-integer value.",
                            field.name
                        ),
                    ))
                }
            };
            *value = RawValue::Int(number);
            validate_ranges(number, ranges, path, &format!("{context}.{}", field.name))
        }
        TypeSpec::Enum(allowed) => {
            let RawValue::String(text) = value else {
                return Err(CompileError::with_path(
                    path,
                    format!(
                        "Type mismatch at {context}.{}: expected enum(...), but received a non-text value.",
                        field.name
                    ),
                ));
            };
            let normalized = normalize_identifier(&*text);
            if allowed.contains(&normalized) {
                *text = normalized;
                Ok(())
            } else {
                Err(CompileError::with_path(
                    path,
                    format!(
                        "Enum mismatch at {context}.{}: received '{}', expected one of: {}.",
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
                        "Type mismatch at {context}.{}: expected file(...), but received a non-text value.",
                        field.name
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
        TypeSpec::Ref(schema_name) => {
            let schema = schemas.get(schema_name).ok_or_else(|| {
                CompileError::with_path(path, format!("unknown schema reference '{schema_name}'"))
            })?;
            validate_schema(schema, value, schemas, path, project_dir)
        }
        TypeSpec::Group(children) => {
            normalize_tag_object(children, value);
            validate_fields(
                children,
                value,
                schemas,
                path,
                &format!("{context}.{}", field.name),
                project_dir,
            )?;
            reorder_object_by_fields(value, children);
            Ok(())
        }
    }
}

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
                statements,
            } => {
                if eval_logic_condition(condition, root, vars, project_dir) {
                    evaluate_logic_statements(
                        statements,
                        root,
                        vars,
                        path,
                        schema_name,
                        project_dir,
                    )?;
                }
            }
            LogicStatement::Derive { path, value } => {
                set_path(root, path, value.clone());
            }
            LogicStatement::Require { condition, message } => {
                if !eval_logic_condition(condition, root, vars, project_dir) {
                    return Err(CompileError::with_path(
                        path,
                        format!(
                            "{schema_name} logic: {}",
                            interpolate_logic_text(message, vars)
                        ),
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
    let mut in_string = false;
    let mut previous = '\0';
    for (index, ch) in text.char_indices() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
                if depth == 0 && index + ch.len_utf8() < text.len() {
                    return false;
                }
            }
        }
        previous = ch;
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
    let mut in_string = false;
    let mut previous = '\0';
    for (index, ch) in text.char_indices() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
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
        previous = ch;
    }
    None
}

fn split_logic_by(text: &str, operator: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0isize;
    let mut in_string = false;
    let mut previous = '\0';
    for (index, ch) in text.char_indices() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth == 0 && text[index..].starts_with(operator) {
                parts.push(text[start..index].trim().to_string());
                start = index + operator.len();
            }
        }
        previous = ch;
    }
    parts.push(text[start..].trim().to_string());
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
    let mut values = if !path.starts_with('.') {
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
        (RawValue::Int(left), RawValue::String(right)) => right.parse::<i64>() == Ok(*left),
        (RawValue::String(left), RawValue::Int(right)) => left.parse::<i64>() == Ok(*right),
        _ => left == right,
    }
}

fn logic_number(value: &RawValue) -> Option<i64> {
    match value {
        RawValue::Int(value) => Some(*value),
        RawValue::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn logic_truthy(value: &RawValue) -> bool {
    match value {
        RawValue::String(value) => !value.is_empty(),
        RawValue::Int(value) => *value != 0,
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

fn interpolate_logic_text(text: &str, vars: &HashMap<String, RawValue>) -> String {
    let mut output = text.to_string();
    for (key, value) in vars {
        if !key.starts_with('$') {
            continue;
        }
        output = output.replace(key, &logic_value_to_message(value));
    }
    output
}

fn logic_value_to_message(value: &RawValue) -> String {
    match value {
        RawValue::String(text) => text.clone(),
        RawValue::Int(number) => number.to_string(),
        RawValue::Array(_) | RawValue::Object(_) => format_raw_value_inline(value),
    }
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

fn remove_object_field(object: &mut RawValue, key: &str) {
    let Some(fields) = object.as_object_mut() else {
        return;
    };
    if let Some(index) = fields.iter().position(|(field_key, _)| field_key == key) {
        fields.remove(index);
    }
}

fn validate_ranges(
    value: i64,
    ranges: &[RangeSpec],
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
        format!(
            "Range mismatch at {field}: received {value}, expected one of these allowed lengths/values: {printable}."
        ),
    ))
}

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

fn escape_raw_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_json_value(value: &RawValue, indent: usize) -> String {
    match value {
        RawValue::String(text) => format!("\"{}\"", escape_json_string(text)),
        RawValue::Int(number) => number.to_string(),
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
