//! The tokenizer (SPEC chapter 3).
//!
//! The lexer produces identifiers, schema names, keywords, punctuation,
//! literals, bare text and the logical line terminator `NL`. Indentation
//! carries no meaning; a logical line continues only while the value-bracket
//! stack is non-empty, plus the two explicit continuations of SPEC §3.6.
//!
//! # Why the lexer needs context
//!
//! SPEC §3.1 lists `bare_text` among the tokens, and §3.5 defines it as "any
//! other unquoted run of characters": inside a value, `/`, `:` and `::` are
//! ordinary characters, while outside one they are punctuation. A lexer that
//! did not know where values begin could not tokenize `link: https://x` at
//! all. This lexer therefore tracks just enough structure to know where a
//! value region starts — the file kind, the enclosing block, and the token
//! that introduces a value (`:` in an instance statement, `=` in a schema
//! default or a `derive`, `.` after a header tag name) — and hands the parser
//! stages a stream in which every value is already a `QuotedString`,
//! `BareText`, a bracketed list, a `#tag` object or a tuple row.
//!
//! Two consequences the parser stages rely on:
//!
//! - a numeric-looking name arrives as `IntLiteral` or `FloatLiteral`, because
//!   a purely numeric identifier is legal (SPEC §4.9); use
//!   [`Token::identifier_text`] wherever a name is expected;
//! - adjacency (SPEC §3.1) is recorded per token in [`Token::glued`], which is
//!   what separates `@since(2)` from the header tag `@since`, and `ref(` from
//!   `ref (`.

use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Position};
use crate::limits::BRACKET_DEPTH;
use crate::source::SourceFile;

/// The UTF-8 byte-order mark as a scalar. Only the one at offset 0 is removed
/// (SPEC §2.2); every other occurrence is an ordinary character and E210.
const BOM_CHAR: char = '\u{feff}';

/// The token kinds of SPEC §3.1.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    /// `A-Z a-z 0-9 _ -`, beginning with `A-Z a-z 0-9 _`, not ending with `-`.
    Identifier(String),
    /// An ASCII letter followed by letters, digits and `_` (SPEC §3.4).
    SchemaName(String),
    /// A punctuation or operator spelling (SPEC §3.1, Appendix B.3).
    Punctuation(&'static str),
    /// A decimal integer literal, kept as written until it is interpreted.
    IntLiteral(String),
    /// A float literal, kept as written until it is interpreted.
    FloatLiteral(String),
    /// A quoted string with its escapes already decoded (SPEC §3.5).
    QuotedString(String),
    /// An unquoted run of characters, trimmed, with no escape processing.
    BareText(String),
    /// The `WIDTHxHEIGHT` lexeme of an `image(…)` alternative (SPEC §4.4.7).
    SizeToken(String),
    /// The logical line terminator.
    Newline,
    /// End of the source file.
    EndOfFile,
}

/// One token with its position in the source file.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub position: Position,
    /// Byte offsets of the token in the source text, `start..end`.
    pub span: (usize, usize),
    /// True when this token began at the very byte the previous token ended,
    /// with no whitespace and no comment between them (SPEC §3.1, adjacency).
    pub glued: bool,
}

impl Token {
    pub fn new(kind: TokenKind, position: Position, span: (usize, usize)) -> Self {
        Self {
            kind,
            position,
            span,
            glued: false,
        }
    }

    /// Records whether this token is adjacent to the one before it.
    pub fn with_glue(mut self, glued: bool) -> Self {
        self.glued = glued;
        self
    }

    /// The text a name position accepts: an identifier, or the lexeme of a
    /// numeric literal, because a field name may be all digits (SPEC §4.9).
    pub fn identifier_text(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Identifier(text)
            | TokenKind::SchemaName(text)
            | TokenKind::IntLiteral(text)
            | TokenKind::FloatLiteral(text) => Some(text),
            _ => None,
        }
    }

    /// True when this token is exactly the given punctuation spelling.
    pub fn is_punctuation(&self, spelling: &str) -> bool {
        matches!(&self.kind, TokenKind::Punctuation(text) if *text == spelling)
    }

    /// True when this token is the given keyword. Keywords are lowercase and
    /// are compared exactly (SPEC §2.6).
    pub fn is_keyword(&self, word: &str) -> bool {
        matches!(&self.kind, TokenKind::Identifier(text) if text == word)
    }

    pub fn is_newline(&self) -> bool {
        matches!(self.kind, TokenKind::Newline)
    }

    pub fn is_end_of_file(&self) -> bool {
        matches!(self.kind, TokenKind::EndOfFile)
    }

    /// The `{found}` substitution of SPEC §9.8, as it reads after
    /// "Unexpected ".
    pub fn describe(&self) -> String {
        match &self.kind {
            TokenKind::Identifier(text)
            | TokenKind::IntLiteral(text)
            | TokenKind::FloatLiteral(text)
            | TokenKind::SizeToken(text) => format!("'{text}'"),
            TokenKind::SchemaName(text) => format!("schema name '{text}'"),
            TokenKind::Punctuation(text) => format!("'{text}'"),
            TokenKind::QuotedString(_) => "quoted string".to_string(),
            TokenKind::BareText(text) => format!("text '{text}'"),
            TokenKind::Newline => "end of line".to_string(),
            TokenKind::EndOfFile => "end of file".to_string(),
        }
    }
}

/// Tokenizes one source file (SPEC chapter 3).
///
/// The stream holds one [`TokenKind::Newline`] between logical lines — never
/// two in a row, and never one before the first token — and always ends with
/// [`TokenKind::EndOfFile`].
pub fn tokenize(source: &SourceFile) -> Result<Vec<Token>, Diagnostics> {
    let (tokens, errors) = tokenize_recovering(source);
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

/// Tokenizes and returns the stream even when it is defective.
///
/// Lexing and parsing are one phase (SPEC §7.1 P1), and within a phase SPEC
/// §11.1 orders diagnostics by source position, so a parse error on line 1
/// outranks a lexical error on line 10. The caller therefore parses the
/// recovered stream and merges the two lists rather than stopping at the first
/// lexical defect.
pub fn tokenize_recovering(source: &SourceFile) -> (Vec<Token>, Diagnostics) {
    let mut lexer = Lexer::new(source);
    lexer.run();
    (lexer.tokens, lexer.errors)
}

/// Normalisation (SPEC §3.3): ASCII-lowercase every character, then replace
/// every `-` with `_`. It is total and locale-independent because identifiers
/// are ASCII-only.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '-' {
            out.push('_');
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

/// True when `text` is an `identifier` (SPEC §3.3): one or more of
/// `A-Z a-z 0-9 _ -`, beginning with `A-Z a-z 0-9 _`, not ending with `-`.
pub fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    if text.ends_with('-') {
        return false;
    }
    text.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

/// True when `text` is a `schema_name` (SPEC §3.4): an ASCII letter followed
/// by ASCII letters, digits and `_`. Schema names are compared exactly.
pub fn is_schema_name(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// True when `text` is an `int_literal` (SPEC §3.5), ignoring its range.
pub fn is_int_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    all_digits(digits)
}

/// True when `text` is a `float_literal` (SPEC §3.5), ignoring its range. At
/// least one digit is required on each side of the `.`, so `.5` and `5.` are
/// not float literals.
pub fn is_float_literal(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(index) => (&body[..index], Some(&body[index + 1..])),
        None => (body, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((whole, fraction)) => all_digits(whole) && all_digits(fraction),
        None => all_digits(mantissa),
    };
    if !mantissa_ok {
        return false;
    }
    match exponent {
        None => mantissa.contains('.'),
        Some(exponent) => all_digits(exponent.strip_prefix(['+', '-']).unwrap_or(exponent)),
    }
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A character that may appear inside an `identifier` (SPEC §3.3).
fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

/// A character an `identifier` may begin with (SPEC §3.3).
fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The run a word is scanned as before it is classified. Non-ASCII letters and
/// digits are included so that E206 can name the whole offending identifier.
fn is_word_char(ch: char) -> bool {
    is_ident_char(ch) || (!ch.is_ascii() && ch.is_alphanumeric())
}

/// Which block a `}` closes, and which line rules apply inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    /// A `schema` body or a group body (SPEC §4.2, §4.7).
    Schema,
    /// A `logic` body or an `if` / `for` block (SPEC §6.3).
    Logic,
    /// An instance body block (SPEC §5.4).
    Instance,
}

/// The line grammar in force at the start of a logical line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    TemplateTop,
    Schema,
    Logic,
    Instance,
}

#[derive(Clone, Copy, Debug)]
struct OpenBracket {
    open: char,
    offset: usize,
    /// True for the `(` of an `image(…)` type, where a `WIDTHxHEIGHT` lexeme
    /// may follow an extension (SPEC §4.4.7).
    image_args: bool,
}

#[derive(Clone, Copy, Debug)]
struct OpenBlock {
    kind: BlockKind,
    offset: usize,
}

/// What terminates a `bare_text` run (SPEC §3.5, §5.2, §5.5, §5.13).
#[derive(Clone, Copy, Debug)]
struct Stops {
    /// A `,` at the run's own bracket depth 0 ends the run.
    comma: bool,
    /// A `@` at depth 0 ends the run: a header tag value ends at the next tag
    /// (SPEC §5.2).
    at_sign: bool,
    /// A trailing `@since(n)` / `@removed(n)` run ends the run (SPEC §5.13).
    annotations: bool,
    /// `[` and `]` are brackets. They are ordinary characters in a header tag
    /// value (SPEC §5.2).
    square: bool,
}

impl Stops {
    const VALUE: Stops = Stops {
        comma: true,
        at_sign: false,
        annotations: false,
        square: true,
    };

    const HEADER: Stops = Stops {
        comma: true,
        at_sign: true,
        annotations: false,
        square: false,
    };

    fn with_annotations(mut self, annotations: bool) -> Stops {
        self.annotations = annotations;
        self
    }
}

struct Lexer<'a> {
    source: &'a SourceFile,
    file: &'a str,
    text: &'a str,
    template: bool,
    pos: usize,
    /// End offset of the previous token, or `usize::MAX` when gluing is
    /// impossible (start of file, start of a logical line).
    last_end: usize,
    tokens: Vec<Token>,
    errors: Diagnostics,
    brackets: Vec<OpenBracket>,
    blocks: Vec<OpenBlock>,
    /// Index into `tokens` of the first token of the current logical line.
    line_start_token: usize,
    /// Set once, so a hostile file reports one depth error, not thousands.
    depth_reported: bool,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            file: &source.path,
            text: &source.text,
            template: source.is_template(),
            pos: 0,
            last_end: usize::MAX,
            tokens: Vec::new(),
            errors: Diagnostics::new(),
            brackets: Vec::new(),
            blocks: Vec::new(),
            line_start_token: 0,
            depth_reported: false,
        }
    }

    // ---------------------------------------------------------------- errors

    fn error(&mut self, id: ErrorId, offset: usize, message: String) {
        let position = self.source.position_at(offset);
        self.errors
            .push(Diagnostic::at(id, self.file, position, message));
    }

    fn unexpected(&mut self, offset: usize, found: &str, expected: &str) {
        self.error(
            ErrorId::E210,
            offset,
            format!("Unexpected {found} here; expected {expected}."),
        );
    }

    // ------------------------------------------------------------ characters

    fn peek(&self) -> Option<char> {
        self.text
            .get(self.pos..)
            .and_then(|rest| rest.chars().next())
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.text
            .get(self.pos..)
            .and_then(|rest| rest.chars().nth(ahead))
    }

    fn char_at(&self, offset: usize) -> Option<char> {
        self.text.get(offset..).and_then(|rest| rest.chars().next())
    }

    fn previous_char(&self) -> Option<char> {
        self.text
            .get(..self.pos)
            .and_then(|head| head.chars().next_back())
    }

    fn bump(&mut self) {
        match self.peek() {
            Some(ch) => self.pos += ch.len_utf8(),
            None => self.pos = self.text.len(),
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    fn at_line_break(&self) -> bool {
        matches!(self.peek(), Some('\n') | Some('\r'))
    }

    fn consume_line_break(&mut self) {
        if self.peek() == Some('\r') {
            self.bump();
        }
        if self.peek() == Some('\n') {
            self.bump();
        }
    }

    /// A `//` starts a comment only at the start of a line or immediately
    /// after a space or tab, and never inside a quoted string (SPEC §3.2).
    fn at_comment_start(&self) -> bool {
        if self.peek() != Some('/') || self.peek_at(1) != Some('/') {
            return false;
        }
        match self.previous_char() {
            None => true,
            Some(ch) => matches!(ch, ' ' | '\t' | '\n' | '\r'),
        }
    }

    /// Skips spaces, tabs and a trailing comment. Never crosses a line break.
    fn skip_inline(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') => self.bump(),
                _ if self.at_comment_start() => {
                    while !self.at_end() && !self.at_line_break() {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    /// Skips whitespace, comments and — while a value bracket is open — line
    /// breaks, which produce no `NL` (SPEC §3.6).
    fn skip_joined(&mut self) {
        loop {
            self.skip_inline();
            if self.at_line_break() && !self.brackets.is_empty() {
                self.consume_line_break();
                continue;
            }
            break;
        }
    }

    // ---------------------------------------------------------------- tokens

    fn emit(&mut self, kind: TokenKind, start: usize, end: usize) {
        let position = self.source.position_at(start);
        let glued = self.last_end == start;
        self.tokens
            .push(Token::new(kind, position, (start, end)).with_glue(glued));
        self.last_end = end;
    }

    fn emit_punctuation(&mut self, spelling: &'static str, start: usize) {
        let end = (start + spelling.len()).min(self.text.len());
        self.pos = end;
        self.emit(TokenKind::Punctuation(spelling), start, end);
    }

    fn finish_line(&mut self) {
        self.skip_inline();
        let start = self.pos;
        if self.at_line_break() {
            self.consume_line_break();
        }
        let end = self.pos;
        self.emit(TokenKind::Newline, start, end);
        self.last_end = usize::MAX;
    }

    fn previous_token(&self) -> Option<&Token> {
        self.tokens.last()
    }

    // ----------------------------------------------------------------- state

    fn context(&self) -> Context {
        match self.blocks.last().map(|block| block.kind) {
            Some(BlockKind::Schema) => Context::Schema,
            Some(BlockKind::Logic) => Context::Logic,
            Some(BlockKind::Instance) => Context::Instance,
            None if self.template => Context::TemplateTop,
            None => Context::Instance,
        }
    }

    fn line_starts_with(&self, keyword: &str) -> bool {
        self.tokens
            .get(self.line_start_token)
            .map(|token| token.is_keyword(keyword))
            .unwrap_or(false)
    }

    // ------------------------------------------------------------- top level

    fn run(&mut self) {
        if let Some(offset) = self.source.first_lone_carriage_return() {
            self.error(
                ErrorId::E210,
                offset,
                "Unexpected carriage return here; expected 'LF' or 'CRLF'.".to_string(),
            );
            let end = self.text.len();
            self.emit(TokenKind::EndOfFile, end, end);
            return;
        }

        loop {
            self.skip_blank();
            if self.at_end() {
                break;
            }
            let before = self.pos;
            self.line_start_token = self.tokens.len();
            match self.context() {
                Context::TemplateTop => self.scan_template_line(),
                Context::Schema => self.scan_schema_line(),
                Context::Logic => self.scan_logic_line(),
                Context::Instance => self.scan_instance_line(),
            }
            if self.pos == before {
                // Defensive: every scanner consumes at least one character, so
                // this only fires if a future change stops making progress.
                self.bump();
            }
        }

        self.report_unclosed();
        let end = self.text.len();
        self.emit(TokenKind::EndOfFile, end, end);
    }

    /// Skips whitespace, comments and blank lines before a logical line.
    fn skip_blank(&mut self) {
        loop {
            self.skip_inline();
            if self.at_line_break() {
                self.consume_line_break();
                self.last_end = usize::MAX;
                continue;
            }
            break;
        }
    }

    fn report_unclosed(&mut self) {
        if let Some(bracket) = self.brackets.last().copied() {
            let position = self.source.position_at(bracket.offset);
            self.error(
                ErrorId::E203,
                bracket.offset,
                format!(
                    "Unclosed '{}' opened at {}:{}.",
                    bracket.open, position.line, position.col
                ),
            );
        }
        if let Some(block) = self.blocks.last().copied() {
            let position = self.source.position_at(block.offset);
            self.error(
                ErrorId::E203,
                block.offset,
                format!(
                    "Unclosed '{{' opened at {}:{}.",
                    position.line, position.col
                ),
            );
        }
    }

    // -------------------------------------------------------------- brackets

    fn push_bracket(&mut self, open: char, offset: usize) {
        let image_args = open == '('
            && self
                .previous_token()
                .map(|token| token.is_keyword("image"))
                .unwrap_or(false);
        if self.brackets.len() >= BRACKET_DEPTH && !self.depth_reported {
            self.depth_reported = true;
            self.error(
                ErrorId::E209,
                offset,
                format!("Bracket nesting depth in a value exceeds the limit of {BRACKET_DEPTH}."),
            );
        }
        // A bracket past the limit is still recorded, so that its matching
        // close pops it and E209 stays the only diagnostic. The stack costs one
        // entry per bracket and never drives recursion; the recursive value
        // scanner is bounded separately in `scan_single_value`.
        self.brackets.push(OpenBracket {
            open,
            offset,
            image_args,
        });
    }

    fn pop_bracket(&mut self, close: char, offset: usize) {
        let expected_open = match close {
            ')' => '(',
            ']' => '[',
            _ => '{',
        };
        match self.brackets.last().copied() {
            Some(bracket) if bracket.open == expected_open => {
                self.brackets.pop();
            }
            Some(bracket) => {
                let position = self.source.position_at(bracket.offset);
                let expected = match bracket.open {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                self.error(
                    ErrorId::E204,
                    offset,
                    format!(
                        "Mismatched bracket: expected '{expected}' to close '{}' opened at {}:{}, found '{close}'.",
                        bracket.open, position.line, position.col
                    ),
                );
                self.brackets.pop();
            }
            None => {
                self.error(ErrorId::E205, offset, format!("Unexpected '{close}'."));
            }
        }
    }

    fn in_image_arguments(&self) -> bool {
        self.brackets
            .last()
            .map(|bracket| bracket.image_args)
            .unwrap_or(false)
    }
}

impl Lexer<'_> {
    // ------------------------------------------------------------ line kinds

    /// A `.abt` top-level line: `versions`, `schema` or `logic` (SPEC §4.1).
    fn scan_template_line(&mut self) {
        loop {
            self.skip_inline();
            if self.at_end() {
                break;
            }
            if self.at_line_break() {
                if self.brackets.is_empty() {
                    break;
                }
                self.consume_line_break();
                continue;
            }
            self.next_structural_token();
        }
        self.finish_line();
    }

    /// A field declaration inside a `schema` or a group body (SPEC §4.3). The
    /// type expression is structural; only a default, introduced by `=`, is a
    /// value region.
    fn scan_schema_line(&mut self) {
        loop {
            self.skip_inline();
            if self.at_end() {
                break;
            }
            if self.at_line_break() {
                if self.brackets.is_empty() {
                    break;
                }
                self.consume_line_break();
                continue;
            }
            if self.at_value_introducing_equals() {
                let start = self.pos;
                self.emit_punctuation("=", start);
                // A trailing annotation is not stripped from a default: it is
                // a modifier written after the default, which the schema
                // parser reports as E303 (SPEC §4.3).
                self.scan_assignment_value(false, false);
                continue;
            }
            self.next_structural_token();
        }
        self.finish_line();
    }

    /// A statement inside a `logic` block or one of its `if` / `for` blocks
    /// (SPEC chapter 6).
    fn scan_logic_line(&mut self) {
        loop {
            self.skip_inline();
            if self.at_end() {
                break;
            }
            if self.at_line_break() {
                if !self.brackets.is_empty() {
                    self.consume_line_break();
                    continue;
                }
                // SPEC §3.6 rule (b): a line terminator immediately before
                // `else` does not end the logical line.
                if self.continue_before_else() {
                    continue;
                }
                break;
            }
            if self.at_value_introducing_equals() {
                let start = self.pos;
                self.emit_punctuation("=", start);
                self.scan_derive_value();
                continue;
            }
            if self.at_literal_iterable() {
                self.scan_value_items(0, false);
                continue;
            }
            self.next_structural_token();
        }
        self.finish_line();
    }

    /// An instance header, clone statement, assignment or body block
    /// (SPEC chapter 5).
    fn scan_instance_line(&mut self) {
        let mut tuple_columns = false;
        loop {
            self.skip_inline();
            if self.at_end() {
                break;
            }
            if self.at_line_break() {
                if self.brackets.is_empty() {
                    break;
                }
                self.consume_line_break();
                continue;
            }
            if self.brackets.is_empty() && self.peek() == Some(':') {
                let start = self.pos;
                if self.peek_at(1) == Some(':') {
                    self.emit_punctuation("::", start);
                    self.scan_header_tags();
                } else {
                    self.emit_punctuation(":", start);
                    self.scan_assignment_value(tuple_columns, true);
                    self.scan_trailing_text();
                }
                continue;
            }
            if self.brackets.is_empty()
                && self.peek() == Some('(')
                && self.tokens.len() > self.line_start_token
            {
                tuple_columns = true;
            }
            if self.tokens.len() == self.line_start_token && self.at_instance_header_name() {
                self.scan_word(true);
                continue;
            }
            // Comparison and logic operators belong to a logic block
            // (SPEC chapter 6); outside a value an instance file has no token
            // they could spell, so one is E210 and the rest of the physical
            // line is abandoned rather than lexed as a value.
            if let Some(ch) = self.peek() {
                if matches!(ch, '!' | '<' | '>' | '=' | '|' | '?') {
                    let at = self.pos;
                    let found = describe_char(ch);
                    self.unexpected(at, &found, "a field name, a value or ':'");
                    self.skip_physical_line();
                    continue;
                }
            }
            self.next_structural_token();
        }
        self.finish_line();
    }

    /// Whatever is left on an assignment's line once its value region has
    /// closed. SPEC §5.5 makes the closing quote of a quoted value the end of
    /// that value, so the remainder is trailing text rather than a second
    /// value: it is taken as one lexeme, and the parser reports E210 at its
    /// first character. Lexing it as a value again would let a stray `"` open
    /// a string and report E201 further right instead.
    fn scan_trailing_text(&mut self) {
        self.skip_inline();
        if !self.brackets.is_empty() || self.at_value_end(false) {
            return;
        }
        self.scan_bare_text(Stops::VALUE);
    }

    /// Consumes the rest of the physical line, leaving the line terminator.
    fn skip_physical_line(&mut self) {
        while !self.at_end() && !self.at_line_break() {
            self.bump();
        }
    }

    // ------------------------------------------------------------- positions

    /// True when the next token is a `schema_name` by position: after the
    /// `schema` / `logic` keyword of a template line, or inside a glued
    /// `ref(` / `$(` (SPEC §4.4.8, §4.4.9).
    fn at_schema_name_position(&self) -> bool {
        let Some(previous) = self.tokens.last() else {
            return false;
        };
        if (previous.is_keyword("schema") || previous.is_keyword("logic"))
            && self.tokens.len() == self.line_start_token + 1
        {
            return true;
        }
        if previous.is_punctuation("(") && previous.glued {
            if let Some(before) = self.tokens.get(self.tokens.len().wrapping_sub(2)) {
                return before.is_keyword("ref") || before.is_punctuation("$");
            }
        }
        false
    }

    /// True when the previous token is the `schema` or `logic` keyword that
    /// introduces a declaration, as opposed to the glued `(` of `ref(` or
    /// `$(`, where the schema name ends at the closing bracket.
    fn at_declaration_keyword(&self) -> bool {
        self.tokens
            .last()
            .map(|token| token.is_keyword("schema") || token.is_keyword("logic"))
            .unwrap_or(false)
    }

    /// True when a single `=` at bracket depth 0 introduces a value: a schema
    /// default (SPEC §4.3) or a `derive` right-hand side (SPEC §6.4). `==` is
    /// the comparison operator and never introduces a value.
    fn at_value_introducing_equals(&self) -> bool {
        self.brackets.is_empty() && self.peek() == Some('=') && self.peek_at(1) != Some('=')
    }

    /// True when the current `[` opens the literal list of a `for` loop
    /// (SPEC §6.2), whose elements are values rather than structure.
    fn at_literal_iterable(&self) -> bool {
        self.peek() == Some('[')
            && self.line_starts_with("for")
            && self
                .tokens
                .last()
                .map(|token| token.is_keyword("in"))
                .unwrap_or(false)
    }

    /// True when the word at the cursor is the schema name of an instance
    /// header: a logical line is a header if and only if its first token is a
    /// schema name and the next token is `::` (SPEC §5.1). A word that is not
    /// a valid schema name stays an identifier, so that the parser reports
    /// E439 naming it, rather than E208.
    fn at_instance_header_name(&self) -> bool {
        let Some(first) = self.peek() else {
            return false;
        };
        if !first.is_ascii_alphabetic() {
            return false;
        }
        let end = self.word_end(self.pos);
        let Some(word) = self.text.get(self.pos..end) else {
            return false;
        };
        if !is_schema_name(word) {
            return false;
        }
        let mut after = end;
        while matches!(self.char_at(after), Some(' ') | Some('\t')) {
            after += 1;
        }
        self.char_at(after) == Some(':') && self.char_at(after + 1) == Some(':')
    }

    /// The end offset of the word run that starts at `from`.
    fn word_end(&self, from: usize) -> usize {
        let mut end = from;
        while let Some(ch) = self.char_at(end) {
            if is_word_char(ch) {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// SPEC §3.6 rule (b). Moves the cursor onto a following `else`, across
    /// line breaks, comments and blank lines, inside a logic block.
    fn continue_before_else(&mut self) -> bool {
        if self.context() != Context::Logic {
            return false;
        }
        let mut probe = self.pos;
        loop {
            match self.char_at(probe) {
                Some(' ') | Some('\t') | Some('\n') | Some('\r') => probe += 1,
                Some('/') if self.char_at(probe + 1) == Some('/') => {
                    while !matches!(self.char_at(probe), None | Some('\n') | Some('\r')) {
                        probe += self.char_at(probe).map(char::len_utf8).unwrap_or(1);
                    }
                }
                _ => break,
            }
        }
        if self.text.get(probe..self.word_end(probe)) != Some("else") {
            return false;
        }
        self.pos = probe;
        self.last_end = usize::MAX;
        true
    }
}

impl Lexer<'_> {
    // ----------------------------------------------------- structural tokens

    /// Scans one token outside a value region: punctuation, an operator
    /// spelling, a word or a quoted string (SPEC §3.1).
    fn next_structural_token(&mut self) {
        let start = self.pos;
        let Some(ch) = self.peek() else {
            return;
        };
        if !ends_size_token(ch) && self.try_size_token() {
            return;
        }
        match ch {
            '"' => self.scan_quoted_string(),
            '(' => {
                self.push_bracket('(', start);
                self.emit_punctuation("(", start);
            }
            '[' => {
                self.push_bracket('[', start);
                self.emit_punctuation("[", start);
            }
            ')' => {
                self.pop_bracket(')', start);
                self.emit_punctuation(")", start);
            }
            ']' => {
                self.pop_bracket(']', start);
                self.emit_punctuation("]", start);
            }
            '{' => self.scan_open_brace(),
            '}' => self.scan_close_brace(),
            ':' if self.peek_at(1) == Some(':') => self.emit_punctuation("::", start),
            ':' => self.emit_punctuation(":", start),
            '.' if self.peek_at(1) == Some('.') => self.emit_punctuation("..", start),
            '.' => self.emit_punctuation(".", start),
            '=' if self.peek_at(1) == Some('=') => self.emit_punctuation("==", start),
            '=' => self.emit_punctuation("=", start),
            '!' if self.peek_at(1) == Some('=') => self.emit_punctuation("!=", start),
            '!' => self.emit_punctuation("!", start),
            '<' if self.peek_at(1) == Some('=') => self.emit_punctuation("<=", start),
            '<' => self.emit_punctuation("<", start),
            '>' if self.peek_at(1) == Some('=') => self.emit_punctuation(">=", start),
            '>' => self.emit_punctuation(">", start),
            '&' if self.peek_at(1) == Some('&') => self.emit_punctuation("&&", start),
            '&' => {
                self.emit_punctuation("&", start);
                self.require_glued_name();
            }
            '|' if self.peek_at(1) == Some('|') => self.emit_punctuation("||", start),
            '|' => {
                self.unexpected(start, "'|'", "'||'");
                self.bump();
            }
            ',' => self.emit_punctuation(",", start),
            '@' | '#' | '$' => {
                let spelling = match ch {
                    '@' => "@",
                    '#' => "#",
                    _ => "$",
                };
                self.emit_punctuation(spelling, start);
                self.require_glued_name();
            }
            '*' => self.emit_punctuation("*", start),
            '?' => {
                let after_derive = self
                    .tokens
                    .last()
                    .map(|token| token.is_keyword("derive") && token.span.1 == start)
                    .unwrap_or(false);
                if after_derive {
                    self.emit_punctuation("?", start);
                } else {
                    self.unexpected(start, "'?'", "'derive?'");
                    self.bump();
                }
            }
            '-' if self
                .peek_at(1)
                .map(|next| next.is_ascii_digit())
                .unwrap_or(false) =>
            {
                let schema_position = self.at_schema_name_position();
                self.scan_word(schema_position);
            }
            _ if is_word_char(ch) => {
                let schema_position = self.at_schema_name_position();
                self.scan_word(schema_position);
            }
            _ => {
                let found = describe_char(ch);
                self.unexpected(start, &found, "a token");
                self.bump();
            }
        }
    }

    /// SPEC §3.1 adjacency: the `#`, `@`, `&` or `$` that introduces a tag
    /// object, a header tag, a modifier, a clone or a variable is one lexeme
    /// with the identifier that follows it.
    fn require_glued_name(&mut self) {
        if matches!(self.peek(), Some(' ') | Some('\t')) {
            let offset = self.pos;
            self.unexpected(offset, "whitespace", "an identifier");
        }
    }

    /// A `{` immediately preceded by `.` opens a multi-path key list; every
    /// other `{` is a block brace and joins no lines (SPEC §3.6, §5.4).
    fn scan_open_brace(&mut self) {
        let start = self.pos;
        let multi_path = self
            .tokens
            .last()
            .map(|token| token.is_punctuation(".") && token.span.1 == start)
            .unwrap_or(false);
        if multi_path {
            self.push_bracket('{', start);
            self.emit_punctuation("{", start);
            return;
        }
        let kind = match self.context() {
            Context::TemplateTop if self.line_starts_with("logic") => BlockKind::Logic,
            Context::TemplateTop | Context::Schema => BlockKind::Schema,
            Context::Logic => BlockKind::Logic,
            Context::Instance => BlockKind::Instance,
        };
        self.blocks.push(OpenBlock {
            kind,
            offset: start,
        });
        self.emit_punctuation("{", start);
    }

    fn scan_close_brace(&mut self) {
        let start = self.pos;
        if self
            .brackets
            .last()
            .map(|bracket| bracket.open == '{')
            .unwrap_or(false)
        {
            self.pop_bracket('}', start);
            self.emit_punctuation("}", start);
            return;
        }
        if self.blocks.pop().is_none() {
            self.error(ErrorId::E205, start, "Unexpected '}'.".to_string());
        }
        self.emit_punctuation("}", start);
    }

    /// A quoted string, decoded (SPEC §3.5). It MUST open and close on the
    /// same physical line (E201); an unknown escape is E202.
    fn scan_quoted_string(&mut self) {
        let start = self.pos;
        self.bump();
        let mut value = String::new();
        loop {
            match self.peek() {
                None | Some('\n') | Some('\r') => {
                    self.error(
                        ErrorId::E201,
                        start,
                        "Unterminated string literal; strings must open and close on the same line."
                            .to_string(),
                    );
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    let escape_at = self.pos;
                    self.bump();
                    match self.peek() {
                        Some('"') => {
                            value.push('"');
                            self.bump();
                        }
                        Some('\\') => {
                            value.push('\\');
                            self.bump();
                        }
                        Some('n') => {
                            value.push('\n');
                            self.bump();
                        }
                        Some('r') => {
                            value.push('\r');
                            self.bump();
                        }
                        Some('t') => {
                            value.push('\t');
                            self.bump();
                        }
                        Some(other) if other != '\n' && other != '\r' => {
                            self.error(
                                ErrorId::E202,
                                escape_at,
                                format!(
                                    "Unknown escape '\\{other}' in a string literal; valid escapes are \\\" \\\\ \\n \\r \\t."
                                ),
                            );
                            value.push(other);
                            self.bump();
                        }
                        _ => {}
                    }
                }
                Some(ch) => {
                    value.push(ch);
                    self.bump();
                }
            }
        }
        let end = self.pos;
        self.emit(TokenKind::QuotedString(value), start, end);
    }

    /// One word: an integer literal, a float literal, a schema name or an
    /// identifier (SPEC §3.3, §3.4, §3.5). The `..` of a range never becomes
    /// part of a float, because a `.` followed by `.` is the two-character
    /// token (SPEC §3.1, longest match).
    fn scan_word(&mut self, schema_position: bool) {
        let start = self.pos;
        let declaration = schema_position && self.at_declaration_keyword();
        if self.peek() == Some('-')
            && self
                .peek_at(1)
                .map(|next| next.is_ascii_digit())
                .unwrap_or(false)
        {
            self.bump();
        }
        self.consume_word_run();
        if !schema_position {
            if self.digits_only(start)
                && self.peek() == Some('.')
                && self
                    .peek_at(1)
                    .map(|next| next.is_ascii_digit())
                    .unwrap_or(false)
            {
                self.bump();
                self.consume_word_run();
            }
            // `+` is not a word character, so a signed exponent is joined here.
            if self.peek() == Some('+')
                && self
                    .peek_at(1)
                    .map(|next| next.is_ascii_digit())
                    .unwrap_or(false)
                && self
                    .text
                    .get(start..self.pos)
                    .map(is_exponent_prefix)
                    .unwrap_or(false)
            {
                self.bump();
                self.consume_word_run();
            }
        }
        if declaration {
            // A `schema` or `logic` line names exactly one schema, so
            // everything up to the block brace is that name. Reading only the
            // first word would leave `schema My Thing {` reporting a stray
            // token; SPEC §10.2 names it as E208's own example.
            while let Some(ch) = self.peek() {
                if ch == '{' || ch == '\n' || ch == '\r' || self.at_comment_start() {
                    break;
                }
                self.bump();
            }
        }
        let raw = self.text.get(start..self.pos).unwrap_or("");
        let trimmed = raw.trim_end_matches([' ', '\t']);
        self.pos = start + trimmed.len();
        let end = self.pos;
        let text = trimmed.to_string();

        if schema_position {
            if !is_schema_name(&text) {
                self.error(
                    ErrorId::E208,
                    start,
                    format!(
                        "Invalid schema name '{text}'; schema names start with a letter and contain letters, digits and '_'."
                    ),
                );
            }
            self.emit(TokenKind::SchemaName(text), start, end);
            return;
        }
        if is_int_literal(&text) {
            if text.parse::<i64>().is_err() {
                self.error(
                    ErrorId::E211,
                    start,
                    format!("'{text}' is not a valid int literal."),
                );
            }
            self.emit(TokenKind::IntLiteral(text), start, end);
            return;
        }
        if is_float_literal(&text) {
            if !text.parse::<f64>().map(f64::is_finite).unwrap_or(false) {
                self.error(
                    ErrorId::E211,
                    start,
                    format!("'{text}' is not a valid float literal."),
                );
            }
            self.emit(TokenKind::FloatLiteral(text), start, end);
            return;
        }
        self.check_identifier(&text, start);
        self.emit(TokenKind::Identifier(text), start, end);
    }

    fn consume_word_run(&mut self) {
        while let Some(ch) = self.peek() {
            if is_word_char(ch) {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn digits_only(&self, start: usize) -> bool {
        self.text
            .get(start..self.pos)
            .map(|text| all_digits(text.strip_prefix('-').unwrap_or(text)))
            .unwrap_or(false)
    }

    fn check_identifier(&mut self, text: &str, start: usize) {
        if let Some(bad) = text.chars().find(|ch| !is_ident_char(*ch)) {
            self.error(
                ErrorId::E206,
                start,
                format!(
                    "Invalid character '{bad}' in identifier '{text}'; identifiers use A-Z a-z 0-9 _ -."
                ),
            );
            return;
        }
        if text.ends_with('-') {
            self.error(
                ErrorId::E207,
                start,
                format!("Identifier '{text}' must not end with '-'."),
            );
            return;
        }
        // SPEC §3.3: "MUST begin with A-Z a-z 0-9 _" is a rule about where a
        // character may stand, not about which characters exist, so it is
        // E210 at the offending character. E206 would deny its own message,
        // which lists `-` among the characters an identifier may contain.
        if let Some(first) = text.chars().next() {
            if !is_ident_start(first) {
                self.unexpected(
                    start,
                    &describe_char(first),
                    "an identifier beginning with A-Z a-z 0-9 _",
                );
            }
        }
    }

    /// The `WIDTHxHEIGHT` lexeme of SPEC §4.4.7. It is recognised only inside
    /// `image(…)` and only after whitespace, so `image(png128x128)` stays the
    /// single extension run E307 names.
    fn try_size_token(&mut self) -> bool {
        if !self.in_image_arguments() || self.last_end == self.pos {
            return false;
        }
        // The first token of an image argument is its extension; only what
        // follows the extension sits in the size-token position (SPEC §4.4.7).
        let after_extension = self
            .previous_token()
            .map(|token| {
                !token.is_punctuation("(")
                    && !token.is_punctuation(",")
                    && !token.is_punctuation(".")
            })
            .unwrap_or(false);
        if !after_extension {
            return false;
        }
        let Some(rest) = self.text.get(self.pos..) else {
            return false;
        };
        // SPEC §4.4.7 makes the size token one lexeme with no internal
        // whitespace, so a malformed one is taken whole and reported as E308
        // rather than split into an identifier the character rules then reject.
        let length = match size_token_length(rest) {
            Some(length) => length,
            None => rest
                .chars()
                .take_while(|&ch| !ends_size_token(ch))
                .map(char::len_utf8)
                .sum(),
        };
        if length == 0 {
            return false;
        }
        let start = self.pos;
        let end = start + length;
        self.pos = end;
        let text = self.text.get(start..end).unwrap_or("").to_string();
        self.emit(TokenKind::SizeToken(text), start, end);
        true
    }
}

/// The control scalar values SPEC §3.5 excludes from bare text: the C0 range,
/// `U+007F` and the C1 range. Every other scalar value is ordinary text, so
/// non-ASCII whitespace such as `U+00A0` or `U+2028` is a character of the
/// value and never a separator (SPEC §3.1).
pub(crate) fn is_control_scalar(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
}

/// The `{found}` spelling of a character that is not a token.
fn describe_char(ch: char) -> String {
    if ch == BOM_CHAR {
        "byte-order mark U+FEFF".to_string()
    } else if is_control_scalar(ch) {
        format!("control character U+{:04X}", ch as u32)
    } else {
        format!("'{ch}'")
    }
}

/// True when `text` is a float mantissa followed by `e` or `E`, so that a `+`
/// after it continues the same literal.
fn is_exponent_prefix(text: &str) -> bool {
    let Some(mantissa) = text.strip_suffix(['e', 'E']) else {
        return false;
    };
    let body = mantissa.strip_prefix('-').unwrap_or(mantissa);
    match body.split_once('.') {
        Some((whole, fraction)) => all_digits(whole) && all_digits(fraction),
        None => all_digits(body),
    }
}

/// The stand-in for the `{` of a `${` on a bare text's own bracket stack.
/// SPEC §3.1 makes `${` one lexeme, so its brace is neither a block brace nor
/// the `{` of a multi-path key list.
const INTERPOLATION_BRACE: char = '\u{0}';

/// The closing character of a bare text's open bracket.
fn closing_of(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

/// How an open bracket is spelled in a diagnostic.
fn spell_open(open: char) -> &'static str {
    match open {
        '(' => "(",
        '[' => "[",
        INTERPOLATION_BRACE => "${",
        _ => "{",
    }
}

/// True when the run of `$` immediately before `at` has odd length, so that the
/// character at `at` is preceded by a `${` introducer rather than by the second
/// `$` of an escaped `$$` (SPEC §5.11). `start` bounds the search to one run.
fn dollar_run_is_odd(text: &str, start: usize, at: usize) -> bool {
    let Some(before) = text.get(start..at) else {
        return false;
    };
    before
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'$')
        .count()
        % 2
        == 1
}

/// True for a character that cannot be part of a size token, so that a
/// malformed one still ends where the argument list's own punctuation begins.
fn ends_size_token(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | ',' | '"')
}

/// The byte length of a `size_token` at the head of `text`, when there is one.
fn size_token_length(text: &str) -> Option<usize> {
    fn side(text: &str) -> Option<usize> {
        if text.starts_with('*') {
            return Some(1);
        }
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            None
        } else {
            Some(digits)
        }
    }
    let first = side(text)?;
    if text.as_bytes().get(first) != Some(&b'x') {
        return None;
    }
    let second = side(text.get(first + 1..)?)?;
    let total = first + 1 + second;
    match text.get(total..).and_then(|rest| rest.chars().next()) {
        Some(ch) if is_word_char(ch) => None,
        _ => Some(total),
    }
}

/// What an argument of a `#tag(…)` list looks like (SPEC §5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TagArgument {
    /// `key: value`, the key possibly dotted.
    KeyValue,
    /// A bare boolean flag.
    Flag,
    /// Anything else; the parser reports E419.
    Other,
}

impl Lexer<'_> {
    // ---------------------------------------------------------------- values

    /// The value region of an assignment, a default or a `derive`
    /// (SPEC §5.5). `tuple` is set when the left side declared columns.
    fn scan_assignment_value(&mut self, tuple: bool, annotations: bool) {
        self.skip_inline();
        if tuple && matches!(self.peek(), Some('(') | Some('[')) {
            self.scan_tuple_rows();
        } else {
            self.scan_value_items(0, annotations);
        }
        self.skip_inline();
        if annotations && self.at_annotation_suffix() {
            self.scan_annotations();
        }
    }

    /// A `bare_list`: single values separated by depth-0 commas (SPEC §5.5).
    /// A trailing comma at depth 0 stays in the stream, so the parser reports
    /// E442 rather than the lexer swallowing the next line.
    fn scan_value_items(&mut self, depth: usize, annotations: bool) {
        loop {
            self.skip_inline();
            if self.at_value_end(annotations) {
                break;
            }
            self.scan_single_value(depth, Stops::VALUE.with_annotations(annotations));
            self.skip_inline();
            if self.peek() == Some(',') {
                let at = self.pos;
                self.emit_punctuation(",", at);
                continue;
            }
            break;
        }
    }

    fn at_value_end(&self, annotations: bool) -> bool {
        if self.at_end() || self.at_line_break() || self.at_comment_start() {
            return true;
        }
        annotations && self.at_annotation_suffix()
    }

    fn scan_single_value(&mut self, depth: usize, stops: Stops) {
        if depth > BRACKET_DEPTH {
            self.report_depth_limit();
            self.scan_bare_text(stops);
            return;
        }
        match self.peek() {
            Some('"') => self.scan_quoted_string(),
            Some('[') => self.scan_list_value(depth),
            Some('#') => self.scan_tag_object(depth),
            _ => self.scan_bare_text(stops),
        }
    }

    fn report_depth_limit(&mut self) {
        if self.depth_reported {
            return;
        }
        self.depth_reported = true;
        let offset = self.pos;
        self.error(
            ErrorId::E209,
            offset,
            format!("Bracket nesting depth in a value exceeds the limit of {BRACKET_DEPTH}."),
        );
    }

    /// `[ … ]` (SPEC §5.5). A value whose first character is `[` is always a
    /// list; a nested list is left to the parser, which reports E441.
    fn scan_list_value(&mut self, depth: usize) {
        let start = self.pos;
        self.push_bracket('[', start);
        self.emit_punctuation("[", start);
        let mut separator_reported = false;
        loop {
            self.skip_joined();
            match self.peek() {
                None => break,
                Some(']') | Some(')') => {
                    self.close_value_bracket();
                    break;
                }
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                    continue;
                }
                _ => {}
            }
            let before = self.pos;
            self.scan_single_value(depth + 1, Stops::VALUE);
            self.skip_joined();
            match self.peek() {
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                }
                Some(']') | Some(')') => {
                    self.close_value_bracket();
                    break;
                }
                None => break,
                Some(ch) => {
                    // The bracket stays open. SPEC §3.6 continues the logical
                    // line while the value-bracket stack is non-empty, so a
                    // list that is never closed is E203 at its own '[', which
                    // outranks this diagnostic on position (SPEC §11.1).
                    if !separator_reported {
                        separator_reported = true;
                        let at = self.pos;
                        let found = describe_char(ch);
                        self.unexpected(at, &found, "',' or ']'");
                    }
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
    }

    /// Emits the closing bracket at the cursor, checking its type (E204/E205).
    fn close_value_bracket(&mut self) {
        let at = self.pos;
        match self.peek() {
            Some(')') => {
                self.pop_bracket(')', at);
                self.emit_punctuation(")", at);
            }
            Some(']') => {
                self.pop_bracket(']', at);
                self.emit_punctuation("]", at);
            }
            _ => {}
        }
    }

    /// `#name` and `#name(key: value, flag)` (SPEC §5.5).
    fn scan_tag_object(&mut self, depth: usize) {
        let start = self.pos;
        self.emit_punctuation("#", start);
        if matches!(self.peek(), Some(' ') | Some('\t')) {
            self.require_glued_name();
            return;
        }
        match self.peek() {
            Some(ch) if is_word_char(ch) => {
                self.scan_word(false);
                self.take_wildcard_suffix();
            }
            other => {
                let at = self.pos;
                let found = other
                    .map(describe_char)
                    .unwrap_or_else(|| "end of file".to_string());
                self.unexpected(at, &found, "an identifier");
                return;
            }
        }
        if self.peek() == Some('(') && self.last_end == self.pos {
            let at = self.pos;
            self.push_bracket('(', at);
            self.emit_punctuation("(", at);
            self.scan_tag_arguments(depth);
        }
    }

    /// Folds a glued trailing `*` into the tag name just emitted.
    ///
    /// SPEC §5.6 position (c) makes `#s*` a prefix wildcard on the tag field,
    /// so the `*` belongs to the name and never becomes a token of its own.
    fn take_wildcard_suffix(&mut self) {
        if self.peek() != Some('*') || self.last_end != self.pos {
            return;
        }
        let Some(token) = self.tokens.last_mut() else {
            return;
        };
        let text = match &token.kind {
            TokenKind::Identifier(text)
            | TokenKind::IntLiteral(text)
            | TokenKind::FloatLiteral(text) => format!("{text}*"),
            _ => return,
        };
        token.kind = TokenKind::Identifier(text);
        token.span.1 = self.pos + 1;
        self.bump();
        self.last_end = self.pos;
    }

    fn scan_tag_arguments(&mut self, depth: usize) {
        loop {
            self.skip_joined();
            match self.peek() {
                None => break,
                Some(')') | Some(']') => {
                    self.close_value_bracket();
                    break;
                }
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                    continue;
                }
                _ => {}
            }
            match self.tag_argument_shape() {
                TagArgument::KeyValue => {
                    loop {
                        self.scan_word(false);
                        if self.peek() == Some('.') {
                            let at = self.pos;
                            self.emit_punctuation(".", at);
                            continue;
                        }
                        break;
                    }
                    self.skip_inline();
                    if self.peek() == Some(':') {
                        let at = self.pos;
                        self.emit_punctuation(":", at);
                    }
                    self.skip_joined();
                    self.scan_single_value(depth + 1, Stops::VALUE);
                }
                TagArgument::Flag => self.scan_word(false),
                TagArgument::Other => self.scan_bare_text(Stops::VALUE),
            }
            self.skip_joined();
            match self.peek() {
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                }
                Some(')') | Some(']') => {
                    self.close_value_bracket();
                    break;
                }
                None => break,
                Some(ch) => {
                    let at = self.pos;
                    let found = describe_char(ch);
                    self.unexpected(at, &found, "',' or ')'");
                    self.brackets.pop();
                    break;
                }
            }
        }
    }

    fn tag_argument_shape(&self) -> TagArgument {
        let mut probe = self.pos;
        let mut segments = 0usize;
        loop {
            let end = self.word_end(probe);
            if end == probe {
                return TagArgument::Other;
            }
            segments += 1;
            probe = end;
            if self.char_at(probe) == Some('.') {
                probe += 1;
                continue;
            }
            break;
        }
        let mut after = probe;
        while matches!(self.char_at(after), Some(' ') | Some('\t')) {
            after += 1;
        }
        match self.char_at(after) {
            Some(':') => TagArgument::KeyValue,
            Some(',') | Some(')') | None if segments == 1 => TagArgument::Flag,
            _ => TagArgument::Other,
        }
    }

    /// Tuple rows, bracketed or not (SPEC §5.5).
    fn scan_tuple_rows(&mut self) {
        if self.peek() == Some('[') {
            let start = self.pos;
            self.push_bracket('[', start);
            self.emit_punctuation("[", start);
            loop {
                self.skip_joined();
                match self.peek() {
                    None => break,
                    Some(']') | Some(')') => {
                        self.close_value_bracket();
                        break;
                    }
                    Some(',') => {
                        let at = self.pos;
                        self.emit_punctuation(",", at);
                        continue;
                    }
                    _ => {}
                }
                self.scan_tuple_row();
                self.skip_joined();
                match self.peek() {
                    Some(',') => {
                        let at = self.pos;
                        self.emit_punctuation(",", at);
                    }
                    // A ')' here closes the '[' of the row list: SPEC §3.6
                    // makes a mismatched closing bracket E204, not E210.
                    Some(']') | Some(')') => {
                        self.close_value_bracket();
                        break;
                    }
                    None => break,
                    Some(ch) => {
                        let at = self.pos;
                        let found = describe_char(ch);
                        self.unexpected(at, &found, "',' or ']'");
                        self.brackets.pop();
                        break;
                    }
                }
            }
            return;
        }
        loop {
            self.scan_tuple_row();
            self.skip_inline();
            if self.peek() == Some(',') {
                let at = self.pos;
                self.emit_punctuation(",", at);
                self.skip_inline();
                // SPEC §3.6: a trailing `,` at value-bracket depth 0 does not
                // continue the statement. The token stays in the stream so the
                // parser reports E442, as it does for `tags: core,`.
                if self.at_value_end(false) {
                    break;
                }
                continue;
            }
            break;
        }
    }

    fn scan_tuple_row(&mut self) {
        if self.peek() != Some('(') {
            let at = self.pos;
            let found = self
                .peek()
                .map(describe_char)
                .unwrap_or_else(|| "end of line".to_string());
            self.unexpected(at, &found, "'('");
            self.scan_bare_text(Stops::VALUE);
            return;
        }
        let start = self.pos;
        self.push_bracket('(', start);
        self.emit_punctuation("(", start);
        loop {
            self.skip_joined();
            match self.peek() {
                None => break,
                Some(')') | Some(']') => {
                    self.close_value_bracket();
                    break;
                }
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                    continue;
                }
                _ => {}
            }
            self.scan_tuple_cell();
            self.skip_joined();
            match self.peek() {
                Some(',') => {
                    let at = self.pos;
                    self.emit_punctuation(",", at);
                }
                // A ']' here closes the '(' of this row: SPEC §3.6 makes a
                // mismatched closing bracket E204, not E210.
                Some(')') | Some(']') => {
                    self.close_value_bracket();
                    break;
                }
                None => break,
                Some(ch) => {
                    let at = self.pos;
                    let found = describe_char(ch);
                    self.unexpected(at, &found, "',' or ')'");
                    self.brackets.pop();
                    break;
                }
            }
        }
    }

    /// A tuple cell is a quoted string or bare text and nothing else
    /// (SPEC §5.5).
    fn scan_tuple_cell(&mut self) {
        match self.peek() {
            Some('"') => self.scan_quoted_string(),
            Some('#') | Some('[') => {
                let at = self.pos;
                let found = self
                    .peek()
                    .map(describe_char)
                    .unwrap_or_else(|| "end of line".to_string());
                self.unexpected(at, &found, "a quoted string or bare text");
                self.scan_bare_text(Stops::VALUE);
            }
            _ => self.scan_bare_text(Stops::VALUE),
        }
    }

    /// `bare_text` (SPEC §3.5): the maximal run of characters up to a line
    /// terminator, a comment, a depth-0 separator or the closing bracket of
    /// the enclosing list. Brackets inside it must be balanced and correctly
    /// typed; because a bare text never contains a line terminator, an
    /// unclosed one is E203 at the end of the physical line rather than a
    /// continuation.
    fn scan_bare_text(&mut self, stops: Stops) {
        self.skip_inline();
        let start = self.pos;
        // SPEC §3.2: an unquoted value MUST NOT begin with `//`. The comment
        // rule does not fire when the `//` follows the `:` with no space, so
        // the rule is stated here in its own right: one spelling, one meaning,
        // whether or not the author left a space.
        if self
            .text
            .get(start..)
            .is_some_and(|rest| rest.starts_with("//"))
        {
            self.unexpected(
                start,
                "'//'",
                "a value; quote a value that begins with '//'",
            );
            self.bump();
            self.bump();
        }
        let mut local: Vec<(char, usize)> = Vec::new();
        let mut stray_brace = false;
        while let Some(ch) = self.peek() {
            if ch == '\n' || ch == '\r' || self.at_comment_start() {
                break;
            }
            if local.is_empty() {
                if stops.comma && ch == ',' {
                    break;
                }
                if ch == '@'
                    && (stops.at_sign || (stops.annotations && self.at_annotation_suffix()))
                {
                    break;
                }
            }
            match ch {
                '(' => {
                    local.push(('(', self.pos));
                    self.bump();
                }
                // SPEC §3.6's block-brace rule governs `{` as a token; inside
                // a value's `bare_text` run `{` and `}` are ordinary
                // characters that raise and lower the run's own bracket depth,
                // which is what makes the depth-0 comma of a brace pattern
                // part of the value (SPEC §3.5, §5.8).
                // SPEC §3.1 makes `${` one lexeme, so its `{` is neither a
                // block brace nor a multi-path `{`. It is tracked apart so that
                // an unterminated one leaves the text intact for interpolation
                // to report as E427 (SPEC §5.11), rather than E203.
                '{' if dollar_run_is_odd(self.text, start, self.pos) => {
                    local.push((INTERPOLATION_BRACE, self.pos));
                    self.bump();
                }
                '{' => {
                    local.push(('{', self.pos));
                    self.bump();
                }
                '}' => {
                    if !self.close_local_brace(&mut local, &mut stray_brace) {
                        break;
                    }
                }
                '[' if stops.square => {
                    local.push(('[', self.pos));
                    self.bump();
                }
                ')' | ']' if stops.square || ch == ')' => {
                    if !self.close_local_bracket(&mut local, ch) {
                        break;
                    }
                }
                // SPEC §3.5 fixes the character set of bare text: every
                // Unicode scalar value except a C0 control, U+007F and a C1
                // control. Non-ASCII whitespace is an ordinary character of
                // the value, so U+00A0 and U+2028 pass here and only the
                // control set is refused.
                // SPEC §2.2: only a BOM at offset 0 is removed; anywhere else
                // it is E210 outside a quoted string, inside a value as well.
                _ if (is_control_scalar(ch) && ch != '\t') || ch == BOM_CHAR => {
                    let at = self.pos;
                    let found = describe_char(ch);
                    self.unexpected(at, &found, "a value character");
                    self.bump();
                }
                _ => self.bump(),
            }
            if local.len() >= BRACKET_DEPTH {
                self.report_depth_limit();
                break;
            }
        }
        // An unterminated `${` is not an unclosed bracket: the text reaches
        // interpolation, which reports E427 (SPEC §5.11, §10.4).
        if let Some(&(open, offset)) = local
            .iter()
            .rev()
            .find(|(open, _)| *open != INTERPOLATION_BRACE)
        {
            let position = self.source.position_at(offset);
            self.error(
                ErrorId::E203,
                offset,
                format!(
                    "Unclosed '{open}' opened at {}:{}.",
                    position.line, position.col
                ),
            );
        }
        let raw = self.text.get(start..self.pos).unwrap_or("");
        let trimmed = raw.trim_end_matches([' ', '\t']);
        if trimmed.is_empty() {
            return;
        }
        let end = start + trimmed.len();
        // SPEC §3.5: E211 is reported wherever a numeric literal appears,
        // including in a value position whose type is decided later.
        if is_int_literal(trimmed) && trimmed.parse::<i64>().is_err() {
            self.error(
                ErrorId::E211,
                start,
                format!("'{trimmed}' is not a valid int literal."),
            );
        } else if is_float_literal(trimmed)
            && !trimmed.parse::<f64>().map(f64::is_finite).unwrap_or(false)
        {
            self.error(
                ErrorId::E211,
                start,
                format!("'{trimmed}' is not a valid float literal."),
            );
        }
        self.emit(TokenKind::BareText(trimmed.to_string()), start, end);
    }

    /// Closes a `}` inside a bare text. Returns false when the `}` belongs to
    /// an open block brace and the run therefore ends at it.
    ///
    /// A `}` that closes nothing at all is E205 (SPEC §3.6) and is consumed as
    /// an ordinary character rather than ending the run. While a block brace is
    /// open the `}` is that block's closer instead, so the run stops there and
    /// the parser reports E210 unless the `}` is the first token of its line.
    fn close_local_brace(&mut self, local: &mut Vec<(char, usize)>, reported: &mut bool) -> bool {
        match local.last().copied() {
            Some(('{', _)) | Some((INTERPOLATION_BRACE, _)) => {
                local.pop();
                self.bump();
            }
            Some((open, offset)) => {
                let position = self.source.position_at(offset);
                let expected = closing_of(open);
                let at = self.pos;
                self.error(
                    ErrorId::E204,
                    at,
                    format!(
                        "Mismatched bracket: expected '{expected}' to close '{}' opened at {}:{}, found '}}'.",
                        spell_open(open), position.line, position.col
                    ),
                );
                local.pop();
                self.bump();
            }
            None => {
                if !self.blocks.is_empty() {
                    return false;
                }
                // One run reports one stray brace: a value made of nothing but
                // closing braces is one defect, not one per character.
                if !*reported {
                    *reported = true;
                    let at = self.pos;
                    self.error(ErrorId::E205, at, "Unexpected '}'.".to_string());
                }
                self.bump();
            }
        }
        true
    }

    /// Closes one bracket inside a bare text. Returns false when the bracket
    /// belongs to the enclosing scanner and ends the run.
    fn close_local_bracket(&mut self, local: &mut Vec<(char, usize)>, close: char) -> bool {
        let expected_open = if close == ')' { '(' } else { '[' };
        match local.last().copied() {
            None => false,
            Some((open, _)) if open == expected_open => {
                local.pop();
                self.bump();
                true
            }
            Some((open, offset)) => {
                let position = self.source.position_at(offset);
                let expected = closing_of(open);
                let at = self.pos;
                self.error(
                    ErrorId::E204,
                    at,
                    format!(
                        "Mismatched bracket: expected '{expected}' to close '{}' opened at {}:{}, found '{close}'.",
                        spell_open(open), position.line, position.col
                    ),
                );
                local.pop();
                self.bump();
                true
            }
        }
    }
}

impl Lexer<'_> {
    // ----------------------------------------------------------- annotations

    /// True when a trailing `@since(n)` / `@removed(n)` run starts at the
    /// cursor and reaches the end of the logical line (SPEC §5.13). The
    /// annotation is removed from the statement before its value is formed,
    /// so `note: deprecated @removed(3)` has the value `deprecated`.
    fn at_annotation_suffix(&self) -> bool {
        if self.peek() != Some('@') {
            return false;
        }
        if !matches!(
            self.previous_char(),
            None | Some(' ') | Some('\t') | Some('\n') | Some('\r')
        ) {
            return false;
        }
        let mut probe = self.pos;
        let mut matched = 0;
        while matched < 2 {
            let Some(next) = self.match_annotation(probe) else {
                break;
            };
            probe = next;
            matched += 1;
            let mut after = probe;
            while matches!(self.char_at(after), Some(' ') | Some('\t')) {
                after += 1;
            }
            if self.char_at(after) == Some('@') && matched < 2 {
                probe = after;
            } else {
                break;
            }
        }
        if matched == 0 {
            return false;
        }
        let mut after = probe;
        while matches!(self.char_at(after), Some(' ') | Some('\t')) {
            after += 1;
        }
        if self.char_at(after) == Some('/') && self.char_at(after + 1) == Some('/') {
            return true;
        }
        matches!(self.char_at(after), None | Some('\n') | Some('\r'))
    }

    /// The offset just past `@since(<uint>)` or `@removed(<uint>)` at
    /// `offset`. The `(` MUST be glued to the keyword, which is what
    /// distinguishes the annotation from the header tag `@since` (SPEC §3.1).
    fn match_annotation(&self, offset: usize) -> Option<usize> {
        if self.char_at(offset) != Some('@') {
            return None;
        }
        let name_start = offset + 1;
        let name_end = self.word_end(name_start);
        let name = self.text.get(name_start..name_end)?;
        if name != "since" && name != "removed" {
            return None;
        }
        if self.char_at(name_end) != Some('(') {
            return None;
        }
        let digits_start = name_end + 1;
        let mut probe = digits_start;
        while self
            .char_at(probe)
            .map(|ch| ch.is_ascii_digit())
            .unwrap_or(false)
        {
            probe += 1;
        }
        if probe == digits_start || self.char_at(probe) != Some(')') {
            return None;
        }
        Some(probe + 1)
    }

    /// Emits the trailing annotations of a body statement as ordinary tokens.
    fn scan_annotations(&mut self) {
        loop {
            self.skip_inline();
            if self.match_annotation(self.pos).is_none() {
                break;
            }
            // `@`, the keyword, `(`, the version and `)`.
            for _ in 0..5 {
                self.next_structural_token();
            }
        }
    }

    // ---------------------------------------------------------- header tags

    /// The header tag list and the instance version window that may follow it
    /// (SPEC §5.2, §5.14).
    fn scan_header_tags(&mut self) {
        loop {
            self.skip_inline();
            if self.at_end() {
                break;
            }
            if self.at_line_break() {
                if self.brackets.is_empty() {
                    break;
                }
                self.consume_line_break();
                continue;
            }
            if self.brackets.is_empty() {
                match self.peek() {
                    Some('@') => {
                        self.scan_header_tag();
                        continue;
                    }
                    Some(',') => {
                        let at = self.pos;
                        self.emit_punctuation(",", at);
                        self.continue_after_header_comma();
                        continue;
                    }
                    _ => {}
                }
            }
            self.next_structural_token();
        }
    }

    fn scan_header_tag(&mut self) {
        let at = self.pos;
        self.emit_punctuation("@", at);
        self.require_glued_name();
        match self.peek() {
            Some(ch) if is_word_char(ch) => self.scan_word(false),
            _ => return,
        }
        if self.last_end != self.pos {
            return;
        }
        match self.peek() {
            // `@since(2)` — an annotation; the rest is scanned structurally.
            Some('(') => {
                let open = self.pos;
                self.push_bracket('(', open);
                self.emit_punctuation("(", open);
            }
            // `@name.value` — the value runs to the next depth-0 `,` or `@`.
            Some('.') => {
                let dot = self.pos;
                self.emit_punctuation(".", dot);
                match self.peek() {
                    Some('"') => self.scan_quoted_string(),
                    _ => self.scan_bare_text(Stops::HEADER),
                }
            }
            _ => {}
        }
    }

    /// SPEC §3.6 rule (a): a header tag list continues onto the next physical
    /// line after a trailing `,`. The continuation is taken only when another
    /// tag follows, so that a `,` followed by anything else stays at the end
    /// of its line and the parser reports E442.
    fn continue_after_header_comma(&mut self) {
        let mut probe = self.pos;
        loop {
            match self.char_at(probe) {
                Some(' ') | Some('\t') => probe += 1,
                Some('/') if self.char_at(probe + 1) == Some('/') => {
                    while !matches!(self.char_at(probe), None | Some('\n') | Some('\r')) {
                        probe = self.step(probe);
                    }
                }
                _ => break,
            }
        }
        if !matches!(self.char_at(probe), Some('\n') | Some('\r')) {
            return;
        }
        loop {
            match self.char_at(probe) {
                Some(' ') | Some('\t') | Some('\n') | Some('\r') => probe += 1,
                Some('/') if self.char_at(probe + 1) == Some('/') => {
                    while !matches!(self.char_at(probe), None | Some('\n') | Some('\r')) {
                        probe = self.step(probe);
                    }
                }
                _ => break,
            }
        }
        if self.char_at(probe) == Some('@') {
            self.pos = probe;
            self.last_end = usize::MAX;
        }
    }

    /// The offset of the character after the one at `offset`.
    fn step(&self, offset: usize) -> usize {
        match self.char_at(offset) {
            Some(ch) => offset + ch.len_utf8(),
            None => offset + 1,
        }
    }

    // --------------------------------------------------------- derive values

    /// The right-hand side of a `derive` (SPEC §6.4). It is a path, a
    /// `length(…)` call, the `version` built-in or a loop variable when its
    /// first token says so; in every other case it is an ordinary value.
    fn scan_derive_value(&mut self) {
        self.skip_inline();
        if self.derive_right_hand_side_is_structural() {
            loop {
                self.skip_inline();
                if self.at_end() {
                    break;
                }
                if self.at_line_break() {
                    if self.brackets.is_empty() {
                        break;
                    }
                    self.consume_line_break();
                    continue;
                }
                self.next_structural_token();
            }
            return;
        }
        self.scan_value_items(0, false);
    }

    fn derive_right_hand_side_is_structural(&self) -> bool {
        match self.peek() {
            Some('.') => true,
            // `$name` is a loop variable; `${` and `$$` belong to a value.
            Some('$') => self.peek_at(1).map(is_ident_start).unwrap_or(false),
            Some(ch) if is_word_char(ch) => {
                let end = self.word_end(self.pos);
                let word = self.text.get(self.pos..end).unwrap_or("");
                if word == "length" {
                    return self.char_at(end) == Some('(');
                }
                if word == "version" {
                    let mut after = end;
                    while matches!(self.char_at(after), Some(' ') | Some('\t')) {
                        after += 1;
                    }
                    if self.char_at(after) == Some('/') && self.char_at(after + 1) == Some('/') {
                        return true;
                    }
                    return matches!(self.char_at(after), None | Some('\n') | Some('\r'));
                }
                false
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::ErrorId;

    /// A compact spelling of a token, so a test can state a whole stream.
    fn spell(token: &Token) -> String {
        match &token.kind {
            TokenKind::Identifier(text) => text.clone(),
            TokenKind::SchemaName(text) => format!("S:{text}"),
            TokenKind::Punctuation(text) => (*text).to_string(),
            TokenKind::IntLiteral(text) => format!("i:{text}"),
            TokenKind::FloatLiteral(text) => format!("f:{text}"),
            TokenKind::QuotedString(text) => format!("q:{text}"),
            TokenKind::BareText(text) => format!("b:{text}"),
            TokenKind::SizeToken(text) => format!("z:{text}"),
            TokenKind::Newline => "NL".to_string(),
            TokenKind::EndOfFile => "EOF".to_string(),
        }
    }

    pub(super) fn lex(path: &str, text: &str) -> Vec<String> {
        let source = SourceFile::new(path, text);
        let tokens = tokenize(&source)
            .unwrap_or_else(|errors| panic!("unexpected diagnostics for {path}:\n{errors}"));
        assert!(
            tokens.last().map(Token::is_end_of_file).unwrap_or(false),
            "the stream always ends with EndOfFile"
        );
        assert!(
            !tokens
                .windows(2)
                .any(|pair| pair[0].is_newline() && pair[1].is_newline()),
            "consecutive NL tokens collapse to one"
        );
        tokens.iter().map(spell).collect()
    }

    fn lex_tokens(path: &str, text: &str) -> Vec<Token> {
        let source = SourceFile::new(path, text);
        tokenize(&source).unwrap_or_else(|errors| panic!("unexpected diagnostics:\n{errors}"))
    }

    pub(super) fn errors(path: &str, text: &str) -> Vec<ErrorId> {
        let source = SourceFile::new(path, text);
        match tokenize(&source) {
            Ok(tokens) => panic!("expected diagnostics, got {:?}", tokens.len()),
            Err(diagnostics) => diagnostics.iter().map(|item| item.id).collect(),
        }
    }

    fn first_error(path: &str, text: &str) -> Diagnostic {
        let source = SourceFile::new(path, text);
        match tokenize(&source) {
            Ok(_) => panic!("expected a diagnostic"),
            Err(diagnostics) => diagnostics.first().cloned().expect("one diagnostic"),
        }
    }

    // ------------------------------------------------------------ §3.3, §3.4

    #[test]
    fn normalisation_folds_case_and_hyphens() {
        assert_eq!(normalise("Max-Count"), "max_count");
        assert_eq!(normalise("MAX_COUNT"), "max_count");
        assert_eq!(normalise("07"), "07");
    }

    #[test]
    fn identifier_rules_follow_the_specification() {
        assert!(is_identifier("a"));
        assert!(is_identifier("_x-1"));
        assert!(is_identifier("07"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("-a"));
        assert!(!is_identifier("a-"));
        assert!(!is_identifier("tamaño"));
    }

    #[test]
    fn schema_names_reject_hyphens_dots_and_leading_digits() {
        assert!(is_schema_name("Product"));
        assert!(is_schema_name("A1_b"));
        assert!(!is_schema_name("1Product"));
        assert!(!is_schema_name("My-Thing"));
        assert!(!is_schema_name("My.Thing"));
        assert!(!is_schema_name(""));
    }

    #[test]
    fn literal_shapes_follow_the_specification() {
        assert!(is_int_literal("-7"));
        assert!(is_int_literal("007"));
        assert!(!is_int_literal("+7"));
        assert!(!is_int_literal("1_000"));
        assert!(is_float_literal("1.5"));
        assert!(is_float_literal("-1.5e-3"));
        assert!(is_float_literal("2e10"));
        assert!(!is_float_literal(".5"));
        assert!(!is_float_literal("5."));
        assert!(!is_float_literal("1.2.3"));
    }

    // ------------------------------------------------------------------ §2.2

    #[test]
    fn a_bom_is_stripped_and_crlf_lines_carry_ordinary_positions() {
        let source = SourceFile::new("a.ab", "\u{feff}Thing :: @id.x\r\nname: y\r\n");
        let tokens = tokenize(&source).expect("lexes");
        assert_eq!(tokens[0].position, Position::new(1, 1));
        let name = tokens
            .iter()
            .find(|token| token.is_keyword("name"))
            .expect("the second line is lexed");
        assert_eq!(name.position, Position::new(2, 1));
    }

    #[test]
    fn a_lone_carriage_return_is_rejected() {
        assert_eq!(errors("a.ab", "name: x\ry"), [ErrorId::E210]);
    }

    // ------------------------------------------------------------------ §3.2

    #[test]
    fn comments_start_only_at_a_line_start_or_after_whitespace() {
        assert_eq!(
            lex(
                "a.ab",
                "// whole line\nlink: https://example.com  // trailing\n"
            ),
            ["link", ":", "b:https://example.com", "NL", "EOF"]
        );
    }

    #[test]
    fn a_double_slash_inside_a_quoted_string_is_not_a_comment() {
        assert_eq!(
            lex(
                "a.ab",
                "note: \"// not a comment\"\ncdn: \"//cdn.example.com/x.png\"\n"
            ),
            [
                "note",
                ":",
                "q:// not a comment",
                "NL",
                "cdn",
                ":",
                "q://cdn.example.com/x.png",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_value_emptied_by_a_comment_leaves_no_value_token() {
        assert_eq!(lex("a.ab", "name: // gone\n"), ["name", ":", "NL", "EOF"]);
    }

    // ------------------------------------------------------------------ §3.5

    #[test]
    fn quoted_strings_decode_the_five_escapes() {
        let tokens = lex_tokens("a.ab", "note: \"a\\\"b\\\\c\\nd\\re\\tf\"\n");
        let value = tokens
            .iter()
            .find_map(|token| match &token.kind {
                TokenKind::QuotedString(text) => Some(text.clone()),
                _ => None,
            })
            .expect("a quoted string");
        assert_eq!(value, "a\"b\\c\nd\re\tf");
    }

    #[test]
    fn an_unterminated_string_is_e201_and_an_unknown_escape_is_e202() {
        assert_eq!(errors("a.ab", "note: \"hello\n"), [ErrorId::E201]);
        assert_eq!(errors("a.ab", "path: \"C:\\pack\"\n"), [ErrorId::E202]);
    }

    #[test]
    fn a_range_never_swallows_the_dots_of_a_float() {
        assert_eq!(
            lex("a.abt", "versions 1..2\n"),
            ["versions", "i:1", "..", "i:2", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.abt", "schema A {\n  x: float(1.5..2.5)\n}\n"),
            [
                "schema", "S:A", "{", "NL", "x", ":", "float", "(", "f:1.5", "..", "f:2.5", ")",
                "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn an_out_of_range_integer_is_e211_wherever_it_appears() {
        assert_eq!(
            errors("a.ab", "count: 99999999999999999999\n"),
            [ErrorId::E211]
        );
        assert_eq!(
            errors("a.abt", "schema A {\n  x: int(99999999999999999999)\n}\n"),
            [ErrorId::E211]
        );
        assert_eq!(errors("a.ab", "x: 1e400\n"), [ErrorId::E211]);
    }

    // ------------------------------------------------------------ §3.3 E2xx

    #[test]
    fn a_non_ascii_identifier_is_e206_and_a_trailing_hyphen_is_e207() {
        let bad = first_error("a.ab", "TAMA\u{d1}O: 3\n");
        assert_eq!(bad.id, ErrorId::E206);
        assert!(bad.message.contains("TAMA\u{d1}O"), "{}", bad.message);
        assert_eq!(errors("a.ab", "size-: 3\n"), [ErrorId::E207]);
    }

    #[test]
    fn a_malformed_schema_name_is_e208() {
        assert_eq!(errors("a.abt", "schema My-Thing {\n}\n"), [ErrorId::E208]);
    }

    // ------------------------------------------------------------------ §3.6

    #[test]
    fn an_open_bracket_joins_lines_and_a_block_brace_does_not() {
        assert_eq!(
            lex("a.ab", "tags: [\n    core,\n    public\n]\n"),
            ["tags", ":", "[", "b:core", ",", "b:public", "]", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.abt", "schema Label {\n    caption: text\n}\n"),
            ["schema", "S:Label", "{", "NL", "caption", ":", "text", "NL", "}", "NL", "EOF"]
        );
    }

    #[test]
    fn a_trailing_comma_at_depth_zero_does_not_continue_the_statement() {
        assert_eq!(
            lex("a.ab", "tags: core,\npublic\n"),
            ["tags", ":", "b:core", ",", "NL", "public", "NL", "EOF"]
        );
    }

    #[test]
    fn a_header_tag_list_continues_after_a_trailing_comma() {
        assert_eq!(
            lex("a.ab", "Product :: @id.atlas,\n           @status.active\n"),
            [
                "S:Product",
                "::",
                "@",
                "id",
                ".",
                "b:atlas",
                ",",
                "@",
                "status",
                ".",
                "b:active",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_header_comma_that_no_tag_follows_stays_at_the_end_of_its_line() {
        assert_eq!(
            lex("a.ab", "Product :: @id.atlas,\nname: x\n"),
            [
                "S:Product",
                "::",
                "@",
                "id",
                ".",
                "b:atlas",
                ",",
                "NL",
                "name",
                ":",
                "b:x",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn require_and_else_throw_may_be_written_on_two_lines() {
        assert_eq!(
            lex(
                "a.abt",
                "logic P {\n    require .a exists\n        else throw \"x\"\n}\n"
            ),
            [
                "logic", "S:P", "{", "NL", "require", ".", "a", "exists", "else", "throw", "q:x",
                "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_closing_brace_and_a_following_else_may_be_written_on_two_lines() {
        let tokens = lex(
            "a.abt",
            "logic P {\n    if .a == true {\n        derive .b = 1\n    }\n    else {\n        derive .b = 2\n    }\n}\n",
        );
        let joined = tokens.join(" ");
        assert!(joined.contains("} else {"), "{joined}");
    }

    // ------------------------------------------------------------ §3.6 E2xx

    #[test]
    fn unbalanced_brackets_are_reported_with_their_position() {
        assert_eq!(errors("a.ab", "tags: [core\n"), [ErrorId::E203]);
        assert_eq!(errors("a.ab", "tags: [core)\n"), [ErrorId::E204]);
        assert_eq!(errors("a.ab", "tags: core]\n"), [ErrorId::E205]);
        let unclosed = first_error("a.ab", "tags: [core\n");
        assert_eq!(unclosed.position, Some(Position::new(1, 7)));
        assert!(
            unclosed.message.contains("opened at 1:7"),
            "{}",
            unclosed.message
        );
    }

    #[test]
    fn an_unclosed_bracket_inside_bare_text_ends_at_the_line_and_is_e203() {
        assert_eq!(errors("a.ab", "desc: f(a\nname: x\n"), [ErrorId::E203]);
    }

    #[test]
    fn a_bracket_nesting_depth_over_the_limit_is_e209() {
        let mut text = String::from("tags: ");
        for _ in 0..70 {
            text.push('[');
        }
        text.push('x');
        for _ in 0..70 {
            text.push(']');
        }
        text.push('\n');
        assert!(errors("a.ab", &text).contains(&ErrorId::E209));
    }

    // ------------------------------------------------------------------ §5.1

    #[test]
    fn a_double_colon_inside_a_value_is_ordinary_text() {
        assert_eq!(
            lex("a.ab", "window: 12::30\n"),
            ["window", ":", "b:12::30", "NL", "EOF"]
        );
    }

    #[test]
    fn a_header_needs_a_schema_name_before_the_double_colon() {
        assert_eq!(
            lex("a.ab", "my-thing :: @id.x\n"),
            ["my-thing", "::", "@", "id", ".", "b:x", "NL", "EOF"]
        );
    }

    #[test]
    fn a_statement_named_data_is_an_ordinary_assignment() {
        assert_eq!(
            lex("a.ab", "Thing :: @id.a\ndata: 1\nname: x\n"),
            [
                "S:Thing", "::", "@", "id", ".", "b:a", "NL", "data", ":", "b:1", "NL", "name",
                ":", "b:x", "NL", "EOF"
            ]
        );
    }

    // ------------------------------------------------------------------ §5.2

    #[test]
    fn a_header_tag_value_ends_at_the_next_comma_or_at_sign() {
        assert_eq!(
            lex("a.ab", "Item :: @id.lantern @since(2)\n"),
            [
                "S:Item",
                "::",
                "@",
                "id",
                ".",
                "b:lantern",
                "@",
                "since",
                "(",
                "i:2",
                ")",
                "NL",
                "EOF"
            ]
        );
        assert_eq!(
            lex("a.ab", "Item :: @since.2\n"),
            ["S:Item", "::", "@", "since", ".", "b:2", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.ab", "Item :: @label.\"hi, there @world\"\n"),
            [
                "S:Item",
                "::",
                "@",
                "label",
                ".",
                "q:hi, there @world",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_header_tag_value_keeps_its_own_leading_dot() {
        assert_eq!(
            lex("a.ab", "Item :: @icon../textures/a.png\n"),
            [
                "S:Item",
                "::",
                "@",
                "icon",
                ".",
                "b:./textures/a.png",
                "NL",
                "EOF"
            ]
        );
    }

    // ------------------------------------------------------------ §5.4, §5.5

    #[test]
    fn a_multi_path_key_list_is_the_one_brace_that_joins_lines() {
        assert_eq!(
            lex("a.ab", "limits.{soft,\n hard}: 10\n"),
            ["limits", ".", "{", "soft", ",", "hard", "}", ":", "b:10", "NL", "EOF"]
        );
    }

    #[test]
    fn a_body_block_opens_and_closes_a_block_brace() {
        assert_eq!(
            lex("a.ab", "owner {\n    team: Knowledge Systems\n}\n"),
            [
                "owner",
                "{",
                "NL",
                "team",
                ":",
                "b:Knowledge Systems",
                "NL",
                "}",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn tag_objects_carry_named_arguments_and_flags() {
        assert_eq!(
            lex(
                "a.ab",
                "capabilities: [#search, #export(limits.soft: 10, featured)]\n"
            ),
            [
                "capabilities",
                ":",
                "[",
                "#",
                "search",
                ",",
                "#",
                "export",
                "(",
                "limits",
                ".",
                "soft",
                ":",
                "b:10",
                ",",
                "featured",
                ")",
                "]",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn tuple_rows_are_lexed_as_rows_of_cells() {
        assert_eq!(
            lex(
                "a.ab",
                "copy(key, value): (en_us, Welcome), (es_es, Bienvenido)\n"
            ),
            [
                "copy",
                "(",
                "key",
                ",",
                "value",
                ")",
                ":",
                "(",
                "b:en_us",
                ",",
                "b:Welcome",
                ")",
                ",",
                "(",
                "b:es_es",
                ",",
                "b:Bienvenido",
                ")",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_bracketed_tuple_list_spreads_rows_over_lines() {
        assert_eq!(
            lex(
                "a.ab",
                "copy(key, value): [\n    (en_us, Welcome),\n    (es_es, Bienvenido),\n]\n"
            ),
            [
                "copy",
                "(",
                "key",
                ",",
                "value",
                ")",
                ":",
                "[",
                "(",
                "b:en_us",
                ",",
                "b:Welcome",
                ")",
                ",",
                "(",
                "b:es_es",
                ",",
                "b:Bienvenido",
                ")",
                ",",
                "]",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_comma_at_depth_zero_always_separates_items() {
        assert_eq!(
            lex("a.ab", "caption: Hello, world\n"),
            ["caption", ":", "b:Hello", ",", "b:world", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.ab", "caption: \"Hello, world\"\n"),
            ["caption", ":", "q:Hello, world", "NL", "EOF"]
        );
    }

    #[test]
    fn a_brace_group_raises_the_bare_text_bracket_depth() {
        // SPEC §5.8's own example: the comma inside the group belongs to the
        // value, so the whole pattern is one bare text.
        assert_eq!(
            lex("a.ab", "images: ./textures/{hero,thumbnail}.png\n"),
            [
                "images",
                ":",
                "b:./textures/{hero,thumbnail}.png",
                "NL",
                "EOF"
            ]
        );
        assert_eq!(
            lex("a.ab", "tags: ./art/{red,blue}/{small,large}.png\n"),
            [
                "tags",
                ":",
                "b:./art/{red,blue}/{small,large}.png",
                "NL",
                "EOF"
            ]
        );
        // A comma outside every group still separates items.
        assert_eq!(
            lex("a.ab", "tags: {a,b}.png, c.png\n"),
            ["tags", ":", "b:{a,b}.png", ",", "b:c.png", "NL", "EOF"]
        );
    }

    // ----------------------------------------------------------------- §5.13

    #[test]
    fn a_trailing_annotation_is_removed_before_the_value_is_formed() {
        assert_eq!(
            lex("a.ab", "note: deprecated @removed(3)\n"),
            [
                "note",
                ":",
                "b:deprecated",
                "@",
                "removed",
                "(",
                "i:3",
                ")",
                "NL",
                "EOF"
            ]
        );
        assert_eq!(
            lex("a.ab", "glow: true               @since(2)\n"),
            ["glow", ":", "b:true", "@", "since", "(", "i:2", ")", "NL", "EOF"]
        );
    }

    #[test]
    fn a_value_may_end_with_text_that_only_looks_like_an_annotation() {
        assert_eq!(
            lex("a.ab", "note: deprecated @since\n"),
            ["note", ":", "b:deprecated @since", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.ab", "note: \"deprecated @since(2)\"\n"),
            ["note", ":", "q:deprecated @since(2)", "NL", "EOF"]
        );
    }

    // -------------------------------------------------------------- chapter 4

    #[test]
    fn a_schema_default_is_a_value_region_and_a_type_expression_is_not() {
        assert_eq!(
            lex(
                "a.abt",
                "schema A {\n    tags[2..4]: enum(core, public) @optional = [core, public]\n}\n"
            ),
            [
                "schema", "S:A", "{", "NL", "tags", "[", "i:2", "..", "i:4", "]", ":", "enum", "(",
                "core", ",", "public", ")", "@", "optional", "=", "[", "b:core", ",", "b:public",
                "]", "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_modifier_written_after_a_default_stays_in_the_default_text() {
        assert_eq!(
            lex("a.abt", "schema A {\n    glow: bool = false @since(2)\n}\n"),
            [
                "schema",
                "S:A",
                "{",
                "NL",
                "glow",
                ":",
                "bool",
                "=",
                "b:false @since(2)",
                "NL",
                "}",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn an_image_size_token_is_one_lexeme_only_after_an_extension() {
        assert_eq!(
            lex(
                "a.abt",
                "schema A {\n    icon: image(png 128x128, jpg 1024x*)\n}\n"
            ),
            [
                "schema",
                "S:A",
                "{",
                "NL",
                "icon",
                ":",
                "image",
                "(",
                "png",
                "z:128x128",
                ",",
                "jpg",
                "z:1024x*",
                ")",
                "NL",
                "}",
                "NL",
                "EOF"
            ]
        );
        assert_eq!(
            lex("a.abt", "schema A {\n    icon: image(png128x128)\n}\n"),
            [
                "schema",
                "S:A",
                "{",
                "NL",
                "icon",
                ":",
                "image",
                "(",
                "png128x128",
                ")",
                "NL",
                "}",
                "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn nested_schema_and_ref_types_name_a_schema() {
        assert_eq!(
            lex(
                "a.abt",
                "schema A {\n    owner: $(Owner)\n    pack: ref(Pack)\n}\n"
            ),
            [
                "schema", "S:A", "{", "NL", "owner", ":", "$", "(", "S:Owner", ")", "NL", "pack",
                ":", "ref", "(", "S:Pack", ")", "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_numbered_key_arrives_as_an_integer_literal() {
        let tokens = lex_tokens(
            "a.abt",
            "schema A {\n    slots {\n        1: $(Slot)\n    }\n}\n",
        );
        let numbered = tokens
            .iter()
            .find(|token| matches!(&token.kind, TokenKind::IntLiteral(text) if text == "1"))
            .expect("the field name 1");
        assert_eq!(numbered.identifier_text(), Some("1"));
    }

    // -------------------------------------------------------------- chapter 6

    #[test]
    fn logic_conditions_lex_every_operator_spelling() {
        assert_eq!(
            lex(
                "a.abt",
                "logic P {\n    require not .a == 1 and .b != 2 or !(.c >= 3 && .d <= 4 || .e > 5)\n        else throw \"x\"\n}\n"
            ),
            [
                "logic", "S:P", "{", "NL", "require", "not", ".", "a", "==", "i:1", "and", ".",
                "b", "!=", "i:2", "or", "!", "(", ".", "c", ">=", "i:3", "&&", ".", "d", "<=",
                "i:4", "||", ".", "e", ">", "i:5", ")", "else", "throw", "q:x", "NL", "}", "NL",
                "EOF"
            ]
        );
    }

    #[test]
    fn a_derive_right_hand_side_is_a_path_a_call_a_builtin_or_a_value() {
        assert_eq!(
            lex(
                "a.abt",
                "logic P {\n    derive .a = .owner.team\n    derive? .b = length(.tags)\n    derive .c = version\n    derive .d = $slot\n    derive .e = textures/${id}.png\n    derive .f = standard\n}\n"
            ),
            [
                "logic", "S:P", "{", "NL",
                "derive", ".", "a", "=", ".", "owner", ".", "team", "NL",
                "derive", "?", ".", "b", "=", "length", "(", ".", "tags", ")", "NL",
                "derive", ".", "c", "=", "version", "NL",
                "derive", ".", "d", "=", "$", "slot", "NL",
                "derive", ".", "e", "=", "b:textures/${id}.png", "NL",
                "derive", ".", "f", "=", "b:standard", "NL",
                "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_for_loop_over_a_literal_list_lexes_its_elements_as_values() {
        assert_eq!(
            lex(
                "a.abt",
                "logic P {\n    for $x in [1, red, \"a\"] {\n    }\n}\n"
            ),
            [
                "logic", "S:P", "{", "NL", "for", "$", "x", "in", "[", "b:1", ",", "b:red", ",",
                "q:a", "]", "{", "NL", "}", "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_for_loop_over_a_path_stays_structural() {
        assert_eq!(
            lex("a.abt", "logic P {\n    for $x in .tags {\n    }\n}\n"),
            [
                "logic", "S:P", "{", "NL", "for", "$", "x", "in", ".", "tags", "{", "NL", "}",
                "NL", "}", "NL", "EOF"
            ]
        );
    }

    #[test]
    fn a_question_mark_is_punctuation_only_after_derive() {
        assert_eq!(
            errors("a.abt", "logic P {\n    derive ? .a = 1\n}\n"),
            [ErrorId::E210]
        );
    }

    // ------------------------------------------------------------- adjacency

    #[test]
    fn adjacency_is_recorded_and_whitespace_inside_a_lexeme_is_e210() {
        let tokens = lex_tokens(
            "a.abt",
            "schema A {\n    x: ref(Pack)\n    y: ref (Pack)\n}\n",
        );
        let glued = tokens
            .iter()
            .filter(|token| token.is_punctuation("("))
            .map(|token| token.glued)
            .collect::<Vec<_>>();
        assert_eq!(glued, [true, false]);
        assert_eq!(errors("a.ab", "Item :: @ id.x\n"), [ErrorId::E210]);
    }

    #[test]
    fn indentation_carries_no_meaning() {
        let flat = lex("a.ab", "Thing :: @id.a\nname: x\n");
        let indented = lex("a.ab", "\tThing :: @id.a\n        name: x\n");
        assert_eq!(flat, indented);
    }

    // ---------------------------------------------------------- never panics

    #[test]
    fn hostile_input_never_panics() {
        let inputs = [
            "", "\n\n\n", ":", "::", "}", "{", "#", "@", "&", "$", "?", "|", "a: [", "a: (",
            "a: #", "a: #(", "a(", "a(): ()", "a.{}: 1", "\u{feff}", "a: \"\\", "a: 1e", "-",
            "a: ,,,", "a: [,,]",
        ];
        for input in inputs {
            for path in ["a.ab", "a.abt"] {
                let source = SourceFile::new(path, input);
                let _ = tokenize(&source);
            }
        }
    }

    #[test]
    fn a_value_at_exactly_the_bracket_limit_is_lexed() {
        // SPEC §3.7 reports E209 only when a limit is exceeded, so a value with
        // 64 nested brackets is lexed and left to the parser, which reports the
        // nested list it is (E441).
        let at_limit = format!(
            "P ::\n    tags: {}a{}\n",
            "[".repeat(BRACKET_DEPTH),
            "]".repeat(BRACKET_DEPTH)
        );
        assert!(tokenize(&SourceFile::new("a.ab", at_limit)).is_ok());
        let over = format!(
            "P ::\n    tags: {}a{}\n",
            "[".repeat(BRACKET_DEPTH + 1),
            "]".repeat(BRACKET_DEPTH + 1)
        );
        assert_eq!(errors("a.ab", &over), [ErrorId::E209]);
    }

    #[test]
    fn a_closing_bracket_of_the_wrong_type_is_a_mismatch() {
        // SPEC §3.6: types must match, in either direction.
        assert_eq!(
            errors("a.ab", "P ::\n    c(id): (search]\n"),
            [ErrorId::E204]
        );
        assert_eq!(errors("a.ab", "P ::\n    t: [core)\n"), [ErrorId::E204]);
    }

    #[test]
    fn a_byte_order_mark_inside_a_value_is_rejected() {
        // SPEC §2.2: only the BOM at offset 0 is removed; anywhere else it is
        // an ordinary character and E210 outside a quoted string.
        assert_eq!(errors("a.ab", "P ::\n    n: a\u{feff}b\n"), [ErrorId::E210]);
        assert!(tokenize(&SourceFile::new("a.ab", "\u{feff}P ::\n    n: ab\n")).is_ok());
    }

    #[test]
    fn an_unterminated_interpolation_is_not_an_unclosed_brace() {
        // SPEC §3.1 makes `${` one lexeme, so its `{` is neither a block brace
        // nor a multi-path `{`; the text reaches interpolation, which reports
        // the unterminated form as E427 (SPEC §5.11).
        assert!(tokenize(&SourceFile::new("a.ab", "P ::\n    l: ${id\n")).is_ok());
        assert!(tokenize(&SourceFile::new("a.ab", "P ::\n    l: ${id}\n")).is_ok());
        assert_eq!(errors("a.ab", "P ::\n    l: a{b\n"), [ErrorId::E203]);
    }

    #[test]
    fn trailing_text_after_a_quoted_value_is_one_lexeme() {
        // SPEC §5.5: the closing quote ends the value, so a stray '"' after it
        // never opens a second string and E201 is not reachable from there.
        let tokens = lex("a.ab", "P ::\n    n: \"Atlas\" b\"\n");
        assert!(
            tokens.iter().any(|token| token == "b:b\""),
            "the trailing run is one bare text: {tokens:?}"
        );
    }
}

/// The shapes the 0.2.0 audit found silently mis-handled, panicking or
/// swallowing input. Each one now ends in a token stream or a diagnostic.
#[cfg(test)]
mod audit_regressions {
    use super::tests::{errors, lex};
    use crate::diagnostics::ErrorId;

    #[test]
    fn a_malformed_tag_object_is_a_diagnostic_not_a_panic() {
        // LP-005: `#tag)x(` aborted the 0.2.0 compiler.
        assert!(errors("a.ab", "Thing :: @id.x\n    note: #tag)x(\n").contains(&ErrorId::E205));
    }

    #[test]
    fn out_of_order_braces_in_a_value_are_a_diagnostic() {
        // LP-006: `a}.{b` aborted brace expansion in 0.2.0. The braces are
        // ordinary characters but must balance (SPEC §3.5, §5.5): the `}`
        // closes nothing, which is E205, and the `{` is then never closed,
        // which is E203 at the end of the line.
        assert_eq!(
            errors("a.ab", "icon: a}.{b\n"),
            [ErrorId::E205, ErrorId::E203]
        );
        // Balanced braces are ordinary characters and raise no diagnostic.
        assert_eq!(
            lex("a.ab", "icon: a{b}.{c}\n"),
            ["icon", ":", "b:a{b}.{c}", "NL", "EOF"]
        );
    }

    #[test]
    fn out_of_order_braces_on_a_left_side_are_a_diagnostic() {
        // LP-007: `a}.{b: v` aborted the multi-path reader.
        // The stray `}` is E205; the `.{` it then leaves open is E203.
        assert_eq!(
            errors(
                "a.ab",
                "a}.{b: v
"
            ),
            [ErrorId::E205, ErrorId::E203]
        );
    }

    #[test]
    fn deeply_nested_tag_arguments_stop_at_the_depth_limit() {
        // LP-008: nesting overflowed the stack.
        let mut text = String::from("note: ");
        for _ in 0..200 {
            text.push_str("#t(a: ");
        }
        text.push('x');
        for _ in 0..200 {
            text.push(')');
        }
        text.push('\n');
        assert!(errors("a.ab", &text).contains(&ErrorId::E209));
    }

    #[test]
    fn a_long_dotted_path_is_lexed_without_recursion() {
        let mut text = String::new();
        for _ in 0..500 {
            text.push_str("a.");
        }
        text.push_str("b: v\n");
        let tokens = lex("a.ab", &text);
        assert_eq!(tokens.last().map(String::as_str), Some("EOF"));
    }

    #[test]
    fn a_spaced_double_slash_is_a_comment_and_a_quoted_one_is_not() {
        // LP-017: ` //` truncated protocol-relative URLs.
        assert_eq!(
            lex("a.ab", "ratio: \"50 // 2\"\n"),
            ["ratio", ":", "q:50 // 2", "NL", "EOF"]
        );
        assert_eq!(
            lex("a.ab", "ratio: 50 // 2\n"),
            ["ratio", ":", "b:50", "NL", "EOF"]
        );
    }

    #[test]
    fn a_quoted_windows_path_keeps_its_backslashes() {
        // LP-026: `\p` and friends were rewritten silently.
        assert_eq!(
            lex("a.ab", "path: \"C:\\\\pack\\\\a.png\"\n"),
            ["path", ":", "q:C:\\pack\\a.png", "NL", "EOF"]
        );
    }

    #[test]
    fn a_control_character_inside_a_quoted_string_is_preserved() {
        // LP-021: U+0001 was rewritten to `$` by an in-band sentinel.
        assert_eq!(
            lex("a.ab", "note: \"cost \u{1} 5\"\n"),
            ["note", ":", "q:cost \u{1} 5", "NL", "EOF"]
        );
        // Outside a quoted string it is not a value character.
        assert_eq!(errors("a.ab", "note: cost \u{1} 5\n"), [ErrorId::E210]);
    }

    #[test]
    fn a_trailing_comma_inside_brackets_is_allowed() {
        assert_eq!(
            lex("a.ab", "tags: [core, public,]\n"),
            ["tags", ":", "[", "b:core", ",", "b:public", ",", "]", "NL", "EOF"]
        );
    }

    #[test]
    fn every_lexical_error_in_a_file_is_collected() {
        // `--max-errors` collects across a file (SPEC §9.3), so the lexer
        // reports more than the first problem.
        let ids = errors("a.ab", "a: \"open\nb: \"also\nc: 1\n");
        assert_eq!(ids, [ErrorId::E201, ErrorId::E201]);
    }
}

/// A deterministic sweep over the punctuation the lexer switches on. SPEC
/// §3.7 requires that no input aborts, panics or crashes; this walks a large
/// slice of the input space without a dependency.
#[cfg(test)]
mod sweep {
    use super::*;

    /// A linear congruential generator, so the sweep is the same on every
    /// machine and every run (P3).
    struct Random(u64);

    impl Random {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) as usize) % bound.max(1)
        }
    }

    #[test]
    fn a_pseudo_random_sweep_never_panics_and_always_terminates() {
        let alphabet: Vec<&str> = vec![
            "a", "1", "-", "_", ".", "..", ":", "::", ",", "=", "==", "(", ")", "[", "]", "{", "}",
            "@", "#", "&", "$", "*", "!", "?", "|", "\"", "\\", "/", "//", " ", "\t", "\n", "e",
            "x", "schema", "logic", "versions", "derive", "require", "else", "throw", "if", "for",
            "in", "image", "png", "ref", "since", "removed", "\u{1}", "\u{feff}", "ñ",
        ];
        let mut random = Random(0x5eed_1234_9abc_def0);
        for case in 0..2_000 {
            let length = 1 + random.next(40);
            let mut text = String::new();
            for _ in 0..length {
                text.push_str(alphabet[random.next(alphabet.len())]);
            }
            let path = if case % 2 == 0 { "a.ab" } else { "a.abt" };
            let source = SourceFile::new(path, text.clone());
            match tokenize(&source) {
                Ok(tokens) => {
                    assert!(
                        tokens.last().map(Token::is_end_of_file).unwrap_or(false),
                        "missing EOF for {text:?}"
                    );
                }
                Err(diagnostics) => assert!(!diagnostics.is_empty()),
            }
        }
    }

    #[test]
    fn a_pathological_bracket_run_terminates_within_the_limits() {
        let mut text = String::from("tags: ");
        text.push_str(&"[".repeat(50_000));
        text.push('\n');
        let source = SourceFile::new("a.ab", text);
        assert!(tokenize(&source).is_err());
    }
}
