//! Complete compiler-owned semantic identities for editor navigation and rename.
use std::collections::{BTreeMap, HashMap};

use super::*;
use crate::ast::Located;
use crate::instance::InstanceDecl;
use crate::lexer::{self, normalise, Token, TokenKind};
use crate::logic::{
    Condition, DeriveExpr, Iterable, LengthArg, LogicPath, LogicStatement, Operand,
};
use crate::schema::{FieldDecl, FieldKind, TypeExpr};

const MAX_SOURCES: usize = 1024;
const MAX_SYMBOLS: usize = 16384;
const MAX_OCCURRENCES: usize = 65536;

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
        return Err("Semantic bindings exceed 1024 sources.".into());
    }
    let (templates, instance_files) =
        crate::parse_sources(sources).map_err(|_| "Sources do not parse.")?;
    let tables = crate::schema::build_tables(&templates)
        .map_err(|_| "Semantic identities are ambiguous.")?;
    let locations = Locations::new(sources, overlays)?;
    let mut graph = Graph::default();

    let mut schemas = HashMap::new();
    for (index, schema) in tables.schemas.iter().enumerate() {
        let declaration = locations.token_after(&schema.at, 1, &schema.name)?;
        let id = format!("schema:{index}");
        let symbol = graph.add(Symbol::new(
            id.clone(),
            schema.name.clone(),
            "schema",
            None,
            schema.name.clone(),
            declaration,
        ))?;
        graph.declare(symbol)?;
        schemas.insert(schema.name.clone(), (symbol, id));
    }

    let mut fields = FieldIndex::default();
    for schema in &tables.schemas {
        let owner = schemas
            .get(&schema.name)
            .ok_or("Schema symbol is missing.")?
            .1
            .clone();
        fields.add_fields(schema, &schema.fields, &[], &owner, &mut graph, &locations)?;
    }

    // Every schema-name token is unambiguous after P2/P3: declarations,
    // logic bindings, instance headers, and ref/nested type arguments.
    for source in sources {
        for token in locations.tokens(&source.path)? {
            if let TokenKind::SchemaName(name) = &token.kind {
                if let Some((symbol, _)) = schemas.get(name) {
                    let location = locations.span(&source.path, token.span.0, token.span.1)?;
                    graph.occurrence(*symbol, location, "reference")?;
                }
            }
        }
    }

    let mut instances = HashMap::new();
    for file in &instance_files {
        for instance in &file.instances {
            let (declaration, implicit) = locations.instance_declaration(instance)?;
            let id = format!("instance:{}", graph.kind_count("instance"));
            let owner = schemas.get(&instance.template).map(|(_, id)| id.clone());
            let symbol = graph.add(
                Symbol::new(
                    id,
                    instance.id.clone(),
                    "instance",
                    owner,
                    format!("{}::{}", instance.template, instance.id),
                    declaration,
                )
                .renamable(!implicit)
                .implicit(implicit),
            )?;
            graph.declare(symbol)?;
            instances.insert(instance.id.clone(), symbol);
        }
    }

    for file in &instance_files {
        scan_instance_file(file, &fields, &instances, &mut graph, &locations)?;
    }

    let mut logic = LogicWalker {
        fields: &fields,
        graph: &mut graph,
        locations: &locations,
        loops: Vec::new(),
    };
    for block in &tables.logic {
        let schema_symbol = schemas
            .get(&block.schema)
            .ok_or("Logic binding has no schema symbol.")?
            .0;
        logic.walk_statements(&block.schema, schema_symbol, &block.statements)?;
    }

    let value = object(vec![
        ("version", number(1)),
        ("complete", Value::Bool(true)),
        ("sources", Value::List(locations.source_values)),
        ("symbols", Value::List(graph.finish()?)),
    ]);
    if json(value.clone())?.len() > MAX_RESPONSE_BYTES - 32768 {
        return Err("Semantic bindings exceed response budget.".into());
    }
    Ok(value)
}

#[derive(Clone)]
struct Location {
    path: String,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
    spelling: String,
    start_byte: usize,
    end_byte: usize,
}

impl Location {
    fn value(&self) -> Value {
        object(vec![
            ("path", text(&self.path)),
            (
                "range",
                object(vec![
                    (
                        "start",
                        object(vec![
                            ("line", number(self.start_line)),
                            ("character", number(self.start_col)),
                        ]),
                    ),
                    (
                        "end",
                        object(vec![
                            ("line", number(self.end_line)),
                            ("character", number(self.end_col)),
                        ]),
                    ),
                ]),
            ),
            ("spelling", text(&self.spelling)),
        ])
    }
}

struct SourceLocation<'a> {
    source: &'a SourceFile,
    path: String,
    preserve_bom: bool,
    tokens: Vec<Token>,
}
struct Locations<'a> {
    sources: HashMap<String, SourceLocation<'a>>,
    source_values: Vec<Value>,
}

impl<'a> Locations<'a> {
    fn new(sources: &'a [SourceFile], overlays: &HashMap<PathBuf, String>) -> Result<Self, String> {
        let mut index = HashMap::new();
        let mut values = BTreeMap::new();
        for source in sources {
            let written = source
                .origin()
                .ok_or("Source has no filesystem identity.")?;
            let origin = fs::canonicalize(written).unwrap_or_else(|_| written.to_path_buf());
            let path = normalise_display_path(&origin.to_string_lossy());
            let preserve_bom = overlays.contains_key(&origin) && source.has_leading_bom();
            let editor_text = if preserve_bom {
                format!("\u{feff}{}", source.text)
            } else {
                source.text.clone()
            };
            let hash = crate::crypto::sha256(editor_text.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let tokens =
                lexer::tokenize(source).map_err(|_| "Validated source cannot be tokenized.")?;
            let value = object(vec![("path", text(&path)), ("sha256", text(hash))]);
            if values.insert(path.clone(), value).is_some()
                || index
                    .insert(
                        source.path.clone(),
                        SourceLocation {
                            source,
                            path,
                            preserve_bom,
                            tokens,
                        },
                    )
                    .is_some()
            {
                return Err("Semantic bindings have duplicate source identities.".into());
            }
        }
        Ok(Self {
            sources: index,
            source_values: values.into_values().collect(),
        })
    }

    fn tokens(&self, file: &str) -> Result<Vec<Token>, String> {
        self.sources
            .get(file)
            .map(|source| source.tokens.clone())
            .ok_or_else(|| "Semantic location source is missing.".into())
    }

    fn token_index(&self, at: &Located) -> Result<usize, String> {
        let source = self
            .sources
            .get(&at.file)
            .ok_or("Semantic location source is missing.")?;
        source
            .tokens
            .binary_search_by_key(&(at.position.line, at.position.col), |token| {
                (token.position.line, token.position.col)
            })
            .map_err(|_| "Semantic position has no exact source token.".into())
    }

    fn token(&self, at: &Located, spelling: &str) -> Result<Location, String> {
        self.token_after(at, 0, spelling)
    }

    fn token_after(&self, at: &Located, offset: usize, spelling: &str) -> Result<Location, String> {
        let source = self
            .sources
            .get(&at.file)
            .ok_or("Semantic location source is missing.")?;
        let token = source
            .tokens
            .get(self.token_index(at)? + offset)
            .ok_or("Semantic token sequence is incomplete.")?;
        if token.identifier_text() != Some(spelling) {
            return Err(format!("Expected semantic token '{spelling}'."));
        }
        self.span(&at.file, token.span.0, token.span.1)
    }

    fn span(&self, file: &str, start: usize, end: usize) -> Result<Location, String> {
        let source = self
            .sources
            .get(file)
            .ok_or("Semantic location source is missing.")?;
        let spelling = source
            .source
            .text
            .get(start..end)
            .ok_or("Semantic span is outside its source.")?
            .to_string();
        let (sl, sc, _) = utf16_range(
            source.source,
            source.source.position_at(start),
            source.preserve_bom,
        );
        let (el, ec, _) = utf16_range(
            source.source,
            source.source.position_at(end),
            source.preserve_bom,
        );
        Ok(Location {
            path: source.path.clone(),
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
            spelling,
            start_byte: start,
            end_byte: end,
        })
    }

    fn instance_declaration(&self, instance: &InstanceDecl) -> Result<(Location, bool), String> {
        let source = self
            .sources
            .get(&instance.at.file)
            .ok_or("Instance source is missing.")?;
        let start = self.token_index(&instance.at)?;
        let mut index = start;
        while index < source.tokens.len() && !source.tokens[index].is_newline() {
            if source.tokens[index].is_punctuation("@")
                && source
                    .tokens
                    .get(index + 1)
                    .and_then(Token::identifier_text)
                    == Some("id")
                && source
                    .tokens
                    .get(index + 2)
                    .is_some_and(|token| token.is_punctuation("."))
            {
                let value = source
                    .tokens
                    .get(index + 3)
                    .ok_or("Explicit instance id has no value token.")?;
                let (mut left, mut right) = value.span;
                if matches!(value.kind, TokenKind::QuotedString(_)) {
                    left += 1;
                    right = right.saturating_sub(1);
                }
                return Ok((self.span(&instance.at.file, left, right)?, false));
            }
            index += 1;
        }
        let anchor = source
            .tokens
            .get(start)
            .ok_or("Instance header is missing.")?
            .span
            .0;
        Ok((self.span(&instance.at.file, anchor, anchor)?, true))
    }
}

struct Occurrence {
    location: Location,
    role: &'static str,
}
struct Symbol {
    id: String,
    name: String,
    kind: &'static str,
    owner_id: Option<String>,
    qualified_name: String,
    declaration: Location,
    occurrences: Vec<Occurrence>,
    type_name: Option<String>,
    shape: Option<String>,
    renamable: bool,
    implicit: bool,
}

impl Symbol {
    fn new(
        id: String,
        name: String,
        kind: &'static str,
        owner_id: Option<String>,
        qualified_name: String,
        declaration: Location,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            owner_id,
            qualified_name,
            declaration,
            occurrences: Vec::new(),
            type_name: None,
            shape: None,
            renamable: true,
            implicit: false,
        }
    }
    fn typed(mut self, type_name: &str, shape: &str) -> Self {
        self.type_name = Some(type_name.into());
        self.shape = Some(shape.into());
        self
    }
    fn renamable(mut self, value: bool) -> Self {
        self.renamable = value;
        self
    }
    fn implicit(mut self, value: bool) -> Self {
        self.implicit = value;
        self
    }
}

#[derive(Default)]
struct Graph {
    symbols: Vec<Symbol>,
    spans: HashMap<(String, usize, usize), usize>,
    occurrences: usize,
}

impl Graph {
    fn add(&mut self, symbol: Symbol) -> Result<usize, String> {
        if self.symbols.len() >= MAX_SYMBOLS {
            return Err("Semantic bindings exceed 16384 symbols.".into());
        }
        let index = self.symbols.len();
        self.symbols.push(symbol);
        Ok(index)
    }
    fn kind_count(&self, kind: &str) -> usize {
        self.symbols.iter().filter(|s| s.kind == kind).count()
    }
    fn declare(&mut self, symbol: usize) -> Result<(), String> {
        let location = self.symbols[symbol].declaration.clone();
        self.occurrence(symbol, location, "declaration")
    }
    fn occurrence(
        &mut self,
        symbol: usize,
        location: Location,
        role: &'static str,
    ) -> Result<(), String> {
        let key = (
            location.path.clone(),
            location.start_byte,
            location.end_byte,
        );
        if let Some(existing) = self.spans.get(&key) {
            if *existing == symbol {
                if role == "declaration" {
                    if let Some(found) = self.symbols[symbol].occurrences.iter_mut().find(|item| {
                        item.location.path == location.path
                            && item.location.start_byte == location.start_byte
                            && item.location.end_byte == location.end_byte
                    }) {
                        found.role = "declaration";
                    }
                }
                return Ok(());
            }
            if location.start_byte != location.end_byte {
                return Err("A semantic token binds to more than one identity.".into());
            }
        } else {
            self.spans.insert(key, symbol);
        }
        if self.occurrences >= MAX_OCCURRENCES {
            return Err("Semantic bindings exceed 65536 occurrences.".into());
        }
        self.occurrences += 1;
        self.symbols[symbol]
            .occurrences
            .push(Occurrence { location, role });
        Ok(())
    }
    fn finish(self) -> Result<Vec<Value>, String> {
        let ids: Vec<String> = self.symbols.iter().map(|s| s.id.clone()).collect();
        let mut out = Vec::with_capacity(self.symbols.len());
        for symbol in self.symbols {
            if symbol.occurrences.is_empty() {
                return Err("A semantic symbol has no occurrence.".into());
            }
            let occurrences = symbol
                .occurrences
                .into_iter()
                .map(|occurrence| {
                    let mut value = match occurrence.location.value() {
                        Value::Object(value) => value,
                        _ => unreachable!(),
                    };
                    value.push(("symbolId".into(), text(&symbol.id)));
                    value.push(("role".into(), text(occurrence.role)));
                    Value::Object(value)
                })
                .collect();
            let mut fields = vec![
                ("id", text(&symbol.id)),
                ("name", text(&symbol.name)),
                ("kind", text(symbol.kind)),
                ("qualifiedName", text(&symbol.qualified_name)),
                ("declaration", symbol.declaration.value()),
                ("renamable", Value::Bool(symbol.renamable)),
                ("implicit", Value::Bool(symbol.implicit)),
                ("occurrences", Value::List(occurrences)),
            ];
            if let Some(owner) = symbol.owner_id {
                if !ids.contains(&owner) {
                    return Err("Semantic symbol owner is missing.".into());
                }
                fields.push(("ownerId", text(owner)));
            }
            if let Some(value) = symbol.type_name {
                fields.push(("type", text(value)));
            }
            if let Some(value) = symbol.shape {
                fields.push(("shape", text(value)));
            }
            out.push(object(fields));
        }
        Ok(out)
    }
}

#[derive(Clone, Debug)]
enum Scope {
    Fields { schema: String, prefix: Vec<String> },
    Scalar,
    Unknown,
}
#[derive(Clone)]
struct FieldInfo {
    symbol: usize,
    next: Scope,
    element: Scope,
    type_name: String,
}
#[derive(Default)]
struct FieldIndex {
    entries: HashMap<(String, Vec<String>), FieldInfo>,
}

impl FieldIndex {
    fn add_fields(
        &mut self,
        schema: &crate::schema::SchemaDecl,
        declarations: &[FieldDecl],
        prefix: &[String],
        owner: &str,
        graph: &mut Graph,
        locations: &Locations<'_>,
    ) -> Result<(), String> {
        for field in declarations {
            let mut path = prefix.to_vec();
            path.push(field.name.clone());
            let declaration = locations.token(&field.at, &field.spelled)?;
            let id = format!("field:{}", graph.kind_count("field"));
            let type_name = field_type(field).to_string();
            let symbol = graph.add(
                Symbol::new(
                    id.clone(),
                    field.name.clone(),
                    "field",
                    Some(owner.into()),
                    format!("{}.{}", schema.name, path.join(".")),
                    declaration,
                )
                .typed(&type_name, field_shape(field)),
            )?;
            graph.declare(symbol)?;
            let next = next_scope(&schema.name, &path, field);
            let element = if field.is_list() {
                next.clone()
            } else {
                Scope::Unknown
            };
            if self
                .entries
                .insert(
                    (schema.name.clone(), path.clone()),
                    FieldInfo {
                        symbol,
                        next,
                        element,
                        type_name,
                    },
                )
                .is_some()
            {
                return Err("Field identity is ambiguous.".into());
            }
            if let FieldKind::Group { fields } = &field.kind {
                self.add_fields(schema, fields, &path, &id, graph, locations)?;
            }
        }
        Ok(())
    }
    fn root(schema: &str) -> Scope {
        Scope::Fields {
            schema: schema.into(),
            prefix: Vec::new(),
        }
    }
    fn lookup(&self, scope: &Scope, name: &str) -> Option<&FieldInfo> {
        let Scope::Fields { schema, prefix } = scope else {
            return None;
        };
        let mut path = prefix.clone();
        path.push(name.to_string());
        self.entries.get(&(schema.clone(), path))
    }
    fn bind(
        &self,
        scope: &Scope,
        name: &str,
        location: Location,
        graph: &mut Graph,
    ) -> Result<Scope, String> {
        let info = self
            .lookup(scope, name)
            .ok_or_else(|| format!("Validated field '{name}' has no semantic identity."))?;
        graph.occurrence(info.symbol, location, "reference")?;
        Ok(info.next.clone())
    }
}

fn field_type(field: &FieldDecl) -> &'static str {
    match field.type_expr() {
        None => "group",
        Some(TypeExpr::Nested { .. }) => "nested",
        Some(ty) => ty.kind(),
    }
}
fn field_shape(field: &FieldDecl) -> &'static str {
    if field.is_list() {
        "list"
    } else if matches!(
        field.kind,
        FieldKind::Group { .. }
            | FieldKind::Scalar {
                ty: TypeExpr::Nested { .. },
                ..
            }
    ) {
        "object"
    } else {
        "scalar"
    }
}
fn next_scope(schema: &str, path: &[String], field: &FieldDecl) -> Scope {
    match &field.kind {
        FieldKind::Group { .. } => Scope::Fields {
            schema: schema.into(),
            prefix: path.to_vec(),
        },
        FieldKind::Scalar {
            ty: TypeExpr::Nested { schema },
            ..
        } => FieldIndex::root(schema),
        _ => Scope::Scalar,
    }
}

fn scan_instance_file(
    file: &crate::ast::InstanceFile,
    fields: &FieldIndex,
    instances: &HashMap<String, usize>,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<(), String> {
    let tokens = locations.tokens(&file.file)?;
    let header_symbols: HashMap<(u32, u32), usize> = file
        .instances
        .iter()
        .filter_map(|instance| {
            instances.get(&instance.id).map(|symbol| {
                (
                    (instance.at.position.line, instance.at.position.col),
                    *symbol,
                )
            })
        })
        .collect();
    let mut index = 0usize;
    let mut current_schema: Option<String> = None;
    let mut current_instance: Option<usize> = None;
    let mut scopes: Vec<Scope> = Vec::new();
    while index < tokens.len() {
        while tokens.get(index).is_some_and(Token::is_newline) {
            index += 1;
        }
        if tokens.get(index).is_none_or(Token::is_end_of_file) {
            break;
        }
        let start = index;
        while tokens
            .get(index)
            .is_some_and(|token| !token.is_newline() && !token.is_end_of_file())
        {
            index += 1;
        }
        let line = &tokens[start..index];
        if line.is_empty() {
            continue;
        }
        if line[0].is_punctuation("}") {
            scopes.pop();
            continue;
        }
        if matches!(&line[0].kind, TokenKind::SchemaName(_))
            && line.get(1).is_some_and(|token| token.is_punctuation("::"))
        {
            let schema = line[0].identifier_text().unwrap_or_default().to_string();
            current_instance = header_symbols
                .get(&(line[0].position.line, line[0].position.col))
                .copied();
            current_schema = Some(schema.clone());
            scopes.clear();
            scopes.push(FieldIndex::root(&schema));
            let mut cursor = 2;
            while cursor + 1 < line.len() {
                if line[cursor].is_punctuation("@") {
                    if let Some(name) = line[cursor + 1].identifier_text() {
                        if !matches!(name, "id" | "since" | "removed") {
                            fields.bind(
                                &FieldIndex::root(&schema),
                                &normalise(name),
                                locations.span(
                                    &file.file,
                                    line[cursor + 1].span.0,
                                    line[cursor + 1].span.1,
                                )?,
                                graph,
                            )?;
                        }
                    }
                }
                cursor += 1;
            }
            continue;
        }
        let schema = current_schema
            .as_deref()
            .ok_or("Instance statement appears before a header.")?;
        if line[0].is_punctuation("&") {
            if let Some(target) = line.get(1).and_then(Token::identifier_text) {
                if let Some(symbol) = instances.get(&normalise(target)) {
                    graph.occurrence(
                        *symbol,
                        locations.span(&file.file, line[1].span.0, line[1].span.1)?,
                        "reference",
                    )?;
                }
            }
            if line.get(2).is_some_and(|token| token.is_punctuation(".")) {
                let names: Vec<usize> = (3..line.len())
                    .filter(|at| line[*at].identifier_text().is_some())
                    .collect();
                bind_token_path(
                    &FieldIndex::root(schema),
                    &names,
                    line,
                    &file.file,
                    fields,
                    graph,
                    locations,
                )?;
            }
            continue;
        }
        let base = scopes
            .last()
            .cloned()
            .unwrap_or_else(|| FieldIndex::root(schema));
        let colon = line.iter().position(|token| token.is_punctuation(":"));
        let opens = line.last().is_some_and(|token| token.is_punctuation("{"));
        let head_end = colon.unwrap_or_else(|| line.len().saturating_sub(usize::from(opens)));
        let head = &line[..head_end];
        let (assigned, value_scope) =
            bind_instance_head(&base, head, &file.file, fields, graph, locations)?;
        if opens {
            scopes.push(assigned);
            continue;
        }
        let Some(colon) = colon else {
            continue;
        };
        let value = &line[colon + 1..];
        bind_tag_arguments(
            &value_scope.unwrap_or_else(|| assigned.clone()),
            value,
            &file.file,
            fields,
            graph,
            locations,
        )?;
        if let Some(info) = field_for_scope(fields, &base, head) {
            if info.type_name == "ref" {
                for token in value {
                    let raw = match &token.kind {
                        TokenKind::BareText(text) | TokenKind::QuotedString(text) => {
                            Some(text.clone())
                        }
                        _ => None,
                    };
                    if let Some(symbol) =
                        raw.and_then(|text| instances.get(&normalise(&text)).copied())
                    {
                        let (mut left, mut right) = token.span;
                        if matches!(token.kind, TokenKind::QuotedString(_)) {
                            left += 1;
                            right = right.saturating_sub(1);
                        }
                        graph.occurrence(
                            symbol,
                            locations.span(&file.file, left, right)?,
                            "reference",
                        )?;
                    }
                }
            }
        }
        scan_interpolations(
            value,
            &file.file,
            |name| {
                if name == "id" {
                    current_instance
                } else {
                    fields
                        .lookup(&FieldIndex::root(schema), name)
                        .map(|item| item.symbol)
                }
            },
            graph,
            locations,
        )?;
    }
    Ok(())
}

fn bind_instance_head(
    base: &Scope,
    head: &[Token],
    file: &str,
    fields: &FieldIndex,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<(Scope, Option<Scope>), String> {
    if let Some(brace) = head.iter().position(|token| token.is_punctuation("{")) {
        let prefix: Vec<usize> = (0..brace)
            .filter(|at| head[*at].identifier_text().is_some())
            .collect();
        let scope = bind_token_path(base, &prefix, head, file, fields, graph, locations)?;
        let mut cursor = brace + 1;
        while cursor < head.len() && !head[cursor].is_punctuation("}") {
            if let Some(name) = head[cursor].identifier_text() {
                fields.bind(
                    &scope,
                    &normalise(name),
                    locations.span(file, head[cursor].span.0, head[cursor].span.1)?,
                    graph,
                )?;
            }
            cursor += 1;
        }
        return Ok((scope, None));
    }
    if let Some(open) = head.iter().position(|token| token.is_punctuation("(")) {
        let prefix: Vec<usize> = (0..open)
            .filter(|at| head[*at].identifier_text().is_some())
            .collect();
        let (assigned, item_scope) =
            bind_token_path_with_item(base, &prefix, head, file, fields, graph, locations)?;
        for token in &head[open + 1..] {
            if let Some(name) = token.identifier_text() {
                fields.bind(
                    &item_scope,
                    &normalise(name),
                    locations.span(file, token.span.0, token.span.1)?,
                    graph,
                )?;
            }
        }
        return Ok((assigned, Some(item_scope)));
    }
    let names: Vec<usize> = (0..head.len())
        .filter(|at| head[*at].identifier_text().is_some())
        .collect();
    let scope = bind_token_path(base, &names, head, file, fields, graph, locations)?;
    Ok((scope, None))
}

fn bind_token_path(
    base: &Scope,
    names: &[usize],
    tokens: &[Token],
    file: &str,
    fields: &FieldIndex,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<Scope, String> {
    Ok(bind_token_path_with_item(base, names, tokens, file, fields, graph, locations)?.0)
}

fn bind_token_path_with_item(
    base: &Scope,
    names: &[usize],
    tokens: &[Token],
    file: &str,
    fields: &FieldIndex,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<(Scope, Scope), String> {
    let mut scope = base.clone();
    let mut element = Scope::Unknown;
    for at in names {
        let spelling = tokens[*at].identifier_text().unwrap_or_default();
        let info = fields
            .lookup(&scope, &normalise(spelling))
            .ok_or_else(|| format!("Validated field '{spelling}' has no semantic identity."))?;
        element = info.element.clone();
        graph.occurrence(
            info.symbol,
            locations.span(file, tokens[*at].span.0, tokens[*at].span.1)?,
            "reference",
        )?;
        scope = info.next.clone();
    }
    Ok((scope, element))
}

fn field_for_scope<'a>(
    fields: &'a FieldIndex,
    base: &Scope,
    head: &[Token],
) -> Option<&'a FieldInfo> {
    let mut scope = base.clone();
    let mut found = None;
    for name in head.iter().filter_map(Token::identifier_text) {
        found = fields.lookup(&scope, &normalise(name));
        scope = found?.next.clone();
    }
    found
}

fn bind_tag_arguments(
    base: &Scope,
    tokens: &[Token],
    file: &str,
    fields: &FieldIndex,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<(), String> {
    let mut index = 0;
    while index < tokens.len() {
        if !tokens[index].is_punctuation("#") {
            index += 1;
            continue;
        }
        index += 2;
        if !tokens
            .get(index)
            .is_some_and(|token| token.is_punctuation("("))
        {
            continue;
        }
        index += 1;
        while index < tokens.len() && !tokens[index].is_punctuation(")") {
            let mut scope = base.clone();
            while index < tokens.len() {
                let Some(name) = tokens[index].identifier_text() else {
                    break;
                };
                scope = fields.bind(
                    &scope,
                    &normalise(name),
                    locations.span(file, tokens[index].span.0, tokens[index].span.1)?,
                    graph,
                )?;
                index += 1;
                if tokens
                    .get(index)
                    .is_some_and(|token| token.is_punctuation("."))
                {
                    index += 1;
                    continue;
                }
                break;
            }
            while index < tokens.len()
                && !tokens[index].is_punctuation(",")
                && !tokens[index].is_punctuation(")")
            {
                index += 1;
            }
            if tokens
                .get(index)
                .is_some_and(|token| token.is_punctuation(","))
            {
                index += 1;
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct LoopBinding {
    name: String,
    symbol: usize,
    element: Scope,
}

struct LogicWalker<'a, 'b> {
    fields: &'a FieldIndex,
    graph: &'b mut Graph,
    locations: &'b Locations<'a>,
    loops: Vec<LoopBinding>,
}

impl LogicWalker<'_, '_> {
    fn walk_statements(
        &mut self,
        schema: &str,
        schema_symbol: usize,
        statements: &[LogicStatement],
    ) -> Result<(), String> {
        for statement in statements {
            match statement {
                LogicStatement::Derive {
                    target, value, at, ..
                } => {
                    self.path(schema, target)?;
                    self.derive_expr(schema, value)?;
                    self.interpolations(schema, at, InterpolationTail::AfterEquals)?;
                }
                LogicStatement::Require { condition, at, .. } => {
                    self.condition(schema, condition)?;
                    self.interpolations(schema, at, InterpolationTail::AfterThrow)?;
                }
                LogicStatement::If {
                    branches,
                    otherwise,
                    ..
                } => {
                    for (condition, body) in branches {
                        self.condition(schema, condition)?;
                        self.walk_statements(schema, schema_symbol, body)?;
                    }
                    if let Some(body) = otherwise {
                        self.walk_statements(schema, schema_symbol, body)?;
                    }
                }
                LogicStatement::For {
                    variable,
                    spelled,
                    variable_at,
                    iterable,
                    body,
                    ..
                } => {
                    let element = match iterable {
                        Iterable::Path(path) => self.path(schema, path)?.1,
                        Iterable::Literal(_) => Scope::Scalar,
                    };
                    let declaration = self.locations.token(variable_at, spelled)?;
                    let parent = self
                        .loops
                        .last()
                        .map(|item| self.graph.symbols[item.symbol].id.clone())
                        .unwrap_or_else(|| self.graph.symbols[schema_symbol].id.clone());
                    let id = format!("loop:{}", self.graph.kind_count("loop"));
                    let symbol = self.graph.add(
                        Symbol::new(
                            id,
                            variable.clone(),
                            "loop",
                            Some(parent),
                            format!(
                                "{}::${}@{}:{}",
                                schema,
                                variable,
                                variable_at.position.line,
                                variable_at.position.col
                            ),
                            declaration,
                        )
                        .typed("loop-variable", scope_shape(&element)),
                    )?;
                    self.graph.declare(symbol)?;
                    self.loops.push(LoopBinding {
                        name: variable.clone(),
                        symbol,
                        element,
                    });
                    self.walk_statements(schema, schema_symbol, body)?;
                    self.loops.pop();
                }
            }
        }
        Ok(())
    }

    fn path(&mut self, schema: &str, path: &LogicPath) -> Result<(Scope, Scope), String> {
        let mut scope = FieldIndex::root(schema);
        if let Some(root) = &path.root {
            let spelling = path.root_spelled.as_deref().unwrap_or(root);
            let location = self.locations.token_after(&path.at, 1, spelling)?;
            if let Some(binding) = self.loops.iter().rev().find(|item| &item.name == root) {
                self.graph
                    .occurrence(binding.symbol, location, "reference")?;
                scope = binding.element.clone();
            } else if let Some(info) = self.fields.lookup(&scope, root) {
                self.graph.occurrence(info.symbol, location, "reference")?;
                scope = info.next.clone();
            }
        }
        let mut last_element = Scope::Unknown;
        for segment in &path.segments {
            if segment.variable {
                let location = self.locations.token(&segment.at, &segment.spelled)?;
                if let Some(binding) = self
                    .loops
                    .iter()
                    .rev()
                    .find(|item| item.name == segment.name)
                {
                    self.graph
                        .occurrence(binding.symbol, location, "reference")?;
                } else if let Some(info) =
                    self.fields.lookup(&FieldIndex::root(schema), &segment.name)
                {
                    self.graph.occurrence(info.symbol, location, "reference")?;
                }
                // A dynamic segment dispatches through data, so no one field
                // declaration owns this token. Refuse Rename for every field
                // that the current scope can select; editing only a subset of
                // its indirect uses would be unsafe.
                mark_dynamic_scope(self.fields, &scope, self.graph);
                scope = Scope::Unknown;
                last_element = Scope::Unknown;
                continue;
            }
            if matches!(scope, Scope::Unknown) {
                // A literal suffix after dynamic dispatch can denote several
                // declarations. It has no unique identity; the affected
                // subtree was already made non-renamable above.
                continue;
            }
            let info = self
                .fields
                .lookup(&scope, &segment.name)
                .ok_or("Validated logic field has no semantic identity.")?;
            self.graph.occurrence(
                info.symbol,
                self.locations.token(&segment.at, &segment.spelled)?,
                "reference",
            )?;
            last_element = info.element.clone();
            scope = info.next.clone();
        }
        Ok((scope, last_element))
    }

    fn derive_expr(&mut self, schema: &str, value: &DeriveExpr) -> Result<(), String> {
        match value {
            DeriveExpr::Path(path) => {
                self.path(schema, path)?;
            }
            DeriveExpr::Length { arg, .. } => self.length_arg(schema, arg)?,
            DeriveExpr::Variable { name, spelled, at } => {
                self.variable(schema, name, spelled, at, false)?
            }
            DeriveExpr::Value { .. } | DeriveExpr::Version { .. } => {}
        }
        Ok(())
    }

    fn condition(&mut self, schema: &str, condition: &Condition) -> Result<(), String> {
        match condition {
            Condition::Comparison { left, right, .. } => {
                self.operand(schema, left)?;
                self.operand(schema, right)?;
            }
            Condition::Exists { operand, .. } | Condition::Truth { operand, .. } => {
                self.operand(schema, operand)?
            }
            Condition::Not(inner) => self.condition(schema, inner)?,
            Condition::And(parts) | Condition::Or(parts) => {
                for part in parts {
                    self.condition(schema, part)?;
                }
            }
        }
        Ok(())
    }

    fn operand(&mut self, schema: &str, operand: &Operand) -> Result<(), String> {
        match operand {
            Operand::Path(path) => {
                self.path(schema, path)?;
            }
            Operand::Variable { name, spelled, at } => {
                self.variable(schema, name, spelled, at, true)?
            }
            Operand::Length { arg, .. } => self.length_arg(schema, arg)?,
            Operand::Group(inner) => self.condition(schema, inner)?,
            Operand::Version { .. } | Operand::Literal { .. } => {}
        }
        Ok(())
    }

    fn length_arg(&mut self, schema: &str, arg: &LengthArg) -> Result<(), String> {
        match arg {
            LengthArg::Path(path) => {
                self.path(schema, path)?;
            }
            LengthArg::Variable { name, spelled, at } => {
                self.variable(schema, name, spelled, at, false)?
            }
        }
        Ok(())
    }

    fn variable(
        &mut self,
        schema: &str,
        name: &str,
        spelling: &str,
        at: &Located,
        after_dollar: bool,
    ) -> Result<(), String> {
        let location = if after_dollar {
            self.locations.token_after(at, 1, spelling)?
        } else {
            self.locations.token(at, spelling)?
        };
        if let Some(binding) = self.loops.iter().rev().find(|item| item.name == name) {
            return self.graph.occurrence(binding.symbol, location, "reference");
        }
        if let Some(field) = self.fields.lookup(&FieldIndex::root(schema), name) {
            return self.graph.occurrence(field.symbol, location, "reference");
        }
        Ok(())
    }

    fn interpolations(
        &mut self,
        schema: &str,
        at: &Located,
        tail: InterpolationTail,
    ) -> Result<(), String> {
        let source = self
            .locations
            .sources
            .get(&at.file)
            .ok_or("Logic source is missing.")?;
        let start = self.locations.token_index(at)?;
        let mut end = start;
        while end < source.tokens.len() && !source.tokens[end].is_newline() {
            end += 1;
        }
        let line = &source.tokens[start..end];
        let marker = match tail {
            InterpolationTail::AfterEquals => {
                line.iter().position(|token| token.is_punctuation("="))
            }
            InterpolationTail::AfterThrow => {
                line.iter().position(|token| token.is_keyword("throw"))
            }
        };
        let tokens = marker
            .map(|index| line[index + 1..].to_vec())
            .unwrap_or_default();
        let loops = self.loops.clone();
        scan_interpolations(
            &tokens,
            &at.file,
            |name| {
                loops
                    .iter()
                    .rev()
                    .find(|item| item.name == name)
                    .map(|item| item.symbol)
                    .or_else(|| {
                        self.fields
                            .lookup(&FieldIndex::root(schema), name)
                            .map(|item| item.symbol)
                    })
            },
            self.graph,
            self.locations,
        )
    }
}

fn scan_interpolations(
    tokens: &[Token],
    file: &str,
    resolve: impl Fn(&str) -> Option<usize>,
    graph: &mut Graph,
    locations: &Locations<'_>,
) -> Result<(), String> {
    let source = locations
        .sources
        .get(file)
        .ok_or("Interpolation source is missing.")?;
    for token in tokens {
        if !matches!(
            token.kind,
            TokenKind::BareText(_) | TokenKind::QuotedString(_)
        ) {
            continue;
        }
        let raw = source
            .source
            .text
            .get(token.span.0..token.span.1)
            .ok_or("Interpolation token is outside its source.")?;
        let bytes = raw.as_bytes();
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            if bytes[cursor] != b'$' {
                cursor += 1;
                continue;
            }
            if bytes.get(cursor + 1) == Some(&b'$') {
                cursor += 2;
                continue;
            }
            let braced = bytes.get(cursor + 1) == Some(&b'{');
            let start = cursor + if braced { 2 } else { 1 };
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-'))
            {
                end += 1;
            }
            if end > start && (!braced || bytes.get(end) == Some(&b'}')) {
                let spelling = &raw[start..end];
                if let Some(symbol) = resolve(&normalise(spelling)) {
                    graph.occurrence(
                        symbol,
                        locations.span(file, token.span.0 + start, token.span.0 + end)?,
                        "reference",
                    )?;
                }
            }
            cursor = end.max(cursor + 1);
        }
    }
    Ok(())
}

fn scope_shape(scope: &Scope) -> &'static str {
    match scope {
        Scope::Fields { .. } => "object",
        Scope::Scalar => "scalar",
        Scope::Unknown => "unknown",
    }
}

#[derive(Clone, Copy)]
enum InterpolationTail {
    AfterEquals,
    AfterThrow,
}

fn mark_dynamic_scope(fields: &FieldIndex, scope: &Scope, graph: &mut Graph) {
    let Scope::Fields { schema, prefix } = scope else {
        return;
    };
    for ((candidate_schema, path), info) in &fields.entries {
        if candidate_schema == schema && path.starts_with(prefix) {
            graph.symbols[info.symbol].renamable = false;
        }
    }
}
