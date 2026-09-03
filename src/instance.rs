//! Instances: headers, header tags, clone statements, body statements and
//! syntax values (SPEC chapter 5).
//!
//! The parser produces *syntax values* and infers no types; the validator
//! interprets each one against the declared type of the field it is assigned
//! to (SPEC §5.10).
//!
//! Everything reported here is decidable from the token stream alone: the
//! shape of a header, where a clone may stand, the shape of a path, a value
//! or a tuple row, and where an annotation may appear. The checks that need a
//! schema or the project version range — E401, E402, E409, E412, E413, E429,
//! E430, E438, E440 — belong to later phases (SPEC §7.1, §7.2).
//! [`duplicate_assignments`] implements the E429 comparison of SPEC §5.2,
//! §5.4 and §5.13 so that P2 can run it once the version range is known; the
//! parser only records the window each assignment was written with.

use crate::ast::{InstanceFile, Located, Path};
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note, Position};
use crate::lexer::{is_identifier, normalise, Token, TokenKind};
use crate::limits::{BRACKET_DEPTH, PATH_SEGMENTS};
use crate::versions::{VersionRange, Window};

/// One `Schema :: …` declaration and everything under it (SPEC §5.1).
#[derive(Clone, Debug)]
pub struct InstanceDecl {
    /// The schema name from the header, exactly as written.
    pub template: String,
    /// The normalised id, from `@id` or from the file stem (SPEC §5.3).
    pub id: String,
    /// Where the id was written: the `@id` tag, or the header itself when the
    /// id comes from the file stem. E413 on the id is reported here (§5.3).
    pub id_at: Located,
    /// The id exactly as written, when it is not an identifier.
    ///
    /// SPEC §5.3 makes the id something the project tables compute, so E428 is
    /// a P2 diagnostic (SPEC §7.2): the parser records the offending text and
    /// P1 still succeeds, which is what lets one run report a duplicate id in
    /// one file and an unusable id in another.
    pub invalid_id: Option<String>,
    /// The instance version window, from annotations at the end of the header.
    pub window: Window,
    pub tags: Vec<HeaderTag>,
    pub clones: Vec<CloneStatement>,
    pub statements: Vec<BodyStatement>,
    pub at: Located,
}

/// `@name.value` or the bare flag `@name` (SPEC §5.2).
#[derive(Clone, Debug)]
pub struct HeaderTag {
    /// Normalised tag name; it targets one root field.
    pub name: String,
    /// The name as it was written, for the `'@{name}'` of E438.
    pub spelled: String,
    pub value: SyntaxValue,
    /// True for the bare flag form `@name`, which SPEC §5.2 allows only on a
    /// `bool` field (E412).
    pub flag: bool,
    pub at: Located,
}

/// `&id`, `&id.*` or `&id.path` (SPEC §5.7).
#[derive(Clone, Debug)]
pub struct CloneStatement {
    /// The normalised id of the clone source.
    pub source: String,
    /// `None` for a full clone; otherwise the subtree to copy.
    pub path: Option<Path>,
    pub at: Located,
}

/// One `path: value` assignment, with its trailing annotations (SPEC §5.4,
/// §5.13).
#[derive(Clone, Debug)]
pub struct BodyStatement {
    pub path: Path,
    pub value: SyntaxValue,
    pub window: Window,
    pub at: Located,
}

/// One argument of a `#tag(…)` object (SPEC §5.5).
#[derive(Clone, Debug, PartialEq)]
pub struct TagArg {
    /// The argument key, which may be dotted into the group's own groups.
    pub path: Path,
    pub value: SyntaxValue,
    /// True for the bare flag form, which is valid only on a `bool` field
    /// (E412).
    pub flag: bool,
}

/// How a list value was written (SPEC §5.5).
///
/// The two spellings mean the same thing, and only one diagnostic tells them
/// apart: the E412 note that offers to quote a literal comma is about the bare
/// spelling and would be a non-sequitur about `[a, b]`, a tuple array or a
/// list a default or a `derive` produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListSpelling {
    /// `a, b` — depth-0 commas, where a literal comma would need quoting.
    Commas,
    /// `[a, b]`, a tuple array, or a list built by a default or by logic.
    Brackets,
}

/// The syntax values of SPEC §5.10. None of them carries a type: the
/// validator decides what each one means from the field it is assigned to.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxValue {
    /// A `quoted_string`, escapes already decoded.
    Quoted(String),
    /// `bare_text`, exactly as written and trimmed.
    Bare(String),
    /// A bracketed list or a depth-0 comma list, with the spelling it was
    /// written in.
    List(Vec<SyntaxValue>, ListSpelling),
    /// `#name` or `#name(key: value, flag)`.
    Tag { name: String, args: Vec<TagArg> },
    /// Fields built from dotted paths, multi-paths or tuple rows.
    Object(Vec<(String, SyntaxValue)>),
}

impl SyntaxValue {
    /// The `{found}` substitution of SPEC §9.8 for a type-mismatch message.
    pub fn shape(&self) -> &'static str {
        match self {
            SyntaxValue::Quoted(_) | SyntaxValue::Bare(_) => "text",
            SyntaxValue::List(_, _) => "list",
            SyntaxValue::Tag { .. } => "tag object",
            SyntaxValue::Object(_) => "object",
        }
    }
}

/// Parses one instance file (SPEC §5.1).
///
/// `file` is the project-relative path the tokens came from; it names the file
/// in every diagnostic and supplies the file stem an instance without `@id`
/// takes its identity from (SPEC §5.3).
pub fn parse_instances(file: &str, tokens: &[Token]) -> Result<InstanceFile, Diagnostics> {
    let mut unit = InstanceFile::new(file);
    if tokens.is_empty() {
        return Ok(unit);
    }
    let mut parser = Parser::new(file, tokens);
    unit.instances = parser.run();
    if parser.errors.is_empty() {
        Ok(unit)
    } else {
        Err(parser.errors)
    }
}

/// The E429 check of SPEC §5.2, §5.4 and §5.13: two header tags with the same
/// normalised name, a header tag and a body statement writing one path, or two
/// body statements whose applicability sets intersect.
///
/// It runs in P2 rather than in the parser because the message names the
/// versions the two assignments share, which needs the project range
/// (SPEC §7.2). The comparison here intersects each statement's annotation
/// range with the project range and the instance window; a caller that also
/// knows the field's existence set (§4.12) intersects that in as well, which
/// can only narrow the result.
pub fn duplicate_assignments(decl: &InstanceDecl, project: VersionRange) -> Diagnostics {
    let mut out = Diagnostics::new();
    let Some(instance) = decl.window.resolve(project) else {
        return out;
    };
    let mut seen: Vec<(String, VersionRange, Located)> = Vec::new();
    let record = |out: &mut Diagnostics,
                  seen: &mut Vec<(String, VersionRange, Located)>,
                  path: String,
                  range: VersionRange,
                  at: Located| {
        for (earlier_path, earlier_range, earlier_at) in seen.iter() {
            if *earlier_path != path {
                continue;
            }
            let Some(shared) = earlier_range.intersect(range) else {
                continue;
            };
            out.push(
                Diagnostic::at(
                    ErrorId::E429,
                    at.file.clone(),
                    at.position,
                    format!("{path} is assigned twice for version(s) {shared}."),
                )
                .with_note(
                    Note::new("first assigned here.")
                        .at(earlier_at.file.clone(), earlier_at.position),
                ),
            );
            break;
        }
        seen.push((path, range, at));
    };

    for tag in &decl.tags {
        record(
            &mut out,
            &mut seen,
            tag.name.clone(),
            instance,
            tag.at.clone(),
        );
    }
    for statement in &decl.statements {
        let Some(range) = statement
            .window
            .resolve(project)
            .and_then(|range| range.intersect(instance))
        else {
            continue;
        };
        record(
            &mut out,
            &mut seen,
            statement.path.text(),
            range,
            statement.at.clone(),
        );
    }
    out
}

/// The file stem of SPEC §5.3: the file name with a final `.ab` or `.abt`
/// removed, compared case-insensitively.
fn file_stem(path: &str) -> &str {
    let name = match path.rfind(['/', '\\']) {
        Some(index) => path.get(index + 1..).unwrap_or(""),
        None => path,
    };
    for extension in [".abt", ".ab"] {
        if name.len() <= extension.len() {
            continue;
        }
        let split = name.len() - extension.len();
        if let (Some(head), Some(tail)) = (name.get(..split), name.get(split..)) {
            if tail.eq_ignore_ascii_case(extension) {
                return head;
            }
        }
    }
    name
}

/// The spelling a token contributes to a reconstructed source fragment.
fn token_spelling(token: &Token) -> String {
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

/// What the left side of an assignment turned out to be (SPEC §5.4).
enum Head {
    /// `path`, which a `:` makes an assignment and a `{` makes a body block.
    Path(Vec<String>),
    /// `prefix.{a, b}`.
    Multi(Vec<String>, Vec<String>),
    /// `name(col, …)`.
    Tuple(Vec<String>, Vec<String>),
}

struct Parser<'a> {
    file: &'a str,
    tokens: &'a [Token],
    index: usize,
    errors: Diagnostics,
}

impl<'a> Parser<'a> {
    fn new(file: &'a str, tokens: &'a [Token]) -> Self {
        Self {
            file,
            tokens,
            index: 0,
            errors: Diagnostics::new(),
        }
    }

    // ---------------------------------------------------------------- cursor

    /// The token at `index`, clamped to the final `EndOfFile`. `tokens` is
    /// never empty here: [`parse_instances`] returns early when it is.
    fn token(&self, index: usize) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        &self.tokens[index.min(last)]
    }

    fn peek(&self) -> &Token {
        self.token(self.index)
    }

    fn ahead(&self, distance: usize) -> &Token {
        self.token(self.index.saturating_add(distance))
    }

    fn bump(&mut self) {
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
    }

    fn goto(&mut self, index: usize) {
        self.index = index.min(self.tokens.len().saturating_sub(1));
    }

    fn at_end(&self) -> bool {
        self.peek().is_end_of_file()
    }

    fn located(&self) -> Located {
        Located::new(self.file, self.peek().position)
    }

    /// The index of the `Newline` (or `EndOfFile`) that closes the logical
    /// line the cursor is on.
    fn line_end(&self) -> usize {
        let mut index = self.index;
        loop {
            let token = self.token(index);
            if token.is_newline() || token.is_end_of_file() {
                return index;
            }
            if index + 1 >= self.tokens.len() {
                return self.tokens.len().saturating_sub(1);
            }
            index += 1;
        }
    }

    fn finish_line(&mut self, end: usize) {
        self.goto(end);
        if self.peek().is_newline() {
            self.bump();
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek().is_newline() {
            self.bump();
        }
    }

    /// Consumes the remainder of a body block whose opening line was
    /// abandoned. Iterative, so a deeply nested file cannot grow the stack.
    fn discard_block(&mut self) {
        let mut open = 1usize;
        while open > 0 && !self.at_end() {
            if self.peek().is_punctuation("{") {
                open += 1;
            } else if self.peek().is_punctuation("}") {
                open -= 1;
            }
            self.bump();
        }
        let end = self.line_end();
        self.finish_line(end);
    }

    /// Reconstructs the source text of `tokens[from..to]`. Adjacency decides
    /// the spacing, so `my thing` and `a.b` both come back as written.
    fn render(&self, from: usize, to: usize) -> String {
        let mut out = String::new();
        let mut index = from;
        while index < to && index < self.tokens.len() {
            let token = &self.tokens[index];
            if !out.is_empty() && !token.glued {
                out.push(' ');
            }
            out.push_str(&token_spelling(token));
            index += 1;
        }
        out
    }

    /// The written form of an assignment's left side, for E316.
    fn head_text(&self, start: usize, end: usize) -> String {
        let mut stop = end;
        let mut index = start;
        while index < end {
            if self.token(index).is_punctuation(":") {
                stop = index;
                break;
            }
            index += 1;
        }
        self.render(start, stop)
    }

    // ---------------------------------------------------------------- errors

    fn error(&mut self, id: ErrorId, position: Position, message: String) {
        self.errors
            .push(Diagnostic::at(id, self.file, position, message));
    }

    fn fixed(&mut self, id: ErrorId, position: Position) {
        let message = id.template().to_string();
        self.error(id, position, message);
    }

    fn unexpected(&mut self, expected: &str) {
        let found = self.peek().describe();
        let position = self.peek().position;
        self.error(
            ErrorId::E210,
            position,
            format!("Unexpected {found} here; expected {expected}."),
        );
    }

    // ------------------------------------------------------------- top level

    fn run(&mut self) -> Vec<InstanceDecl> {
        let mut instances = Vec::new();
        let mut reported_stray = false;
        loop {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            if self.at_instance_header() {
                let decl = self.parse_instance();
                instances.push(decl);
                continue;
            }
            if !reported_stray {
                reported_stray = true;
                self.report_stray_line();
            }
            let end = self.line_end();
            let opens_block = end > self.index && self.token(end - 1).is_punctuation("{");
            self.finish_line(end);
            if opens_block {
                self.discard_block();
            }
        }
        instances
    }

    /// A logical line is an `instance_header` if and only if its first token
    /// is a schema name and the next one is `::` (SPEC §5.1).
    fn at_instance_header(&self) -> bool {
        matches!(self.peek().kind, TokenKind::SchemaName(_)) && self.ahead(1).is_punctuation("::")
    }

    /// A line before the first header: E439 when it carries a `::`, naming the
    /// text before it, E210 for a template declaration (§2.1), and E403
    /// otherwise (SPEC §5.1).
    fn report_stray_line(&mut self) {
        let start = self.index;
        let end = self.line_end();
        if self.report_double_colon(start, end) {
            return;
        }
        if self.report_template_declaration() {
            return;
        }
        let position = self.peek().position;
        self.fixed(ErrorId::E403, position);
    }

    /// SPEC §2.1: `schema`, `logic` and `versions` at statement position in a
    /// `.ab` file are E210. All three are also ordinary field names
    /// (Appendix B.3), so the keyword opens a declaration only when what
    /// follows it cannot continue an assignment head.
    fn report_template_declaration(&mut self) -> bool {
        let keyword = self.peek();
        let Some(word) = ["schema", "logic", "versions"]
            .into_iter()
            .find(|word| keyword.is_keyword(word))
        else {
            return false;
        };
        let next = self.ahead(1);
        let heads_an_assignment = next.is_punctuation(":")
            || next.is_punctuation(".")
            || next.is_punctuation("(")
            || next.is_punctuation("{")
            || next.is_punctuation("[")
            || next.is_punctuation("@")
            || next.is_newline()
            || next.is_end_of_file();
        if heads_an_assignment {
            return false;
        }
        let position = keyword.position;
        self.error(
            ErrorId::E210,
            position,
            format!(
                "Unexpected '{word}' here; 'schema', 'logic' and 'versions' belong in a .abt file."
            ),
        );
        true
    }

    /// SPEC §5.1: a logical line whose first token is not a schema name and
    /// which carries a `::` token is E439, naming the text before it. The `::`
    /// of a header is consumed with the header, and a `::` inside a value is
    /// never a token, so any `::` reaching here is out of place.
    fn report_double_colon(&mut self, start: usize, end: usize) -> bool {
        let mut index = start;
        while index < end {
            if self.token(index).is_punctuation("::") {
                let text = self.render(start, index);
                let position = self.token(index).position;
                self.error(
                    ErrorId::E439,
                    position,
                    format!("'::' starts an instance header, but '{text}' is not a schema name."),
                );
                return true;
            }
            index += 1;
        }
        false
    }

    // ---------------------------------------------------------------- header

    fn parse_instance(&mut self) -> InstanceDecl {
        let at = self.located();
        let template = self
            .peek()
            .identifier_text()
            .unwrap_or_default()
            .to_string();
        self.bump(); // the schema name
        self.bump(); // `::`
        let (tags, window) = self.parse_header();
        let (id, id_at, invalid_id) = self.instance_id(&tags, &at);
        let mut decl = InstanceDecl {
            template,
            id,
            id_at,
            invalid_id,
            window,
            tags,
            clones: Vec::new(),
            statements: Vec::new(),
            at,
        };
        self.parse_instance_body(&mut decl);
        decl
    }

    /// The header tag list and the instance version window that closes it
    /// (SPEC §5.2, §5.14).
    fn parse_header(&mut self) -> (Vec<HeaderTag>, Window) {
        let end = self.line_end();
        let mut tags = Vec::new();
        let mut window = Window::UNANNOTATED;
        let mut window_at: Option<Position> = None;
        while self.index < end {
            if self.peek().is_punctuation("@") {
                if self.annotation_name().is_some() {
                    let position = self.peek().position;
                    self.read_annotation(&mut window);
                    window_at.get_or_insert(position);
                    continue;
                }
                if window_at.is_some() {
                    // A header tag after the window: the annotation list ends
                    // the header (SPEC §5.14, grammar `instance_header`).
                    let position = self.peek().position;
                    self.fixed(ErrorId::E437, position);
                }
                match self.parse_header_tag(end) {
                    Some(tag) => tags.push(tag),
                    None => break,
                }
                continue;
            }
            if self.peek().is_punctuation(",") {
                // GRAMMAR `header_tag_list` begins with a tag, never with a
                // separator, and SPEC §5.2 gives the `,` no other job than
                // separating and continuing the list.
                if tags.is_empty() && window_at.is_none() {
                    self.unexpected("a header tag, '@since(n)', '@removed(n)' or end of line");
                    break;
                }
                let position = self.peek().position;
                self.bump();
                let continues = self.index < end
                    && self.peek().is_punctuation("@")
                    && self.annotation_name().is_none();
                if !continues {
                    self.fixed(ErrorId::E442, position);
                }
                continue;
            }
            if self.peek().is_punctuation("::") {
                let position = self.peek().position;
                self.fixed(ErrorId::E436, position);
                break;
            }
            self.unexpected("a header tag, '@since(n)', '@removed(n)' or end of line");
            break;
        }
        self.finish_line(end);
        (tags, window)
    }

    fn parse_header_tag(&mut self, end: usize) -> Option<HeaderTag> {
        let at = self.located();
        self.bump(); // `@`
        let spelled = match self.peek().identifier_text() {
            Some(text) => text.to_string(),
            None => {
                self.unexpected("a header tag name");
                return None;
            }
        };
        if !is_identifier(&spelled) {
            let position = self.peek().position;
            self.error(
                ErrorId::E316,
                position,
                format!("Invalid field name '{spelled}'."),
            );
            return None;
        }
        self.bump();
        // A tag that names an envelope key is E410, reported in P4 with every
        // other envelope assignment (SPEC §5.2, §7.3 step 1).
        let name = normalise(&spelled);
        // `@name` with nothing after it is the bare flag form; the value form
        // needs the separator dot glued to the name (SPEC §5.2).
        if self.index >= end || !self.peek().is_punctuation(".") {
            return Some(HeaderTag {
                name,
                spelled,
                value: SyntaxValue::Bare("true".to_string()),
                flag: true,
                at,
            });
        }
        self.bump(); // `.`
        if self.index >= end {
            // `@id.` carries an empty id, which P2 reports as E428 (SPEC §5.3);
            // any other empty tag value is a missing value.
            if name != "id" {
                self.unexpected("a value");
            }
            return Some(HeaderTag {
                name,
                spelled,
                value: SyntaxValue::Bare(String::new()),
                flag: false,
                at,
            });
        }
        let value = match &self.peek().kind {
            TokenKind::QuotedString(text) => SyntaxValue::Quoted(text.clone()),
            TokenKind::BareText(text) => {
                let text = text.clone();
                self.report_header_colon(&text);
                SyntaxValue::Bare(text)
            }
            _ => {
                self.unexpected("a quoted string or bare text");
                return None;
            }
        };
        self.bump();
        Some(HeaderTag {
            name,
            spelled,
            value,
            flag: false,
            at,
        })
    }

    /// SPEC §5.1: a second `::` inside a header is E436. The lexer keeps a
    /// `::` inside a header tag value in the value lexeme (§5.2), so the
    /// second `::` of `A :: @id.x :: y` is found here rather than as a token.
    fn report_header_colon(&mut self, text: &str) {
        let Some(offset) = text.find("::") else {
            return;
        };
        let column = text
            .get(..offset)
            .map(|head| head.chars().count() as u32)
            .unwrap_or(0);
        let start = self.peek().position;
        let position = Position::new(start.line, start.col.saturating_add(column));
        self.fixed(ErrorId::E436, position);
    }

    /// The instance's identity: the `@id` tag when there is one, and the file
    /// stem otherwise (SPEC §5.3).
    ///
    /// An id that is not an identifier is carried out as `invalid_id` rather
    /// than reported here; E428 belongs to P2 (SPEC §7.2, §5.3).
    fn instance_id(
        &mut self,
        tags: &[HeaderTag],
        header: &Located,
    ) -> (String, Located, Option<String>) {
        for tag in tags {
            if tag.name != "id" {
                continue;
            }
            if tag.flag {
                // SPEC §5.2: an instance's identity is never the boolean true.
                return (String::new(), tag.at.clone(), Some("@id".to_string()));
            }
            let text = match &tag.value {
                SyntaxValue::Quoted(text) | SyntaxValue::Bare(text) => text.clone(),
                other => other.shape().to_string(),
            };
            if text.contains("::") {
                // The second `::` of the header was already reported as E436;
                // the text it left in the value is not a second defect.
                return (normalise(&text), tag.at.clone(), None);
            }
            let invalid = (!is_identifier(&text)).then(|| text.clone());
            return (normalise(&text), tag.at.clone(), invalid);
        }
        let stem = file_stem(self.file).to_string();
        let invalid = (!is_identifier(&stem)).then(|| stem.clone());
        (normalise(&stem), header.clone(), invalid)
    }

    // ----------------------------------------------------------- annotations

    /// `since` or `removed` when the cursor is on a version annotation. The
    /// `(` MUST be glued to the keyword, which is what separates `@since(2)`
    /// from the header tag `@since` (SPEC §3.1, §5.2).
    fn annotation_name(&self) -> Option<&'static str> {
        if !self.peek().is_punctuation("@") {
            return None;
        }
        let name = match &self.ahead(1).kind {
            TokenKind::Identifier(text) if text == "since" => "since",
            TokenKind::Identifier(text) if text == "removed" => "removed",
            _ => return None,
        };
        let open = self.ahead(2);
        if !open.is_punctuation("(") || !open.glued {
            return None;
        }
        Some(name)
    }

    fn read_annotation(&mut self, window: &mut Window) {
        let Some(name) = self.annotation_name() else {
            return;
        };
        let position = self.peek().position;
        self.bump(); // `@`
        self.bump(); // `since` / `removed`
        self.bump(); // `(`
        let version = match &self.peek().kind {
            TokenKind::IntLiteral(text) => text.parse::<u32>().ok(),
            _ => None,
        };
        if version.is_some() {
            self.bump();
        } else {
            self.unexpected("a version number");
            if !self.peek().is_punctuation(")") {
                self.bump();
            }
        }
        if self.peek().is_punctuation(")") {
            self.bump();
        } else {
            self.unexpected("')'");
        }
        let slot = if name == "since" {
            &mut window.since
        } else {
            &mut window.removed
        };
        if slot.is_some() {
            self.error(
                ErrorId::E303,
                position,
                format!("Modifier '@{name}' is repeated."),
            );
            return;
        }
        *slot = version;
    }

    /// The index at which the trailing annotation run of a body statement
    /// begins, or `end` when there is none (SPEC §5.13).
    fn annotation_start(&self, end: usize) -> usize {
        let mut index = self.index;
        while index < end {
            if self.token(index).is_punctuation("@") {
                let saved = index;
                let probe = Parser {
                    file: self.file,
                    tokens: self.tokens,
                    index: saved,
                    errors: Diagnostics::new(),
                };
                if probe.annotation_name().is_some() {
                    return saved;
                }
            }
            index += 1;
        }
        end
    }

    // ------------------------------------------------------------------ body

    fn parse_instance_body(&mut self, decl: &mut InstanceDecl) {
        let mut seen_statement = false;
        loop {
            self.skip_newlines();
            if self.at_end() || self.at_instance_header() {
                return;
            }
            if self.peek().is_punctuation("&") {
                let position = self.peek().position;
                let clone = self.parse_clone();
                if seen_statement {
                    // SPEC §5.7: clones stand between the header and the first
                    // body statement and nowhere else.
                    self.fixed(ErrorId::E404, position);
                } else if let Some(clone) = clone {
                    decl.clones.push(clone);
                }
                continue;
            }
            seen_statement = true;
            self.parse_body_item(&mut decl.statements, &[]);
        }
    }

    fn parse_clone(&mut self) -> Option<CloneStatement> {
        let end = self.line_end();
        let at = self.located();
        self.bump(); // `&`
        let source = match self.peek().identifier_text() {
            Some(text) => text.to_string(),
            None => {
                self.unexpected("an instance id");
                self.finish_line(end);
                return None;
            }
        };
        if !is_identifier(&source) {
            let position = self.peek().position;
            self.error(
                ErrorId::E428,
                position,
                format!("Invalid instance id '{source}'; ids are identifiers (A-Z a-z 0-9 _ -)."),
            );
            self.finish_line(end);
            return None;
        }
        self.bump();
        let mut path = None;
        if self.index < end && self.peek().is_punctuation(".") {
            let start = self.index;
            self.bump();
            if self.index < end && self.peek().is_punctuation("*") {
                self.bump();
            } else {
                let at = self.located();
                let mut segments = Vec::new();
                if !self.take_segment(&mut segments, start, end) {
                    self.finish_line(end);
                    return None;
                }
                while self.index < end && self.peek().is_punctuation(".") {
                    self.bump();
                    if !self.take_segment(&mut segments, start, end) {
                        self.finish_line(end);
                        return None;
                    }
                }
                // A clone path that names an envelope key is E410, reported in
                // P4 with every other envelope assignment (SPEC §5.7).
                path = Some(Path::new(segments, at));
            }
        }
        if self.index < end && self.peek().is_punctuation("@") {
            // SPEC §5.13: a clone carries no annotation of its own.
            let position = self.peek().position;
            self.fixed(ErrorId::E437, position);
        } else if self.index < end {
            self.unexpected("end of line");
        }
        self.finish_line(end);
        Some(CloneStatement {
            source: normalise(&source),
            path,
            at,
        })
    }

    fn parse_body_item(&mut self, out: &mut Vec<BodyStatement>, prefix: &[String]) {
        let start = self.index;
        let end = self.line_end();
        let at = self.located();
        if self.report_double_colon(start, end) {
            self.finish_line(end);
            return;
        }
        // SPEC §2.1: a template declaration in a `.ab` file is E210, not the
        // invalid field name its colon-less line would otherwise produce.
        let Some(head) = (if self.report_template_declaration() {
            None
        } else {
            self.parse_head(start, end)
        }) else {
            let opens_block = end > start && self.token(end - 1).is_punctuation("{");
            self.finish_line(end);
            if opens_block {
                self.discard_block();
            }
            return;
        };
        // An annotation before the `:` or the `{` is out of position; SPEC
        // §5.13 allows one only at the end of a body statement.
        while self.index < end && self.peek().is_punctuation("@") {
            let position = self.peek().position;
            self.fixed(ErrorId::E437, position);
            if self.annotation_name().is_some() {
                let mut ignored = Window::UNANNOTATED;
                self.read_annotation(&mut ignored);
            } else {
                self.bump();
            }
        }
        match head {
            Head::Path(segments) => {
                if self.index < end && self.peek().is_punctuation("{") {
                    self.bump();
                    if self.index < end {
                        self.unexpected("end of line");
                    }
                    self.finish_line(end);
                    self.parse_block(out, prefix, segments, at);
                    return;
                }
                if !self.expect_colon(end) {
                    self.finish_line(end);
                    return;
                }
                let Some(path) = self.full_path(prefix, segments, &at) else {
                    self.finish_line(end);
                    return;
                };
                let (value, window) = self.parse_statement_value(end, None, &path);
                if let Some(value) = value {
                    out.push(BodyStatement {
                        path,
                        value,
                        window,
                        at,
                    });
                }
                self.finish_line(end);
            }
            Head::Multi(segments, keys) => {
                if !self.expect_colon(end) {
                    self.finish_line(end);
                    return;
                }
                let mut paths = Vec::new();
                for key in &keys {
                    let mut full = segments.clone();
                    full.push(key.clone());
                    if let Some(path) = self.full_path(prefix, full, &at) {
                        paths.push(path);
                    }
                }
                let first = paths.first().cloned();
                let sample = first.unwrap_or_else(|| Path::new(segments.clone(), at.clone()));
                let (value, window) = self.parse_statement_value(end, None, &sample);
                if let Some(value) = value {
                    for path in paths {
                        out.push(BodyStatement {
                            path,
                            value: value.clone(),
                            window,
                            at: at.clone(),
                        });
                    }
                }
                self.finish_line(end);
            }
            Head::Tuple(segments, columns) => {
                if !self.expect_colon(end) {
                    self.finish_line(end);
                    return;
                }
                let Some(path) = self.full_path(prefix, segments, &at) else {
                    self.finish_line(end);
                    return;
                };
                let (value, window) = self.parse_statement_value(end, Some(&columns), &path);
                if let Some(value) = value {
                    out.push(BodyStatement {
                        path,
                        value,
                        window,
                        at,
                    });
                }
                self.finish_line(end);
            }
        }
    }

    fn expect_colon(&mut self, end: usize) -> bool {
        if self.index < end && self.peek().is_punctuation(":") {
            self.bump();
            return true;
        }
        self.unexpected("':'");
        false
    }

    /// Prefixes a body block's path and checks the envelope keys and the depth
    /// limit (SPEC §5.4, §3.7).
    fn full_path(&mut self, prefix: &[String], own: Vec<String>, at: &Located) -> Option<Path> {
        let mut segments = Vec::with_capacity(prefix.len() + own.len());
        segments.extend_from_slice(prefix);
        segments.extend(own);
        let path = Path::new(segments, at.clone());
        if path.exceeds_segment_limit() {
            self.error(
                ErrorId::E209,
                at.position,
                format!("The number of path segments exceeds the limit of {PATH_SEGMENTS}."),
            );
            return None;
        }
        // A path that names an envelope key is E410, reported in P4 with every
        // other envelope assignment (SPEC §5.4, §7.3 step 1).
        Some(path)
    }

    fn parse_block(
        &mut self,
        out: &mut Vec<BodyStatement>,
        prefix: &[String],
        segments: Vec<String>,
        at: Located,
    ) {
        let mut inner: Vec<String> = Vec::with_capacity(prefix.len() + segments.len());
        inner.extend_from_slice(prefix);
        inner.extend(segments);
        if inner.len() >= PATH_SEGMENTS {
            self.error(
                ErrorId::E209,
                at.position,
                format!("The number of path segments exceeds the limit of {PATH_SEGMENTS}."),
            );
            self.discard_block();
            return;
        }
        loop {
            self.skip_newlines();
            if self.at_end() {
                // Unreachable on a stream the lexer accepted: an unclosed
                // block brace is E203 there.
                return;
            }
            if self.peek().is_punctuation("}") {
                self.bump();
                let end = self.line_end();
                if self.index < end {
                    self.unexpected("end of line");
                }
                self.finish_line(end);
                return;
            }
            if self.peek().is_punctuation("&") {
                let position = self.peek().position;
                self.fixed(ErrorId::E404, position);
                let end = self.line_end();
                self.finish_line(end);
                continue;
            }
            if self.at_instance_header() {
                self.unexpected("a body statement, a body block or '}'");
                let end = self.line_end();
                self.finish_line(end);
                continue;
            }
            self.parse_body_item(out, &inner);
        }
    }

    // ------------------------------------------------------------------ head

    fn parse_head(&mut self, start: usize, end: usize) -> Option<Head> {
        let mut segments = Vec::new();
        if !self.take_segment(&mut segments, start, end) {
            return None;
        }
        while self.index < end && self.peek().is_punctuation(".") {
            if self.ahead(1).is_punctuation("{") {
                let brace = self.ahead(1).position;
                self.bump(); // `.`
                self.bump(); // `{`
                let keys = self.parse_multi_keys(end, brace)?;
                return Some(Head::Multi(segments, keys));
            }
            self.bump();
            if !self.take_segment(&mut segments, start, end) {
                return None;
            }
        }
        if self.index < end {
            if self.peek().is_punctuation("(") {
                // SPEC §3.1: a tuple-array column list is one lexeme with the
                // path it follows, so whitespace before its '(' is E210.
                if !self.peek().glued {
                    let position = self.peek().position;
                    self.error(
                        ErrorId::E210,
                        position,
                        format!(
                            "Unexpected whitespace here; expected '(' immediately after '{}'.",
                            segments.join(".")
                        ),
                    );
                }
                self.bump();
                let columns = self.parse_columns(end)?;
                return Some(Head::Tuple(segments, columns));
            }
            // Anything else still glued to the path is part of a malformed
            // one: `a..b`, `name[0]` and `a.` are all E316 (SPEC §5.4).
            let follows = self.peek().is_punctuation(":")
                || self.peek().is_punctuation("{")
                || self.peek().is_punctuation("@");
            if !follows {
                self.invalid_path(start, end);
                return None;
            }
        }
        Some(Head::Path(segments))
    }

    /// One path segment. The lexer joins `1.5` into a single float lexeme
    /// (SPEC §3.1, longest match), so a segment token may spell two segments.
    fn take_segment(&mut self, segments: &mut Vec<String>, start: usize, end: usize) -> bool {
        if self.index >= end {
            self.invalid_path(start, end);
            return false;
        }
        let Some(text) = self.peek().identifier_text().map(str::to_string) else {
            self.invalid_path(start, end);
            return false;
        };
        for part in text.split('.') {
            if !is_identifier(part) {
                self.invalid_path(start, end);
                return false;
            }
            segments.push(normalise(part));
        }
        self.bump();
        if segments.len() > PATH_SEGMENTS {
            let position = self.token(start).position;
            self.error(
                ErrorId::E209,
                position,
                format!("The number of path segments exceeds the limit of {PATH_SEGMENTS}."),
            );
            return false;
        }
        true
    }

    /// A left side that is not a usable path.
    ///
    /// SPEC §5.4 makes the left side one lexical unit that must be a path, and
    /// E316's own condition is "invalid field name", so a left side built only
    /// from path material — names, `.`, `..` and the `[` `]` of an index — is
    /// E316 naming the whole text, at its first token. That covers SPEC §5.4's
    /// own `a..b`, `.a`, `a.` and `name[0]`.
    ///
    /// When the left side contains a token that can never be part of a path,
    /// the two candidates sit at different positions, and SPEC §11.1 decides:
    /// rule (3) prefers the earlier position, which is E316 at the start of
    /// the left side (`note #x`); when the offending token *is* the first one
    /// there is no field name to name and both candidates share a position,
    /// where rule (4) selects the lower identifier, E210 (`(a, a)`).
    fn invalid_path(&mut self, start: usize, end: usize) {
        if self.first_alien_token(start, end) != Some(start) {
            let text = self.head_text(start, end);
            let position = self.token(start).position;
            self.error(
                ErrorId::E316,
                position,
                format!("Invalid field name '{text}'."),
            );
            return;
        }
        let token = self.token(start);
        let found = token.describe();
        let position = token.position;
        self.error(
            ErrorId::E210,
            position,
            format!("Unexpected {found} here; expected a field name."),
        );
    }

    /// The first token of the head that could never occur in a path.
    fn first_alien_token(&self, start: usize, end: usize) -> Option<usize> {
        let mut index = start;
        while index < end {
            let token = self.token(index);
            if token.is_punctuation(":") {
                break;
            }
            let path_material = token.identifier_text().is_some()
                || token.is_punctuation(".")
                || token.is_punctuation("..")
                || token.is_punctuation("[")
                || token.is_punctuation("]");
            if !path_material {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn parse_multi_keys(&mut self, end: usize, brace: Position) -> Option<Vec<String>> {
        let mut keys = Vec::new();
        let mut saw_key = false;
        let mut after_comma = false;
        loop {
            if self.index >= end {
                self.unexpected("'}'");
                return None;
            }
            if self.peek().is_punctuation("}") {
                // GRAMMAR `multi_path_assignment` allows no trailing
                // separator: after a `,` the next token must be a key name.
                if after_comma {
                    self.unexpected("a key name");
                    return None;
                }
                self.bump();
                break;
            }
            let key_start = self.index;
            let Some(text) = self.peek().identifier_text().map(str::to_string) else {
                self.unexpected("a key name");
                return None;
            };
            saw_key = true;
            self.bump();
            if !is_identifier(&text) || (self.index < end && self.peek().is_punctuation(".")) {
                // SPEC §5.4: keys are single identifiers; a dotted key is E316.
                while self.index < end
                    && !self.peek().is_punctuation(",")
                    && !self.peek().is_punctuation("}")
                {
                    self.bump();
                }
                let spelled = self.render(key_start, self.index);
                let position = self.token(key_start).position;
                self.error(
                    ErrorId::E316,
                    position,
                    format!("Invalid field name '{spelled}'."),
                );
            } else {
                keys.push(normalise(&text));
            }
            if self.index < end && self.peek().is_punctuation(",") {
                self.bump();
                after_comma = true;
                continue;
            }
            if self.index < end && self.peek().is_punctuation("}") {
                self.bump();
                break;
            }
            self.unexpected("',' or '}'");
            return None;
        }
        if keys.is_empty() {
            if !saw_key {
                self.fixed(ErrorId::E433, brace);
            }
            return None;
        }
        Some(keys)
    }

    fn parse_columns(&mut self, end: usize) -> Option<Vec<String>> {
        let mut columns: Vec<String> = Vec::new();
        loop {
            if self.index >= end {
                self.unexpected("')'");
                return None;
            }
            if self.peek().is_punctuation(")") {
                self.bump();
                break;
            }
            let Some(text) = self.peek().identifier_text().map(str::to_string) else {
                self.unexpected("a column name");
                return None;
            };
            let position = self.peek().position;
            self.bump();
            if !is_identifier(&text) {
                self.error(
                    ErrorId::E316,
                    position,
                    format!("Invalid field name '{text}'."),
                );
            } else {
                let name = normalise(&text);
                if columns.contains(&name) {
                    self.error(
                        ErrorId::E417,
                        position,
                        format!("Duplicate tuple column '{name}'."),
                    );
                }
                // The column is kept either way, so that the row arity check
                // still compares against the number of columns written.
                columns.push(name);
            }
            if self.index < end && self.peek().is_punctuation(",") {
                self.bump();
                continue;
            }
            if self.index < end && self.peek().is_punctuation(")") {
                self.bump();
                break;
            }
            self.unexpected("',' or ')'");
            return None;
        }
        Some(columns)
    }

    // ---------------------------------------------------------------- values

    fn parse_statement_value(
        &mut self,
        end: usize,
        columns: Option<&[String]>,
        path: &Path,
    ) -> (Option<SyntaxValue>, Window) {
        let limit = self.annotation_start(end);
        let before = self.errors.len();
        let value = match columns {
            Some(columns) => self.parse_tuple_rows(limit, columns, path),
            None => self.parse_value(limit),
        };
        if self.index < limit {
            // Text the value did not consume is E210 — unless the value has
            // already been reported, in which case one diagnostic is enough.
            if self.errors.len() == before {
                self.unexpected("end of line");
            }
            self.goto(limit);
        }
        let mut window = Window::UNANNOTATED;
        while self.index < end && self.annotation_name().is_some() {
            self.read_annotation(&mut window);
        }
        if self.index < end {
            self.unexpected("end of line");
        }
        (value, window)
    }

    /// A `bare_list` or a `list_value` (SPEC §5.5).
    fn parse_value(&mut self, limit: usize) -> Option<SyntaxValue> {
        let before = self.errors.len();
        let mut items: Vec<(SyntaxValue, Position)> = Vec::new();
        let mut trailing: Option<Position> = None;
        loop {
            if self.index >= limit {
                break;
            }
            if self.peek().is_punctuation(",") {
                let position = self.peek().position;
                self.error(ErrorId::E441, position, "Empty list item.".to_string());
                self.bump();
                trailing = None;
                continue;
            }
            let position = self.peek().position;
            let Some(item) = self.parse_single_value(limit, 0) else {
                break;
            };
            items.push((item, position));
            trailing = None;
            if self.index < limit && self.peek().is_punctuation(",") {
                trailing = Some(self.peek().position);
                self.bump();
                continue;
            }
            break;
        }
        if let Some(position) = trailing {
            self.fixed(ErrorId::E442, position);
        }
        if items.is_empty() {
            if self.errors.len() == before {
                // SPEC §5.4: an assignment with nothing after the `:` is E210.
                self.unexpected("a value");
            }
            return None;
        }
        if items.len() > 1 {
            for (item, position) in &items {
                if matches!(item, SyntaxValue::List(_, _)) {
                    self.error(
                        ErrorId::E441,
                        *position,
                        "Nested lists are not supported.".to_string(),
                    );
                }
            }
        }
        if items.len() == 1 {
            return Some(items.remove(0).0);
        }
        // The bare spelling: a literal comma in one of these items would
        // have to be quoted, which is what the E412 note offers (SPEC §5.5).
        Some(SyntaxValue::List(
            items.into_iter().map(|(item, _)| item).collect(),
            ListSpelling::Commas,
        ))
    }

    fn parse_single_value(&mut self, limit: usize, depth: usize) -> Option<SyntaxValue> {
        if depth > BRACKET_DEPTH {
            let position = self.peek().position;
            self.error(
                ErrorId::E209,
                position,
                format!("Value nesting depth exceeds the limit of {BRACKET_DEPTH}."),
            );
            return None;
        }
        if self.index >= limit {
            self.unexpected("a value");
            return None;
        }
        match &self.peek().kind {
            TokenKind::QuotedString(text) => {
                let value = SyntaxValue::Quoted(text.clone());
                self.bump();
                Some(value)
            }
            TokenKind::BareText(text) => {
                let value = SyntaxValue::Bare(text.clone());
                self.bump();
                Some(value)
            }
            TokenKind::Punctuation("[") => self.parse_list(limit, depth),
            TokenKind::Punctuation("#") => self.parse_tag(limit, depth),
            _ => {
                self.unexpected("a value");
                None
            }
        }
    }

    fn parse_list(&mut self, limit: usize, depth: usize) -> Option<SyntaxValue> {
        self.bump(); // `[`
        let mut items = Vec::new();
        loop {
            if self.index >= limit {
                self.unexpected("']'");
                break;
            }
            if self.peek().is_punctuation("]") {
                self.bump();
                break;
            }
            if self.peek().is_punctuation(",") {
                let position = self.peek().position;
                self.error(ErrorId::E441, position, "Empty list item.".to_string());
                self.bump();
                continue;
            }
            let position = self.peek().position;
            let nested = self.peek().is_punctuation("[");
            let Some(item) = self.parse_single_value(limit, depth + 1) else {
                break;
            };
            if nested {
                self.error(
                    ErrorId::E441,
                    position,
                    "Nested lists are not supported.".to_string(),
                );
            }
            items.push(item);
            if self.index < limit && self.peek().is_punctuation(",") {
                self.bump();
                continue;
            }
            if self.index < limit && self.peek().is_punctuation("]") {
                self.bump();
                break;
            }
            self.unexpected("',' or ']'");
            break;
        }
        Some(SyntaxValue::List(items, ListSpelling::Brackets))
    }

    fn parse_tag(&mut self, limit: usize, depth: usize) -> Option<SyntaxValue> {
        self.bump(); // `#`
        let Some(spelled) = self.peek().identifier_text().map(str::to_string) else {
            self.unexpected("a tag name");
            return None;
        };
        // A tag name may end with the `*` of a prefix wildcard, which
        // replicates the whole object once per matching member (SPEC §5.6).
        let stem = spelled.strip_suffix('*').unwrap_or(&spelled);
        if !is_identifier(stem) {
            self.unexpected("a tag name");
            self.bump();
            return None;
        }
        self.bump();
        let name = match spelled.strip_suffix('*') {
            Some(prefix) => format!("{}*", normalise(prefix)),
            None => normalise(&spelled),
        };
        let mut args = Vec::new();
        if self.index < limit && self.peek().is_punctuation("(") && self.peek().glued {
            self.bump();
            loop {
                if self.index >= limit {
                    self.unexpected("')'");
                    break;
                }
                if self.peek().is_punctuation(")") {
                    self.bump();
                    break;
                }
                if self.peek().is_punctuation(",") {
                    let position = self.peek().position;
                    self.error(
                        ErrorId::E419,
                        position,
                        format!(
                            "Invalid argument '' in '#{name}(...)'; arguments are 'key: value' or a bare boolean flag."
                        ),
                    );
                    self.bump();
                    continue;
                }
                match self.parse_tag_argument(limit, depth, &name) {
                    Some(argument) => args.push(argument),
                    None => {
                        self.close_tag_arguments(limit);
                        break;
                    }
                }
                if self.index < limit && self.peek().is_punctuation(",") {
                    self.bump();
                    continue;
                }
                if self.index < limit && self.peek().is_punctuation(")") {
                    self.bump();
                    break;
                }
                self.unexpected("',' or ')'");
                break;
            }
        }
        Some(SyntaxValue::Tag { name, args })
    }

    fn parse_tag_argument(&mut self, limit: usize, depth: usize, tag: &str) -> Option<TagArg> {
        let start = self.index;
        let at = self.located();
        if let TokenKind::BareText(text) = &self.peek().kind {
            // The lexer hands an argument that is neither `key: value` nor a
            // bare identifier over as one bare run (SPEC §5.5).
            let text = text.clone();
            let position = self.peek().position;
            self.error(
                ErrorId::E419,
                position,
                format!(
                    "Invalid argument '{text}' in '#{tag}(...)'; arguments are 'key: value' or a bare boolean flag."
                ),
            );
            self.bump();
            return None;
        }
        let mut segments = Vec::new();
        while let Some(text) = self.peek().identifier_text().map(str::to_string) {
            if !is_identifier(&text) {
                break;
            }
            segments.push(normalise(&text));
            self.bump();
            if self.index < limit && self.peek().is_punctuation(".") {
                self.bump();
                continue;
            }
            break;
        }
        if segments.is_empty() {
            self.invalid_argument(start, limit, tag);
            return None;
        }
        if self.index < limit && self.peek().is_punctuation(":") {
            self.bump();
            let value = self.parse_single_value(limit, depth + 1)?;
            return Some(TagArg {
                path: Path::new(segments, at),
                value,
                flag: false,
            });
        }
        let closed = self.index >= limit
            || self.peek().is_punctuation(",")
            || self.peek().is_punctuation(")");
        if !closed || segments.len() != 1 {
            self.invalid_argument(start, limit, tag);
            return None;
        }
        Some(TagArg {
            path: Path::new(segments, at),
            value: SyntaxValue::Bare("true".to_string()),
            flag: true,
        })
    }

    /// Skips to the `)` that closes an argument list whose argument was
    /// rejected, so that one malformed argument reports one diagnostic.
    fn close_tag_arguments(&mut self, limit: usize) {
        let mut open = 0usize;
        while self.index < limit {
            if self.peek().is_punctuation("(") {
                open += 1;
            } else if self.peek().is_punctuation(")") {
                if open == 0 {
                    self.bump();
                    return;
                }
                open -= 1;
            }
            self.bump();
        }
    }

    fn invalid_argument(&mut self, start: usize, limit: usize, tag: &str) {
        while self.index < limit
            && !self.peek().is_punctuation(",")
            && !self.peek().is_punctuation(")")
        {
            self.bump();
        }
        let text = self.render(start, self.index);
        let position = self.token(start).position;
        self.error(
            ErrorId::E419,
            position,
            format!(
                "Invalid argument '{text}' in '#{tag}(...)'; arguments are 'key: value' or a bare boolean flag."
            ),
        );
    }

    /// Tuple rows, bracketed or not (SPEC §5.5). The result is always a list:
    /// a tuple array names a list field.
    fn parse_tuple_rows(
        &mut self,
        limit: usize,
        columns: &[String],
        path: &Path,
    ) -> Option<SyntaxValue> {
        let field = path.text();
        let head = self.peek().position;
        let bracketed = self.index < limit && self.peek().is_punctuation("[");
        if bracketed {
            self.bump();
        }
        let mut rows: Vec<SyntaxValue> = Vec::new();
        let mut trailing: Option<Position> = None;
        loop {
            if self.index >= limit {
                if bracketed {
                    self.unexpected("']'");
                }
                break;
            }
            if bracketed && self.peek().is_punctuation("]") {
                self.bump();
                trailing = None;
                break;
            }
            if !self.peek().is_punctuation("(") {
                self.unexpected("'('");
                break;
            }
            let number = rows.len() + 1;
            let row = self.parse_tuple_row(limit, columns, &field, number);
            rows.push(row);
            trailing = None;
            if self.index < limit && self.peek().is_punctuation(",") {
                trailing = Some(self.peek().position);
                self.bump();
                continue;
            }
            if bracketed && self.index < limit && self.peek().is_punctuation("]") {
                self.bump();
                break;
            }
            if self.index >= limit {
                break;
            }
            self.unexpected(if bracketed { "',' or ']'" } else { "','" });
            break;
        }
        if let Some(position) = trailing {
            self.fixed(ErrorId::E442, position);
        }
        if columns.is_empty() {
            // SPEC §5.5: a tuple array declares at least one column.
            let found = match rows.first() {
                Some(SyntaxValue::Object(fields)) => fields.len(),
                _ => 0,
            };
            self.errors.push(
                Diagnostic::at(
                    ErrorId::E416,
                    self.file,
                    head,
                    format!("Tuple row 1 has {found} values but {field} declares 0 columns ()."),
                )
                .with_note_text("a tuple array declares at least one column."),
            );
            return None;
        }
        if rows.is_empty() {
            self.errors.push(Diagnostic::at(
                ErrorId::E210,
                self.file,
                head,
                "Unexpected end of value here; expected '('.".to_string(),
            ));
            return None;
        }
        Some(SyntaxValue::List(rows, ListSpelling::Brackets))
    }

    fn parse_tuple_row(
        &mut self,
        limit: usize,
        columns: &[String],
        field: &str,
        number: usize,
    ) -> SyntaxValue {
        let position = self.peek().position;
        self.bump(); // `(`
        let mut cells: Vec<SyntaxValue> = Vec::new();
        let mut malformed = false;
        loop {
            if self.index >= limit {
                self.unexpected("')'");
                malformed = true;
                break;
            }
            if self.peek().is_punctuation(")") {
                self.bump();
                break;
            }
            if self.peek().is_punctuation(",") {
                self.unexpected("a quoted string or bare text");
                malformed = true;
                self.bump();
                continue;
            }
            match &self.peek().kind {
                TokenKind::QuotedString(text) => {
                    cells.push(SyntaxValue::Quoted(text.clone()));
                    self.bump();
                }
                TokenKind::BareText(text) => {
                    cells.push(SyntaxValue::Bare(text.clone()));
                    self.bump();
                }
                _ => {
                    self.unexpected("a quoted string or bare text");
                    malformed = true;
                    self.bump();
                    continue;
                }
            }
            if self.index < limit && self.peek().is_punctuation(",") {
                self.bump();
                continue;
            }
            if self.index < limit && self.peek().is_punctuation(")") {
                self.bump();
                break;
            }
            self.unexpected("',' or ')'");
            malformed = true;
            break;
        }
        if !malformed && !columns.is_empty() && cells.len() != columns.len() {
            let expected = columns.len();
            let found = cells.len();
            let list = columns.join(", ");
            self.error(
                ErrorId::E416,
                position,
                format!(
                    "Tuple row {number} has {found} values but {field} declares {expected} columns ({list})."
                ),
            );
        }
        let fields = columns
            .iter()
            .cloned()
            .zip(cells)
            .collect::<Vec<(String, SyntaxValue)>>();
        SyntaxValue::Object(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn unit(path: &str, text: &str) -> Result<InstanceFile, Diagnostics> {
        let source = SourceFile::new(path, text);
        let tokens = crate::lexer::tokenize(&source).expect("the lexer accepts this source");
        parse_instances(&source.path, &tokens)
    }

    fn parsed(text: &str) -> InstanceFile {
        unit("data/a.ab", text).expect("the parser accepts this source")
    }

    fn errors(text: &str) -> Vec<(ErrorId, String)> {
        let reported = unit("data/a.ab", text).expect_err("the parser rejects this source");
        reported
            .iter()
            .map(|item| (item.id, item.message.clone()))
            .collect()
    }

    fn ids(text: &str) -> Vec<ErrorId> {
        errors(text).into_iter().map(|(id, _)| id).collect()
    }

    fn bare(text: &str) -> SyntaxValue {
        SyntaxValue::Bare(text.to_string())
    }

    #[test]
    fn a_header_carries_tags_and_a_body() {
        let unit = parsed("Product :: @id.atlas, @status.active\n    name: Atlas Search\n");
        assert_eq!(unit.instances.len(), 1);
        let instance = &unit.instances[0];
        assert_eq!(instance.template, "Product");
        assert_eq!(instance.id, "atlas");
        assert_eq!(instance.tags.len(), 2);
        assert_eq!(instance.tags[1].name, "status");
        assert_eq!(instance.tags[1].value, bare("active"));
        assert!(!instance.tags[1].flag);
        assert_eq!(instance.statements.len(), 1);
        assert_eq!(instance.statements[0].path.text(), "name");
        assert_eq!(instance.statements[0].value, bare("Atlas Search"));
        assert_eq!(instance.window, Window::UNANNOTATED);
    }

    #[test]
    fn a_bare_header_tag_is_the_flag_form() {
        let unit = parsed("Product :: @id.atlas, @featured\n");
        let tag = &unit.instances[0].tags[1];
        assert!(tag.flag);
        assert_eq!(tag.value, bare("true"));
    }

    #[test]
    fn the_id_falls_back_to_the_file_stem_and_is_normalised() {
        let unit = unit("data/Winter-Pack.ab", "Product ::\n").expect("parses");
        assert_eq!(unit.instances[0].id, "winter_pack");
        assert_eq!(unit.instances[0].id_at, unit.instances[0].at);
    }

    #[test]
    fn an_id_tag_is_normalised_and_never_type_inferred() {
        assert_eq!(parsed("P :: @id.42\n").instances[0].id, "42");
        assert_eq!(
            parsed("P :: @id.Winter-Pack\n").instances[0].id,
            "winter_pack"
        );
    }

    #[test]
    fn a_malformed_id_is_reported() {
        // E428 is a P2 diagnostic (SPEC §5.3, §7.2): the parser records the
        // text it could not use and P1 still succeeds, so that a duplicate id
        // in one file and an unusable id in another are reported together.
        let invalid = |text: &str| parsed(text).instances[0].invalid_id.clone();
        assert_eq!(invalid("P :: @id.\n"), Some(String::new()));
        assert_eq!(invalid("P :: @id\n"), Some("@id".to_string()));
        assert_eq!(invalid("P :: @id.a b\n"), Some("a b".to_string()));
        assert_eq!(invalid("P :: @id.torch\n"), None);
        assert_eq!(
            unit("data/frost.v2.ab", "P ::\n")
                .expect("the stem is not an identifier, which is P2's to report")
                .instances[0]
                .invalid_id,
            Some("frost.v2".to_string())
        );
    }

    #[test]
    fn a_header_window_is_read_and_a_tag_after_it_is_rejected() {
        let unit = parsed("Item :: @id.lantern @since(2) @removed(4)\n");
        assert_eq!(
            unit.instances[0].window,
            Window {
                since: Some(2),
                removed: Some(4)
            }
        );
        assert_eq!(ids("Item :: @since(2) @status.x\n"), [ErrorId::E437]);
        assert_eq!(ids("Item :: @since(2) @since(3)\n"), [ErrorId::E303]);
    }

    #[test]
    fn a_dotted_since_is_a_header_tag_not_a_window() {
        let unit = parsed("Item :: @id.x, @since.2\n");
        assert_eq!(unit.instances[0].window, Window::UNANNOTATED);
        assert_eq!(unit.instances[0].tags[1].name, "since");
        assert_eq!(unit.instances[0].tags[1].value, bare("2"));
    }

    #[test]
    fn a_header_tag_value_ends_at_the_next_tag_and_quotes_protect_separators() {
        let unit = parsed("P :: @label.\"hi, there @world\", @icon../textures/a.png\n");
        assert_eq!(
            unit.instances[0].tags[0].value,
            SyntaxValue::Quoted("hi, there @world".to_string())
        );
        assert_eq!(unit.instances[0].tags[1].value, bare("./textures/a.png"));
    }

    #[test]
    fn a_trailing_comma_in_a_header_is_rejected_and_a_continued_one_is_not() {
        assert_eq!(ids("P :: @a,\n"), [ErrorId::E442]);
        assert_eq!(ids("P :: @a, @since(2)\n"), [ErrorId::E442]);
        let unit = parsed("P :: @a,\n     @b\n");
        assert_eq!(unit.instances[0].tags.len(), 2);
    }

    #[test]
    fn a_second_double_colon_in_a_header_is_reported() {
        assert_eq!(ids("Thing :: @a :: @b\n"), [ErrorId::E436]);
        assert_eq!(ids("A :: @id.x :: y\n"), [ErrorId::E436]);
    }

    #[test]
    fn a_double_colon_outside_a_header_names_the_text_before_it() {
        assert_eq!(
            errors("my thing :: @id.x\n"),
            [(
                ErrorId::E439,
                "'::' starts an instance header, but 'my thing' is not a schema name.".to_string()
            )]
        );
    }

    #[test]
    fn a_statement_before_the_first_header_is_reported_once() {
        assert_eq!(ids("name: Atlas\nowner: x\n"), [ErrorId::E403]);
        assert_eq!(ids("&base.*\nP :: @id.x\n"), [ErrorId::E403]);
    }

    #[test]
    fn a_double_colon_in_a_body_line_is_reported_where_it_stands() {
        assert_eq!(
            errors("P ::\n    my thing :: x\n"),
            [(
                ErrorId::E439,
                "'::' starts an instance header, but 'my thing' is not a schema name.".to_string()
            )]
        );
    }

    #[test]
    fn a_header_tag_names_one_root_field_and_splits_at_the_first_dot() {
        // SPEC §5.2: `@owner.team` assigns the root field `owner` the text
        // `team`; reaching a nested field is E438, decided against the schema.
        let unit = parsed("P :: @owner.team\n");
        assert_eq!(unit.instances[0].tags[0].name, "owner");
        assert_eq!(unit.instances[0].tags[0].value, bare("team"));
    }

    #[test]
    fn a_header_tag_list_continues_over_a_comment_and_a_line_break() {
        let unit = parsed("P :: @a, // note\n     @b\n");
        assert_eq!(unit.instances[0].tags.len(), 2);
    }

    #[test]
    fn a_list_spread_over_several_lines_is_one_value() {
        let unit = parsed("P ::\n    tags: [\n        core,\n        public\n    ]\n");
        assert_eq!(
            unit.instances[0].statements[0].value,
            SyntaxValue::List(vec![bare("core"), bare("public")], ListSpelling::Brackets)
        );
    }

    #[test]
    fn a_value_may_contain_a_double_colon() {
        let unit = parsed("A ::\n    window: 12::30\n");
        assert_eq!(unit.instances[0].statements[0].value, bare("12::30"));
    }

    #[test]
    fn clones_are_read_before_the_first_statement_and_nowhere_else() {
        let unit = parsed("P ::\n&base_product.*\n&Other.deep.path\n&plain\n    name: x\n");
        let clones = &unit.instances[0].clones;
        assert_eq!(clones.len(), 3);
        assert_eq!(clones[0].source, "base_product");
        assert!(clones[0].path.is_none());
        assert_eq!(clones[1].source, "other");
        assert_eq!(
            clones[1].path.as_ref().map(Path::text),
            Some("deep.path".to_string())
        );
        assert!(clones[2].path.is_none());
        assert_eq!(ids("P ::\n    name: x\n&base.*\n"), [ErrorId::E404]);
        assert_eq!(ids("P ::\n    owner {\n&base.*\n    }\n"), [ErrorId::E404]);
    }

    #[test]
    fn a_clone_carries_no_annotation_and_never_names_an_envelope_key() {
        assert_eq!(ids("P ::\n&base.* @since(2)\n"), [ErrorId::E437]);
        // E410 on a clone path is P4's to report (SPEC §7.3 step 1); the
        // parser keeps the path as written.
        let clone_path = |text: &str| {
            parsed(text).instances[0].clones[0]
                .path
                .as_ref()
                .map(Path::text)
        };
        assert_eq!(clone_path("P ::\n&base.id\n"), Some("id".to_string()));
        assert_eq!(
            clone_path("P ::\n&base.template\n"),
            Some("template".to_string())
        );
    }

    #[test]
    fn dotted_paths_and_body_blocks_produce_the_same_statements() {
        let dotted = parsed("P ::\n    owner.team: Knowledge\n    owner.contact: a@b\n");
        let block =
            parsed("P ::\n    owner {\n        team: Knowledge\n        contact: a@b\n    }\n");
        let paths = |unit: &InstanceFile| -> Vec<String> {
            unit.instances[0]
                .statements
                .iter()
                .map(|statement| statement.path.text())
                .collect()
        };
        assert_eq!(paths(&dotted), ["owner.team", "owner.contact"]);
        assert_eq!(paths(&block), paths(&dotted));
        assert_eq!(
            block.instances[0].statements[0].value,
            dotted.instances[0].statements[0].value
        );
    }

    #[test]
    fn blocks_nest_and_carry_no_annotation() {
        let unit = parsed("P ::\n    a {\n        b {\n            c: 1\n        }\n    }\n");
        assert_eq!(unit.instances[0].statements[0].path.text(), "a.b.c");
        assert_eq!(
            ids("P ::\n    a @since(2) {\n        b: 1\n    }\n"),
            [ErrorId::E437]
        );
    }

    #[test]
    fn an_empty_block_writes_nothing() {
        let unit = parsed("P ::\n    owner {\n    }\n    name: x\n");
        assert_eq!(unit.instances[0].statements.len(), 1);
        assert_eq!(unit.instances[0].statements[0].path.text(), "name");
    }

    #[test]
    fn an_instance_header_inside_a_block_is_rejected() {
        assert_eq!(ids("P ::\n    a {\nQ :: @id.q\n    }\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_multi_path_assigns_every_key_the_same_value() {
        let unit = parsed("P ::\n    limits.tier.{soft, hard}: 10\n");
        let statements = &unit.instances[0].statements;
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].path.text(), "limits.tier.soft");
        assert_eq!(statements[1].path.text(), "limits.tier.hard");
        assert_eq!(statements[1].value, bare("10"));
        assert_eq!(ids("P ::\n    a.{}: 1\n"), [ErrorId::E433]);
        assert_eq!(ids("P ::\n    a.{b.c}: 1\n"), [ErrorId::E316]);
    }

    #[test]
    fn invalid_paths_are_reported_with_the_text_the_author_wrote() {
        assert_eq!(
            errors("P ::\n    a..b: 1\n"),
            [(ErrorId::E316, "Invalid field name 'a..b'.".to_string())]
        );
        assert_eq!(
            errors("P ::\n    name[0]: 1\n"),
            [(ErrorId::E316, "Invalid field name 'name[0]'.".to_string())]
        );
        assert_eq!(
            errors("P ::\n    .a: 1\n"),
            [(ErrorId::E316, "Invalid field name '.a'.".to_string())]
        );
        assert_eq!(
            errors("P ::\n    a.: 1\n"),
            [(ErrorId::E316, "Invalid field name 'a.'.".to_string())]
        );
    }

    #[test]
    fn a_numeric_segment_run_is_split_back_into_segments() {
        let unit = parsed("P ::\n    a.1.5: x\n");
        assert_eq!(unit.instances[0].statements[0].path.text(), "a.1.5");
    }

    #[test]
    fn envelope_keys_can_never_be_assigned() {
        // The parser keeps these as written; E410 is P4's (SPEC §7.3 step 1).
        let path = |text: &str| parsed(text).instances[0].statements[0].path.text();
        assert_eq!(path("P ::\n    id: other\n"), "id");
        assert_eq!(path("P ::\n    template.x: other\n"), "template.x");
        assert_eq!(
            parsed("P :: @template.Other\n").instances[0].tags[0].name,
            "template"
        );
        assert_eq!(path("P ::\n    owner.id: keep\n"), "owner.id");
    }

    #[test]
    fn lists_are_written_with_brackets_or_with_commas() {
        let bracketed = parsed("P ::\n    tags: [core, public]\n");
        let bare_list = parsed("P ::\n    tags: core, public\n");
        // The two spellings mean the same list and differ only in what the
        // AST records, which E412's note reads (SPEC §5.5).
        let items = vec![bare("core"), bare("public")];
        assert_eq!(
            bracketed.instances[0].statements[0].value,
            SyntaxValue::List(items.clone(), ListSpelling::Brackets)
        );
        assert_eq!(
            bare_list.instances[0].statements[0].value,
            SyntaxValue::List(items, ListSpelling::Commas)
        );
        let single = parsed("P ::\n    tags: core\n");
        assert_eq!(single.instances[0].statements[0].value, bare("core"));
        let empty = parsed("P ::\n    tags: []\n");
        assert_eq!(
            empty.instances[0].statements[0].value,
            SyntaxValue::List(Vec::new(), ListSpelling::Brackets)
        );
        let one = parsed("P ::\n    tags: [core]\n");
        assert_eq!(
            one.instances[0].statements[0].value,
            SyntaxValue::List(vec![bare("core")], ListSpelling::Brackets)
        );
    }

    #[test]
    fn a_trailing_comma_is_allowed_inside_brackets_only() {
        let unit = parsed("P ::\n    tags: [core, public,]\n");
        assert_eq!(
            unit.instances[0].statements[0].value,
            SyntaxValue::List(vec![bare("core"), bare("public")], ListSpelling::Brackets)
        );
        assert_eq!(ids("P ::\n    tags: core, public,\n"), [ErrorId::E442]);
    }

    #[test]
    fn empty_and_nested_list_items_are_rejected() {
        assert_eq!(
            errors("P ::\n    tags: a,,b\n"),
            [(ErrorId::E441, "Empty list item.".to_string())]
        );
        assert_eq!(
            errors("P ::\n    tags: [[a]]\n"),
            [(ErrorId::E441, "Nested lists are not supported.".to_string())]
        );
        assert_eq!(ids("P ::\n    tags: a, [b]\n"), [ErrorId::E441]);
    }

    #[test]
    fn an_empty_right_hand_side_is_rejected() {
        assert_eq!(
            errors("P ::\n    name:\n"),
            [(
                ErrorId::E210,
                "Unexpected end of line here; expected a value.".to_string()
            )]
        );
        assert_eq!(ids("P ::\n    name: @since(2)\n"), [ErrorId::E210]);
    }

    #[test]
    fn text_after_a_quoted_value_is_rejected() {
        assert_eq!(ids("P ::\n    x: \"q\" trailing\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_tag_object_carries_nested_paths_lists_and_tags() {
        let unit = parsed("P ::\n    caps: [#search(limits.soft: 10, flag, inner: #b), #x]\n");
        let SyntaxValue::List(items, _) = &unit.instances[0].statements[0].value else {
            panic!("a bracketed value is a list");
        };
        let SyntaxValue::Tag { name, args } = &items[0] else {
            panic!("the first element is a tag object");
        };
        assert_eq!(name, "search");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].path.text(), "limits.soft");
        assert_eq!(args[0].value, bare("10"));
        assert!(!args[0].flag);
        assert_eq!(args[1].path.text(), "flag");
        assert!(args[1].flag);
        assert_eq!(args[1].value, bare("true"));
        assert_eq!(
            args[2].value,
            SyntaxValue::Tag {
                name: "b".to_string(),
                args: Vec::new()
            }
        );
        assert_eq!(
            items[1],
            SyntaxValue::Tag {
                name: "x".to_string(),
                args: Vec::new()
            }
        );
    }

    #[test]
    fn a_tag_argument_list_holds_only_named_arguments_and_flags() {
        assert_eq!(
            errors("P ::\n    t: #a(: 1)\n"),
            [(
                ErrorId::E419,
                "Invalid argument ': 1' in '#a(...)'; arguments are 'key: value' or a bare boolean flag."
                    .to_string()
            )]
        );
        assert_eq!(ids("P ::\n    t: #a(x.y)\n"), [ErrorId::E419]);
        assert_eq!(ids("P ::\n    t: #a(x,,y)\n"), [ErrorId::E419]);
    }

    #[test]
    fn text_after_a_tag_object_is_rejected() {
        // SPEC §5.5: nothing after the closing ')' is silently discarded.
        assert_eq!(ids("P ::\n    t: #a(x: 1) trailing\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_path_longer_than_the_limit_is_reported_rather_than_built() {
        let dotted: Vec<String> = (0..=PATH_SEGMENTS)
            .map(|index| format!("s{index}"))
            .collect();
        let source = format!("P ::\n    {}: 1\n", dotted.join("."));
        assert_eq!(ids(&source), [ErrorId::E209]);
        let nested = format!(
            "P ::\n{}{}",
            "    a {\n".repeat(PATH_SEGMENTS),
            "    }\n".repeat(PATH_SEGMENTS)
        );
        assert_eq!(ids(&nested), [ErrorId::E209]);
    }

    #[test]
    fn a_bracketed_list_inside_a_tag_argument_is_a_list() {
        let unit = parsed("P ::\n    t: #a(x: [1, 2])\n");
        let SyntaxValue::Tag { args, .. } = &unit.instances[0].statements[0].value else {
            panic!("a leading '#' starts a tag object");
        };
        assert_eq!(
            args[0].value,
            SyntaxValue::List(vec![bare("1"), bare("2")], ListSpelling::Brackets)
        );
    }

    #[test]
    fn tuple_rows_become_objects_keyed_by_the_columns() {
        let unit = parsed("P ::\n    copy(key, value): (en_us, Welcome), (es_es, Bienvenido)\n");
        let statement = &unit.instances[0].statements[0];
        assert_eq!(statement.path.text(), "copy");
        assert_eq!(
            statement.value,
            SyntaxValue::List(
                vec![
                    SyntaxValue::Object(vec![
                        ("key".to_string(), bare("en_us")),
                        ("value".to_string(), bare("Welcome")),
                    ]),
                    SyntaxValue::Object(vec![
                        ("key".to_string(), bare("es_es")),
                        ("value".to_string(), bare("Bienvenido")),
                    ]),
                ],
                ListSpelling::Brackets
            )
        );
    }

    #[test]
    fn bracketed_tuple_rows_span_lines_and_allow_a_trailing_comma() {
        let unit =
            parsed("P ::\n    copy(key, value): [\n        (a, b),\n        (c, d),\n    ]\n");
        let SyntaxValue::List(rows, _) = &unit.instances[0].statements[0].value else {
            panic!("a tuple array is a list");
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn tuple_arity_columns_and_cells_are_checked() {
        assert_eq!(
            errors("P ::\n    copy(key, value): (a, b, c)\n"),
            [(
                ErrorId::E416,
                "Tuple row 1 has 3 values but copy declares 2 columns (key, value).".to_string()
            )]
        );
        assert_eq!(
            errors("P ::\n    copy(key, key): (a, b)\n"),
            [(ErrorId::E417, "Duplicate tuple column 'key'.".to_string())]
        );
        let empty = errors("P ::\n    caps(): ()\n");
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].0, ErrorId::E416);
        // Anything between two rows other than one comma is E210 (SPEC §5.5).
        assert_eq!(ids("P ::\n    copy(key): (a) (b)\n"), [ErrorId::E210]);
    }

    #[test]
    fn statement_annotations_are_stripped_from_the_value() {
        let unit = parsed("P ::\n    note: deprecated @removed(3)\n    glow: true @since(2)\n");
        let statements = &unit.instances[0].statements;
        assert_eq!(statements[0].value, bare("deprecated"));
        assert_eq!(
            statements[0].window,
            Window {
                since: None,
                removed: Some(3)
            }
        );
        assert_eq!(
            statements[1].window,
            Window {
                since: Some(2),
                removed: None
            }
        );
    }

    #[test]
    fn an_at_sign_glued_to_a_value_is_not_an_annotation() {
        let unit = parsed("P ::\n    note: x@since(2)\n");
        assert_eq!(unit.instances[0].statements[0].value, bare("x@since(2)"));
        assert_eq!(unit.instances[0].statements[0].window, Window::UNANNOTATED);
    }

    #[test]
    fn a_repeated_statement_annotation_is_reported() {
        assert_eq!(ids("P ::\n    x: 1 @since(2) @since(3)\n"), [ErrorId::E303]);
    }

    #[test]
    fn a_brace_file_pattern_is_kept_as_written() {
        let unit = parsed("P ::\n    icon: ./textures/{hero}.png\n");
        assert_eq!(
            unit.instances[0].statements[0].value,
            bare("./textures/{hero}.png")
        );
    }

    #[test]
    fn several_instances_live_in_one_file() {
        let unit = parsed("Item :: @id.lantern @since(2)\n    name: Lantern\n\nItem :: @id.candle @removed(3)\n    name: Candle\n");
        assert_eq!(unit.instances.len(), 2);
        assert_eq!(unit.instances[0].id, "lantern");
        assert_eq!(unit.instances[1].id, "candle");
        assert_eq!(unit.instances[1].statements[0].path.text(), "name");
    }

    #[test]
    fn an_empty_file_parses_to_no_instances() {
        assert!(parsed("").instances.is_empty());
        assert!(parsed("\n\n// only a comment\n").instances.is_empty());
    }

    #[test]
    fn duplicate_assignments_compare_annotation_windows() {
        let project = VersionRange::new(1, 3).expect("a valid range");
        let unit = parsed("P :: @id.x, @name.a\n    name: b\n");
        let reported = duplicate_assignments(&unit.instances[0], project);
        assert_eq!(reported.len(), 1);
        assert_eq!(reported.first().map(|item| item.id), Some(ErrorId::E429));
        assert_eq!(
            reported.first().map(|item| item.message.clone()),
            Some("name is assigned twice for version(s) 1..3.".to_string())
        );

        let disjoint = parsed("P ::\n    glow: a @removed(2)\n    glow: b @since(2)\n");
        assert!(duplicate_assignments(&disjoint.instances[0], project).is_empty());

        let overlapping = parsed("P ::\n    glow: a @since(2)\n    glow: b @removed(3)\n");
        let reported = duplicate_assignments(&overlapping.instances[0], project);
        assert_eq!(
            reported.first().map(|item| item.message.clone()),
            Some("glow is assigned twice for version(s) 2.".to_string())
        );

        let multi = parsed("P ::\n    a.{b, c}: 1\n    a.b: 2\n");
        assert_eq!(duplicate_assignments(&multi.instances[0], project).len(), 1);

        let scoped = parsed("P :: @removed(2)\n    x: 1 @since(2)\n    x: 2 @since(2)\n");
        assert!(duplicate_assignments(&scoped.instances[0], project).is_empty());
    }

    #[test]
    fn a_clone_and_a_statement_writing_one_path_are_not_a_duplicate() {
        let project = VersionRange::new(1, 1).expect("a valid range");
        let unit = parsed("P ::\n&base.*\n    name: x\n");
        assert!(duplicate_assignments(&unit.instances[0], project).is_empty());
    }

    #[test]
    fn the_file_stem_drops_one_extension_case_insensitively() {
        assert_eq!(file_stem("data/items/Torch.AB"), "Torch");
        assert_eq!(file_stem("data/Item.abt"), "Item");
        assert_eq!(file_stem("torch.ab"), "torch");
        assert_eq!(file_stem("data\\torch.ab"), "torch");
        assert_eq!(file_stem(".ab"), ".ab");
    }

    #[test]
    fn syntax_values_report_the_shape_a_type_error_names() {
        assert_eq!(bare("x").shape(), "text");
        assert_eq!(SyntaxValue::Quoted("x".to_string()).shape(), "text");
        assert_eq!(
            SyntaxValue::List(Vec::new(), ListSpelling::Brackets).shape(),
            "list"
        );
        assert_eq!(
            SyntaxValue::Tag {
                name: "x".to_string(),
                args: Vec::new()
            }
            .shape(),
            "tag object"
        );
        assert_eq!(SyntaxValue::Object(Vec::new()).shape(), "object");
    }

    /// SPEC §3.7: no input aborts or exhausts the stack. These shapes exercise
    /// the recovery paths and the depth guards.
    #[test]
    fn hostile_shapes_end_in_diagnostics_rather_than_a_panic() {
        let deep_blocks = format!("P ::\n{}{}", "    a {\n".repeat(200), "    }\n".repeat(200));
        let cases = [
            "P ::".to_string(),
            "P :: @".to_string(),
            "P :: @a.".to_string(),
            "P :: ,".to_string(),
            "P ::\n&".to_string(),
            "P ::\n&.".to_string(),
            "P ::\n    a".to_string(),
            "P ::\n    a.".to_string(),
            "P ::\n    a.{".to_string(),
            "P ::\n    a(".to_string(),
            "P ::\n    a(): ".to_string(),
            "P ::\n    a: #".to_string(),
            "P ::\n    a: #b(".to_string(),
            "P ::\n    a: [".to_string(),
            "P ::\n    }".to_string(),
            "::".to_string(),
            deep_blocks,
        ];
        for case in cases {
            let source = SourceFile::new("data/a.ab", &case);
            // Whatever the lexer makes of it, parsing it must terminate.
            if let Ok(tokens) = crate::lexer::tokenize(&source) {
                let _ = parse_instances(&source.path, &tokens);
            }
        }
    }

    #[test]
    fn a_header_tag_list_begins_with_a_tag() {
        // GRAMMAR `header_tag_list`: the `,` separates and continues, and never
        // opens the list.
        assert_eq!(ids("P :: , @id.x\n    n: a\n"), [ErrorId::E210]);
        assert!(unit("data/a.ab", "P :: @id.x,\n     @s.a\n    n: a\n").is_ok());
    }

    #[test]
    fn a_multi_path_key_list_carries_no_trailing_separator() {
        // GRAMMAR `multi_path_assignment`: after a `,` the next token is a key.
        assert_eq!(ids("P ::\n    o.{team, contact,}: X\n"), [ErrorId::E210]);
        assert!(unit("data/a.ab", "P ::\n    o.{team, contact}: X\n").is_ok());
    }

    #[test]
    fn a_tuple_column_list_is_glued_to_its_path() {
        // SPEC §3.1 lists a tuple-array column list among the single lexemes.
        assert_eq!(
            ids("P ::\n    copy (key, value): (en, Hi)\n"),
            [ErrorId::E210]
        );
        assert!(unit("data/a.ab", "P ::\n    copy(key, value): (en, Hi)\n").is_ok());
    }

    #[test]
    fn a_trailing_comma_after_a_tuple_row_is_a_trailing_comma() {
        // SPEC §3.6: a trailing `,` at value-bracket depth 0 is E442, and the
        // bracketed spelling is where a trailing comma is allowed (SPEC §5.5).
        assert_eq!(ids("P ::\n    copy(k, v): (en, Hi),\n"), [ErrorId::E442]);
        assert!(unit("data/a.ab", "P ::\n    copy(k, v): [(en, Hi),]\n").is_ok());
    }

    #[test]
    fn a_body_block_brace_ends_its_line() {
        // SPEC §3.6, §5.4: a block's `{` is the last token of its logical line.
        assert_eq!(ids("P ::\n    owner { team: K }\n"), [ErrorId::E210]);
        assert!(unit("data/a.ab", "P ::\n    owner {\n        team: K\n    }\n").is_ok());
    }

    #[test]
    fn a_template_declaration_in_an_instance_file_is_rejected() {
        // SPEC §2.1: `schema`, `logic` and `versions` at statement position in
        // a `.ab` file are E210 — but all three are also field names (B.3).
        assert_eq!(
            ids("P :: @id.x\n    n: a\nschema Other {\n}\n"),
            [ErrorId::E210]
        );
        assert_eq!(
            ids("versions 1..2\n\nP :: @id.x\n    n: a\n"),
            [ErrorId::E210]
        );
        assert_eq!(ids("P :: @id.x\n    logic T {\n}\n"), [ErrorId::E210]);
        assert!(unit(
            "data/a.ab",
            "P ::\n    schema: a\n    logic: b\n    versions: c\n"
        )
        .is_ok());
    }
}
