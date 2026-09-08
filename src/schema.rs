//! Schemas: `schema` declarations, field heads, types, defaults and the
//! `versions` declaration (SPEC chapter 4).
//!
//! [`parse_template`] reads one template file into schema declarations, logic
//! block frames and `versions` declarations, and reports every defect a single
//! file can show on its own: field heads, modifiers, types, ranges, `@tag`
//! placement, defaults and the reserved envelope names.
//!
//! Whatever needs the whole project — a duplicate schema name, a `$(Schema)`
//! or `ref(Schema)` target, a logic block's binding, a version annotation
//! measured against the project range — is reported by [`build_tables`] (P2)
//! and [`validate_schemas`] (P3), which see every template file at once.

use crate::ast::{Located, TemplateFile, TemplateItem};
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note, Position};
use crate::instance::{ListSpelling, SyntaxValue};
use crate::lexer::{is_identifier, normalise, Token, TokenKind};
use crate::limits::{BRACKET_DEPTH, GROUP_DEPTH};
use crate::logic::LogicBlock;
use crate::versions::{VersionRange, Window, MAX_PROJECT_VERSIONS};

// ---------------------------------------------------------------- the model

/// `versions min..max`, at most one per project (SPEC §4.12).
#[derive(Clone, Debug)]
pub struct VersionsDecl {
    pub range: VersionRange,
    pub at: Located,
}

/// One `schema Name { … }` declaration (SPEC §4.2).
#[derive(Clone, Debug)]
pub struct SchemaDecl {
    /// The schema name, kept exactly as declared: schema names are compared
    /// case-sensitively (SPEC §3.4).
    pub name: String,
    /// The declared fields, in declaration order, which fixes the output key
    /// order (SPEC §8.3). An explicitly declared `id` is not among them: it
    /// constrains the envelope key and is never an ordinary field (§4.11).
    pub fields: Vec<FieldDecl>,
    /// The ranges of an explicit `id: text(…)` declaration, when one is
    /// written. `Some(vec![])` is the unconstrained spelling `id: text`.
    pub id_ranges: Option<Vec<IntRange>>,
    pub at: Located,
}

/// The implicit declaration of the `id` field, `id: text(1..64)` (SPEC §4.11).
pub const IMPLICIT_ID_RANGE: IntRange = IntRange { min: 1, max: 64 };

impl SchemaDecl {
    /// The field a normalised name denotes, if the schema declares it.
    pub fn field(&self, name: &str) -> Option<&FieldDecl> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The declared field names, in declaration order. The envelope keys are
    /// never among them (SPEC §4.11).
    pub fn field_names(&self) -> Vec<&str> {
        self.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect()
    }

    /// The ranges an instance id must satisfy: the declared ones, or the
    /// implicit `1..64` (SPEC §4.11).
    pub fn id_ranges(&self) -> Vec<IntRange> {
        match &self.id_ranges {
            Some(ranges) if ranges.is_empty() => Vec::new(),
            Some(ranges) => ranges.clone(),
            None => vec![IMPLICIT_ID_RANGE],
        }
    }
}

/// One field of a schema or of a group (SPEC §4.3).
#[derive(Clone, Debug)]
pub struct FieldDecl {
    /// The normalised field name (SPEC §3.3); it fixes the output key.
    pub name: String,
    /// The name as the author spelled it, for diagnostics.
    pub spelled: String,
    /// Present when the head carries `[]`, with its optional cardinality.
    pub list: Option<Cardinality>,
    pub modifiers: Modifiers,
    pub kind: FieldKind,
    /// Where the type expression, or a group field's `{`, begins.
    pub type_at: Located,
    pub at: Located,
}

impl FieldDecl {
    pub fn is_list(&self) -> bool {
        self.list.is_some()
    }

    /// The cardinality a present value must satisfy; `[]` and a non-list field
    /// alike accept any count.
    pub fn cardinality(&self) -> Cardinality {
        self.list.unwrap_or(Cardinality::ANY)
    }

    pub fn is_optional(&self) -> bool {
        self.modifiers.optional
    }

    pub fn is_tag(&self) -> bool {
        self.modifiers.tag
    }

    /// Author nomination only; this does not establish an exported or runtime contract.
    pub fn is_public(&self) -> bool {
        self.modifiers.public_
    }

    /// The declared default, when the field has one.
    pub fn default(&self) -> Option<&DefaultValue> {
        match &self.kind {
            FieldKind::Scalar { default, .. } => default.as_ref(),
            FieldKind::Group { .. } => None,
        }
    }

    /// A field with neither `@optional` nor a default MUST have a value after
    /// logic runs (SPEC §4.10, E411).
    pub fn is_required(&self) -> bool {
        !self.is_optional() && self.default().is_none()
    }

    /// The declared type, or `None` for a group field.
    pub fn type_expr(&self) -> Option<&TypeExpr> {
        match &self.kind {
            FieldKind::Scalar { ty, .. } => Some(ty),
            FieldKind::Group { .. } => None,
        }
    }

    /// The fields of an inline group, or `None` for a scalar field.
    pub fn group_fields(&self) -> Option<&[FieldDecl]> {
        match &self.kind {
            FieldKind::Group { fields } => Some(fields),
            FieldKind::Scalar { .. } => None,
        }
    }

    /// The `@tag` field of this group, when it declares one (SPEC §4.8).
    pub fn tag_field(&self) -> Option<&FieldDecl> {
        self.group_fields()?.iter().find(|field| field.is_tag())
    }

    /// The `{kind}` substitution of SPEC §9.8 for this field.
    pub fn kind_word(&self) -> &'static str {
        if self.is_list() {
            return "list";
        }
        match &self.kind {
            FieldKind::Group { .. } => "object",
            FieldKind::Scalar { ty, .. } => ty.kind(),
        }
    }

    /// The versions this field's own annotations select, before any ancestor
    /// window is intersected in (SPEC §4.12). `None` selects no version.
    pub fn window_in(&self, project: VersionRange) -> Option<VersionRange> {
        self.modifiers.window.resolve(project)
    }

    /// True when this field exists in `version` by its own annotations alone.
    pub fn exists_in(&self, version: u32, project: VersionRange) -> bool {
        self.window_in(project)
            .map(|window| window.contains(version))
            .unwrap_or(false)
    }
}

/// A field is either a typed scalar/list field or an inline group.
#[derive(Clone, Debug)]
pub enum FieldKind {
    /// `name: <type> [= default]`.
    Scalar {
        ty: TypeExpr,
        default: Option<DefaultValue>,
    },
    /// `name { … }`: an inline nested object (SPEC §4.7).
    Group { fields: Vec<FieldDecl> },
}

/// A field default: the syntax value the same grammar instances use produces,
/// the text it was written as, and where it was written (SPEC §4.10).
#[derive(Clone, Debug)]
pub struct DefaultValue {
    pub value: SyntaxValue,
    /// The source spelling, for the `{value}` substitution of E313 and E425.
    pub text: String,
    pub at: Located,
}

/// `[]`, `[min..]` or `[min..max]` (SPEC §4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cardinality {
    pub min: u32,
    pub max: Option<u32>,
}

impl Cardinality {
    pub const ANY: Cardinality = Cardinality { min: 0, max: None };

    pub fn accepts(self, count: u32) -> bool {
        count >= self.min && self.max.map(|max| count <= max).unwrap_or(true)
    }

    /// The `{expected}` substitution of E445.
    pub fn describe(self) -> String {
        match self.max {
            Some(max) if max == self.min => format!("exactly {max}"),
            Some(max) => format!("{}..{max}", self.min),
            None if self.min == 0 => "any number of elements".to_string(),
            None => format!("at least {}", self.min),
        }
    }
}

/// The field modifiers of SPEC §4.3, each allowed at most once.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub optional: bool,
    pub tag: bool,
    /// Whether the author nominated this field with `@public` (SPEC §4.3).
    pub public_: bool,
    pub window: Window,
    /// Where `@tag` was written, for E310 and E322.
    pub tag_at: Option<Position>,
    /// Where the first `@public` was written; the owning field supplies its file.
    pub public_at: Option<Position>,
    /// Where `@since(n)` was written, for E603 and E604.
    pub since_at: Option<Position>,
    /// Where `@removed(n)` was written, for E603 and E604.
    pub removed_at: Option<Position>,
}

/// The nine types of SPEC §4.4.
#[derive(Clone, Debug)]
pub enum TypeExpr {
    Text {
        ranges: Vec<IntRange>,
    },
    Int {
        ranges: Vec<IntRange>,
    },
    Float {
        ranges: Vec<FloatRange>,
    },
    Bool,
    Enum {
        /// Normalised members, in declaration order.
        members: Vec<String>,
    },
    File {
        /// Canonicalised extensions (SPEC §4.4.6).
        extensions: Vec<String>,
    },
    Image {
        alternatives: Vec<ImageAlt>,
    },
    Ref {
        schema: String,
    },
    /// `$(Schema)`: a nested object validated against a named schema.
    Nested {
        schema: String,
    },
}

impl TypeExpr {
    /// The `{kind}` substitution of SPEC §9.8 for this type.
    pub fn kind(&self) -> &'static str {
        match self {
            TypeExpr::Text { .. } => "text",
            TypeExpr::Int { .. } => "int",
            TypeExpr::Float { .. } => "float",
            TypeExpr::Bool => "bool",
            TypeExpr::Enum { .. } => "enum",
            TypeExpr::File { .. } => "file",
            TypeExpr::Image { .. } => "image",
            TypeExpr::Ref { .. } => "ref",
            TypeExpr::Nested { .. } => "object",
        }
    }

    /// The schema name a `ref(…)` or `$(…)` type names, for E309.
    pub fn schema_reference(&self) -> Option<&str> {
        match self {
            TypeExpr::Ref { schema } | TypeExpr::Nested { schema } => Some(schema),
            _ => None,
        }
    }

    /// The extensions a `file` or `image` type accepts, canonicalised.
    pub fn extensions(&self) -> Vec<String> {
        match self {
            TypeExpr::File { extensions } => extensions.clone(),
            TypeExpr::Image { alternatives } => {
                let mut out: Vec<String> = Vec::new();
                for alternative in alternatives {
                    if !out.contains(&alternative.extension) {
                        out.push(alternative.extension.clone());
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }
}

/// A closed integer interval, or an exact value when `min == max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntRange {
    pub min: i64,
    pub max: i64,
}

impl IntRange {
    pub fn contains(self, value: i64) -> bool {
        value >= self.min && value <= self.max
    }

    /// The spelling used in the `{ranges}` substitution of E413.
    pub fn describe(self) -> String {
        if self.min == self.max {
            self.min.to_string()
        } else {
            format!("{}..{}", self.min, self.max)
        }
    }
}

/// A closed float interval. Both bounds must be finite (SPEC §4.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatRange {
    pub min: f64,
    pub max: f64,
}

impl FloatRange {
    pub fn contains(self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }
}

/// One alternative of an `image(...)` type: an extension and an optional
/// `WIDTHxHEIGHT` constraint where `None` spells `*` (SPEC §4.4.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageAlt {
    pub extension: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl ImageAlt {
    /// The spelling used in the `{list}` substitution of E423.
    pub fn describe(&self) -> String {
        match (self.width, self.height) {
            (None, None) => self.extension.clone(),
            (width, height) => format!(
                "{} {}x{}",
                self.extension,
                width.map(|v| v.to_string()).unwrap_or_else(|| "*".into()),
                height.map(|v| v.to_string()).unwrap_or_else(|| "*".into()),
            ),
        }
    }
}

/// The image formats SPEC §4.4.7 supports, in the order its messages list.
pub const IMAGE_FORMATS: [&str; 5] = ["png", "jpg", "gif", "bmp", "webp"];

/// Canonicalises a `file`/`image` extension: strip one leading `.`,
/// ASCII-lowercase, and map `jpeg` to `jpg` (SPEC §4.4.6). No other spellings
/// are folded.
pub fn canonical_extension(text: &str) -> String {
    let trimmed = text.strip_prefix('.').unwrap_or(text).to_ascii_lowercase();
    if trimmed == "jpeg" {
        "jpg".to_string()
    } else {
        trimmed
    }
}

// ------------------------------------------------------------ project tables

/// The template half of the project tables of SPEC §7.2 (P2).
#[derive(Clone, Debug)]
pub struct TemplateTables {
    /// Every schema, in the source order of SPEC §2.4.
    pub schemas: Vec<SchemaDecl>,
    /// Every logic block, in the same order, each bound to a declared schema.
    pub logic: Vec<LogicBlock>,
    /// The project version range; `1..1` when no declaration is present.
    pub versions: VersionRange,
    /// Where the range was declared, when it was.
    pub versions_at: Option<Located>,
}

impl TemplateTables {
    pub fn schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.iter().find(|schema| schema.name == name)
    }

    pub fn logic_for(&self, name: &str) -> Option<&LogicBlock> {
        self.logic.iter().find(|block| block.schema == name)
    }

    /// Declared schema names, in declaration order.
    pub fn schema_names(&self) -> Vec<&str> {
        self.schemas
            .iter()
            .map(|schema| schema.name.as_str())
            .collect()
    }
}

/// Builds the project tables from the parsed template files (SPEC §7.2, P2).
///
/// Reports E601 and E602 for the version range, E301 for a repeated schema
/// name, and E501 and E502 for the logic blocks' binding.
pub fn build_tables(files: &[TemplateFile]) -> Result<TemplateTables, Diagnostics> {
    let mut errors = Diagnostics::new();
    let mut schemas: Vec<SchemaDecl> = Vec::new();
    let mut blocks: Vec<LogicBlock> = Vec::new();
    let mut versions = VersionRange::DEFAULT;
    let mut versions_at: Option<Located> = None;

    for file in files {
        for item in &file.items {
            match item {
                TemplateItem::Versions(decl) => match &versions_at {
                    Some(first) => errors.push(
                        Diagnostic::at(
                            ErrorId::E601,
                            decl.at.file.clone(),
                            decl.at.position,
                            "The project already declares a version range.",
                        )
                        .with_note(
                            Note::new("first declared here.")
                                .at(first.file.clone(), first.position),
                        ),
                    ),
                    None => {
                        versions = decl.range;
                        versions_at = Some(decl.at.clone());
                    }
                },
                TemplateItem::Schema(schema) => {
                    match schemas.iter().find(|other| other.name == schema.name) {
                        Some(first) => errors.push(
                            Diagnostic::at(
                                ErrorId::E301,
                                schema.at.file.clone(),
                                schema.at.position,
                                format!("Schema '{}' is already declared.", schema.name),
                            )
                            .with_note(
                                Note::new("first declared here.")
                                    .at(first.at.file.clone(), first.at.position),
                            ),
                        ),
                        None => schemas.push(schema.clone()),
                    }
                }
                TemplateItem::Logic(block) => blocks.push(block.clone()),
            }
        }
    }

    // A logic block binds to a declared schema; the schema table is complete
    // before any block is checked, so forward and cross-file blocks bind
    // (SPEC §6.1).
    let mut bound: Vec<LogicBlock> = Vec::new();
    for block in blocks {
        if !schemas.iter().any(|schema| schema.name == block.schema) {
            let names = schemas
                .iter()
                .map(|schema| schema.name.as_str())
                .collect::<Vec<_>>();
            let mut message = format!("Unknown schema '{}' in logic block.", block.schema);
            if let Some(hint) = suggest(&block.schema, &names, TieBreak::ScalarOrder) {
                message.push_str(&format!(" Did you mean '{hint}'?"));
            }
            errors.push(Diagnostic::at(
                ErrorId::E501,
                block.at.file.clone(),
                block.at.position,
                message,
            ));
            continue;
        }
        if let Some(first) = bound.iter().find(|other| other.schema == block.schema) {
            errors.push(
                Diagnostic::at(
                    ErrorId::E502,
                    block.at.file.clone(),
                    block.at.position,
                    format!("Schema '{}' already has a logic block.", block.schema),
                )
                .with_note(
                    Note::new("first declared here.").at(first.at.file.clone(), first.at.position),
                ),
            );
            continue;
        }
        bound.push(block);
    }

    if errors.is_empty() {
        Ok(TemplateTables {
            schemas,
            logic: bound,
            versions,
            versions_at,
        })
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------- suggestions

/// How ties are broken among equally close suggestions (SPEC §9.8 step 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieBreak {
    /// Candidate sets with a declaration order: fields and enum members.
    DeclarationOrder,
    /// Everything else, compared as sequences of Unicode scalar values.
    ScalarOrder,
}

/// The `Did you mean '{suggestion}'?` candidate of SPEC §9.8, or `None` when
/// no candidate survives. The algorithm is normative because suggestions
/// appear in byte-exact golden output.
pub fn suggest(text: &str, candidates: &[&str], ties: TieBreak) -> Option<String> {
    let offending = normalise(text);
    let length = offending.chars().count();
    let mut best: Option<(usize, usize, &str)> = None;
    for (order, candidate) in candidates.iter().enumerate() {
        let distance = levenshtein(&offending, &normalise(candidate));
        if distance > 2 || distance * 2 > length {
            continue;
        }
        let better = match best {
            None => true,
            Some((best_distance, best_order, best_text)) => {
                if distance != best_distance {
                    distance < best_distance
                } else {
                    match ties {
                        TieBreak::DeclarationOrder => order < best_order,
                        TieBreak::ScalarOrder => candidate.chars().lt(best_text.chars()),
                    }
                }
            }
        };
        if better {
            best = Some((distance, order, candidate));
        }
    }
    best.map(|(_, _, candidate)| candidate.to_string())
}

/// Levenshtein distance counting Unicode scalar values (SPEC §9.8 step 2).
fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current: Vec<usize> = vec![0; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_char != right_char);
            let deletion = previous[column + 1] + 1;
            let insertion = current[column] + 1;
            current[column + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

// -------------------------------------------------------------- token helpers

/// The source spelling of one token, used to quote a construct back at the
/// author. Adjacency (SPEC §3.1) restores the original spacing.
fn spelling(token: &Token) -> String {
    match &token.kind {
        TokenKind::Identifier(text)
        | TokenKind::SchemaName(text)
        | TokenKind::IntLiteral(text)
        | TokenKind::FloatLiteral(text)
        | TokenKind::BareText(text)
        | TokenKind::SizeToken(text) => text.clone(),
        TokenKind::Punctuation(text) => (*text).to_string(),
        TokenKind::QuotedString(text) => format!("\"{text}\""),
        TokenKind::Newline | TokenKind::EndOfFile => String::new(),
    }
}

/// The source text a run of tokens was written as. A token that is not glued
/// to its predecessor was separated by whitespace, which is restored as one
/// space; that is enough for a diagnostic to quote the construct back.
fn join_source(tokens: &[Token]) -> String {
    let mut out = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 && !token.glued {
            out.push(' ');
        }
        out.push_str(&spelling(token));
    }
    out
}

/// JSON string escaping for the `{value}` substitution (SPEC §8.8, §9.8).
fn quote_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Renders a comma-separated list substitution (SPEC §9.8).
fn list_of<T: AsRef<str>>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| item.as_ref())
        .collect::<Vec<_>>()
        .join(", ")
}

/// True when `text` carries an interpolation reference, so its content cannot
/// be checked until it is filled in (SPEC §4.10, §5.11). `$$` is a literal
/// `$` and is not a reference.
fn carries_interpolation(text: &str) -> bool {
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '$' {
            index += 1;
            continue;
        }
        match bytes.get(index + 1) {
            Some('$') => index += 2,
            _ => return true,
        }
    }
    false
}

/// The trailing `@modifier` run of a default lexeme, when there is one
/// (SPEC §4.3). Schema defaults keep their trailing annotations, so this is
/// where a modifier written after the `=` is found.
fn trailing_modifier_run(text: &str) -> Option<(usize, String)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (position, (offset, ch)) in chars.iter().enumerate() {
        if *ch != '@' || position == 0 {
            continue;
        }
        if !matches!(chars[position - 1].1, ' ' | '\t') {
            continue;
        }
        let rest = &text[*offset..];
        if is_modifier_run(rest) {
            return Some((*offset, rest.to_string()));
        }
    }
    None
}

/// True when `text` is one or more whitespace-separated `@name` or
/// `@name(digits)` groups and nothing else.
fn is_modifier_run(text: &str) -> bool {
    let mut any = false;
    for part in text.split([' ', '\t']) {
        if part.is_empty() {
            continue;
        }
        any = true;
        let Some(body) = part.strip_prefix('@') else {
            return false;
        };
        let name = match body.split_once('(') {
            Some((name, arguments)) => {
                let Some(digits) = arguments.strip_suffix(')') else {
                    return false;
                };
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return false;
                }
                name
            }
            None => body,
        };
        if name.is_empty() || !is_identifier(name) {
            return false;
        }
    }
    any
}

// -------------------------------------------------------------- the parser

/// Parses the `schema`, `logic` and `versions` items of one template file
/// (SPEC §4.1).
///
/// `text` is the file's source, which every token's span indexes into. A
/// diagnostic that must quote a construct **as written** slices it from there
/// rather than re-spelling it from the token stream.
pub fn parse_template(
    file: &str,
    text: &str,
    tokens: &[Token],
) -> Result<TemplateFile, Diagnostics> {
    let mut parser = Parser::new(file, text, tokens);
    let items = parser.parse_items();
    if parser.errors.is_empty() {
        let mut template = TemplateFile::new(file);
        template.items = items;
        Ok(template)
    } else {
        Err(parser.errors)
    }
}

struct Parser<'a> {
    file: &'a str,
    /// The file's source text; token spans index into it.
    text: &'a str,
    tokens: &'a [Token],
    index: usize,
    errors: Diagnostics,
    end: Token,
    depth_reported: bool,
}

impl<'a> Parser<'a> {
    fn new(file: &'a str, text: &'a str, tokens: &'a [Token]) -> Self {
        let position = tokens
            .last()
            .map(|token| token.position)
            .unwrap_or_else(|| Position::new(1, 1));
        Self {
            file,
            text,
            tokens,
            index: 0,
            errors: Diagnostics::new(),
            end: Token::new(TokenKind::EndOfFile, position, (0, 0)),
            depth_reported: false,
        }
    }

    // ------------------------------------------------------------- cursor

    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&self.end)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn at_end(&self) -> bool {
        self.peek().is_end_of_file()
    }

    fn at_newline(&self) -> bool {
        self.peek().is_newline()
    }

    fn at_punctuation(&self, spelling: &str) -> bool {
        self.peek().is_punctuation(spelling)
    }

    fn located(&self, token: &Token) -> Located {
        Located::new(self.file, token.position)
    }

    fn here(&self) -> Located {
        Located::new(self.file, self.peek().position)
    }

    fn error(&mut self, id: ErrorId, at: &Located, message: impl Into<String>) {
        self.errors
            .push(Diagnostic::at(id, at.file.clone(), at.position, message));
    }

    fn error_with_note(
        &mut self,
        id: ErrorId,
        at: &Located,
        message: impl Into<String>,
        note: Note,
    ) {
        self.errors
            .push(Diagnostic::at(id, at.file.clone(), at.position, message).with_note(note));
    }

    /// E210 at the current token, naming what was expected instead.
    fn unexpected(&mut self, expected: &str) {
        let token = self.peek().clone();
        let at = self.located(&token);
        self.error(
            ErrorId::E210,
            &at,
            format!("Unexpected {} here; expected {expected}.", token.describe()),
        );
    }

    fn skip_newlines(&mut self) {
        while self.at_newline() {
            self.bump();
        }
    }

    /// Consumes everything up to and including the next `NL`.
    fn skip_line(&mut self) {
        while !self.at_end() && !self.at_newline() {
            self.bump();
        }
        if self.at_newline() {
            self.bump();
        }
    }

    /// The text a malformed field declaration wrote where the name belongs:
    /// everything up to the `:` that introduces the type, or to the end of
    /// the logical line when there is none.
    fn field_name_text(&self) -> String {
        let mut end = self.index;
        while end < self.tokens.len() {
            let token = &self.tokens[end];
            if token.is_punctuation(":") || matches!(token.kind, TokenKind::Newline) {
                break;
            }
            end += 1;
        }
        join_source(&self.tokens[self.index..end])
    }

    /// Consumes a balanced `{ … }` block without recursing, so that a file
    /// nested far deeper than SPEC §3.7 allows costs no stack. The cursor is
    /// on the opening `{`.
    fn skip_block(&mut self) {
        if !self.at_punctuation("{") {
            return;
        }
        self.bump();
        self.skip_block_body();
    }

    /// Consumes the rest of a block whose opening `{` was already read.
    fn skip_block_body(&mut self) {
        let mut depth = 1usize;
        loop {
            if self.at_end() {
                return;
            }
            if self.at_punctuation("{") {
                depth += 1;
            } else if self.at_punctuation("}") {
                depth -= 1;
                self.bump();
                if depth == 0 {
                    self.skip_line();
                    return;
                }
                continue;
            }
            self.bump();
        }
    }

    /// After an unrecognised top-level construct, resumes at the next line
    /// that starts a template item, so one junk region reports one error.
    fn resume_at_template_item(&mut self) {
        loop {
            self.skip_line();
            if self.at_end() {
                return;
            }
            self.skip_newlines();
            let token = self.peek();
            if token.is_end_of_file()
                || token.is_keyword("schema")
                || token.is_keyword("logic")
                || token.is_keyword("versions")
            {
                return;
            }
        }
    }

    // ------------------------------------------------------- template items

    fn parse_items(&mut self) -> Vec<TemplateItem> {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() {
                return items;
            }
            let token = self.peek().clone();
            if token.is_keyword("versions") {
                if let Some(item) = self.parse_versions() {
                    items.push(TemplateItem::Versions(item));
                }
            } else if token.is_keyword("schema") {
                if let Some(item) = self.parse_schema() {
                    items.push(TemplateItem::Schema(item));
                }
            } else if token.is_keyword("logic") {
                if let Some(item) = self.parse_logic() {
                    items.push(TemplateItem::Logic(item));
                }
            } else {
                self.unexpected("'schema', 'logic' or 'versions'");
                self.resume_at_template_item();
            }
        }
    }

    /// `versions <min> .. <max>` (SPEC §4.12).
    fn parse_versions(&mut self) -> Option<VersionsDecl> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let start = self.index;
        let min = self.peek().clone();
        if min.identifier_text().is_none() {
            self.unexpected("a version number");
            self.skip_line();
            return None;
        }
        self.bump();
        if !self.at_punctuation("..") {
            self.unexpected("'..'");
            self.skip_line();
            return None;
        }
        self.bump();
        let max = self.peek().clone();
        if max.identifier_text().is_none() {
            self.unexpected("a version number");
            self.skip_line();
            return None;
        }
        self.bump();
        let text = join_source(self.tokens.get(start..self.index).unwrap_or(&[]));
        if !self.at_newline() && !self.at_end() {
            self.unexpected("end of line");
            self.skip_line();
            return None;
        }
        self.skip_line();

        // The numbers are read as `u128` so that an over-large one is told
        // apart from a malformed one and each is reported with a message that
        // describes the rule it broke.
        let bounds = min
            .identifier_text()
            .and_then(|text| text.parse::<u128>().ok())
            .zip(
                max.identifier_text()
                    .and_then(|text| text.parse::<u128>().ok()),
            )
            .filter(|(min, max)| *min >= 1 && min <= max);
        let Some((min, max)) = bounds else {
            self.error(
                ErrorId::E602,
                &at,
                format!("Invalid version range '{text}'; both are integers >= 1 and min <= max."),
            );
            return None;
        };
        if max - min + 1 > u128::from(MAX_PROJECT_VERSIONS) {
            self.error(
                ErrorId::E602,
                &at,
                format!(
                    "Invalid version range '{text}'; a project range covers at most {MAX_PROJECT_VERSIONS} versions."
                ),
            );
            return None;
        }
        if max > u128::from(u32::MAX) {
            self.error(
                ErrorId::E602,
                &at,
                format!(
                    "Invalid version range '{text}'; a version number is at most {}.",
                    u32::MAX
                ),
            );
            return None;
        }
        let range = VersionRange::new(min as u32, max as u32)?;
        Some(VersionsDecl { range, at })
    }

    /// `logic <SchemaName> { … }` (SPEC §6.1). The binding and the frame are
    /// this file's grammar; the statements inside the braces are chapter 6's
    /// and are read by [`crate::logic::parse_block`].
    fn parse_logic(&mut self) -> Option<LogicBlock> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let Some(name) = self.parse_schema_name() else {
            self.resume_at_template_item();
            return None;
        };
        if !self.at_punctuation("{") {
            self.unexpected("'{'");
            self.resume_at_template_item();
            return None;
        }
        let (statements, index, errors) =
            crate::logic::parse_block(self.file, self.text, self.tokens, self.index);
        self.index = index;
        self.errors.extend(errors);
        Some(LogicBlock {
            schema: name,
            statements,
            at,
        })
    }

    fn parse_schema_name(&mut self) -> Option<String> {
        match &self.peek().kind {
            TokenKind::SchemaName(name) => {
                let name = name.clone();
                self.bump();
                Some(name)
            }
            _ => {
                self.unexpected("a schema name");
                None
            }
        }
    }

    /// `schema <SchemaName> { <field_list> }` (SPEC §4.2).
    fn parse_schema(&mut self) -> Option<SchemaDecl> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let Some(name) = self.parse_schema_name() else {
            self.resume_at_template_item();
            return None;
        };
        if !self.at_punctuation("{") {
            self.unexpected("'{'");
            self.resume_at_template_item();
            return None;
        }
        self.bump();
        if !self.at_newline() {
            self.unexpected("end of line");
            self.skip_block_body();
            return None;
        }
        self.bump();

        let mut schema = SchemaDecl {
            name,
            fields: Vec::new(),
            id_ranges: None,
            at,
        };
        let mut block = Block::new(None, true);
        self.parse_field_list(&mut schema, &mut block, 1);
        schema.fields = block.fields;
        // The closing `}` must be alone on its logical line (SPEC §4.2).
        if self.at_punctuation("}") {
            self.bump();
            if !self.at_newline() && !self.at_end() {
                self.unexpected("end of line");
            }
            self.skip_line();
        }
        Some(schema)
    }

    // ----------------------------------------------------------- field list

    fn parse_field_list(&mut self, schema: &mut SchemaDecl, block: &mut Block, depth: usize) {
        loop {
            self.skip_newlines();
            if self.at_end() || self.at_punctuation("}") {
                return;
            }
            self.parse_field(schema, block, depth);
        }
    }

    fn parse_field(&mut self, schema: &mut SchemaDecl, block: &mut Block, depth: usize) {
        let head_start = self.index;
        let name_token = self.peek().clone();
        let at = self.located(&name_token);
        let Some(spelled) = name_token.identifier_text().map(str::to_string) else {
            // Inside a schema body every declaration starts with a field name
            // (SPEC §4.3), so a token that cannot be one is an invalid field
            // name rather than a stray token: SPEC §4.13 maps that to E316.
            let text = self.field_name_text();
            self.error(ErrorId::E316, &at, format!("Invalid field name '{text}'."));
            self.skip_line();
            return;
        };
        if !is_identifier(&spelled) {
            self.error(
                ErrorId::E316,
                &at,
                format!("Invalid field name '{spelled}'."),
            );
            self.skip_line();
            return;
        }
        self.bump();
        let name = normalise(&spelled);

        let list = self.parse_list_head(&spelled, &at);
        // `name[][]` is E315: a field is a list or it is not (SPEC §4.6).
        if self.at_punctuation("[") {
            self.error(
                ErrorId::E315,
                &at,
                format!("'{spelled}[][]' is not valid; a field is a list or it is not."),
            );
            self.skip_line();
            return;
        }

        let mut modifiers = Modifiers::default();
        let head_modifiers = self.index;
        self.parse_modifiers(&mut modifiers);
        let modifiers_before_colon = self.index > head_modifiers;

        if self.at_punctuation("{") {
            self.parse_group_field(
                schema, block, depth, head_start, spelled, name, at, list, modifiers,
            );
            return;
        }
        if modifiers_before_colon {
            let token = self
                .tokens
                .get(head_modifiers)
                .cloned()
                .unwrap_or_else(|| self.end.clone());
            let position = self.located(&token);
            self.error(
                ErrorId::E210,
                &position,
                format!(
                    "Unexpected {} here; expected ':' or '{{'.",
                    token.describe()
                ),
            );
        }
        if !self.at_punctuation(":") {
            self.unexpected("':' or '{'");
            self.skip_line();
            return;
        }
        self.bump();
        self.parse_scalar_field(
            schema, block, head_start, spelled, name, at, list, modifiers,
        );
    }

    /// `[ ]`, `[min..]` or `[min..max]`, one lexeme with no internal
    /// whitespace (SPEC §3.1, §4.6).
    fn parse_list_head(&mut self, spelled: &str, at: &Located) -> Option<Cardinality> {
        if !self.at_punctuation("[") {
            return None;
        }
        let start = self.index;
        // The `[` itself must be glued to the field name: SPEC §4.3 marks a
        // list field with `[]` *immediately* after the name, and SPEC §3.1
        // makes the whole head one lexeme.
        let mut glued = self.peek().glued;
        self.bump();
        while !self.at_end() && !self.at_newline() && !self.at_punctuation("]") {
            glued &= self.peek().glued;
            self.bump();
        }
        if !self.at_punctuation("]") {
            self.unexpected("']'");
            return None;
        }
        glued &= self.peek().glued;
        self.bump();
        let inner = self.tokens.get(start + 1..self.index - 1).unwrap_or(&[]);
        let text = join_source(self.tokens.get(start..self.index).unwrap_or(&[]));
        if !glued {
            self.error(
                ErrorId::E210,
                at,
                format!("Unexpected whitespace here; expected the list head '{spelled}{text}' to be written as one lexeme."),
            );
            return Some(Cardinality::ANY);
        }
        match parse_cardinality(inner) {
            Some(cardinality) => Some(cardinality),
            None => {
                self.error(
                    ErrorId::E323,
                    at,
                    format!(
                        "Invalid list cardinality '{text}'; write '[]', '[min..]' or '[min..max]' with 0 <= min <= max."
                    ),
                );
                Some(Cardinality::ANY)
            }
        }
    }

    /// `{ "@optional" | "@tag" | "@public" | "@since(n)" | "@removed(n)" }` (SPEC §4.3).
    fn parse_modifiers(&mut self, modifiers: &mut Modifiers) {
        while self.at_punctuation("@") {
            let sign = self.bump();
            let at = self.located(&sign);
            let word = self.peek().clone();
            let Some(text) = word.identifier_text().map(str::to_string) else {
                self.unexpected("a modifier name");
                return;
            };
            self.bump();
            match text.as_str() {
                "optional" => {
                    if modifiers.optional {
                        self.error(
                            ErrorId::E303,
                            &at,
                            "Modifier '@optional' is repeated.".to_string(),
                        );
                    }
                    modifiers.optional = true;
                }
                "tag" => {
                    if modifiers.tag {
                        self.error(
                            ErrorId::E303,
                            &at,
                            "Modifier '@tag' is repeated.".to_string(),
                        );
                    }
                    modifiers.tag = true;
                    modifiers.tag_at = Some(at.position);
                }
                "public" => {
                    if modifiers.public_ {
                        self.error(
                            ErrorId::E303,
                            &at,
                            "Modifier '@public' is repeated.".to_string(),
                        );
                    }
                    modifiers.public_ = true;
                    modifiers.public_at.get_or_insert(at.position);
                }
                "since" | "removed" => {
                    let Some(number) = self.parse_annotation_argument(&text) else {
                        return;
                    };
                    let repeated = if text == "since" {
                        modifiers.window.since.is_some()
                    } else {
                        modifiers.window.removed.is_some()
                    };
                    if repeated {
                        self.error(
                            ErrorId::E303,
                            &at,
                            format!("Modifier '@{text}' is repeated."),
                        );
                    }
                    if text == "since" {
                        modifiers.window.since = Some(number);
                        modifiers.since_at = Some(at.position);
                    } else {
                        modifiers.window.removed = Some(number);
                        modifiers.removed_at = Some(at.position);
                    }
                }
                other => {
                    self.error(
                        ErrorId::E303,
                        &at,
                        format!("Unknown field modifier '@{other}'."),
                    );
                    // An unknown modifier may carry an argument list; skipping
                    // it keeps the rest of the declaration from cascading.
                    if self.at_punctuation("(") && self.peek().glued {
                        self.skip_arguments();
                    }
                }
            }
        }
    }

    /// `( <version_number> )`, glued to the annotation keyword (SPEC §3.1).
    /// The argument list is always consumed, so one mistake reports once.
    fn parse_annotation_argument(&mut self, name: &str) -> Option<u32> {
        if !self.at_punctuation("(") {
            self.unexpected("'('");
            return None;
        }
        if !self.peek().glued {
            let at = self.here();
            self.error(
                ErrorId::E210,
                &at,
                format!("Unexpected whitespace here; expected '(' immediately after '@{name}'."),
            );
            self.skip_arguments();
            return None;
        }
        self.bump();
        let number = self.peek().clone();
        let value = number
            .identifier_text()
            .and_then(|text| text.parse::<u32>().ok());
        if value.is_none() {
            self.unexpected("a version number");
            while !self.at_end() && !self.at_newline() && !self.at_punctuation(")") {
                self.bump();
            }
            if self.at_punctuation(")") {
                self.bump();
            }
            return None;
        }
        self.bump();
        if !self.at_punctuation(")") {
            self.unexpected("')'");
            return None;
        }
        self.bump();
        value
    }

    // --------------------------------------------------------- group fields

    #[allow(clippy::too_many_arguments)]
    fn parse_group_field(
        &mut self,
        schema: &mut SchemaDecl,
        block: &mut Block,
        depth: usize,
        head_start: usize,
        spelled: String,
        name: String,
        at: Located,
        list: Option<Cardinality>,
        modifiers: Modifiers,
    ) {
        let type_at = self.here();
        self.bump();
        if !self.at_newline() {
            self.unexpected("end of line");
            self.skip_block_body();
            return;
        }
        self.bump();

        let mut fields = Vec::new();
        if depth > GROUP_DEPTH {
            if !self.depth_reported {
                self.depth_reported = true;
                self.error(
                    ErrorId::E209,
                    &at,
                    format!("Schema group nesting exceeds the limit of {GROUP_DEPTH}."),
                );
            }
            self.skip_block_body();
        } else {
            let mut inner = Block::new(Some(spelled.clone()), false);
            self.parse_field_list(schema, &mut inner, depth + 1);
            fields = inner.fields;
            if self.at_punctuation("}") {
                self.bump();
                if !self.at_newline() && !self.at_end() {
                    // `owner { … } = x` is not expressible (SPEC §4.7).
                    if self.at_punctuation("=") {
                        let position = self.here();
                        self.error(
                            ErrorId::E312,
                            &position,
                            "A group field cannot declare a default.".to_string(),
                        );
                    } else {
                        self.unexpected("end of line");
                    }
                }
                self.skip_line();
            }
        }

        let field = FieldDecl {
            name,
            spelled,
            list,
            modifiers,
            kind: FieldKind::Group { fields },
            type_at,
            at,
        };
        let head = self.tokens.get(head_start..self.index).unwrap_or(&[]);
        self.finish_field(schema, block, field, head);
    }

    // -------------------------------------------------------- scalar fields

    #[allow(clippy::too_many_arguments)]
    fn parse_scalar_field(
        &mut self,
        schema: &mut SchemaDecl,
        block: &mut Block,
        head_start: usize,
        spelled: String,
        name: String,
        at: Located,
        list: Option<Cardinality>,
        mut modifiers: Modifiers,
    ) {
        let type_at = self.here();
        let context = block.context(&schema.name);
        let Some(ty) = self.parse_type(&context, &spelled) else {
            // A `{` in the type position opens a block the lexer already
            // matched; consuming it keeps its `}` from closing the schema.
            if self.at_punctuation("{") {
                self.skip_block();
            } else {
                self.skip_line();
            }
            return;
        };
        self.parse_modifiers(&mut modifiers);

        let mut default = None;
        if self.at_punctuation("=") {
            let equals = self.bump();
            let value_at = self.located(self.tokens.get(self.index).unwrap_or(&equals));
            let start = self.index;
            while !self.at_end() && !self.at_newline() {
                self.bump();
            }
            let tokens = self.tokens.get(start..self.index).unwrap_or(&[]).to_vec();
            let text = join_source(&tokens);
            if tokens.is_empty() {
                self.unexpected("a default value");
            } else if let Some(value) = self.parse_value(&tokens, &value_at) {
                default = Some(DefaultValue {
                    value,
                    text,
                    at: value_at,
                });
            }
        }
        if !self.at_newline() && !self.at_end() {
            self.unexpected("end of line, a modifier or '='");
        }
        self.skip_line();

        let field = FieldDecl {
            name,
            spelled,
            list,
            modifiers,
            kind: FieldKind::Scalar { ty, default },
            type_at,
            at,
        };
        let head = self.tokens.get(head_start..self.index).unwrap_or(&[]);
        self.finish_field(schema, block, field, head);
    }

    // ---------------------------------------------------------------- types

    fn parse_type(&mut self, context: &str, field: &str) -> Option<TypeExpr> {
        let token = self.peek().clone();
        let at = self.located(&token);
        if token.is_punctuation("$") {
            return self.parse_nested_type(&at);
        }
        let Some(word) = token.identifier_text().map(str::to_string) else {
            self.error(
                ErrorId::E304,
                &at,
                format!(
                    "Unknown type '{}'; expected text, int, float, bool, enum, file, image, ref or $(Schema).",
                    spelling(&token)
                ),
            );
            return None;
        };
        let known = matches!(
            word.as_str(),
            "text" | "int" | "float" | "bool" | "enum" | "file" | "image" | "ref"
        );
        if !known {
            self.error(
                ErrorId::E304,
                &at,
                format!(
                    "Unknown type '{word}'; expected text, int, float, bool, enum, file, image, ref or $(Schema)."
                ),
            );
            return None;
        }
        let keyword_index = self.index;
        self.bump();
        // A parameterised type's `(` is glued to its keyword; whitespace
        // before it is E304 (SPEC §3.1, §4.4).
        if self.at_punctuation("(") && !self.peek().glued {
            let text = self.spaced_type_text(keyword_index);
            self.error(
                ErrorId::E304,
                &at,
                format!(
                    "Unknown type '{text}'; expected text, int, float, bool, enum, file, image, ref or $(Schema)."
                ),
            );
            self.skip_arguments();
            return None;
        }
        let arguments = self.at_punctuation("(");
        match word.as_str() {
            "bool" => {
                if arguments {
                    self.unexpected("end of line, a modifier or '='");
                    self.skip_arguments();
                    return None;
                }
                Some(TypeExpr::Bool)
            }
            "text" | "int" => {
                let ranges = if arguments {
                    self.parse_int_ranges()?
                } else {
                    Vec::new()
                };
                if word == "text" {
                    for range in &ranges {
                        if range.min < 0 || range.max < 0 {
                            self.error(
                                ErrorId::E318,
                                &at,
                                format!(
                                    "Constraint '{}' on {context}.{field} can never be satisfied.",
                                    range.describe()
                                ),
                            );
                        }
                    }
                    Some(TypeExpr::Text { ranges })
                } else {
                    Some(TypeExpr::Int { ranges })
                }
            }
            "float" => {
                let ranges = if arguments {
                    self.parse_float_ranges()?
                } else {
                    Vec::new()
                };
                Some(TypeExpr::Float { ranges })
            }
            "enum" => self.parse_enum_type(&at, arguments),
            "file" => self.parse_file_type(&at, arguments),
            "image" => self.parse_image_type(&at, context, field, arguments),
            _ => self.parse_ref_type(&at, arguments),
        }
    }

    /// The source text of a type keyword and the argument list a space
    /// separated from it, for the E304 message.
    fn spaced_type_text(&self, keyword_index: usize) -> String {
        let mut end = self.index;
        let mut depth = 0usize;
        while let Some(token) = self.tokens.get(end) {
            if token.is_newline() || token.is_end_of_file() {
                break;
            }
            if token.is_punctuation("(") {
                depth += 1;
            }
            end += 1;
            if token.is_punctuation(")") {
                depth -= usize::from(depth > 0);
                if depth == 0 {
                    break;
                }
            }
        }
        join_source(self.tokens.get(keyword_index..end).unwrap_or(&[]))
    }

    /// Consumes a balanced argument list after an error, so the rest of the
    /// line does not cascade.
    fn skip_arguments(&mut self) {
        if !self.at_punctuation("(") {
            return;
        }
        let mut depth = 0usize;
        while !self.at_end() && !self.at_newline() {
            if self.at_punctuation("(") {
                depth += 1;
            }
            let closing = self.at_punctuation(")");
            self.bump();
            if closing {
                depth -= usize::from(depth > 0);
                if depth == 0 {
                    return;
                }
            }
        }
    }

    /// The argument tokens of a parameterised type, split at depth-0 commas.
    /// `None` means the list was malformed and was already reported.
    fn parse_arguments(&mut self) -> Option<Vec<Vec<Token>>> {
        let open = self.bump();
        let start = self.index;
        let mut depth = 0usize;
        loop {
            if self.at_end() || self.at_newline() {
                let at = self.located(&open);
                self.error(
                    ErrorId::E210,
                    &at,
                    "Unexpected end of line here; expected ')'.",
                );
                return None;
            }
            if self.at_punctuation("(") {
                depth += 1;
            }
            if self.at_punctuation(")") {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            self.bump();
        }
        let inner = self.tokens.get(start..self.index).unwrap_or(&[]).to_vec();
        self.bump();
        Some(split_on_commas(&inner))
    }

    fn parse_int_ranges(&mut self) -> Option<Vec<IntRange>> {
        let parts = self.parse_arguments()?;
        let mut ranges = Vec::new();
        for part in &parts {
            if part.is_empty() && parts.len() == 1 {
                // `text()` and `int()` are valid and unconstrained (SPEC §4.4).
                return Some(Vec::new());
            }
            match parse_int_range(part) {
                Some(range) => ranges.push(range),
                None => {
                    let at = self.range_position(part);
                    self.error(
                        ErrorId::E305,
                        &at,
                        format!(
                            "Invalid range '{}'; ranges are 'value' or 'min..max' with min <= max.",
                            join_source(part)
                        ),
                    );
                }
            }
        }
        Some(ranges)
    }

    fn parse_float_ranges(&mut self) -> Option<Vec<FloatRange>> {
        let parts = self.parse_arguments()?;
        let mut ranges = Vec::new();
        for part in &parts {
            if part.is_empty() && parts.len() == 1 {
                return Some(Vec::new());
            }
            match parse_float_range(part) {
                Some(range) => ranges.push(range),
                None => {
                    let at = self.range_position(part);
                    self.error(
                        ErrorId::E305,
                        &at,
                        format!(
                            "Invalid range '{}'; ranges are 'value' or 'min..max' with min <= max.",
                            join_source(part)
                        ),
                    );
                }
            }
        }
        Some(ranges)
    }

    /// Where a malformed range part is reported: its first token, or the end
    /// of the argument list for an empty part.
    fn range_position(&self, part: &[Token]) -> Located {
        match part.first() {
            Some(token) => self.located(token),
            None => Located::new(self.file, self.peek().position),
        }
    }

    fn parse_enum_type(&mut self, at: &Located, arguments: bool) -> Option<TypeExpr> {
        if !arguments {
            self.error(
                ErrorId::E317,
                at,
                "enum(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let parts = self.parse_arguments()?;
        if parts.len() == 1 && parts[0].is_empty() {
            // SPEC §4.4.5, §4.13 and §10.3 all name E306 for an empty member
            // list, and E306's own template spells this message; §4.4 names
            // E317 for the same spelling.
            self.error(
                ErrorId::E306,
                at,
                "enum(...) must declare at least one member.".to_string(),
            );
            return None;
        }
        let mut members: Vec<String> = Vec::new();
        for part in &parts {
            if part.is_empty() {
                self.error(
                    ErrorId::E306,
                    at,
                    "enum(...) must not declare an empty member.".to_string(),
                );
                continue;
            }
            let single = part.len() == 1;
            let name = part[0].identifier_text().unwrap_or("");
            if !single || !is_identifier(name) {
                // An enum member is an identifier (SPEC §4.4.5, GRAMMAR
                // `enum_type`). E306 covers only an empty member list and a
                // duplicate member (SPEC §10.3), so anything else in a member
                // position is a token that is not valid here.
                let offender = if !single && is_identifier(name) {
                    &part[1]
                } else {
                    &part[0]
                };
                self.error(
                    ErrorId::E210,
                    &self.located(offender),
                    format!(
                        "Unexpected {} here; expected an enum member.",
                        offender.describe()
                    ),
                );
                continue;
            }
            let normalised = normalise(name);
            if members.contains(&normalised) {
                self.error(
                    ErrorId::E306,
                    &self.located(&part[0]),
                    format!("Duplicate enum member '{normalised}'."),
                );
                continue;
            }
            members.push(normalised);
        }
        if members.is_empty() {
            return None;
        }
        Some(TypeExpr::Enum { members })
    }

    fn parse_file_type(&mut self, at: &Located, arguments: bool) -> Option<TypeExpr> {
        if !arguments {
            self.error(
                ErrorId::E317,
                at,
                "file(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let parts = self.parse_arguments()?;
        if parts.len() == 1 && parts[0].is_empty() {
            self.error(
                ErrorId::E317,
                at,
                "file(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let mut extensions: Vec<String> = Vec::new();
        for part in &parts {
            if part.is_empty() {
                self.error(
                    ErrorId::E319,
                    at,
                    "Empty extension in file(...).".to_string(),
                );
                continue;
            }
            let Some((extension, rest)) = self.read_extension(part) else {
                continue;
            };
            if !rest.is_empty() {
                self.error(
                    ErrorId::E210,
                    &self.located(&rest[0]),
                    format!(
                        "Unexpected {} here; expected ',' or ')'.",
                        rest[0].describe()
                    ),
                );
                continue;
            }
            if extensions.contains(&extension) {
                self.error(
                    ErrorId::E319,
                    &self.located(&part[0]),
                    format!("Duplicate extension '{extension}' in file(...)."),
                );
                continue;
            }
            extensions.push(extension);
        }
        if extensions.is_empty() {
            return None;
        }
        Some(TypeExpr::File { extensions })
    }

    fn parse_image_type(
        &mut self,
        at: &Located,
        context: &str,
        field: &str,
        arguments: bool,
    ) -> Option<TypeExpr> {
        if !arguments {
            self.error(
                ErrorId::E317,
                at,
                "image(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let parts = self.parse_arguments()?;
        if parts.len() == 1 && parts[0].is_empty() {
            self.error(
                ErrorId::E317,
                at,
                "image(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let mut alternatives: Vec<ImageAlt> = Vec::new();
        for part in &parts {
            if part.is_empty() {
                self.error(
                    ErrorId::E319,
                    at,
                    "Empty extension in image(...).".to_string(),
                );
                continue;
            }
            let Some((extension, rest)) = self.read_extension(part) else {
                continue;
            };
            if !IMAGE_FORMATS.contains(&extension.as_str()) {
                self.error(
                    ErrorId::E307,
                    &self.located(&part[0]),
                    format!(
                        "Unsupported image format '{extension}'; supported: {}.",
                        list_of(&IMAGE_FORMATS)
                    ),
                );
                continue;
            }
            let (width, height) = match rest.first() {
                None => (None, None),
                Some(token) => match &token.kind {
                    TokenKind::SizeToken(text) if rest.len() == 1 => match parse_size_token(text) {
                        Some(size) => size,
                        None => {
                            self.error(
                                    ErrorId::E308,
                                    &self.located(token),
                                    format!(
                                        "Invalid image size '{text}'; expected WIDTHxHEIGHT where each side is a number or '*'."
                                    ),
                                );
                            continue;
                        }
                    },
                    _ => {
                        self.error(
                            ErrorId::E308,
                            &self.located(token),
                            format!(
                                "Invalid image size '{}'; expected WIDTHxHEIGHT where each side is a number or '*'.",
                                join_source(rest)
                            ),
                        );
                        continue;
                    }
                },
            };
            if width == Some(0) || height == Some(0) {
                let alternative = ImageAlt {
                    extension: extension.clone(),
                    width,
                    height,
                };
                self.error(
                    ErrorId::E318,
                    &self.located(&part[0]),
                    format!(
                        "Constraint '{}' on {context}.{field} can never be satisfied.",
                        alternative.describe()
                    ),
                );
                continue;
            }
            let alternative = ImageAlt {
                extension,
                width,
                height,
            };
            if alternatives.contains(&alternative) {
                self.error(
                    ErrorId::E319,
                    &self.located(&part[0]),
                    format!(
                        "Duplicate extension '{}' in image(...).",
                        alternative.extension
                    ),
                );
                continue;
            }
            alternatives.push(alternative);
        }
        if alternatives.is_empty() {
            return None;
        }
        Some(TypeExpr::Image { alternatives })
    }

    /// An `extension`: an optional leading `.` then letters and digits
    /// (SPEC §4.4.6). Returns the canonical form and the tokens after it.
    fn read_extension<'t>(&mut self, part: &'t [Token]) -> Option<(String, &'t [Token])> {
        let mut index = 0;
        if part[0].is_punctuation(".") {
            index = 1;
        }
        let token = part.get(index);
        let text = token.and_then(Token::identifier_text).unwrap_or("");
        let alphanumeric = !text.is_empty() && text.chars().all(|ch| ch.is_ascii_alphanumeric());
        if !alphanumeric || (index == 1 && !part[1].glued) {
            self.error(
                ErrorId::E210,
                &self.located(&part[0]),
                format!(
                    "Unexpected {} here; expected a file extension.",
                    part[0].describe()
                ),
            );
            return None;
        }
        Some((canonical_extension(text), &part[index + 1..]))
    }

    fn parse_ref_type(&mut self, at: &Located, arguments: bool) -> Option<TypeExpr> {
        if !arguments {
            self.error(
                ErrorId::E317,
                at,
                "ref(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        let parts = self.parse_arguments()?;
        if parts.len() == 1 && parts[0].is_empty() {
            self.error(
                ErrorId::E317,
                at,
                "ref(...) requires at least one argument.".to_string(),
            );
            return None;
        }
        if parts.len() != 1 || parts[0].len() != 1 {
            self.error(
                ErrorId::E210,
                at,
                "Unexpected argument list here; expected one schema name.".to_string(),
            );
            return None;
        }
        match &parts[0][0].kind {
            TokenKind::SchemaName(name) => Some(TypeExpr::Ref {
                schema: name.clone(),
            }),
            _ => {
                self.error(
                    ErrorId::E210,
                    &self.located(&parts[0][0]),
                    format!(
                        "Unexpected {} here; expected a schema name.",
                        parts[0][0].describe()
                    ),
                );
                None
            }
        }
    }

    fn parse_nested_type(&mut self, at: &Located) -> Option<TypeExpr> {
        self.bump();
        if !self.at_punctuation("(") || !self.peek().glued {
            self.error(
                ErrorId::E304,
                at,
                "Unknown type '$'; expected text, int, float, bool, enum, file, image, ref or $(Schema).".to_string(),
            );
            return None;
        }
        let parts = self.parse_arguments()?;
        if parts.len() != 1 || parts[0].len() != 1 {
            self.error(
                ErrorId::E210,
                at,
                "Unexpected argument list here; expected one schema name.".to_string(),
            );
            return None;
        }
        match &parts[0][0].kind {
            TokenKind::SchemaName(name) => Some(TypeExpr::Nested {
                schema: name.clone(),
            }),
            _ => {
                self.error(
                    ErrorId::E210,
                    &self.located(&parts[0][0]),
                    format!(
                        "Unexpected {} here; expected a schema name.",
                        parts[0][0].describe()
                    ),
                );
                None
            }
        }
    }

    // --------------------------------------------------------------- values

    /// The `value` grammar a default shares with instances (SPEC §4.10, §5.5).
    fn parse_value(&mut self, tokens: &[Token], at: &Located) -> Option<SyntaxValue> {
        let items = split_on_commas(tokens);
        if items.len() > 1 && items.last().map(Vec::is_empty).unwrap_or(false) {
            self.error(
                ErrorId::E442,
                at,
                "Trailing comma; a statement does not continue onto the next line.".to_string(),
            );
            return None;
        }
        let mut values = Vec::new();
        for item in &items {
            values.push(self.parse_single_value(item, at, 0)?);
        }
        match values.len() {
            0 => None,
            1 => values.pop(),
            _ => Some(SyntaxValue::List(values, ListSpelling::Brackets)),
        }
    }

    fn parse_single_value(
        &mut self,
        tokens: &[Token],
        at: &Located,
        depth: usize,
    ) -> Option<SyntaxValue> {
        if depth > BRACKET_DEPTH {
            self.error(
                ErrorId::E209,
                at,
                format!("Bracket nesting depth in a value exceeds the limit of {BRACKET_DEPTH}."),
            );
            return None;
        }
        let Some(first) = tokens.first() else {
            self.error(ErrorId::E441, at, "Empty list item.".to_string());
            return None;
        };
        if first.is_punctuation("[") {
            if depth > 0 {
                self.error(
                    ErrorId::E441,
                    at,
                    "Nested lists are not supported.".to_string(),
                );
                return None;
            }
            let last = tokens.last().expect("a non-empty slice has a last token");
            if !last.is_punctuation("]") || tokens.len() < 2 {
                self.error(
                    ErrorId::E210,
                    &self.located(first),
                    "Unexpected '[' here; expected a list closed by ']'.".to_string(),
                );
                return None;
            }
            let inner = &tokens[1..tokens.len() - 1];
            if inner.is_empty() {
                return Some(SyntaxValue::List(Vec::new(), ListSpelling::Brackets));
            }
            let mut items = split_on_commas(inner);
            // A trailing comma is allowed inside brackets (SPEC §5.5).
            if items.len() > 1 && items.last().map(Vec::is_empty).unwrap_or(false) {
                items.pop();
            }
            let mut values = Vec::new();
            for item in &items {
                values.push(self.parse_single_value(item, at, depth + 1)?);
            }
            return Some(SyntaxValue::List(values, ListSpelling::Brackets));
        }
        if first.is_punctuation("#") {
            let name = tokens
                .get(1)
                .and_then(Token::identifier_text)
                .map(normalise)
                .unwrap_or_default();
            return Some(SyntaxValue::Tag {
                name,
                args: Vec::new(),
            });
        }
        if tokens.len() == 1 {
            match &first.kind {
                TokenKind::QuotedString(text) => return Some(SyntaxValue::Quoted(text.clone())),
                TokenKind::BareText(text) => return Some(SyntaxValue::Bare(text.clone())),
                _ => {}
            }
        }
        self.error(
            ErrorId::E210,
            &self.located(first),
            format!("Unexpected {} here; expected a value.", first.describe()),
        );
        None
    }

    // ---------------------------------------------------- per-field checking

    /// The checks a field can receive without leaving its own block, then the
    /// duplicate-name check that files it into the block.
    fn finish_field(
        &mut self,
        schema: &mut SchemaDecl,
        block: &mut Block,
        field: FieldDecl,
        head: &[Token],
    ) {
        let context = block.context(&schema.name);
        if block.is_root {
            if field.name == "template" {
                self.error(
                    ErrorId::E314,
                    &field.at,
                    "'template' is a reserved envelope key and cannot be declared as a field."
                        .to_string(),
                );
                return;
            }
            if field.name == "id" {
                self.check_id_declaration(schema, &field, head);
                return;
            }
        }
        self.check_tag(block, &field, &context);
        self.check_default(&field, &context);
        if let Some(first) = block.fields.iter().find(|other| other.name == field.name) {
            self.errors.push(
                Diagnostic::at(
                    ErrorId::E302,
                    field.at.file.clone(),
                    field.at.position,
                    format!("Field '{}' is already declared in this block.", field.name),
                )
                .with_note(
                    Note::new("first declared here.").at(first.at.file.clone(), first.at.position),
                )
                .with_note_text("annotations do not make two declarations of one name distinct."),
            );
            return;
        }
        if field.is_tag() {
            block.tag = Some((field.name.clone(), field.at.clone()));
        }
        block.fields.push(field);
    }

    /// `id` may only be declared as `id: text` or `id: text(a..b)`, with no
    /// list head, no modifier and no default (SPEC §4.11).
    fn check_id_declaration(&mut self, schema: &mut SchemaDecl, field: &FieldDecl, head: &[Token]) {
        let ranges = match &field.kind {
            FieldKind::Scalar {
                ty: TypeExpr::Text { ranges },
                default: None,
            } => Some(ranges.clone()),
            _ => None,
        };
        let plain = field.list.is_none()
            && !field.modifiers.optional
            && !field.modifiers.tag
            && !field.modifiers.public_
            && !field.modifiers.window.is_annotated();
        match ranges {
            Some(ranges) if plain => {
                if schema.id_ranges.is_some() {
                    let note = Note::new("first declared here.")
                        .at(schema.at.file.clone(), schema.at.position);
                    self.error_with_note(
                        ErrorId::E302,
                        &field.at,
                        "Field 'id' is already declared in this block.".to_string(),
                        note,
                    );
                    return;
                }
                schema.id_ranges = Some(ranges);
            }
            _ => {
                let found = match field.kind {
                    FieldKind::Group { .. } => format!("{} {{ … }}", field.spelled),
                    FieldKind::Scalar { .. } => join_source(trim_trailing_newline(head)),
                };
                self.error(
                    ErrorId::E314,
                    &field.at,
                    format!(
                        "'id' may only be declared as 'id: text' or 'id: text(min..max)' with no modifiers and no default; found {found}."
                    ),
                );
            }
        }
    }

    /// `@tag` placement and arity (SPEC §4.8). The lowest applicable
    /// identifier is reported, as SPEC §11.1 requires at one position.
    fn check_tag(&mut self, block: &Block, field: &FieldDecl, context: &str) {
        if !field.is_tag() {
            return;
        }
        let at = match field.modifiers.tag_at {
            Some(position) => Located::new(self.file, position),
            None => field.at.clone(),
        };
        if let Some((first, first_at)) = &block.tag {
            let group = block.name.clone().unwrap_or_else(|| context.to_string());
            let note = Note::new("first @tag field declared here.")
                .at(first_at.file.clone(), first_at.position);
            self.error_with_note(
                ErrorId::E310,
                &at,
                format!("Group '{group}' already has a @tag field '{first}'."),
                note,
            );
            return;
        }
        if block.is_root {
            self.error(
                ErrorId::E311,
                &at,
                format!(
                    "@tag is only allowed on a field inside a group; '{}' is a root field of schema '{context}'.",
                    field.name
                ),
            );
            return;
        }
        let taggable = !field.is_list()
            && matches!(
                field.type_expr(),
                Some(TypeExpr::Text { .. })
                    | Some(TypeExpr::Int { .. })
                    | Some(TypeExpr::Bool)
                    | Some(TypeExpr::Enum { .. })
            );
        if !taggable {
            self.error(
                ErrorId::E322,
                &at,
                format!(
                    "@tag requires a non-list field of type text, int, bool or enum; '{}' is {}.",
                    field.name,
                    field.kind_word()
                ),
            );
        }
    }

    /// A default is validated against its own field's type exactly as an
    /// authored value would be (SPEC §4.10, §7.2). E320 precedes E313.
    fn check_default(&mut self, field: &FieldDecl, context: &str) {
        let Some(default) = field.default() else {
            return;
        };
        // A modifier written after the `=` stays inside the default lexeme,
        // which is where SPEC §4.3 says it is rejected.
        if let SyntaxValue::Bare(text) = &default.value {
            if let Some((offset, run)) = trailing_modifier_run(text) {
                let column = default.at.position.col + text[..offset].chars().count() as u32;
                let at = Located::new(self.file, Position::new(default.at.position.line, column));
                self.error(
                    ErrorId::E303,
                    &at,
                    format!("Modifier '{run}' appears after the default; write it before the '='."),
                );
                return;
            }
        }
        if field.is_optional() {
            self.error(
                ErrorId::E320,
                &default.at,
                format!(
                    "{context}.{} declares both @optional and a default; a default already makes the field present.",
                    field.name
                ),
            );
            return;
        }
        let Some(ty) = field.type_expr() else {
            return;
        };
        let rendered = render_default(&default.value, ty);
        if let Err(reason) = check_default_value(&default.value, ty, field.is_list()) {
            let mut diagnostic = Diagnostic::at(
                ErrorId::E313,
                default.at.file.clone(),
                default.at.position,
                format!(
                    "Default value {rendered} is invalid for {context}.{}: {reason}.",
                    field.name
                ),
            );
            if !field.is_list() && matches!(default.value, SyntaxValue::List(_, _)) {
                diagnostic = diagnostic.with_note_text("this field is not a list.");
            }
            self.errors.push(diagnostic);
        }
    }
}

/// One field block being read: the fields collected so far, the group's name
/// and the `@tag` field it has already seen.
struct Block {
    name: Option<String>,
    is_root: bool,
    fields: Vec<FieldDecl>,
    tag: Option<(String, Located)>,
}

impl Block {
    fn new(name: Option<String>, is_root: bool) -> Self {
        Self {
            name,
            is_root,
            fields: Vec::new(),
            tag: None,
        }
    }

    /// The `{schema}` substitution: the schema name at root, and the dotted
    /// path of the group inside it otherwise.
    fn context(&self, schema: &str) -> String {
        match &self.name {
            Some(name) => format!("{schema}.{name}"),
            None => schema.to_string(),
        }
    }
}

// ---------------------------------------------------- free parsing helpers

/// Splits a token run at depth-0 commas. Brackets and parentheses raise the
/// depth; a `quoted_string` is one token and can hold neither.
fn split_on_commas(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut parts: Vec<Vec<Token>> = vec![Vec::new()];
    let mut depth = 0usize;
    for token in tokens {
        if token.is_punctuation("(") || token.is_punctuation("[") {
            depth += 1;
        } else if token.is_punctuation(")") || token.is_punctuation("]") {
            depth -= usize::from(depth > 0);
        } else if depth == 0 && token.is_punctuation(",") {
            parts.push(Vec::new());
            continue;
        }
        parts
            .last_mut()
            .expect("parts always holds one part")
            .push(token.clone());
    }
    parts
}

fn trim_trailing_newline(tokens: &[Token]) -> &[Token] {
    match tokens.last() {
        Some(token) if token.is_newline() => &tokens[..tokens.len() - 1],
        _ => tokens,
    }
}

/// `[]`, `[min..]` or `[min..max]` (SPEC §4.6). The bracket tokens are not
/// part of `inner`.
fn parse_cardinality(inner: &[Token]) -> Option<Cardinality> {
    if inner.is_empty() {
        return Some(Cardinality::ANY);
    }
    let min = uint_of(inner.first()?)?;
    if !inner.get(1)?.is_punctuation("..") {
        return None;
    }
    match inner.len() {
        2 => Some(Cardinality { min, max: None }),
        3 => {
            let max = uint_of(&inner[2])?;
            if max < min {
                return None;
            }
            Some(Cardinality {
                min,
                max: Some(max),
            })
        }
        _ => None,
    }
}

fn uint_of(token: &Token) -> Option<u32> {
    match &token.kind {
        TokenKind::IntLiteral(text) => text.parse::<u32>().ok(),
        _ => None,
    }
}

fn int_of(token: &Token) -> Option<i64> {
    match &token.kind {
        TokenKind::IntLiteral(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

fn number_of(token: &Token) -> Option<f64> {
    match &token.kind {
        TokenKind::IntLiteral(text) | TokenKind::FloatLiteral(text) => {
            text.parse::<f64>().ok().filter(|value| value.is_finite())
        }
        _ => None,
    }
}

/// `int_range = int_literal , [ ".." , int_literal ]` (SPEC §4.5).
fn parse_int_range(part: &[Token]) -> Option<IntRange> {
    match part.len() {
        1 => {
            let value = int_of(&part[0])?;
            Some(IntRange {
                min: value,
                max: value,
            })
        }
        3 => {
            if !part[1].is_punctuation("..") {
                return None;
            }
            let min = int_of(&part[0])?;
            let max = int_of(&part[2])?;
            if min > max {
                return None;
            }
            Some(IntRange { min, max })
        }
        _ => None,
    }
}

/// `float_range = number , [ ".." , number ]`, both bounds finite (SPEC §4.5).
fn parse_float_range(part: &[Token]) -> Option<FloatRange> {
    match part.len() {
        1 => {
            let value = number_of(&part[0])?;
            Some(FloatRange {
                min: value,
                max: value,
            })
        }
        3 => {
            if !part[1].is_punctuation("..") {
                return None;
            }
            let min = number_of(&part[0])?;
            let max = number_of(&part[2])?;
            if min > max {
                return None;
            }
            Some(FloatRange { min, max })
        }
        _ => None,
    }
}

/// `WIDTHxHEIGHT`, each side a decimal integer or `*` (SPEC §4.4.7).
fn parse_size_token(text: &str) -> Option<(Option<u32>, Option<u32>)> {
    let (width, height) = text.split_once('x')?;
    Some((parse_size_side(width)?, parse_size_side(height)?))
}

fn parse_size_side(text: &str) -> Option<Option<u32>> {
    if text == "*" {
        return Some(None);
    }
    text.parse::<u32>().ok().map(Some)
}

// -------------------------------------------------------- default checking

/// Renders a default for the `{value}` substitution of E313 (SPEC §9.8).
fn render_default(value: &SyntaxValue, ty: &TypeExpr) -> String {
    match value {
        SyntaxValue::Bare(text) => match ty {
            TypeExpr::Int { .. } if text.parse::<i64>().is_ok() => text.clone(),
            TypeExpr::Float { .. } if text.parse::<f64>().map(f64::is_finite).unwrap_or(false) => {
                text.clone()
            }
            TypeExpr::Bool if text == "true" || text == "false" => text.clone(),
            _ => quote_text(text),
        },
        SyntaxValue::Quoted(text) => quote_text(text),
        SyntaxValue::List(items, _) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| render_default(item, ty))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxValue::Tag { name, .. } => format!("#{name}"),
        SyntaxValue::Object(_) => "an object".to_string(),
    }
}

/// Checks a default against the declared type, returning the `{reason}` of
/// E313 when it does not hold (SPEC §4.10, §5.10).
fn check_default_value(value: &SyntaxValue, ty: &TypeExpr, is_list: bool) -> Result<(), String> {
    if let SyntaxValue::List(items, _) = value {
        if !is_list {
            return Err("this field is not a list".to_string());
        }
        for item in items {
            if matches!(item, SyntaxValue::List(_, _)) {
                return Err("nested lists are not supported".to_string());
            }
            check_element(item, ty, is_list)?;
        }
        return Ok(());
    }
    check_element(value, ty, is_list)
}

fn check_element(value: &SyntaxValue, ty: &TypeExpr, is_list: bool) -> Result<(), String> {
    let text = match value {
        SyntaxValue::Bare(text) => text.as_str(),
        SyntaxValue::Quoted(text) => {
            if matches!(
                ty,
                TypeExpr::Int { .. } | TypeExpr::Float { .. } | TypeExpr::Bool
            ) {
                return Err(format!(
                    "expected {}, found text; quoting does not coerce",
                    ty.kind()
                ));
            }
            text.as_str()
        }
        SyntaxValue::Tag { .. } => {
            return Err(format!("expected {}, found a tag object", ty.kind()));
        }
        SyntaxValue::Object(_) => {
            return Err(format!("expected {}, found an object", ty.kind()));
        }
        SyntaxValue::List(_, _) => return Err("nested lists are not supported".to_string()),
    };
    let quoted = matches!(value, SyntaxValue::Quoted(_));
    // A default is interpolated when it is filled in, so a value that carries
    // a reference is checked then, not here (SPEC §4.10, §7.3 step 4).
    let deferred = !quoted && carries_interpolation(text);

    match ty {
        TypeExpr::Text { ranges } => {
            if deferred || ranges.is_empty() {
                return Ok(());
            }
            let length = text.chars().count() as i64;
            if ranges.iter().any(|range| range.contains(length)) {
                Ok(())
            } else {
                Err(format!(
                    "length {length} is not in {}",
                    list_of(
                        &ranges
                            .iter()
                            .map(|range| range.describe())
                            .collect::<Vec<_>>()
                    )
                ))
            }
        }
        TypeExpr::Int { ranges } => {
            let Ok(number) = text.parse::<i64>() else {
                return Err(format!("expected int, found '{text}'"));
            };
            if ranges.is_empty() || ranges.iter().any(|range| range.contains(number)) {
                Ok(())
            } else {
                Err(format!(
                    "{number} is not in {}",
                    list_of(
                        &ranges
                            .iter()
                            .map(|range| range.describe())
                            .collect::<Vec<_>>()
                    )
                ))
            }
        }
        TypeExpr::Float { ranges } => {
            let number = text
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .filter(|_| {
                    crate::lexer::is_float_literal(text) || crate::lexer::is_int_literal(text)
                });
            let Some(number) = number else {
                return Err(format!("expected float, found '{text}'"));
            };
            if ranges.is_empty() || ranges.iter().any(|range| range.contains(number)) {
                Ok(())
            } else {
                Err(format!("{text} is outside the declared ranges"))
            }
        }
        TypeExpr::Bool => {
            if text == "true" || text == "false" {
                Ok(())
            } else {
                Err(format!("expected bool, found '{text}'"))
            }
        }
        TypeExpr::Enum { members } => {
            if deferred {
                return Ok(());
            }
            let normalised = normalise(text);
            if members.contains(&normalised) {
                return Ok(());
            }
            // A wildcard replicates a whole element and is available only on
            // an enum list field (SPEC §5.6).
            if is_list && !quoted {
                if let Some(prefix) = normalised.strip_suffix('*') {
                    if members.iter().any(|member| member.starts_with(prefix)) {
                        return Ok(());
                    }
                    return Err(format!(
                        "no member of {} starts with '{prefix}'",
                        list_of(members)
                    ));
                }
            }
            Err(format!("'{normalised}' is not one of {}", list_of(members)))
        }
        TypeExpr::File { .. } | TypeExpr::Image { .. } => {
            if deferred || text.contains('{') {
                return Ok(());
            }
            let allowed = ty.extensions();
            let extension = text
                .rsplit_once('.')
                .map(|(_, extension)| canonical_extension(extension))
                .unwrap_or_default();
            if allowed.contains(&extension) {
                Ok(())
            } else {
                Err(format!(
                    "extension '{extension}' is not allowed; allowed: {}",
                    list_of(&allowed)
                ))
            }
        }
        TypeExpr::Ref { schema } => {
            if deferred {
                return Ok(());
            }
            if is_identifier(text) {
                Ok(())
            } else {
                Err(format!("'{text}' is not a valid id of a '{schema}'"))
            }
        }
        TypeExpr::Nested { schema } => Err(format!(
            "a $({schema}) field takes an object, which a default cannot express"
        )),
    }
}

// ------------------------------------------------------------ P3 validation

/// P3 schema validation (SPEC §7.2): the checks that need the whole project.
///
/// For every schema, in source order and within a schema in field declaration
/// order: `$(Schema)` and `ref(Schema)` targets resolve (E309), version
/// annotations lie inside the project range and select a non-empty set
/// (E603, E604, E605), and no cycle of required non-list `$(Schema)` fields
/// exists (E321).
pub fn validate_schemas(tables: &TemplateTables) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    let names = tables.schema_names();
    for schema in &tables.schemas {
        let context = schema.name.clone();
        walk_fields(
            &schema.fields,
            &context,
            tables.versions,
            tables,
            &names,
            &mut errors,
            0,
        );
    }
    check_recursion(tables, &mut errors);
    // P3 covers the logic blocks too, in the same order (SPEC §7.2), so that
    // `lint` on a project with no instances reports every defect a schema or
    // a block can hold.
    errors.extend(crate::logic::check_logic_blocks(tables));
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_fields(
    fields: &[FieldDecl],
    context: &str,
    inherited: VersionRange,
    tables: &TemplateTables,
    names: &[&str],
    errors: &mut Diagnostics,
    depth: usize,
) {
    if depth > GROUP_DEPTH {
        return;
    }
    for field in fields {
        let Some(window) = check_window(field, context, inherited, tables.versions, errors) else {
            continue;
        };
        match &field.kind {
            FieldKind::Group { fields: inner } => {
                let inner_context = format!("{context}.{}", field.name);
                walk_fields(
                    inner,
                    &inner_context,
                    window,
                    tables,
                    names,
                    errors,
                    depth + 1,
                );
            }
            FieldKind::Scalar { ty, .. } => {
                let Some(target) = ty.schema_reference() else {
                    continue;
                };
                let Some(referenced) = tables.schema(target) else {
                    let mut message = format!("Unknown schema '{target}' referenced here.");
                    if let Some(hint) = suggest(target, names, TieBreak::ScalarOrder) {
                        message.push_str(&format!(" Did you mean '{hint}'?"));
                    }
                    errors.push(Diagnostic::at(
                        ErrorId::E309,
                        field.type_at.file.clone(),
                        field.type_at.position,
                        message,
                    ));
                    continue;
                };
                if matches!(ty, TypeExpr::Nested { .. }) {
                    check_nested_window(field, context, window, referenced, tables, errors);
                }
            }
        }
    }
}

/// The version annotations of one field: E603, then E604, then E605
/// (SPEC §4.12). Returns the effective existence set, ancestors included.
fn check_window(
    field: &FieldDecl,
    context: &str,
    inherited: VersionRange,
    project: VersionRange,
    errors: &mut Diagnostics,
) -> Option<VersionRange> {
    let annotations = [
        (field.modifiers.window.since, field.modifiers.since_at),
        (field.modifiers.window.removed, field.modifiers.removed_at),
    ];
    let mut outside = false;
    for (number, at) in annotations {
        let Some(number) = number else {
            continue;
        };
        if project.contains(number) {
            continue;
        }
        outside = true;
        let position = at.unwrap_or(field.at.position);
        errors.push(Diagnostic::at(
            ErrorId::E603,
            field.at.file.clone(),
            position,
            format!(
                "Version {number} is outside the project range {}..{}.",
                project.min, project.max
            ),
        ));
    }
    if outside {
        return None;
    }
    let since = field.modifiers.window.since.unwrap_or(project.min);
    if let Some(removed) = field.modifiers.window.removed {
        if removed <= since {
            let position = field.modifiers.removed_at.unwrap_or(field.at.position);
            errors.push(Diagnostic::at(
                ErrorId::E604,
                field.at.file.clone(),
                position,
                format!("@removed({removed}) must be greater than @since({since})."),
            ));
            return None;
        }
    }
    let own = field.window_in(project);
    let Some(own) = own else {
        errors.push(Diagnostic::at(
            ErrorId::E605,
            field.at.file.clone(),
            field.at.position,
            format!(
                "{context}.{} exists in no version: its annotations select no version of the project range {project}.",
                field.name
            ),
        ));
        return None;
    };
    match own.intersect(inherited) {
        Some(window) => Some(window),
        None => {
            errors.push(Diagnostic::at(
                ErrorId::E605,
                field.at.file.clone(),
                field.at.position,
                format!(
                    "{context}.{} exists in no version: the field is declared for versions {own}, the object that holds it for versions {inherited}.",
                    field.name
                ),
            ));
            None
        }
    }
}

/// A `$(Schema)` field's window is intersected with the union of the
/// referenced schema's fields' windows; an empty result is E605 (SPEC §4.12).
fn check_nested_window(
    field: &FieldDecl,
    context: &str,
    window: VersionRange,
    referenced: &SchemaDecl,
    tables: &TemplateTables,
    errors: &mut Diagnostics,
) {
    let project = tables.versions;
    let reachable: Vec<u32> = window
        .iter()
        .filter(|version| {
            referenced
                .fields
                .iter()
                .any(|inner| inner.exists_in(*version, project))
        })
        .collect();
    if !reachable.is_empty() {
        return;
    }
    errors.push(
        Diagnostic::at(
            ErrorId::E605,
            field.type_at.file.clone(),
            field.type_at.position,
            format!(
                "{context}.{} exists in no version: '{}' declares no field in versions {window}.",
                field.name, referenced.name
            ),
        )
        .with_note(
            Note::new(format!("schema '{}' is declared here.", referenced.name))
                .at(referenced.at.file.clone(), referenced.at.position),
        ),
    );
}

/// A cycle of required, non-list `$(Schema)` fields can never be built
/// (SPEC §7.2, E321). A cycle through an `@optional` or a list field is legal.
fn check_recursion(tables: &TemplateTables, errors: &mut Diagnostics) {
    let mut edges: Vec<Vec<(usize, Located)>> = vec![Vec::new(); tables.schemas.len()];
    for (index, schema) in tables.schemas.iter().enumerate() {
        collect_mandatory_edges(tables, &schema.fields, index, &mut edges, 0);
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Colour {
        White,
        Grey,
        Black,
    }
    let mut colour = vec![Colour::White; tables.schemas.len()];
    let mut reported = vec![false; tables.schemas.len()];

    for root in 0..tables.schemas.len() {
        if colour[root] != Colour::White {
            continue;
        }
        // Iterative depth-first search: the stack holds each node and the
        // index of the next edge to follow, so nothing recurses.
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        colour[root] = Colour::Grey;
        while let Some((node, edge)) = stack.pop() {
            if edge >= edges[node].len() {
                colour[node] = Colour::Black;
                continue;
            }
            stack.push((node, edge + 1));
            let (next, at) = edges[node][edge].clone();
            match colour[next] {
                Colour::White => {
                    colour[next] = Colour::Grey;
                    stack.push((next, 0));
                }
                Colour::Grey => {
                    if !reported[next] {
                        reported[next] = true;
                        errors.push(Diagnostic::at(
                            ErrorId::E321,
                            at.file.clone(),
                            at.position,
                            format!(
                                "Schema '{}' requires '{}' which requires '{}'; the structure can never be built.",
                                tables.schemas[next].name,
                                tables.schemas[node].name,
                                tables.schemas[next].name
                            ),
                        ));
                    }
                }
                Colour::Black => {}
            }
        }
    }
}

fn collect_mandatory_edges(
    tables: &TemplateTables,
    fields: &[FieldDecl],
    from: usize,
    edges: &mut [Vec<(usize, Located)>],
    depth: usize,
) {
    if depth > GROUP_DEPTH {
        return;
    }
    for field in fields {
        if field.is_list() || !field.is_required() {
            continue;
        }
        match &field.kind {
            FieldKind::Group { fields: inner } => {
                collect_mandatory_edges(tables, inner, from, edges, depth + 1);
            }
            FieldKind::Scalar {
                ty: TypeExpr::Nested { schema },
                ..
            } => {
                if let Some(target) = tables
                    .schemas
                    .iter()
                    .position(|other| &other.name == schema)
                {
                    edges[from].push((target, field.type_at.clone()));
                }
            }
            FieldKind::Scalar { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn parse(text: &str) -> Result<TemplateFile, Diagnostics> {
        let source = SourceFile::new("templates/T.abt", text);
        let tokens = crate::lexer::tokenize(&source).expect("the fixture lexes");
        parse_template("templates/T.abt", &source.text, &tokens)
    }

    fn schemas(text: &str) -> Vec<SchemaDecl> {
        let file = parse(text).unwrap_or_else(|errors| panic!("unexpected diagnostics:\n{errors}"));
        file.items
            .into_iter()
            .filter_map(|item| match item {
                TemplateItem::Schema(schema) => Some(schema),
                _ => None,
            })
            .collect()
    }

    fn one_schema(text: &str) -> SchemaDecl {
        schemas(text).pop().expect("one schema")
    }

    fn ids(text: &str) -> Vec<ErrorId> {
        match parse(text) {
            Ok(file) => panic!("expected diagnostics, parsed {} items", file.items.len()),
            Err(errors) => errors.iter().map(|item| item.id).collect(),
        }
    }

    fn first(text: &str) -> Diagnostic {
        match parse(text) {
            Ok(_) => panic!("expected a diagnostic"),
            Err(errors) => errors.first().cloned().expect("one diagnostic"),
        }
    }

    fn tables(text: &str) -> Result<TemplateTables, Diagnostics> {
        let file = parse(text).unwrap_or_else(|errors| panic!("unexpected diagnostics:\n{errors}"));
        build_tables(&[file])
    }

    fn validated(text: &str) -> Result<TemplateTables, Diagnostics> {
        let tables = tables(text).unwrap_or_else(|errors| panic!("unexpected tables:\n{errors}"));
        validate_schemas(&tables)?;
        Ok(tables)
    }

    fn validation_ids(text: &str) -> Vec<ErrorId> {
        match validated(text) {
            Ok(_) => panic!("expected diagnostics"),
            Err(errors) => errors.iter().map(|item| item.id).collect(),
        }
    }

    // ---------------------------------------------------------------- §4.1

    #[test]
    fn a_template_file_holds_schemas_logic_and_one_versions_declaration() {
        let file = parse("versions 1..2\n\nschema Product {\n    name: text\n}\n\nlogic Product {\n    require .name exists else throw \"x\"\n}\n")
            .expect("the file parses");
        assert_eq!(file.items.len(), 3);
        assert!(matches!(file.items[0], TemplateItem::Versions(_)));
        assert!(matches!(file.items[1], TemplateItem::Schema(_)));
        assert!(matches!(file.items[2], TemplateItem::Logic(_)));
    }

    #[test]
    fn an_unrecognised_top_level_construct_is_e210_once() {
        assert_eq!(ids("this is not a schema line\n"), [ErrorId::E210]);
        assert_eq!(ids("schem Thing {\n    a: text\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("id: text(1..40)\n"), [ErrorId::E210]);
        let junk = first("random { garbage }\n");
        assert_eq!(junk.id, ErrorId::E210);
        assert_eq!(junk.position, Some(Position::new(1, 1)));
    }

    #[test]
    fn a_one_line_schema_and_a_brace_on_the_next_line_are_both_rejected() {
        assert_eq!(
            ids("schema Thing { id: text(1..40) name: text }\n"),
            [ErrorId::E210]
        );
        assert_eq!(ids("schema Thing\n{\n    a: text\n}\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_schema_may_declare_zero_fields() {
        let schema = one_schema("schema Empty {\n}\n");
        assert!(schema.fields.is_empty());
        assert_eq!(schema.name, "Empty");
    }

    // ---------------------------------------------------------------- §4.3

    #[test]
    fn field_names_are_normalised_and_keep_their_declared_order() {
        let schema = one_schema("schema A {\n    Max-Count: int\n    b: text\n}\n");
        assert_eq!(schema.field_names(), ["max_count", "b"]);
        assert_eq!(schema.fields[0].spelled, "Max-Count");
    }

    #[test]
    fn two_fields_that_normalise_alike_are_e302_whatever_their_annotations() {
        let error = first("schema A {\n    Name: text\n    name: text\n}\n");
        assert_eq!(error.id, ErrorId::E302);
        assert_eq!(error.position, Some(Position::new(3, 5)));
        assert_eq!(error.notes.len(), 2);
        assert!(error.notes[1]
            .text
            .contains("annotations do not make two declarations"));
        assert_eq!(
            ids("versions 1..2\nschema A {\n    a: text @since(2)\n    a: text @removed(2)\n}\n"),
            [ErrorId::E302]
        );
    }

    #[test]
    fn a_doubled_list_marker_is_e315_and_a_bad_cardinality_is_e323() {
        assert_eq!(ids("schema A {\n    t[][]: text\n}\n"), [ErrorId::E315]);
        for head in ["t[..4]", "t[3]", "t[4..2]", "t[-1..]"] {
            assert_eq!(
                ids(&format!("schema A {{\n    {head}: text\n}}\n")),
                [ErrorId::E323],
                "{head}"
            );
        }
    }

    #[test]
    fn cardinality_bounds_are_read_from_the_head() {
        let schema =
            one_schema("schema A {\n    a[]: text\n    b[2..]: text\n    c[2..4]: text\n}\n");
        assert_eq!(schema.fields[0].cardinality(), Cardinality::ANY);
        assert_eq!(
            schema.fields[1].cardinality(),
            Cardinality { min: 2, max: None }
        );
        assert_eq!(
            schema.fields[2].cardinality(),
            Cardinality {
                min: 2,
                max: Some(4)
            }
        );
    }

    #[test]
    fn an_unknown_or_repeated_modifier_is_e303() {
        assert_eq!(
            ids("schema A {\n    a: text @optionnal\n}\n"),
            [ErrorId::E303]
        );
        assert_eq!(
            ids("schema A {\n    a: text @optional @optional\n}\n"),
            [ErrorId::E303]
        );
        // `@was` is not a modifier of Abstract 1.0.
        assert_eq!(ids("schema A {\n    a: text @was(b)\n}\n"), [ErrorId::E303]);
    }

    #[test]
    fn a_modifier_written_after_a_default_is_e303() {
        let error = first("versions 1..2\nschema A {\n    glow: bool = false @since(2)\n}\n");
        assert_eq!(error.id, ErrorId::E303);
        assert_eq!(error.position, Some(Position::new(3, 24)));
        assert!(error.message.contains("@since(2)"), "{}", error.message);
        // The accepted spelling puts the modifier before the '='.
        let schema = one_schema("versions 1..2\nschema A {\n    glow: bool @since(2) = false\n}\n");
        assert_eq!(schema.fields[0].modifiers.window.since, Some(2));
    }

    #[test]
    fn a_modifier_before_the_colon_is_rejected() {
        assert_eq!(
            ids("schema A {\n    a @optional: text\n}\n"),
            [ErrorId::E210]
        );
    }

    // ---------------------------------------------------------------- §4.4

    #[test]
    fn every_type_parses() {
        let schema = one_schema(
            "schema A {\n    a: text\n    b: text(1..40)\n    c: int\n    d: int(1, 3..12)\n    e: float(0..1)\n    f: bool\n    g: enum(draft, active)\n    h: file(png, jpg)\n    i: image(png 128x128, jpg 1024x*)\n    j: ref(Pack)\n    k: $(Owner)\n}\n",
        );
        let kinds: Vec<&str> = schema
            .fields
            .iter()
            .map(|field| field.kind_word())
            .collect();
        assert_eq!(
            kinds,
            [
                "text", "text", "int", "int", "float", "bool", "enum", "file", "image", "ref",
                "object"
            ]
        );
        assert!(matches!(
            schema.fields[3].type_expr(),
            Some(TypeExpr::Int { ranges }) if ranges.len() == 2
        ));
    }

    #[test]
    fn whitespace_before_a_parameter_list_is_e304() {
        let error = first("schema A {\n    a: text (1..40)\n}\n");
        assert_eq!(error.id, ErrorId::E304);
        assert!(error.message.contains("text (1..40)"), "{}", error.message);
        assert_eq!(ids("schema A {\n    a: enum (x)\n}\n"), [ErrorId::E304]);
    }

    #[test]
    fn an_unknown_type_keyword_is_e304() {
        assert_eq!(ids("schema A {\n    a: string\n}\n"), [ErrorId::E304]);
        assert!(parse("schema A {\n    a[]: text\n    b: text\n}\n").is_ok());
    }

    #[test]
    fn a_parameterised_type_with_no_argument_is_e317_but_text_int_float_are_not() {
        // SPEC §4.4.5, §4.13 and §10.3 name E306 for `enum()`; SPEC §4.4
        // names E317 for the same spelling.
        assert_eq!(
            ids("schema A {
    a: enum()
}
"),
            [ErrorId::E306]
        );
        for spelling in ["file()", "image()", "ref()"] {
            assert_eq!(
                ids(&format!("schema A {{\n    a: {spelling}\n}}\n")),
                [ErrorId::E317],
                "{spelling}"
            );
        }
        let schema = one_schema("schema A {\n    a: text()\n    b: int()\n    c: float()\n}\n");
        assert!(matches!(
            schema.fields[0].type_expr(),
            Some(TypeExpr::Text { ranges }) if ranges.is_empty()
        ));
    }

    #[test]
    fn an_empty_or_duplicate_enum_member_is_e306() {
        assert_eq!(ids("schema A {\n    a: enum(x,,y)\n}\n"), [ErrorId::E306]);
        assert_eq!(ids("schema A {\n    a: enum(x,)\n}\n"), [ErrorId::E306]);
        assert_eq!(
            ids("schema A {\n    a: enum(red, red)\n}\n"),
            [ErrorId::E306]
        );
        assert_eq!(
            ids("schema A {\n    a: enum(Red, red)\n}\n"),
            [ErrorId::E306]
        );
    }

    #[test]
    fn an_enum_member_that_is_not_an_identifier_is_e210() {
        // E306 covers only an empty member list and a duplicate member
        // (SPEC §10.3); anything else in a member position is E210.
        // A member can never end with '*', so a wildcard never shadows one.
        assert_eq!(ids("schema A {\n    a: enum(ab, a*)\n}\n"), [ErrorId::E210]);
        assert_eq!(
            ids("schema A {\n    a: enum(a, @optional, b)\n}\n"),
            [ErrorId::E210]
        );
        assert_eq!(
            ids("schema A {\n    a: enum(\"quoted\", b)\n}\n"),
            [ErrorId::E210]
        );
    }

    #[test]
    fn enum_members_are_normalised_in_declaration_order() {
        let schema = one_schema("schema A {\n    a: enum(Draft, ACTIVE, re-tired)\n}\n");
        assert!(matches!(
            schema.fields[0].type_expr(),
            Some(TypeExpr::Enum { members }) if members == &["draft", "active", "re_tired"]
        ));
    }

    #[test]
    fn extensions_canonicalise_and_repeat_as_e319() {
        let schema = one_schema("schema A {\n    a: file(.PNG, JPEG, tga)\n}\n");
        assert!(matches!(
            schema.fields[0].type_expr(),
            Some(TypeExpr::File { extensions }) if extensions == &["png", "jpg", "tga"]
        ));
        assert_eq!(
            ids("schema A {\n    a: file(jpg, jpeg)\n}\n"),
            [ErrorId::E319]
        );
        assert_eq!(
            ids("schema A {\n    a: file(png, png)\n}\n"),
            [ErrorId::E319]
        );
        assert_eq!(
            ids("schema A {\n    a: file(png,,jpg)\n}\n"),
            [ErrorId::E319]
        );
    }

    #[test]
    fn an_unsupported_image_format_is_e307_and_a_bad_size_is_e308() {
        assert_eq!(ids("schema A {\n    a: image(tga)\n}\n"), [ErrorId::E307]);
        assert_eq!(
            ids("schema A {\n    a: image(png128x128)\n}\n"),
            [ErrorId::E307]
        );
        assert_eq!(
            ids("schema A {\n    a: image(png 128 x 128)\n}\n"),
            [ErrorId::E308]
        );
        assert_eq!(
            ids("schema A {\n    a: image(png 128)\n}\n"),
            [ErrorId::E308]
        );
    }

    #[test]
    fn image_alternatives_may_repeat_an_extension_with_a_different_size() {
        let schema =
            one_schema("schema A {\n    a: image(png 1920x1080, jpg 1920x1080, png 64x64)\n}\n");
        assert!(matches!(
            schema.fields[0].type_expr(),
            Some(TypeExpr::Image { alternatives }) if alternatives.len() == 3
        ));
        assert_eq!(
            ids("schema A {\n    a: image(png 64x64, png 64x64)\n}\n"),
            [ErrorId::E319]
        );
    }

    #[test]
    fn a_declared_image_size_of_zero_can_never_be_satisfied() {
        let error = first("schema A {\n    a: image(png 0x0)\n}\n");
        assert_eq!(error.id, ErrorId::E318);
        assert!(error.message.contains("A.a"), "{}", error.message);
    }

    // ---------------------------------------------------------------- §4.5

    #[test]
    fn open_ended_and_reversed_ranges_are_e305() {
        for spelling in ["int(5..)", "int(..5)", "int(10..5)", "float(1.0..0.5)"] {
            assert_eq!(
                ids(&format!("schema A {{\n    a: {spelling}\n}}\n")),
                [ErrorId::E305],
                "{spelling}"
            );
        }
    }

    #[test]
    fn a_negative_text_range_is_e318() {
        let error = first("schema A {\n    a: text(-5..-1)\n}\n");
        assert_eq!(error.id, ErrorId::E318);
        assert!(error.message.contains("A.a"), "{}", error.message);
        assert!(error.message.contains("-5..-1"), "{}", error.message);
        // A negative bound is not a range error on int, which is signed.
        assert!(parse("schema A {\n    a: int(-5..-1)\n}\n").is_ok());
    }

    #[test]
    fn overlapping_range_parts_are_permitted() {
        let schema = one_schema("schema A {\n    a: int(1..5, 3..9, 4)\n}\n");
        assert!(matches!(
            schema.fields[0].type_expr(),
            Some(TypeExpr::Int { ranges }) if ranges.len() == 3
        ));
    }

    // ------------------------------------------------------------ §4.7, §4.8

    #[test]
    fn groups_and_list_groups_parse_with_their_own_field_lists() {
        let schema = one_schema(
            "schema Product {\n    owner {\n        team: text(1..50)\n    }\n    capabilities[] {\n        id: enum(search, sync) @tag\n        availability: enum(alpha, stable) = stable\n    }\n}\n",
        );
        assert_eq!(schema.fields[0].group_fields().map(<[_]>::len), Some(1));
        assert!(schema.fields[1].is_list());
        let tag = schema.fields[1].tag_field().expect("a @tag field");
        assert_eq!(tag.name, "id");
        let availability = &schema.fields[1].group_fields().unwrap()[1];
        assert_eq!(
            availability.default().map(|d| d.text.as_str()),
            Some("stable")
        );
    }

    #[test]
    fn a_group_field_cannot_declare_a_default() {
        assert_eq!(
            ids("schema A {\n    owner {\n        team: text\n    } = x\n}\n"),
            [ErrorId::E312]
        );
    }

    #[test]
    fn a_second_tag_in_one_group_is_e310_and_a_root_tag_is_e311() {
        let error = first(
            "schema A {\n    caps[] {\n        a: text @tag\n        b: text @tag\n    }\n}\n",
        );
        assert_eq!(error.id, ErrorId::E310);
        assert!(error.message.contains("'caps'"), "{}", error.message);
        assert_eq!(error.notes.len(), 1);

        let root = first("schema A {\n    status: enum(a, b) @tag\n}\n");
        assert_eq!(root.id, ErrorId::E311);
        assert!(root.message.contains("schema 'A'"), "{}", root.message);
    }

    #[test]
    fn a_tag_on_an_unsupported_field_is_e322() {
        let error = first("schema A {\n    caps[] {\n        icon: image(png) @tag\n    }\n}\n");
        assert_eq!(error.id, ErrorId::E322);
        assert!(error.message.contains("is image"), "{}", error.message);
        let listed = first("schema A {\n    caps[] {\n        keys[]: text @tag\n    }\n}\n");
        assert_eq!(listed.id, ErrorId::E322);
        assert!(listed.message.contains("is list"), "{}", listed.message);
    }

    #[test]
    fn a_group_nested_deeper_than_the_limit_is_e209() {
        let mut text = String::from("schema A {\n");
        for index in 0..(GROUP_DEPTH + 4) {
            text.push_str(&format!("g{index} {{\n"));
        }
        text.push_str("leaf: text\n");
        for _ in 0..(GROUP_DEPTH + 4) {
            text.push_str("}\n");
        }
        text.push_str("}\n");
        assert_eq!(ids(&text), [ErrorId::E209]);
    }

    // ---------------------------------------------------------------- §4.9

    #[test]
    fn a_numbered_key_is_an_ordinary_field_name() {
        let schema = one_schema(
            "schema Pack {\n    slots {\n        1: $(Slot)\n        2: $(Slot) @optional\n    }\n}\n",
        );
        let slots = schema.fields[0].group_fields().expect("a group");
        assert_eq!(slots[0].name, "1");
        assert_eq!(slots[1].name, "2");
        assert!(slots[1].is_optional());
    }

    #[test]
    fn no_keyword_is_reserved_as_a_field_name_or_an_enum_member() {
        // SPEC Appendix B.3: keywords are recognised only where they mean
        // something, and never as names.
        let schema = one_schema(
            "schema A {\n    schema: text\n    text: int\n    versions: bool\n    logic: enum(text, true, version)\n    data: text\n    format: text\n}\n",
        );
        assert_eq!(
            schema.field_names(),
            ["schema", "text", "versions", "logic", "data", "format"]
        );
        assert!(matches!(
            schema.field("logic").and_then(FieldDecl::type_expr),
            Some(TypeExpr::Enum { members }) if members == &["text", "true", "version"]
        ));
    }

    #[test]
    fn a_field_name_that_is_not_an_identifier_is_e316() {
        assert_eq!(ids("schema A {\n    1.5: text\n}\n"), [ErrorId::E316]);
    }

    // --------------------------------------------------------------- §4.10

    #[test]
    fn defaults_are_validated_against_their_own_type() {
        let error = first("schema Thing {\n    n: int(1..10) = 999\n}\n");
        assert_eq!(error.id, ErrorId::E313);
        assert!(error.message.contains("Thing.n"), "{}", error.message);
        assert!(error.message.contains("999"), "{}", error.message);

        assert_eq!(ids("schema A {\n    n: int = x\n}\n"), [ErrorId::E313]);
        assert_eq!(ids("schema A {\n    n: int = \"5\"\n}\n"), [ErrorId::E313]);
        assert_eq!(ids("schema A {\n    b: bool = yes\n}\n"), [ErrorId::E313]);
        assert_eq!(
            ids("schema A {\n    s: enum(a, b) = c\n}\n"),
            [ErrorId::E313]
        );
        assert_eq!(
            ids("schema A {\n    t: text(1..3) = abcd\n}\n"),
            [ErrorId::E313]
        );
        assert_eq!(
            ids("schema A {\n    i: file(png) = a.tga\n}\n"),
            [ErrorId::E313]
        );
    }

    #[test]
    fn a_list_default_may_be_bare_or_bracketed_and_a_scalar_one_may_not_be_a_list() {
        let schema = one_schema(
            "schema A {\n    tags[]: enum(core, public) = [core, public]\n    more[]: enum(core, public) = core, public\n    one[]: enum(core, public) = core\n}\n",
        );
        for field in &schema.fields {
            match &field.default().expect("a default").value {
                SyntaxValue::List(items, _) if field.name != "one" => assert_eq!(items.len(), 2),
                SyntaxValue::Bare(text) => assert_eq!(text, "core"),
                other => panic!("unexpected default {other:?}"),
            }
        }
        let error = first("schema A {\n    t: text = a, b\n}\n");
        assert_eq!(error.id, ErrorId::E313);
        assert!(error
            .notes
            .iter()
            .any(|note| note.text.contains("not a list")));
    }

    #[test]
    fn a_default_and_optional_together_are_e320_and_e320_precedes_e313() {
        assert_eq!(
            ids("schema A {\n    n: int @optional = 1\n}\n"),
            [ErrorId::E320]
        );
        assert_eq!(
            ids("schema A {\n    n: int(1..10) @optional = 999\n}\n"),
            [ErrorId::E320]
        );
    }

    #[test]
    fn a_default_that_carries_an_interpolation_reference_is_checked_when_it_is_filled() {
        assert!(parse("schema A {\n    label: text(1..4) = ${id}_display\n}\n").is_ok());
        assert!(parse("schema A {\n    icon: file(png) = textures/$id.png\n}\n").is_ok());
    }

    #[test]
    fn a_wildcard_default_is_accepted_only_on_an_enum_list_field() {
        assert!(parse("schema A {\n    flags[]: enum(hat_a, hat_b) = hat_*\n}\n").is_ok());
        assert_eq!(
            ids("schema A {\n    flags[]: enum(hat_a) = cap_*\n}\n"),
            [ErrorId::E313]
        );
        assert_eq!(
            ids("schema A {\n    flag: enum(hat_a) = hat_*\n}\n"),
            [ErrorId::E313]
        );
    }

    // --------------------------------------------------------------- §4.11

    #[test]
    fn template_is_never_a_field_name_at_root_but_is_ordinary_in_a_group() {
        assert_eq!(ids("schema A {\n    template: text\n}\n"), [ErrorId::E314]);
        let schema = one_schema("schema A {\n    caps[] {\n        template: text\n    }\n}\n");
        assert_eq!(
            schema.fields[0]
                .group_fields()
                .map(|fields| fields[0].name.clone()),
            Some("template".to_string())
        );
    }

    #[test]
    fn id_may_be_declared_only_as_text_and_keeps_out_of_the_field_list() {
        let plain = one_schema("schema Pack {\n    id: text\n    title: text(1..40)\n}\n");
        assert_eq!(plain.field_names(), ["title"]);
        assert_eq!(plain.id_ranges(), Vec::new());

        let ranged = one_schema("schema Sticker {\n    id: text(3..24)\n    title: text\n}\n");
        assert_eq!(ranged.id_ranges(), vec![IntRange { min: 3, max: 24 }]);

        let implicit = one_schema("schema Plain {\n    title: text\n}\n");
        assert_eq!(implicit.id_ranges(), vec![IMPLICIT_ID_RANGE]);
    }

    #[test]
    fn any_other_id_declaration_is_e314() {
        for spelling in [
            "id: int",
            "id: text @optional",
            "id: text @tag",
            "id[]: text",
            "id: text = x",
            "id: enum(a, b)",
        ] {
            let error = first(&format!("schema A {{\n    {spelling}\n}}\n"));
            assert_eq!(error.id, ErrorId::E314, "{spelling}");
            assert!(error.message.contains("found"), "{}", error.message);
        }
    }

    #[test]
    fn id_inside_a_group_is_an_ordinary_field() {
        let schema =
            one_schema("schema A {\n    caps[] {\n        id: enum(search, sync) @tag\n    }\n}\n");
        let tag = schema.fields[0].tag_field().expect("a @tag field");
        assert_eq!(tag.name, "id");
    }

    // --------------------------------------------------------------- §4.12

    #[test]
    fn the_versions_declaration_is_read_and_validated() {
        let tables = tables("versions 2..5\nschema A {\n    a: text\n}\n").expect("tables");
        assert_eq!(tables.versions, VersionRange::new(2, 5).unwrap());
        assert_eq!(ids("versions 0..3\n"), [ErrorId::E602]);
        assert_eq!(ids("versions 3..1\n"), [ErrorId::E602]);
        assert_eq!(ids("versions -1..3\n"), [ErrorId::E602]);
        assert_eq!(ids("versions 1\n"), [ErrorId::E210]);
    }

    #[test]
    fn an_absent_versions_declaration_means_one_to_one() {
        let tables = tables("schema A {\n    a: text\n}\n").expect("tables");
        assert_eq!(tables.versions, VersionRange::DEFAULT);
        assert!(tables.versions_at.is_none());
    }

    #[test]
    fn a_second_versions_declaration_is_e601() {
        let errors = tables("versions 1..2\nversions 1..3\nschema A {\n    a: text\n}\n")
            .expect_err("two ranges");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E601));
        assert_eq!(errors.first().map(|item| item.notes.len()), Some(1));
    }

    #[test]
    fn an_annotation_outside_the_project_range_is_e603() {
        assert_eq!(
            validation_ids("versions 1..3\nschema A {\n    a: text @since(9)\n}\n"),
            [ErrorId::E603]
        );
        assert_eq!(
            validation_ids("versions 1..3\nschema A {\n    a: text @removed(0)\n}\n"),
            [ErrorId::E603]
        );
    }

    #[test]
    fn removed_must_be_greater_than_since_and_an_empty_window_is_reported() {
        assert_eq!(
            validation_ids("versions 1..3\nschema A {\n    a: text @since(3) @removed(2)\n}\n"),
            [ErrorId::E604]
        );
        assert_eq!(
            validation_ids("versions 1..3\nschema A {\n    a: text @since(2) @removed(2)\n}\n"),
            [ErrorId::E604]
        );
    }

    #[test]
    fn a_child_that_outlives_its_group_is_e605() {
        let errors = validation_ids(
            "versions 1..3\nschema A {\n    g @removed(2) {\n        a: text @since(2)\n    }\n}\n",
        );
        assert_eq!(errors, [ErrorId::E605]);
    }

    #[test]
    fn a_nested_schema_with_no_field_in_the_window_is_e605() {
        let errors = validation_ids(
            "versions 1..2\nschema Owner {\n    team: text @since(2)\n}\nschema A {\n    owner: $(Owner) @removed(2)\n}\n",
        );
        assert_eq!(errors, [ErrorId::E605]);
        // Both windows intersect in version 2, so this one is legal.
        assert!(validated(
            "versions 1..2\nschema Owner {\n    team: text @since(2)\n}\nschema A {\n    owner: $(Owner) @since(2)\n}\n"
        )
        .is_ok());
    }

    #[test]
    fn a_nested_schema_that_declares_no_field_exists_in_no_version() {
        // SPEC §4.12: a $(Schema) field's window is intersected with the union
        // of the referenced schema's fields' windows, so a schema with no
        // field at all can never be reached through one.
        assert_eq!(
            validation_ids("schema Empty {\n}\nschema A {\n    e: $(Empty)\n}\n"),
            [ErrorId::E605]
        );
        // Declaring the schema is still legal; only reaching it is not.
        assert!(validated("schema Empty {\n}\n").is_ok());
    }

    #[test]
    fn field_windows_answer_which_versions_a_field_exists_in() {
        let tables = validated(
            "versions 1..3\nschema Item {\n    name: text\n    glow: bool @since(2) = false\n    legacy_tint: int(0..255) @removed(3) @optional\n}\n",
        )
        .expect("the schema validates");
        let schema = tables.schema("Item").expect("Item");
        let project = tables.versions;
        let glow = schema.field("glow").expect("glow");
        let legacy = schema.field("legacy_tint").expect("legacy_tint");
        assert!(!glow.exists_in(1, project));
        assert!(glow.exists_in(2, project) && glow.exists_in(3, project));
        assert!(legacy.exists_in(1, project) && legacy.exists_in(2, project));
        assert!(!legacy.exists_in(3, project));
    }

    // ---------------------------------------------------------- P2 and P3

    #[test]
    fn a_duplicate_schema_name_is_e301_and_names_both_sites() {
        let errors = tables("schema Item {\n    a: text\n}\nschema Item {\n    b: text\n}\n")
            .expect_err("a duplicate");
        let first = errors.first().expect("one diagnostic");
        assert_eq!(first.id, ErrorId::E301);
        assert_eq!(first.position, Some(Position::new(4, 1)));
        assert_eq!(first.notes[0].position, Some(Position::new(1, 1)));
    }

    #[test]
    fn a_logic_block_must_name_a_declared_schema() {
        let errors = tables("schema Product {\n    a: text\n}\nlogic Prodcut {\n}\n")
            .expect_err("an unbound block");
        let first = errors.first().expect("one diagnostic");
        assert_eq!(first.id, ErrorId::E501);
        assert!(
            first.message.contains("Did you mean 'Product'?"),
            "{}",
            first.message
        );

        let tables = tables("schema Product {\n    a: text\n}\nlogic Product {\n}\n")
            .expect("a bound block");
        assert!(tables.logic_for("Product").is_some());
    }

    #[test]
    fn a_second_logic_block_for_one_schema_is_e502() {
        let errors = tables("schema P {\n    a: text\n}\nlogic P {\n}\nlogic P {\n}\n")
            .expect_err("two blocks");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E502));
    }

    #[test]
    fn an_unknown_schema_in_a_type_is_e309_with_a_suggestion() {
        let errors = validation_ids("schema Product {\n    a: $(prodcut)\n}\n");
        assert_eq!(errors, [ErrorId::E309]);
        let tables = tables("schema Product {\n    a: $(Prodcut)\n}\n").expect("tables");
        let message = validate_schemas(&tables)
            .expect_err("an unknown schema")
            .first()
            .expect("one diagnostic")
            .message
            .clone();
        assert!(message.contains("Did you mean 'Product'?"), "{message}");
        assert_eq!(
            validation_ids("schema A {\n    a: ref(Nope)\n}\n"),
            [ErrorId::E309]
        );
    }

    #[test]
    fn a_required_non_list_cycle_is_e321_and_an_optional_one_is_not() {
        assert_eq!(
            validation_ids("schema A {\n    child: $(A)\n}\n"),
            [ErrorId::E321]
        );
        assert_eq!(
            validation_ids("schema A {\n    b: $(B)\n}\nschema B {\n    a: $(A)\n}\n"),
            [ErrorId::E321]
        );
        assert!(validated("schema A {\n    child: $(A) @optional\n}\n").is_ok());
        assert!(validated("schema A {\n    children[]: $(A)\n}\n").is_ok());
    }

    #[test]
    fn a_schema_with_no_instance_is_still_fully_checked() {
        assert_eq!(
            validation_ids("schema A {\n    a: $(Missing)\n    b: text\n}\n"),
            [ErrorId::E309]
        );
        // A constraint that can never hold is reported without any instance.
        assert_eq!(ids("schema A {\n    b: text(-1..2)\n}\n"), [ErrorId::E318]);
    }

    #[test]
    fn the_worked_example_of_appendix_c_parses_and_validates() {
        let text = "versions 1..2\n\
                    \n\
                    schema Pack {\n\
                    \x20   id: text(3..24)\n\
                    \x20   title: text(1..40)\n\
                    \x20   tier: enum(free, plus) = free\n\
                    \x20   slot_count: int(1..9) @optional\n\
                    }\n\
                    \n\
                    schema Sticker {\n\
                    \x20   pack: ref(Pack)\n\
                    \x20   title: text(1..60)\n\
                    \x20   icon: image(png 128x128)\n\
                    \x20   rarity: enum(common, rare, epic) = common\n\
                    \x20   glow: bool @since(2) = false\n\
                    \x20   tint: int(0..255) @optional @removed(2)\n\
                    \x20   owner {\n\
                    \x20       team: text(1..40)\n\
                    \x20       contact: text(1..60) @optional\n\
                    \x20   }\n\
                    \x20   tags[]: enum(core_ui, core_game, promo) @optional\n\
                    \x20   copy[] {\n\
                    \x20       key: enum(en_us, es_es, es_mx) @tag\n\
                    \x20       value: text(1..80)\n\
                    \x20   }\n\
                    }\n\
                    \n\
                    logic Sticker {\n\
                    \x20   derive? .owner.contact = support@example.com\n\
                    }\n";
        let tables = validated(text).expect("the specification's own example");
        assert_eq!(tables.versions, VersionRange::new(1, 2).unwrap());
        assert_eq!(tables.schema_names(), ["Pack", "Sticker"]);
        assert!(tables.logic_for("Sticker").is_some());

        let pack = tables.schema("Pack").expect("Pack");
        assert_eq!(pack.field_names(), ["title", "tier", "slot_count"]);
        assert_eq!(pack.id_ranges(), vec![IntRange { min: 3, max: 24 }]);
        assert_eq!(
            pack.field("tier")
                .and_then(FieldDecl::default)
                .map(|value| value.text.as_str()),
            Some("free")
        );
        assert!(pack.field("slot_count").expect("slot_count").is_optional());

        let sticker = tables.schema("Sticker").expect("Sticker");
        assert_eq!(
            sticker.field_names(),
            ["pack", "title", "icon", "rarity", "glow", "tint", "owner", "tags", "copy"]
        );
        assert_eq!(sticker.id_ranges(), vec![IMPLICIT_ID_RANGE]);

        let project = tables.versions;
        let glow = sticker.field("glow").expect("glow");
        assert!(!glow.exists_in(1, project) && glow.exists_in(2, project));
        let tint = sticker.field("tint").expect("tint");
        assert!(tint.exists_in(1, project) && !tint.exists_in(2, project));

        let copy = sticker.field("copy").expect("copy");
        assert!(copy.is_list());
        assert_eq!(
            copy.tag_field().map(|field| field.name.as_str()),
            Some("key")
        );
        assert!(matches!(
            sticker.field("icon").and_then(FieldDecl::type_expr),
            Some(TypeExpr::Image { alternatives })
                if alternatives == &[ImageAlt {
                    extension: "png".to_string(),
                    width: Some(128),
                    height: Some(128),
                }]
        ));
    }

    // ------------------------------------------------------- recovery, E210

    #[test]
    fn a_block_written_where_a_type_belongs_is_e304_and_reports_once() {
        assert_eq!(
            ids("schema A {\n    caps[]: {\n        x: text\n    }\n}\n"),
            [ErrorId::E304]
        );
    }

    #[test]
    fn an_annotation_needs_its_parentheses_and_a_version_number() {
        assert_eq!(ids("schema A {\n    a: text @since\n}\n"), [ErrorId::E210]);
        assert_eq!(
            ids("schema A {\n    a: text @since (2)\n}\n"),
            [ErrorId::E210]
        );
        assert_eq!(
            ids("schema A {\n    a: text @since(x)\n}\n"),
            [ErrorId::E210]
        );
    }

    #[test]
    fn a_nested_schema_or_ref_type_names_exactly_one_schema() {
        assert_eq!(ids("schema A {\n    a: $()\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a: $(X, Y)\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a: ref(X, Y)\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a: $\n}\n"), [ErrorId::E304]);
    }

    #[test]
    fn bool_takes_no_arguments_and_an_extension_is_letters_and_digits() {
        assert_eq!(ids("schema A {\n    a: bool(1)\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a: file(pn-g)\n}\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_list_head_carries_no_internal_whitespace() {
        assert!(parse("schema A {\n    a[]: text\n}\n").is_ok());
        // SPEC §4.3: `[]` marks a list field only immediately after the name.
        assert_eq!(ids("schema A {\n    a []: text\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a [ ]: text\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a[2 .. 4]: text\n}\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_group_named_id_or_template_at_root_is_still_rejected() {
        let identity = first("schema A {\n    id {\n        x: text\n    }\n}\n");
        assert_eq!(identity.id, ErrorId::E314);
        assert!(
            identity.message.contains("id { … }"),
            "{}",
            identity.message
        );
        assert_eq!(
            ids("schema A {\n    template {\n        x: text\n    }\n}\n"),
            [ErrorId::E314]
        );
    }

    #[test]
    fn an_empty_default_is_rejected_and_a_tag_default_is_e313() {
        assert_eq!(ids("schema A {\n    a: text =\n}\n"), [ErrorId::E210]);
        assert_eq!(ids("schema A {\n    a: text = #x\n}\n"), [ErrorId::E313]);
        assert_eq!(
            ids("schema A {\n    a[]: text = [[x]]\n}\n"),
            [ErrorId::E441]
        );
        assert_eq!(ids("schema A {\n    a[]: text = x,\n}\n"), [ErrorId::E442]);
        assert_eq!(ids("schema A {\n    a: $(O) = x\n}\n"), [ErrorId::E313]);
    }

    #[test]
    fn a_file_nested_far_deeper_than_the_limit_costs_no_stack() {
        // The audit's `deep-groups-3000` case: the parser must report E209 and
        // return, never recurse once per level.
        let levels = 3000;
        let mut text = String::from("schema A {\n");
        for index in 0..levels {
            text.push_str(&format!("g{index} {{\n"));
        }
        text.push_str("leaf: text\n");
        for _ in 0..levels {
            text.push_str("}\n");
        }
        text.push_str("}\n");
        assert_eq!(ids(&text), [ErrorId::E209]);
    }

    #[test]
    fn a_ref_default_names_an_id_and_is_resolved_at_instance_time() {
        let schema = one_schema("schema A {\n    pack: ref(Pack) = winter_2026\n}\n");
        assert_eq!(
            schema.fields[0].default().map(|value| value.text.as_str()),
            Some("winter_2026")
        );
        assert_eq!(
            ids("schema A {\n    pack: ref(Pack) = not an id\n}\n"),
            [ErrorId::E313]
        );
    }

    // -------------------------------------------------------------- helpers

    #[test]
    fn extensions_canonicalise_jpeg_and_nothing_else() {
        assert_eq!(canonical_extension(".PNG"), "png");
        assert_eq!(canonical_extension("JPEG"), "jpg");
        assert_eq!(canonical_extension("jpg"), "jpg");
        assert_eq!(canonical_extension("tga"), "tga");
    }

    #[test]
    fn cardinality_bounds_are_inclusive() {
        assert!(Cardinality::ANY.accepts(0));
        let two_to_four = Cardinality {
            min: 2,
            max: Some(4),
        };
        assert!(!two_to_four.accepts(1));
        assert!(two_to_four.accepts(2));
        assert!(two_to_four.accepts(4));
        assert!(!two_to_four.accepts(5));
        let at_least_two = Cardinality { min: 2, max: None };
        assert!(at_least_two.accepts(99));
        assert_eq!(at_least_two.describe(), "at least 2");
        assert_eq!(two_to_four.describe(), "2..4");
    }

    #[test]
    fn suggestions_follow_the_normative_algorithm() {
        let candidates = ["status", "state", "name"];
        assert_eq!(
            suggest("statu", &candidates, TieBreak::DeclarationOrder),
            Some("status".to_string())
        );
        // Distance 3 is beyond the cut-off, and so is half the length.
        assert_eq!(
            suggest("zzz", &candidates, TieBreak::DeclarationOrder),
            None
        );
        assert_eq!(suggest("ab", &["cd"], TieBreak::ScalarOrder), None);
        // Ties break by declaration order, or by scalar order when asked.
        assert_eq!(
            suggest("xtate", &["state", "otate"], TieBreak::DeclarationOrder),
            Some("state".to_string())
        );
        assert_eq!(
            suggest("xtate", &["state", "otate"], TieBreak::ScalarOrder),
            Some("otate".to_string())
        );
    }

    #[test]
    fn a_trailing_modifier_run_is_recognised_only_after_whitespace() {
        assert!(trailing_modifier_run("false @since(2)").is_some());
        assert!(trailing_modifier_run("false @since(2) @removed(3)").is_some());
        assert!(trailing_modifier_run("x @optional").is_some());
        assert!(trailing_modifier_run("systems@example.com").is_none());
        assert!(trailing_modifier_run("false @since(2) extra").is_none());
        assert!(trailing_modifier_run("plain").is_none());
    }

    #[test]
    fn interpolation_is_detected_and_a_literal_dollar_is_not() {
        assert!(carries_interpolation("$id"));
        assert!(carries_interpolation("a${id}b"));
        assert!(!carries_interpolation("$$99"));
        assert!(!carries_interpolation("plain"));
    }

    #[test]
    fn parsing_never_panics_on_hostile_input() {
        let inputs = [
            "",
            "\n\n\n",
            "schema",
            "schema A",
            "schema A {",
            "schema A {\n",
            "schema A {\n    a\n}\n",
            "schema A {\n    a:\n}\n",
            "schema A {\n    a: \n}\n",
            "schema A {\n    : text\n}\n",
            "schema A {\n    a: enum(\n}\n",
            "schema A {\n    a: image(\n}\n",
            "schema A {\n    a: $(\n}\n",
            "schema A {\n    a: text =\n}\n",
            "schema A {\n    a: text = [\n}\n",
            "schema A {\n    a: text = [[x]]\n}\n",
            "schema A {\n    a: text = x,\n}\n",
            "logic",
            "logic A",
            "logic A {\n",
            "versions",
            "versions 1..",
            "@",
            "}",
            "{",
        ];
        for input in inputs {
            let source = SourceFile::new("templates/T.abt", input);
            if let Ok(tokens) = crate::lexer::tokenize(&source) {
                let _ = parse_template("templates/T.abt", &source.text, &tokens);
            }
        }
    }

    #[test]
    fn a_malformed_image_size_token_is_an_invalid_size() {
        // SPEC §4.4.7: a size token is one lexeme in the size position, so a
        // malformed one is E308 and never the identifier rules' E206.
        assert_eq!(
            ids("schema A {\n    i: image(png -1x-1)\n}\n"),
            [ErrorId::E308]
        );
        assert!(parse("schema A {\n    i: image(png 128x128, jpg *x64)\n}\n").is_ok());
    }

    #[test]
    fn a_group_at_exactly_the_nesting_limit_is_accepted() {
        // SPEC §3.7 reports E209 only when a limit is exceeded.
        let body = |levels: usize| {
            let mut text = String::from("schema A {\n");
            for index in 0..levels {
                text.push_str(&format!(
                    "{}g{index} @optional {{\n",
                    "    ".repeat(index + 1)
                ));
            }
            text.push_str(&format!("{}x: text\n", "    ".repeat(levels + 1)));
            for index in (0..levels).rev() {
                text.push_str(&format!("{}}}\n", "    ".repeat(index + 1)));
            }
            text.push_str("}\n");
            text
        };
        assert!(parse(&body(GROUP_DEPTH)).is_ok());
        assert_eq!(ids(&body(GROUP_DEPTH + 1)), [ErrorId::E209]);
    }

    #[test]
    fn a_project_version_range_is_bounded_by_count_not_by_number() {
        // SPEC §7.4 compiles every version in the range, so the count is
        // bounded; SPEC §4.12 bounds neither number, so the numbers are free.
        assert!(parse(&format!("versions 1..{MAX_PROJECT_VERSIONS}\n")).is_ok());
        assert_eq!(
            ids(&format!("versions 1..{}\n", MAX_PROJECT_VERSIONS + 1)),
            [ErrorId::E602]
        );
        assert_eq!(ids("versions 1..2000000000\n"), [ErrorId::E602]);
        assert!(parse(&format!("versions 100..{}\n", 99 + MAX_PROJECT_VERSIONS)).is_ok());
    }
}
