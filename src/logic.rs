//! The logic language: `logic` blocks, conditions, `derive`, `require`, `if`
//! and `for` (SPEC chapter 6).
//!
//! Logic is attached to a schema, not to an instance file, and runs for every
//! instance of that schema and for every nested object validated against it.
//! Evaluation is a pure function of the authored object, the schema set and
//! the version being compiled (SPEC §6.12).
//!
//! The module holds three stages of one language:
//!
//! - the statement and expression parser, which [`crate::schema`] calls once
//!   it has read `logic <SchemaName> {`;
//! - [`check_logic`], the P3 checks of SPEC §7.2: every path, every `derive`
//!   target, every operand's arity and type, every `length()` argument and
//!   every loop variable, decided from the schemas alone;
//! - [`evaluate`], step 5 of SPEC §7.3, which runs one block against one
//!   object for one version.
//!
//! Because P3 decides everything that can be decided from the schema, the
//! evaluator has very little left to report: a failed `require` (E515), a
//! `derive` that executes against a field absent in this version (E521), a
//! `derive` whose value cannot be assigned to its target (E412), an unknown
//! `$name` in a value or a message (E425), and logic that demands more loop
//! iterations than SPEC §3.7 allows one instance in one version (E523).

use crate::ast::Located;
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note};
use crate::instance::{ListSpelling, SyntaxValue};
use crate::lexer::{is_float_literal, is_identifier, is_int_literal, normalise, Token, TokenKind};
use crate::limits::{BRACKET_DEPTH, LOGIC_BLOCK_DEPTH, LOGIC_WORK, PATH_SEGMENTS};
use crate::resolve::{
    element_fields, lookup_path, substitute, variable_table, AuthoredObject, PathLookup, Step,
    ValuePath, VariableTable,
};
use crate::schema::{suggest, FieldDecl, SchemaDecl, TemplateTables, TieBreak, TypeExpr};
use crate::validate::ValidationContext;
use crate::versions::render_version_set;

// ===========================================================================
// The syntax tree
// ===========================================================================

/// `logic Schema { … }`, at most one per schema (SPEC §6.1).
#[derive(Clone, Debug)]
pub struct LogicBlock {
    /// The schema this block is attached to, compared exactly.
    pub schema: String,
    pub statements: Vec<LogicStatement>,
    pub at: Located,
}

/// The statements of SPEC §6.2.
#[derive(Clone, Debug)]
pub enum LogicStatement {
    /// `derive <path> = <expr>`; `if_missing` spells `derive?`.
    Derive {
        target: LogicPath,
        value: DeriveExpr,
        if_missing: bool,
        at: Located,
    },
    /// `require <condition> else throw "<message>"`.
    Require {
        condition: Condition,
        message: String,
        at: Located,
    },
    /// `if … { … } else if … { … } else { … }`.
    If {
        branches: Vec<(Condition, Vec<LogicStatement>)>,
        otherwise: Option<Vec<LogicStatement>>,
        at: Located,
    },
    /// `for $name in <iterable> { … }`.
    For {
        /// The normalised variable name, which is how it is looked up.
        variable: String,
        /// The name as written, for diagnostics.
        spelled: String,
        iterable: Iterable,
        body: Vec<LogicStatement>,
        at: Located,
    },
}

impl LogicStatement {
    /// Where the statement was written, which every logic diagnostic names
    /// (SPEC §6.12).
    pub fn at(&self) -> &Located {
        match self {
            LogicStatement::Derive { at, .. }
            | LogicStatement::Require { at, .. }
            | LogicStatement::If { at, .. }
            | LogicStatement::For { at, .. } => at,
        }
    }
}

/// What a `for` iterates: a list field, or a literal list (SPEC §6.2).
#[derive(Clone, Debug)]
pub enum Iterable {
    Path(LogicPath),
    Literal(Vec<SyntaxValue>),
}

/// A path read by logic: `.a.b` from the object being evaluated, or `$x.a`
/// from a loop variable (SPEC §6.5).
#[derive(Clone, Debug)]
pub struct LogicPath {
    /// `None` for a root path; otherwise the normalised loop variable it
    /// starts at.
    pub root: Option<String>,
    /// The loop variable as written.
    pub root_spelled: Option<String>,
    pub segments: Vec<LogicSegment>,
    pub at: Located,
}

impl LogicPath {
    /// The path as a diagnostic spells it: `.owner.team`, `.items[0].id`,
    /// `$slot.mode`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(root) = &self.root_spelled {
            out.push('$');
            out.push_str(root);
        }
        for segment in &self.segments {
            out.push('.');
            if segment.variable {
                out.push('$');
            }
            out.push_str(&segment.spelled);
            if let Some(index) = segment.index {
                out.push('[');
                out.push_str(&index.to_string());
                out.push(']');
            }
        }
        out
    }

    /// The normalised segment names, for a target a `derive` writes.
    fn names(&self) -> Vec<String> {
        self.segments
            .iter()
            .map(|segment| segment.name.clone())
            .collect()
    }
}

/// One segment of a logic path, with an optional read index.
#[derive(Clone, Debug)]
pub struct LogicSegment {
    /// The normalised field name, or the normalised loop-variable name.
    pub name: String,
    /// The name as written.
    pub spelled: String,
    /// True when the segment is a `$variable` used as a dynamic name.
    pub variable: bool,
    /// A read index; write positions never carry one (E506).
    pub index: Option<usize>,
}

/// The argument of `length(…)`: a path or a loop variable (SPEC §6.10).
#[derive(Clone, Debug)]
pub enum LengthArg {
    Path(LogicPath),
    Variable { name: String, spelled: String },
}

impl LengthArg {
    fn text(&self) -> String {
        match self {
            LengthArg::Path(path) => path.text(),
            LengthArg::Variable { spelled, .. } => format!("${spelled}"),
        }
    }
}

/// The operands of SPEC §6.6.
#[derive(Clone, Debug)]
pub enum Operand {
    Path(LogicPath),
    Variable {
        name: String,
        spelled: String,
        at: Located,
    },
    Length {
        arg: LengthArg,
        at: Located,
    },
    /// The built-in integer `version`: the version being compiled.
    Version {
        at: Located,
    },
    Literal {
        value: SyntaxValue,
        at: Located,
    },
    /// `( <condition> )`, whose value is a boolean (SPEC §6.6).
    Group(Box<Condition>),
}

/// The comparison operators of SPEC §6.6, all non-associative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOp {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Contains,
}

impl ComparisonOp {
    /// The `{op}` substitution of SPEC §9.8.
    pub fn spelling(self) -> &'static str {
        match self {
            ComparisonOp::Equal => "==",
            ComparisonOp::NotEqual => "!=",
            ComparisonOp::Less => "<",
            ComparisonOp::LessOrEqual => "<=",
            ComparisonOp::Greater => ">",
            ComparisonOp::GreaterOrEqual => ">=",
            ComparisonOp::Contains => "contains",
        }
    }

    /// True for the four operators that require numbers (SPEC §6.8).
    fn is_ordering(self) -> bool {
        matches!(
            self,
            ComparisonOp::Less
                | ComparisonOp::LessOrEqual
                | ComparisonOp::Greater
                | ComparisonOp::GreaterOrEqual
        )
    }
}

/// A boolean condition. Abstract has no truthiness: a bare operand is a
/// condition only when it is a `bool` value (SPEC §6.6).
#[derive(Clone, Debug)]
pub enum Condition {
    Comparison {
        left: Operand,
        op: ComparisonOp,
        right: Operand,
        at: Located,
    },
    Exists {
        operand: Operand,
        at: Located,
    },
    /// A bare operand, which must be a boolean.
    Truth {
        operand: Operand,
        at: Located,
    },
    Not(Box<Condition>),
    /// `and` and `or` are held flat, so that a long chain costs no stack.
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

/// The five right-hand sides a `derive` accepts (SPEC §6.4).
#[derive(Clone, Debug)]
pub enum DeriveExpr {
    Path(LogicPath),
    Length {
        arg: LengthArg,
        at: Located,
    },
    Version {
        at: Located,
    },
    Variable {
        name: String,
        spelled: String,
        at: Located,
    },
    Value {
        value: SyntaxValue,
        at: Located,
    },
}

// ===========================================================================
// The parser
// ===========================================================================

/// Parses the body of one `logic` block. The cursor is on the opening `{`;
/// the returned index is the first token after the block's closing line.
///
/// The block's frame — the `logic` keyword and the schema name — is read by
/// [`crate::schema`], which owns the template-file grammar; everything inside
/// the braces is chapter 6 and is read here.
pub fn parse_block(
    file: &str,
    tokens: &[Token],
    index: usize,
) -> (Vec<LogicStatement>, usize, Diagnostics) {
    let mut parser = Parser::new(file, tokens, index);
    let statements = parser.body();
    (statements, parser.index, parser.errors)
}

struct Parser<'a> {
    file: &'a str,
    tokens: &'a [Token],
    index: usize,
    errors: Diagnostics,
    end: Token,
    depth_reported: bool,
}

impl<'a> Parser<'a> {
    fn new(file: &'a str, tokens: &'a [Token], index: usize) -> Self {
        let position = tokens
            .last()
            .map(|token| token.position)
            .unwrap_or_else(|| crate::diagnostics::Position::new(1, 1));
        Self {
            file,
            tokens,
            index,
            errors: Diagnostics::new(),
            end: Token::new(TokenKind::EndOfFile, position, (0, 0)),
            depth_reported: false,
        }
    }

    // ---------------------------------------------------------- the cursor

    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&self.end)
    }

    fn peek_at(&self, ahead: usize) -> &Token {
        self.tokens.get(self.index + ahead).unwrap_or(&self.end)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn at_end(&self) -> bool {
        self.peek().is_end_of_file() || self.index >= self.tokens.len()
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

    /// Consumes a balanced `{ … }` without recursing, so that a file nested
    /// far deeper than SPEC §3.7 allows costs no stack. The cursor is on the
    /// opening `{`, and ends just after the matching `}`.
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
        while !self.at_end() {
            if self.at_punctuation("{") {
                depth += 1;
            } else if self.at_punctuation("}") {
                depth -= 1;
                self.bump();
                if depth == 0 {
                    return;
                }
                continue;
            }
            self.bump();
        }
    }

    /// The tokens of the rest of the current logical line, not including the
    /// `NL` that ends it.
    fn rest_of_line(&self) -> &'a [Token] {
        let all = self.tokens;
        let mut end = self.index;
        while let Some(token) = all.get(end) {
            if token.is_newline() || token.is_end_of_file() {
                break;
            }
            end += 1;
        }
        all.get(self.index..end).unwrap_or(&[])
    }

    /// A statement ends at the end of its logical line.
    fn end_of_statement(&mut self) {
        if self.at_newline() {
            self.bump();
            return;
        }
        if self.at_end() || self.at_punctuation("}") {
            return;
        }
        self.unexpected("end of line");
        self.skip_line();
    }

    // ------------------------------------------------------------- the body

    fn body(&mut self) -> Vec<LogicStatement> {
        if !self.at_punctuation("{") {
            self.unexpected("'{'");
            return Vec::new();
        }
        self.bump();
        // SPEC §3.6, §6.3: the `{` is the last token of its logical line.
        if !self.at_newline() {
            self.unexpected("end of line");
            self.skip_block_body();
            return Vec::new();
        }
        self.bump();
        let statements = self.statements(1);
        if self.at_punctuation("}") {
            self.bump();
            if !self.at_newline() && !self.at_end() {
                self.unexpected("end of line");
            }
            self.skip_line();
        }
        statements
    }

    fn statements(&mut self, depth: usize) -> Vec<LogicStatement> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() || self.at_punctuation("}") {
                return out;
            }
            let before = self.index;
            if let Some(statement) = self.statement(depth) {
                out.push(statement);
            }
            // Recovery always makes progress, so a malformed body terminates.
            if self.index == before {
                self.bump();
            }
        }
    }

    fn statement(&mut self, depth: usize) -> Option<LogicStatement> {
        let token = self.peek().clone();
        if token.is_keyword("derive") {
            return self.derive();
        }
        if token.is_keyword("require") {
            return self.require();
        }
        if token.is_keyword("if") {
            return self.if_statement(depth);
        }
        if token.is_keyword("for") {
            return self.for_statement(depth);
        }
        if token.is_keyword("else") {
            let at = self.located(&token);
            self.error(
                ErrorId::E517,
                &at,
                "'else' must follow the closing '}' of an if block.",
            );
            self.bump();
            if self.at_punctuation("{") {
                self.skip_block();
                self.end_of_statement();
            } else {
                self.skip_line();
            }
            return None;
        }
        self.unexpected("'derive', 'require', 'if', 'for' or '}'");
        self.skip_line();
        None
    }

    // ------------------------------------------------------------- derive

    fn derive(&mut self) -> Option<LogicStatement> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let if_missing = if self.at_punctuation("?") {
            self.bump();
            true
        } else {
            false
        };
        let target = self.logic_path()?;
        if !self.at_punctuation("=") {
            self.unexpected("'='");
            self.skip_line();
            return None;
        }
        self.bump();
        let value = self.derive_expr(&at)?;
        self.end_of_statement();
        Some(LogicStatement::Derive {
            target,
            value,
            if_missing,
            at,
        })
    }

    /// SPEC §6.4: the right-hand side is a path, a `length(…)` call, the
    /// `version` built-in or a loop variable when its first token says so; in
    /// every other case it is an ordinary value.
    fn derive_expr(&mut self, at: &Located) -> Option<DeriveExpr> {
        let rest = self.rest_of_line();
        let Some(first) = rest.first() else {
            self.unexpected("a value");
            self.skip_line();
            return None;
        };
        if first.is_punctuation(".") {
            return self.logic_path().map(DeriveExpr::Path);
        }
        if first.is_punctuation("$") {
            let dotted = rest.get(2).map(|token| token.is_punctuation(".")) == Some(true);
            if dotted {
                return self.logic_path().map(DeriveExpr::Path);
            }
            self.bump();
            let spelled = self.variable_name()?;
            return Some(DeriveExpr::Variable {
                name: normalise(&spelled),
                spelled,
                at: at.clone(),
            });
        }
        if first.is_keyword("length")
            && rest.get(1).map(|token| token.is_punctuation("(")) == Some(true)
        {
            self.bump();
            let arg = self.length_arg()?;
            return Some(DeriveExpr::Length {
                arg,
                at: at.clone(),
            });
        }
        if first.is_keyword("version") && rest.len() == 1 {
            self.bump();
            return Some(DeriveExpr::Version { at: at.clone() });
        }
        let count = rest.len();
        let value = self.value_from(rest, at)?;
        self.index += count;
        Some(DeriveExpr::Value {
            value,
            at: at.clone(),
        })
    }

    // ------------------------------------------------------------ require

    fn require(&mut self) -> Option<LogicStatement> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let text = self.spell_condition();
        let Some(condition) = self.condition(&text, 0) else {
            self.skip_line();
            return None;
        };
        if !self.peek().is_keyword("else") || !self.peek_at(1).is_keyword("throw") {
            let here = self.here();
            self.error(
                ErrorId::E507,
                &here,
                "require must be followed by 'else throw \"message\"'.",
            );
            self.skip_line();
            return None;
        }
        self.bump();
        self.bump();
        let token = self.peek().clone();
        let TokenKind::QuotedString(message) = &token.kind else {
            let position = self.located(&token);
            self.error(
                ErrorId::E508,
                &position,
                "A throw message must be a quoted string.",
            );
            self.skip_line();
            return None;
        };
        let message = message.clone();
        self.bump();
        self.end_of_statement();
        Some(LogicStatement::Require {
            condition,
            message,
            at,
        })
    }

    // ----------------------------------------------------------- if / for

    fn if_statement(&mut self, depth: usize) -> Option<LogicStatement> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        let mut branches = Vec::new();
        let mut otherwise = None;

        let text = self.spell_condition();
        let Some(condition) = self.condition(&text, 0) else {
            self.recover_from_head();
            return None;
        };
        branches.push((condition, self.block(depth)?));

        while self.peek().is_keyword("else") {
            self.bump();
            if self.peek().is_keyword("if") {
                self.bump();
                let text = self.spell_condition();
                let Some(condition) = self.condition(&text, 0) else {
                    self.recover_from_head();
                    return None;
                };
                branches.push((condition, self.block(depth)?));
                continue;
            }
            otherwise = Some(self.block(depth)?);
            break;
        }
        self.end_of_statement();
        Some(LogicStatement::If {
            branches,
            otherwise,
            at,
        })
    }

    fn for_statement(&mut self, depth: usize) -> Option<LogicStatement> {
        let keyword = self.bump();
        let at = self.located(&keyword);
        // SPEC §6.9: a loop variable is spelled `$name` everywhere.
        if !self.at_punctuation("$") {
            self.unexpected("a loop variable");
            self.recover_from_head();
            return None;
        }
        self.bump();
        let spelled = self.variable_name()?;
        if !self.peek().is_keyword("in") {
            self.unexpected("'in'");
            self.recover_from_head();
            return None;
        }
        self.bump();
        let iterable = if self.at_punctuation("[") {
            Iterable::Literal(self.literal_list()?)
        } else if self.at_punctuation(".") || self.at_punctuation("$") {
            Iterable::Path(self.logic_path()?)
        } else {
            self.unexpected("a list field or a literal list");
            self.recover_from_head();
            return None;
        };
        let body = self.block(depth)?;
        self.end_of_statement();
        Some(LogicStatement::For {
            variable: normalise(&spelled),
            spelled,
            iterable,
            body,
            at,
        })
    }

    /// After a malformed `if` or `for` head, skips the block it introduces so
    /// that one defect reports one diagnostic.
    fn recover_from_head(&mut self) {
        let mut index = self.index;
        while let Some(token) = self.tokens.get(index) {
            if token.is_newline() || token.is_end_of_file() {
                break;
            }
            if token.is_punctuation("{") {
                self.index = index;
                self.skip_block();
                self.skip_line();
                return;
            }
            index += 1;
        }
        self.skip_line();
    }

    fn block(&mut self, depth: usize) -> Option<Vec<LogicStatement>> {
        if !self.at_punctuation("{") {
            self.unexpected("'{'");
            self.skip_line();
            return None;
        }
        if depth > LOGIC_BLOCK_DEPTH {
            if !self.depth_reported {
                self.depth_reported = true;
                let here = self.here();
                self.error(
                    ErrorId::E209,
                    &here,
                    format!(
                        "Nesting depth of if / for blocks in one logic block exceeds the limit of {LOGIC_BLOCK_DEPTH}."
                    ),
                );
            }
            self.skip_block();
            return None;
        }
        self.bump();
        if !self.at_newline() {
            self.unexpected("end of line");
            self.skip_block_body();
            return None;
        }
        self.bump();
        let statements = self.statements(depth + 1);
        if self.at_punctuation("}") {
            self.bump();
        } else {
            self.unexpected("'}'");
        }
        Some(statements)
    }

    // -------------------------------------------------------------- paths

    fn logic_path(&mut self) -> Option<LogicPath> {
        let start = self.peek().clone();
        let at = self.located(&start);
        let mut root = None;
        let mut root_spelled = None;
        if self.at_punctuation("$") {
            self.bump();
            let spelled = self.variable_name()?;
            root = Some(normalise(&spelled));
            root_spelled = Some(spelled);
            if !self.at_punctuation(".") {
                self.unexpected("'.'");
                self.skip_line();
                return None;
            }
        } else if !self.at_punctuation(".") {
            self.unexpected("a path");
            self.skip_line();
            return None;
        }

        let mut segments = Vec::new();
        while self.at_punctuation(".") {
            self.bump();
            let segment = self.segment()?;
            segments.push(segment);
            if segments.len() > PATH_SEGMENTS {
                let position = at.clone();
                self.error(
                    ErrorId::E209,
                    &position,
                    format!("Segments in a logic path exceed the limit of {PATH_SEGMENTS}."),
                );
                self.skip_line();
                return None;
            }
        }
        if segments.is_empty() {
            self.unexpected("a field name");
            self.skip_line();
            return None;
        }
        Some(LogicPath {
            root,
            root_spelled,
            segments,
            at,
        })
    }

    fn segment(&mut self) -> Option<LogicSegment> {
        let variable = self.at_punctuation("$");
        if variable {
            self.bump();
        }
        let token = self.peek().clone();
        let Some(text) = token.identifier_text().map(str::to_string) else {
            self.unexpected("a field name");
            self.skip_line();
            return None;
        };
        if !is_identifier(&text) {
            let at = self.located(&token);
            self.error(ErrorId::E210, &at, format!("Invalid field name '{text}'."));
            self.skip_line();
            return None;
        }
        self.bump();
        let mut index = None;
        if self.at_punctuation("[") {
            self.bump();
            let token = self.peek().clone();
            let parsed = match &token.kind {
                TokenKind::IntLiteral(text) => text.parse::<usize>().ok(),
                _ => None,
            };
            let Some(parsed) = parsed else {
                self.unexpected("a non-negative index");
                self.skip_line();
                return None;
            };
            self.bump();
            if !self.at_punctuation("]") {
                self.unexpected("']'");
                self.skip_line();
                return None;
            }
            self.bump();
            index = Some(parsed);
        }
        Some(LogicSegment {
            name: normalise(&text),
            spelled: text,
            variable,
            index,
        })
    }

    fn variable_name(&mut self) -> Option<String> {
        let token = self.peek().clone();
        let Some(text) = token.identifier_text().map(str::to_string) else {
            self.unexpected("a variable name");
            self.skip_line();
            return None;
        };
        if !is_identifier(&text) {
            let at = self.located(&token);
            self.error(
                ErrorId::E210,
                &at,
                format!("Invalid variable name '{text}'."),
            );
            self.skip_line();
            return None;
        }
        self.bump();
        Some(text)
    }

    fn length_arg(&mut self) -> Option<LengthArg> {
        if !self.at_punctuation("(") {
            self.unexpected("'('");
            self.skip_line();
            return None;
        }
        self.bump();
        let bare_variable = self.at_punctuation("$") && !self.peek_at(2).is_punctuation(".");
        let arg = if bare_variable {
            self.bump();
            let spelled = self.variable_name()?;
            LengthArg::Variable {
                name: normalise(&spelled),
                spelled,
            }
        } else {
            LengthArg::Path(self.logic_path()?)
        };
        if !self.at_punctuation(")") {
            self.unexpected("')'");
            self.skip_line();
            return None;
        }
        self.bump();
        Some(arg)
    }

    // --------------------------------------------------------- conditions

    fn condition(&mut self, text: &str, depth: usize) -> Option<Condition> {
        let mut parts = vec![self.and_condition(text, depth)?];
        while self.peek().is_keyword("or") || self.at_punctuation("||") {
            self.bump();
            parts.push(self.and_condition(text, depth)?);
        }
        Some(if parts.len() == 1 {
            parts.pop().expect("one part")
        } else {
            Condition::Or(parts)
        })
    }

    fn and_condition(&mut self, text: &str, depth: usize) -> Option<Condition> {
        let mut parts = vec![self.not_condition(text, depth)?];
        while self.peek().is_keyword("and") || self.at_punctuation("&&") {
            self.bump();
            parts.push(self.not_condition(text, depth)?);
        }
        Some(if parts.len() == 1 {
            parts.pop().expect("one part")
        } else {
            Condition::And(parts)
        })
    }

    /// `not` is folded at parse time: a run of prefixes costs no tree depth,
    /// and double negation is exactly the identity (SPEC §6.6).
    fn not_condition(&mut self, text: &str, depth: usize) -> Option<Condition> {
        let mut negations = 0usize;
        while self.peek().is_keyword("not") || self.at_punctuation("!") {
            self.bump();
            negations += 1;
        }
        let inner = self.comparison(text, depth)?;
        Some(if negations % 2 == 1 {
            Condition::Not(Box::new(inner))
        } else {
            inner
        })
    }

    fn comparison(&mut self, text: &str, depth: usize) -> Option<Condition> {
        let start = self.peek().clone();
        let at = self.located(&start);
        let left = self.operand(text, depth)?;

        if self.peek().is_keyword("exists") {
            self.bump();
            self.finish_comparison(text)?;
            return Some(Condition::Exists { operand: left, at });
        }
        if let Some(op) = self.comparison_op() {
            self.bump();
            let right = self.operand(text, depth)?;
            self.finish_comparison(text)?;
            return Some(Condition::Comparison {
                left,
                op,
                right,
                at,
            });
        }
        self.finish_comparison(text)?;
        Some(Condition::Truth { operand: left, at })
    }

    fn comparison_op(&self) -> Option<ComparisonOp> {
        match &self.peek().kind {
            TokenKind::Punctuation("==") => Some(ComparisonOp::Equal),
            TokenKind::Punctuation("!=") => Some(ComparisonOp::NotEqual),
            TokenKind::Punctuation("<") => Some(ComparisonOp::Less),
            TokenKind::Punctuation("<=") => Some(ComparisonOp::LessOrEqual),
            TokenKind::Punctuation(">") => Some(ComparisonOp::Greater),
            TokenKind::Punctuation(">=") => Some(ComparisonOp::GreaterOrEqual),
            TokenKind::Identifier(word) if word == "contains" => Some(ComparisonOp::Contains),
            _ => None,
        }
    }

    /// A comparison is non-associative and ends at a connector, a `)`, an
    /// `else`, a `{` or the end of the line (SPEC §6.6).
    fn finish_comparison(&mut self, text: &str) -> Option<()> {
        if self.comparison_op().is_some() || self.peek().is_keyword("exists") {
            let here = self.here();
            self.error(
                ErrorId::E512,
                &here,
                "Comparisons cannot be chained; use 'and'.",
            );
            return None;
        }
        if self.at_condition_end() {
            return Some(());
        }
        let found = self.peek().describe();
        self.report_invalid(text, format!("{found} is not an operator"));
        None
    }

    fn at_condition_end(&self) -> bool {
        let token = self.peek();
        token.is_newline()
            || token.is_end_of_file()
            || token.is_punctuation(")")
            || token.is_punctuation("{")
            || token.is_keyword("else")
            || token.is_keyword("and")
            || token.is_keyword("or")
            || token.is_punctuation("&&")
            || token.is_punctuation("||")
    }

    fn operand(&mut self, text: &str, depth: usize) -> Option<Operand> {
        let token = self.peek().clone();
        let at = self.located(&token);
        if token.is_punctuation("(") {
            if depth >= BRACKET_DEPTH {
                if !self.depth_reported {
                    self.depth_reported = true;
                    self.error(
                        ErrorId::E209,
                        &at,
                        format!(
                            "Bracket nesting depth in a condition exceeds the limit of {BRACKET_DEPTH}."
                        ),
                    );
                }
                return None;
            }
            self.bump();
            let inner = self.condition(text, depth + 1)?;
            if !self.at_punctuation(")") {
                // Defensive only, and no condition reaches it: `(` and `)`
                // ride the value-bracket stack of SPEC §3.6, so an unclosed
                // `(` is E203 and a stray `)` is E205, both reported while
                // the file is lexed and therefore before this parser runs.
                // SPEC §6.6 states the two reasons that do reach E511 and
                // no longer names this one.
                self.report_invalid(text, "the parentheses are unbalanced");
                return None;
            }
            self.bump();
            return Some(Operand::Group(Box::new(inner)));
        }
        if token.is_punctuation(".") {
            return self.logic_path().map(Operand::Path);
        }
        if token.is_punctuation("$") {
            if self.peek_at(2).is_punctuation(".") {
                return self.logic_path().map(Operand::Path);
            }
            self.bump();
            let spelled = self.variable_name()?;
            return Some(Operand::Variable {
                name: normalise(&spelled),
                spelled,
                at,
            });
        }
        match &token.kind {
            TokenKind::Identifier(word) if word == "length" => {
                self.bump();
                let arg = self.length_arg()?;
                Some(Operand::Length { arg, at })
            }
            TokenKind::Identifier(word) if word == "version" => {
                self.bump();
                Some(Operand::Version { at })
            }
            TokenKind::Identifier(word) if word == "true" || word == "false" => {
                let value = SyntaxValue::Bare(word.clone());
                self.bump();
                Some(Operand::Literal { value, at })
            }
            TokenKind::IntLiteral(number) | TokenKind::FloatLiteral(number) => {
                let value = SyntaxValue::Bare(number.clone());
                self.bump();
                Some(Operand::Literal { value, at })
            }
            TokenKind::QuotedString(quoted) => {
                let value = SyntaxValue::Quoted(quoted.clone());
                self.bump();
                Some(Operand::Literal { value, at })
            }
            _ => {
                let found = token.describe();
                self.report_invalid(text, format!("{found} is not an operand"));
                None
            }
        }
    }

    fn report_invalid(&mut self, text: &str, reason: impl Into<String>) {
        let here = self.here();
        let reason = reason.into();
        self.error(
            ErrorId::E511,
            &here,
            format!("Invalid condition '{}': {reason}.", text.trim()),
        );
    }

    /// The `{text}` substitution of E511: the condition as written, from the
    /// cursor to the `else`, the `{` or the end of the logical line.
    fn spell_condition(&self) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        let mut index = self.index;
        while let Some(token) = self.tokens.get(index) {
            if token.is_newline() || token.is_end_of_file() {
                break;
            }
            if depth == 0 && (token.is_keyword("else") || token.is_punctuation("{")) {
                break;
            }
            if token.is_punctuation("(") || token.is_punctuation("[") {
                depth += 1;
            }
            if token.is_punctuation(")") || token.is_punctuation("]") {
                depth = depth.saturating_sub(1);
            }
            append_spelling(&mut out, token);
            index += 1;
        }
        out
    }

    // ------------------------------------------------------------- values

    fn literal_list(&mut self) -> Option<Vec<SyntaxValue>> {
        let open = self.peek().clone();
        let at = self.located(&open);
        self.bump();
        let all = self.tokens;
        let start = self.index;
        let mut depth = 0usize;
        while !self.at_end() && !self.at_newline() {
            if self.at_punctuation("[") {
                depth += 1;
            } else if self.at_punctuation("]") {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            self.bump();
        }
        if !self.at_punctuation("]") {
            self.unexpected("']'");
            self.skip_line();
            return None;
        }
        let inner = all.get(start..self.index).unwrap_or(&[]);
        self.bump();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        let mut items = split_commas(inner);
        if items.len() > 1 && items.last().map(|item| item.is_empty()) == Some(true) {
            items.pop();
        }
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.single_value(item, &at)?);
        }
        Some(values)
    }

    /// The `value` grammar a `derive` right-hand side shares with instances
    /// (SPEC §5.5, §6.4).
    fn value_from(&mut self, tokens: &'a [Token], at: &Located) -> Option<SyntaxValue> {
        let mut items = split_commas(tokens);
        if items.len() > 1 && items.last().map(|item| item.is_empty()) == Some(true) {
            let position = at.clone();
            self.error(
                ErrorId::E442,
                &position,
                "Trailing comma; a statement does not continue onto the next line.",
            );
            return None;
        }
        if items.len() == 1 {
            return self.single_value(items.pop().expect("one item"), at);
        }
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.single_value(item, at)?);
        }
        Some(SyntaxValue::List(values, ListSpelling::Brackets))
    }

    fn single_value(&mut self, tokens: &'a [Token], at: &Located) -> Option<SyntaxValue> {
        let Some(first) = tokens.first() else {
            let position = at.clone();
            self.error(ErrorId::E441, &position, "Empty list item.");
            return None;
        };
        if first.is_punctuation("[") {
            let last = tokens.last().expect("a non-empty slice has a last token");
            if !last.is_punctuation("]") || tokens.len() < 2 {
                let position = self.located(first);
                self.error(
                    ErrorId::E210,
                    &position,
                    "Unexpected '[' here; expected a list closed by ']'.",
                );
                return None;
            }
            let inner = tokens.get(1..tokens.len() - 1).unwrap_or(&[]);
            if inner.is_empty() {
                return Some(SyntaxValue::List(Vec::new(), ListSpelling::Brackets));
            }
            let mut items = split_commas(inner);
            if items.len() > 1 && items.last().map(|item| item.is_empty()) == Some(true) {
                items.pop();
            }
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                if item.first().map(|token| token.is_punctuation("[")) == Some(true) {
                    let position = self.located(first);
                    self.error(ErrorId::E441, &position, "Nested lists are not supported.");
                    return None;
                }
                values.push(self.single_value(item, at)?);
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
                TokenKind::Identifier(text)
                | TokenKind::IntLiteral(text)
                | TokenKind::FloatLiteral(text) => return Some(SyntaxValue::Bare(text.clone())),
                _ => {}
            }
        }
        let position = self.located(first);
        self.error(
            ErrorId::E210,
            &position,
            format!("Unexpected {} here; expected a value.", first.describe()),
        );
        None
    }
}

/// Splits a token run on the commas at bracket depth 0.
fn split_commas(tokens: &[Token]) -> Vec<&[Token]> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if token.is_punctuation("[") || token.is_punctuation("(") {
            depth += 1;
        } else if token.is_punctuation("]") || token.is_punctuation(")") {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_punctuation(",") {
            out.push(tokens.get(start..index).unwrap_or(&[]));
            start = index + 1;
        }
    }
    out.push(tokens.get(start..).unwrap_or(&[]));
    out
}

/// Appends one token to a rendering of a condition, with the spacing a reader
/// expects: none around `.`, none inside brackets, one elsewhere.
fn append_spelling(out: &mut String, token: &Token) {
    let text = match &token.kind {
        TokenKind::Identifier(text)
        | TokenKind::SchemaName(text)
        | TokenKind::IntLiteral(text)
        | TokenKind::FloatLiteral(text)
        | TokenKind::SizeToken(text)
        | TokenKind::BareText(text) => text.clone(),
        TokenKind::Punctuation(text) => (*text).to_string(),
        TokenKind::QuotedString(text) => format!("\"{text}\""),
        TokenKind::Newline | TokenKind::EndOfFile => String::new(),
    };
    if text.is_empty() {
        return;
    }
    let tight_left = matches!(text.as_str(), "." | "[" | "]" | ")" | "," | "$");
    let tight_right = out.ends_with('.')
        || out.ends_with('(')
        || out.ends_with('[')
        || out.ends_with('$')
        || out.is_empty();
    if !tight_left && !tight_right {
        out.push(' ');
    }
    out.push_str(&text);
}

/// A value as a diagnostic spells it.
fn render_value(value: &SyntaxValue) -> String {
    match value {
        SyntaxValue::Bare(text) => text.clone(),
        SyntaxValue::Quoted(text) => format!("\"{text}\""),
        SyntaxValue::List(items, _) => format!(
            "[{}]",
            items
                .iter()
                .map(render_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxValue::Tag { name, .. } => format!("#{name}"),
        SyntaxValue::Object(_) => "an object".to_string(),
    }
}

// ===========================================================================
// Kinds shared by the checker and the evaluator
// ===========================================================================

/// What a value is, as far as the logic language is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Text,
    Bool,
    List,
    Object,
    /// Not decidable from the schema — a dynamic segment, or a literal list
    /// whose elements disagree. Nothing is reported about an unknown kind.
    Unknown,
}

/// The kind a field's value has, or the kind of one of its elements.
fn field_kind(field: &FieldDecl, element: bool) -> Kind {
    if field.is_list() && !element {
        return Kind::List;
    }
    match field.type_expr() {
        None => Kind::Object,
        Some(TypeExpr::Int { .. }) | Some(TypeExpr::Float { .. }) => Kind::Number,
        Some(TypeExpr::Bool) => Kind::Bool,
        Some(TypeExpr::Nested { .. }) => Kind::Object,
        Some(_) => Kind::Text,
    }
}

/// The `{kind}` substitution of SPEC §9.8 for a field, or for one element.
fn field_word(field: &FieldDecl, element: bool) -> &'static str {
    if field.is_list() && !element {
        return "list";
    }
    match field.type_expr() {
        None => "object",
        Some(ty) => ty.kind(),
    }
}

/// True when a value of `kind` may be written to `field` (SPEC §6.4, E412).
/// A list field also accepts a single element, which §5.10 coerces.
fn accepts(field: &FieldDecl, kind: Kind) -> bool {
    if kind == Kind::Unknown {
        return true;
    }
    if field.is_list() && kind == Kind::List {
        return true;
    }
    kind == field_kind(field, true)
}

// ===========================================================================
// P3: checking one logic block
// ===========================================================================

/// Runs the P3 checks over every logic block of a project, in table order
/// (SPEC §7.2).
pub fn check_logic_blocks(tables: &TemplateTables) -> Diagnostics {
    let mut errors = Diagnostics::new();
    for block in &tables.logic {
        let Some(schema) = tables.schema(&block.schema) else {
            continue; // E501 was reported when the tables were built.
        };
        if let Err(reported) = check_logic(block, schema, tables) {
            errors.extend(reported);
        }
    }
    errors
}

/// P3 checks over one logic block: paths, derive targets, operand arity and
/// types, `length()` arguments and loop-variable shadowing (SPEC §7.2).
pub fn check_logic(
    block: &LogicBlock,
    schema: &SchemaDecl,
    tables: &TemplateTables,
) -> Result<(), Diagnostics> {
    let mut checker = Checker {
        tables,
        schema,
        block: &block.schema,
        loops: Vec::new(),
        errors: Diagnostics::new(),
    };
    checker.statements(&block.statements);
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

/// One loop variable, as P3 knows it.
struct LoopInfo<'t> {
    name: String,
    /// The fields of the object an element holds, when it holds one.
    fields: Option<&'t [FieldDecl]>,
    kind: Kind,
    word: &'static str,
}

/// What a path resolves to, from the schemas alone.
struct PathFacts<'t> {
    /// The field the final segment names, when exactly one is possible.
    field: Option<&'t FieldDecl>,
    /// True when an index selected one element of the final list field.
    element: bool,
    /// The first list field the path crossed with no index (SPEC §6.5).
    crossed: Option<String>,
    /// True when a dynamic segment made the result undecidable.
    ambiguous: bool,
    /// False when a diagnostic was already reported for this path.
    ok: bool,
}

impl PathFacts<'_> {
    fn kind(&self) -> Kind {
        if self.crossed.is_some() {
            return Kind::List;
        }
        match self.field {
            Some(field) if !self.ambiguous => field_kind(field, self.element),
            _ => Kind::Unknown,
        }
    }

    fn word(&self) -> &'static str {
        if self.crossed.is_some() {
            return "list";
        }
        match self.field {
            Some(field) if !self.ambiguous => field_word(field, self.element),
            _ => "text",
        }
    }
}

/// One operand as P3 sees it.
struct OperandFacts {
    kind: Kind,
    word: &'static str,
    text: String,
    /// The list field an operand's path crossed, which E519 names.
    crossed: Option<String>,
}

struct Checker<'t> {
    tables: &'t TemplateTables,
    schema: &'t SchemaDecl,
    /// The schema the block names, for the `{schema}` substitution of E503.
    block: &'t str,
    loops: Vec<LoopInfo<'t>>,
    errors: Diagnostics,
}

impl<'t> Checker<'t> {
    fn error(&mut self, id: ErrorId, at: &Located, message: impl Into<String>) {
        self.errors
            .push(Diagnostic::at(id, at.file.clone(), at.position, message));
    }

    fn error_with_note(
        &mut self,
        id: ErrorId,
        at: &Located,
        message: impl Into<String>,
        note: impl Into<String>,
    ) {
        self.errors.push(
            Diagnostic::at(id, at.file.clone(), at.position, message).with_note_text(note.into()),
        );
    }

    fn binding(&self, name: &str) -> Option<&LoopInfo<'t>> {
        self.loops.iter().find(|item| item.name == name)
    }

    fn statements(&mut self, statements: &[LogicStatement]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &LogicStatement) {
        match statement {
            LogicStatement::Derive {
                target,
                value,
                if_missing,
                at,
            } => self.derive(target, value, *if_missing, at),
            LogicStatement::Require {
                condition,
                message,
                at,
                ..
            } => {
                self.condition(condition);
                self.check_references(message, at);
            }
            LogicStatement::If {
                branches,
                otherwise,
                ..
            } => {
                for (condition, body) in branches {
                    self.condition(condition);
                    self.statements(body);
                }
                if let Some(body) = otherwise {
                    self.statements(body);
                }
            }
            LogicStatement::For {
                variable,
                spelled,
                iterable,
                body,
                at,
            } => self.for_statement(variable, spelled, iterable, body, at),
        }
    }

    // ------------------------------------------------------------- derive

    fn derive(&mut self, target: &LogicPath, value: &DeriveExpr, if_missing: bool, at: &Located) {
        self.derive_target(target, if_missing, at);
        match value {
            DeriveExpr::Path(path) => {
                let facts = self.path(path);
                if facts.ok {
                    if let Some(field) = &facts.crossed {
                        let message = format!(
                            "derive requires a single value; '{}' crosses list field '{field}'.",
                            path.text()
                        );
                        self.error(ErrorId::E519, &path.at, message);
                    }
                }
            }
            DeriveExpr::Length { arg, at } => self.length(arg, at),
            DeriveExpr::Version { .. } => {}
            DeriveExpr::Variable { name, spelled, at } => {
                // SPEC §6.9: a `$name` in a derive value that no enclosing
                // loop binds resolves against the current object.
                if self.binding(name).is_none() {
                    self.check_variable_name(name, spelled, at);
                }
            }
            DeriveExpr::Value { value, at } => self.check_value_references(value, at),
        }
    }

    fn derive_target(&mut self, target: &LogicPath, if_missing: bool, at: &Located) {
        let text = target.text();
        if target.root.is_some() {
            self.error(
                ErrorId::E505,
                &target.at,
                format!(
                    "derive target '{text}' is not a declared field of '{}'.",
                    self.schema.name
                ),
            );
            return;
        }
        if let Some(segment) = target.segments.iter().find(|segment| segment.variable) {
            self.error(
                ErrorId::E520,
                &target.at,
                format!(
                    "A derive target cannot contain the variable '${}'; write the field name.",
                    segment.spelled
                ),
            );
            return;
        }
        if target.segments.len() == 1 {
            let name = &target.segments[0].name;
            if name == "template" || name == "id" {
                self.error(
                    ErrorId::E504,
                    &target.at,
                    format!("derive cannot write '{name}'."),
                );
                return;
            }
        }
        if let Some(segment) = target
            .segments
            .iter()
            .find(|segment| segment.index.is_some())
        {
            self.error(
                ErrorId::E506,
                &target.at,
                format!(
                    "derive cannot write through a list; '{text}' crosses list field '{}'.",
                    segment.spelled
                ),
            );
            return;
        }

        let mut scope: &[FieldDecl] = &self.schema.fields;
        let last = target.segments.len().saturating_sub(1);
        for (index, segment) in target.segments.iter().enumerate() {
            let Some(field) = scope.iter().find(|field| field.name == segment.name) else {
                let declared: Vec<&str> = scope.iter().map(|field| field.name.as_str()).collect();
                let mut message = format!(
                    "derive target '{text}' is not a declared field of '{}'.",
                    self.schema.name
                );
                if let Some(hint) = suggest(&segment.name, &declared, TieBreak::DeclarationOrder) {
                    message.push_str(&format!(" Did you mean '{hint}'?"));
                }
                let note = format!("declared fields: {}.", declared.join(", "));
                self.error_with_note(ErrorId::E505, &target.at, message, note);
                return;
            };
            if index == last {
                if if_missing && field.default().is_some() {
                    self.error(
                        ErrorId::E518,
                        at,
                        format!(
                            "derive? can never apply to '{}': the schema declares a default, which is filled before logic runs.",
                            field.name
                        ),
                    );
                }
                return;
            }
            if field.is_list() {
                self.error(
                    ErrorId::E506,
                    &target.at,
                    format!(
                        "derive cannot write through a list; '{text}' crosses list field '{}'.",
                        field.name
                    ),
                );
                return;
            }
            let Some(inner) = element_fields(self.tables, field) else {
                let mut message = format!(
                    "derive target '{text}' is not a declared field of '{}'.",
                    self.schema.name
                );
                message.push_str(&format!(
                    " '{}' is {}, which holds no fields.",
                    field.name,
                    field_word(field, false)
                ));
                self.error(ErrorId::E505, &target.at, message);
                return;
            };
            scope = inner;
        }
    }

    // ---------------------------------------------------------------- for

    fn for_statement(
        &mut self,
        variable: &str,
        spelled: &str,
        iterable: &Iterable,
        body: &[LogicStatement],
        at: &Located,
    ) {
        if self.binding(variable).is_some() {
            self.error(
                ErrorId::E516,
                at,
                format!("Loop variable '${spelled}' is already bound by an enclosing loop."),
            );
            return;
        }
        let info = match iterable {
            Iterable::Path(path) => {
                let facts = self.path(path);
                let mut kind = Kind::Unknown;
                let mut word = "text";
                let mut fields = None;
                match facts.field {
                    Some(field) if !facts.ambiguous && facts.crossed.is_none() => {
                        if !field.is_list() || facts.element {
                            self.error(
                                ErrorId::E509,
                                &path.at,
                                format!(
                                    "'{}' is not a list; for iterates lists and literal lists.",
                                    path.text()
                                ),
                            );
                            return;
                        }
                        kind = field_kind(field, true);
                        word = field_word(field, true);
                        fields = element_fields(self.tables, field);
                    }
                    _ if !facts.ok => return,
                    _ => {}
                }
                LoopInfo {
                    name: variable.to_string(),
                    fields,
                    kind,
                    word,
                }
            }
            Iterable::Literal(items) => {
                let (kind, word) = literal_kind(items);
                LoopInfo {
                    name: variable.to_string(),
                    fields: None,
                    kind,
                    word,
                }
            }
        };
        self.loops.push(info);
        self.statements(body);
        self.loops.pop();
    }

    // --------------------------------------------------------- conditions

    fn condition(&mut self, condition: &Condition) {
        match condition {
            Condition::Comparison {
                left,
                op,
                right,
                at,
            } => {
                let left = self.operand(left);
                let right = self.operand(right);
                if *op != ComparisonOp::Contains {
                    for facts in [&left, &right] {
                        if let Some(field) = &facts.crossed {
                            self.error(
                                ErrorId::E519,
                                at,
                                format!(
                                    "Operator '{}' requires a single value; '{}' crosses list field '{field}'. Use 'contains'.",
                                    op.spelling(),
                                    facts.text
                                ),
                            );
                            return;
                        }
                    }
                }
                if !self.comparable(*op, &left, &right) {
                    self.error(
                        ErrorId::E513,
                        at,
                        format!(
                            "Operator '{}' cannot compare {} and {}.",
                            op.spelling(),
                            left.word,
                            right.word
                        ),
                    );
                }
            }
            Condition::Exists { operand, .. } => {
                self.operand(operand);
            }
            Condition::Truth { operand, at } => {
                let facts = self.operand(operand);
                // SPEC §6.6: a bare condition is valid only when it names a
                // `bool` field, so a bare literal is E513 even when it is a
                // boolean literal. Abstract has no truthiness.
                if matches!(operand, Operand::Literal { .. }) {
                    self.error(
                        ErrorId::E513,
                        at,
                        format!(
                            "A condition must name a bool field; '{}' is a literal.",
                            facts.text
                        ),
                    );
                } else if facts.kind != Kind::Bool && facts.kind != Kind::Unknown {
                    self.error(
                        ErrorId::E513,
                        at,
                        format!(
                            "A condition must be a boolean; '{}' is {}.",
                            facts.text, facts.word
                        ),
                    );
                }
            }
            Condition::Not(inner) => self.condition(inner),
            Condition::And(parts) | Condition::Or(parts) => {
                for part in parts {
                    self.condition(part);
                }
            }
        }
    }

    /// The type rules of SPEC §6.8, decided from the declared types.
    fn comparable(&self, op: ComparisonOp, left: &OperandFacts, right: &OperandFacts) -> bool {
        if left.kind == Kind::Unknown || right.kind == Kind::Unknown {
            return true;
        }
        match op {
            ComparisonOp::Contains => match left.kind {
                Kind::List => !matches!(right.kind, Kind::List | Kind::Object),
                Kind::Text => right.kind == Kind::Text,
                _ => false,
            },
            _ if op.is_ordering() => left.kind == Kind::Number && right.kind == Kind::Number,
            _ => {
                !matches!(left.kind, Kind::List | Kind::Object)
                    && !matches!(right.kind, Kind::List | Kind::Object)
            }
        }
    }

    fn operand(&mut self, operand: &Operand) -> OperandFacts {
        match operand {
            Operand::Path(path) => {
                let facts = self.path(path);
                OperandFacts {
                    kind: facts.kind(),
                    word: facts.word(),
                    text: path.text(),
                    crossed: facts.crossed.clone(),
                }
            }
            Operand::Variable { name, spelled, at } => {
                let Some(binding) = self.binding(name) else {
                    // SPEC §6.9: a `$name` used as an operand that no
                    // enclosing loop binds is E510.
                    let at = at.clone();
                    self.error(
                        ErrorId::E510,
                        &at,
                        format!("Unbound variable '${spelled}'."),
                    );
                    return OperandFacts {
                        kind: Kind::Unknown,
                        word: "text",
                        text: format!("${spelled}"),
                        crossed: None,
                    };
                };
                OperandFacts {
                    kind: binding.kind,
                    word: binding.word,
                    text: format!("${spelled}"),
                    crossed: None,
                }
            }
            Operand::Length { arg, at } => {
                self.length(arg, at);
                OperandFacts {
                    kind: Kind::Number,
                    word: "int",
                    text: format!("length({})", arg.text()),
                    crossed: None,
                }
            }
            Operand::Version { .. } => OperandFacts {
                kind: Kind::Number,
                word: "int",
                text: "version".to_string(),
                crossed: None,
            },
            Operand::Literal { value, .. } => {
                let (kind, word) = value_kind(value);
                OperandFacts {
                    kind,
                    word,
                    text: render_value(value),
                    crossed: None,
                }
            }
            Operand::Group(inner) => {
                self.condition(inner);
                OperandFacts {
                    kind: Kind::Bool,
                    word: "bool",
                    text: "( … )".to_string(),
                    crossed: None,
                }
            }
        }
    }

    /// SPEC §6.10: the argument's declared type MUST be a list field or a
    /// `text` field, or a loop variable bound to a list or to text.
    fn length(&mut self, arg: &LengthArg, at: &Located) {
        let (kind, word, text) = match arg {
            LengthArg::Path(path) => {
                let facts = self.path(path);
                if !facts.ok {
                    return;
                }
                (facts.kind(), facts.word(), path.text())
            }
            LengthArg::Variable { name, spelled } => {
                let Some(binding) = self.binding(name) else {
                    let at = at.clone();
                    self.error(
                        ErrorId::E510,
                        &at,
                        format!("Unbound variable '${spelled}'."),
                    );
                    return;
                };
                (binding.kind, binding.word, format!("${spelled}"))
            }
        };
        if matches!(kind, Kind::List | Kind::Text | Kind::Unknown) {
            return;
        }
        self.error(
            ErrorId::E514,
            at,
            format!("length() requires a list or text value; '{text}' is {word}."),
        );
    }

    // -------------------------------------------------------------- paths

    /// Walks one read path through the schemas (SPEC §6.5). Every literal
    /// segment must name a declared field of the scope in force at that
    /// point, considering the union of all versions.
    fn path(&mut self, path: &LogicPath) -> PathFacts<'t> {
        let mut facts = PathFacts {
            field: None,
            element: false,
            crossed: None,
            ambiguous: false,
            ok: true,
        };
        let mut scopes: Vec<&'t [FieldDecl]> = match &path.root {
            None => vec![self.schema.fields.as_slice()],
            Some(name) => {
                let Some(binding) = self.binding(name) else {
                    let spelled = path.root_spelled.clone().unwrap_or_else(|| name.clone());
                    let at = path.at.clone();
                    self.error(
                        ErrorId::E510,
                        &at,
                        format!("Unbound variable '${spelled}'."),
                    );
                    facts.ok = false;
                    return facts;
                };
                match binding.fields {
                    Some(fields) => vec![fields],
                    None => Vec::new(),
                }
            }
        };

        let last = path.segments.len().saturating_sub(1);
        for (index, segment) in path.segments.iter().enumerate() {
            if segment.variable {
                if self.binding(&segment.name).is_none() {
                    let at = path.at.clone();
                    self.error(
                        ErrorId::E510,
                        &at,
                        format!("Unbound variable '${}'.", segment.spelled),
                    );
                    facts.ok = false;
                    return facts;
                }
                facts.ambiguous = true;
                if index == last {
                    return facts;
                }
                // SPEC §6.5: after a dynamic segment the schema in scope is
                // the set of declared types of the fields at that position.
                scopes = scopes
                    .iter()
                    .flat_map(|scope| scope.iter())
                    .filter_map(|field| element_fields(self.tables, field))
                    .collect();
                continue;
            }

            let matched: Vec<&'t FieldDecl> = scopes
                .iter()
                .flat_map(|scope| scope.iter())
                .filter(|field| field.name == segment.name)
                .collect();
            if matched.is_empty() {
                let declared: Vec<&str> = scopes
                    .iter()
                    .flat_map(|scope| scope.iter())
                    .map(|field| field.name.as_str())
                    .collect();
                let mut message = format!(
                    "Unknown path '{}' in logic for '{}'.",
                    path.text(),
                    self.block
                );
                if let Some(hint) = suggest(&segment.name, &declared, TieBreak::DeclarationOrder) {
                    message.push_str(&format!(" Did you mean '{hint}'?"));
                }
                let at = path.at.clone();
                self.error(ErrorId::E503, &at, message);
                facts.ok = false;
                return facts;
            }
            if matched.len() > 1 {
                facts.ambiguous = true;
            }
            let field = matched[0];
            if index == last {
                facts.field = Some(field);
                facts.element = segment.index.is_some();
                return facts;
            }
            if field.is_list() && segment.index.is_none() && facts.crossed.is_none() {
                facts.crossed = Some(field.name.clone());
            }
            scopes = matched
                .iter()
                .filter_map(|field| element_fields(self.tables, field))
                .collect();
        }
        facts
    }

    // -------------------------------------------------- `$name` references

    /// Every `$name` in a `derive` value or a `throw` message that no
    /// enclosing loop binds must name a declared root field (SPEC §6.9).
    fn check_value_references(&mut self, value: &SyntaxValue, at: &Located) {
        match value {
            SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => self.check_references(text, at),
            SyntaxValue::List(items, _) => {
                for item in items {
                    self.check_value_references(item, at);
                }
            }
            SyntaxValue::Object(fields) => {
                for (_, item) in fields {
                    self.check_value_references(item, at);
                }
            }
            SyntaxValue::Tag { .. } => {}
        }
    }

    fn check_references(&mut self, text: &str, at: &Located) {
        for name in variable_references(text) {
            let normalised = normalise(&name);
            if self.binding(&normalised).is_some() {
                continue;
            }
            self.check_variable_name(&normalised, &name, at);
        }
    }

    fn check_variable_name(&mut self, name: &str, spelled: &str, at: &Located) {
        if name == "id" || self.schema.field(name).is_some() {
            return;
        }
        let mut declared: Vec<&str> = vec!["id"];
        declared.extend(self.schema.field_names());
        let mut message = format!("Unknown path '${spelled}' in logic for '{}'.", self.block);
        if let Some(hint) = suggest(name, &declared, TieBreak::DeclarationOrder) {
            message.push_str(&format!(" Did you mean '{hint}'?"));
        }
        let at = at.clone();
        self.error(ErrorId::E503, &at, message);
    }
}

/// The static kind of one literal value (SPEC §3.5).
fn value_kind(value: &SyntaxValue) -> (Kind, &'static str) {
    match value {
        SyntaxValue::Quoted(_) => (Kind::Text, "text"),
        SyntaxValue::Bare(text) => {
            if is_int_literal(text) {
                (Kind::Number, "int")
            } else if is_float_literal(text) {
                (Kind::Number, "float")
            } else if text == "true" || text == "false" {
                (Kind::Bool, "bool")
            } else {
                (Kind::Text, "text")
            }
        }
        SyntaxValue::List(_, _) => (Kind::List, "list"),
        SyntaxValue::Object(_) => (Kind::Object, "object"),
        SyntaxValue::Tag { .. } => (Kind::Unknown, "text"),
    }
}

/// The kind every element of a literal list shares, or `Unknown`.
fn literal_kind(items: &[SyntaxValue]) -> (Kind, &'static str) {
    let mut kind: Option<(Kind, &'static str)> = None;
    for item in items {
        let found = value_kind(item);
        match kind {
            None => kind = Some(found),
            Some(current) if current.0 == found.0 => {}
            Some(_) => return (Kind::Unknown, "text"),
        }
    }
    kind.unwrap_or((Kind::Unknown, "text"))
}

/// The `$name` and `${name}` references of one text, by the scanning rules of
/// SPEC §5.11. Malformed references are skipped here; interpolation reports
/// them (E426, E427) when the statement runs.
fn variable_references(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    if !text.contains('$') {
        return out;
    }
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '$' {
            index += 1;
            continue;
        }
        match characters.get(index + 1).copied() {
            Some('$') => index += 2,
            Some('{') => {
                let mut end = index + 2;
                while end < characters.len() && characters[end] != '}' {
                    end += 1;
                }
                if end >= characters.len() {
                    return out;
                }
                out.push(characters[index + 2..end].iter().collect());
                index = end + 1;
            }
            Some(character) if is_reference_character(character) => {
                let mut end = index + 1;
                while end < characters.len() && is_reference_character(characters[end]) {
                    end += 1;
                }
                out.push(characters[index + 1..end].iter().collect());
                index = end;
            }
            _ => index += 1,
        }
    }
    out
}

fn is_reference_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

// ===========================================================================
// P4 step 5: evaluating one block against one object
// ===========================================================================

/// What the object being evaluated is called in a diagnostic.
#[derive(Clone, Copy, Debug)]
pub struct Binding<'a> {
    /// The `{context}` substitution of SPEC §9.8: the dotted path of this
    /// object, rooted at the instance id.
    pub context: &'a str,
    /// The instance id, which is the `$id` of SPEC §5.11.
    pub id: &'a str,
    /// The instance header, which every logic diagnostic notes.
    pub at: Option<&'a Located>,
}

/// A number, kept exact while both sides are integers (SPEC §6.8).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn as_float(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }

    fn equals(self, other: Number) -> bool {
        match (self, other) {
            (Number::Int(left), Number::Int(right)) => left == right,
            _ => self.as_float() == other.as_float(),
        }
    }

    fn compare(self, other: Number) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Number::Int(left), Number::Int(right)) => Some(left.cmp(&right)),
            _ => self.as_float().partial_cmp(&other.as_float()),
        }
    }
}

/// One value, read through the declared type of the field it came from.
#[derive(Clone, Debug, PartialEq)]
enum Datum {
    Number(Number),
    Text(String),
    Bool(bool),
    List(Vec<Datum>),
    Object,
    /// Present, but not readable as its declared type. Step 7 reports it; no
    /// comparison it appears in is true.
    Unreadable,
}

impl Datum {
    fn kind(&self) -> Kind {
        match self {
            Datum::Number(_) => Kind::Number,
            Datum::Text(_) => Kind::Text,
            Datum::Bool(_) => Kind::Bool,
            Datum::List(_) => Kind::List,
            Datum::Object => Kind::Object,
            Datum::Unreadable => Kind::Unknown,
        }
    }

    fn word(&self) -> &'static str {
        match self {
            Datum::Number(Number::Int(_)) => "int",
            Datum::Number(Number::Float(_)) => "float",
            Datum::Text(_) => "text",
            Datum::Bool(_) => "bool",
            Datum::List(_) => "list",
            Datum::Object => "object",
            Datum::Unreadable => "text",
        }
    }

    /// SPEC §6.8: two scalars are equal when they are the same kind and the
    /// same value. Comparison of strings is exact.
    fn equals(&self, other: &Datum) -> bool {
        match (self, other) {
            (Datum::Number(left), Datum::Number(right)) => left.equals(*right),
            (Datum::Text(left), Datum::Text(right)) => left == right,
            (Datum::Bool(left), Datum::Bool(right)) => left == right,
            _ => false,
        }
    }
}

/// The declared shape of one value the evaluator is holding.
#[derive(Clone, Copy)]
struct Shape<'t> {
    ty: Option<&'t TypeExpr>,
    fields: Option<&'t [FieldDecl]>,
    /// True when the value is the whole list of a list field.
    whole_list: bool,
    word: &'static str,
}

impl Shape<'_> {
    fn untyped() -> Self {
        Shape {
            ty: None,
            fields: None,
            whole_list: false,
            word: "text",
        }
    }
}

/// One value a logic path resolved to.
struct Slot<'t> {
    value: SyntaxValue,
    shape: Shape<'t>,
}

impl Slot<'_> {
    fn datum(&self) -> Datum {
        read_datum(&self.value, self.shape)
    }
}

/// One loop variable, bound to one element.
struct LoopBinding<'t> {
    name: String,
    value: SyntaxValue,
    shape: Shape<'t>,
    index: usize,
}

/// Evaluates one logic block against one object and one version
/// (SPEC §6.11, §7.3 step 5).
///
/// The first failure aborts the instance: a `require` that fails is E515 and
/// no later statement runs (SPEC §6.12).
pub fn evaluate<'t>(
    block: &LogicBlock,
    object: &mut AuthoredObject,
    schema: &'t SchemaDecl,
    validation: &ValidationContext<'t>,
    binding: &Binding<'_>,
) -> Result<(), Diagnostics> {
    let mut evaluator = Evaluator {
        validation,
        schema,
        block: &block.schema,
        binding: Binding {
            context: binding.context,
            id: binding.id,
            at: binding.at,
        },
        loops: Vec::new(),
    };
    evaluator.statements(&block.statements, object)
}

struct Evaluator<'a, 't> {
    validation: &'a ValidationContext<'t>,
    schema: &'t SchemaDecl,
    block: &'a str,
    binding: Binding<'a>,
    loops: Vec<LoopBinding<'t>>,
}

impl<'t> Evaluator<'_, 't> {
    // -------------------------------------------------------- diagnostics

    fn notes(&self) -> Vec<Note> {
        let mut notes = Vec::new();
        if let Some(at) = self.binding.at {
            notes.push(
                Note::new(format!("instance '{}'.", self.binding.id))
                    .at(at.file.clone(), at.position),
            );
        }
        notes
    }

    fn loop_notes(&self) -> Vec<Note> {
        self.loops
            .iter()
            .map(|binding| {
                Note::new(format!(
                    "at index {}, item {}.",
                    binding.index,
                    render_value(&binding.value)
                ))
            })
            .collect()
    }

    fn diagnostic(
        &self,
        id: ErrorId,
        at: &Located,
        message: String,
        extra: Vec<Note>,
    ) -> Diagnostics {
        let mut diagnostic = Diagnostic::at(id, at.file.clone(), at.position, message);
        for note in self.notes() {
            diagnostic = diagnostic.with_note(note);
        }
        for note in extra {
            diagnostic = diagnostic.with_note(note);
        }
        for note in self.loop_notes() {
            diagnostic = diagnostic.with_note(note);
        }
        Diagnostics::one(diagnostic)
    }

    // --------------------------------------------------------- statements

    fn statements(
        &mut self,
        statements: &[LogicStatement],
        object: &mut AuthoredObject,
    ) -> Result<(), Diagnostics> {
        for statement in statements {
            self.statement(statement, object)?;
        }
        Ok(())
    }

    fn statement(
        &mut self,
        statement: &LogicStatement,
        object: &mut AuthoredObject,
    ) -> Result<(), Diagnostics> {
        match statement {
            LogicStatement::Derive {
                target,
                value,
                if_missing,
                at,
            } => self.derive(target, value, *if_missing, at, object),
            LogicStatement::Require {
                condition,
                message,
                at,
            } => {
                if self.condition(condition, object) {
                    return Ok(());
                }
                let variables = self.variables(object);
                let text = match substitute(message, &variables, Some(at)) {
                    Ok((text, _)) => text,
                    Err(reported) => return Err(reported),
                };
                Err(self.diagnostic(
                    ErrorId::E515,
                    at,
                    format!("{} logic: {text}", self.block),
                    vec![Note::new(format!(
                        "compiling version {}.",
                        self.validation.version
                    ))],
                ))
            }
            LogicStatement::If {
                branches,
                otherwise,
                ..
            } => {
                for (condition, body) in branches {
                    if self.condition(condition, object) {
                        return self.statements(body, object);
                    }
                }
                match otherwise {
                    Some(body) => self.statements(body, object),
                    None => Ok(()),
                }
            }
            LogicStatement::For {
                variable,
                iterable,
                body,
                at,
                ..
            } => self.for_statement(variable, iterable, body, at, object),
        }
    }

    fn for_statement(
        &mut self,
        variable: &str,
        iterable: &Iterable,
        body: &[LogicStatement],
        at: &Located,
        object: &mut AuthoredObject,
    ) -> Result<(), Diagnostics> {
        let elements: Vec<(SyntaxValue, Shape<'t>)> = match iterable {
            Iterable::Path(path) => {
                let mut out = Vec::new();
                for slot in self.resolve(path, object) {
                    let element = Shape {
                        whole_list: false,
                        ..slot.shape
                    };
                    match slot.value {
                        SyntaxValue::List(items, _) => {
                            for item in items {
                                out.push((item, element));
                            }
                        }
                        other => out.push((other, element)),
                    }
                }
                out
            }
            Iterable::Literal(items) => items
                .iter()
                .map(|item| (normalised_literal(item), Shape::untyped()))
                .collect(),
        };

        for (index, (value, shape)) in elements.into_iter().enumerate() {
            self.charge(at)?;
            self.loops.push(LoopBinding {
                name: variable.to_string(),
                value,
                shape,
                index,
            });
            let outcome = self.statements(body, object);
            self.loops.pop();
            outcome?;
        }
        Ok(())
    }

    /// SPEC §3.7: one unit of logic work is one execution of a `for` body,
    /// and one instance may spend [`LOGIC_WORK`] of them in one version.
    /// Nothing else in the language multiplies: `derive`, `require` and
    /// `if` each run at most once per enclosing iteration, so charging the
    /// iteration bounds the whole evaluation. Crossing the bound is E523,
    /// reported at the `for` whose iteration crossed it.
    fn charge(&self, at: &Located) -> Result<(), Diagnostics> {
        if self.validation.charge_logic_work() {
            return Ok(());
        }
        Err(self.diagnostic(
            ErrorId::E523,
            at,
            format!(
                "Logic work limit exceeded at {}: the logic executed more than {LOGIC_WORK} loop iterations in version {}.",
                self.binding.context, self.validation.version
            ),
            Vec::new(),
        ))
    }

    // ------------------------------------------------------------- derive

    fn derive(
        &mut self,
        target: &LogicPath,
        value: &DeriveExpr,
        if_missing: bool,
        at: &Located,
        object: &mut AuthoredObject,
    ) -> Result<(), Diagnostics> {
        // A target that P3 rejected never reaches P4; a defensive bail keeps
        // the evaluator total when it is called on its own.
        if target.root.is_some()
            || target
                .segments
                .iter()
                .any(|s| s.variable || s.index.is_some())
        {
            return Ok(());
        }
        let segments = target.names();
        let lookup = lookup_path(
            self.validation.tables,
            &self.schema.fields,
            &segments,
            self.validation.tables.versions,
        );
        let PathLookup::Found { field, window } = lookup else {
            return Ok(());
        };
        let (context, name) = self.target_context(&segments);

        // SPEC §6.4: a derive that executes against a field absent in the
        // version being compiled is E521, whether or not it would write.
        let exists = window
            .map(|range| range.contains(self.validation.version))
            .unwrap_or(false);
        if !exists {
            let versions: Vec<u32> = window
                .map(|range| range.iter().collect())
                .unwrap_or_default();
            let rendered = if versions.is_empty() {
                "none".to_string()
            } else {
                render_version_set(&versions)
            };
            let guard = versions.first().copied().unwrap_or(1);
            return Err(self.diagnostic(
                ErrorId::E521,
                at,
                format!(
                    "derive cannot write {context}.{name} in version {}; the field exists in versions {rendered}.",
                    self.validation.version
                ),
                vec![Note::new(format!(
                    "guard the statement, for example with 'if version >= {guard}'."
                ))],
            ));
        }

        if if_missing && !self.resolve(target, object).is_empty() {
            return Ok(());
        }

        let Some(produced) = self.produce(value, object, at)? else {
            return Ok(());
        };
        if !accepts(field, produced.kind) {
            return Err(self.diagnostic(
                ErrorId::E412,
                at,
                format!(
                    "Type mismatch at {context}.{name}: expected {}, found {}.",
                    field.kind_word(),
                    produced.word
                ),
                Vec::new(),
            ));
        }

        let dotted = segments.join(".");
        object.set_origin(&dotted, at.clone());
        // A value copied from elsewhere in the object is already interpreted,
        // so a brace left in it is literal and must not expand again
        // (SPEC §5.8, §7.3 step 7).
        if produced.interpreted {
            if let SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) = &produced.value {
                if text.contains('{') || text.contains('}') {
                    let path: ValuePath = segments
                        .iter()
                        .map(|name| Step::Field(name.clone()))
                        .collect();
                    object.set_spans(path, vec![(0, text.len())]);
                }
            }
        }
        write_at(&mut object.fields, &segments, produced.value);
        Ok(())
    }

    fn target_context(&self, segments: &[String]) -> (String, String) {
        let last = segments.last().cloned().unwrap_or_default();
        let mut context = self.binding.context.to_string();
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            context.push('.');
            context.push_str(segment);
        }
        (context, last)
    }

    /// The value a `derive` writes, or `None` when the statement writes
    /// nothing (SPEC §6.4).
    fn produce(
        &mut self,
        value: &DeriveExpr,
        object: &AuthoredObject,
        at: &Located,
    ) -> Result<Option<Produced>, Diagnostics> {
        match value {
            DeriveExpr::Path(path) => {
                let mut slots = self.resolve(path, object);
                if slots.len() != 1 {
                    return Ok(None);
                }
                let slot = slots.pop().expect("one slot");
                let datum = slot.datum();
                Ok(Some(Produced {
                    kind: datum.kind(),
                    word: if slot.shape.whole_list {
                        "list"
                    } else {
                        slot.shape.word
                    },
                    value: slot.value,
                    interpreted: true,
                }))
            }
            DeriveExpr::Length { arg, .. } => {
                let count = self.length(arg, object);
                Ok(Some(Produced {
                    value: SyntaxValue::Bare(count.to_string()),
                    kind: Kind::Number,
                    word: "int",
                    interpreted: true,
                }))
            }
            DeriveExpr::Version { .. } => Ok(Some(Produced {
                value: SyntaxValue::Bare(self.validation.version.to_string()),
                kind: Kind::Number,
                word: "int",
                interpreted: true,
            })),
            DeriveExpr::Variable { name, spelled, .. } => {
                if let Some(binding) = self.loops.iter().rev().find(|item| &item.name == name) {
                    let datum = read_datum(&binding.value, binding.shape);
                    return Ok(Some(Produced {
                        kind: datum.kind(),
                        word: datum.word(),
                        value: binding.value.clone(),
                        interpreted: true,
                    }));
                }
                // SPEC §6.9: an unbound `$name` in a derive value resolves
                // against the current object.
                let variables = self.variables(object);
                let (text, _) = substitute(&format!("${spelled}"), &variables, Some(at))?;
                Ok(Some(Produced {
                    value: SyntaxValue::Bare(text),
                    kind: Kind::Text,
                    word: "text",
                    interpreted: true,
                }))
            }
            DeriveExpr::Value { value, .. } => {
                let variables = self.variables(object);
                let value = interpolate_value(value, &variables, at)?;
                Ok(Some(Produced {
                    kind: Kind::Unknown,
                    word: "text",
                    value,
                    interpreted: false,
                }))
            }
        }
    }

    // --------------------------------------------------------- conditions

    /// Conditions never fail: an operand that resolves to no value makes every
    /// comparison false (SPEC §6.8), and every type rule was decided at P3.
    fn condition(&self, condition: &Condition, object: &AuthoredObject) -> bool {
        match condition {
            Condition::Comparison {
                left, op, right, ..
            } => self.compare(left, *op, right, object),
            Condition::Exists { operand, .. } => self.exists(operand, object),
            Condition::Truth { operand, .. } => {
                matches!(self.value_of(operand, object), Some(Datum::Bool(true)))
            }
            Condition::Not(inner) => !self.condition(inner, object),
            Condition::And(parts) => parts.iter().all(|part| self.condition(part, object)),
            Condition::Or(parts) => parts.iter().any(|part| self.condition(part, object)),
        }
    }

    fn compare(
        &self,
        left: &Operand,
        op: ComparisonOp,
        right: &Operand,
        object: &AuthoredObject,
    ) -> bool {
        let (Some(left), Some(right)) = (self.value_of(left, object), self.value_of(right, object))
        else {
            return false;
        };
        if left == Datum::Unreadable || right == Datum::Unreadable {
            return false;
        }
        match op {
            ComparisonOp::Contains => match (&left, &right) {
                (Datum::List(items), scalar) => items.iter().any(|item| item.equals(scalar)),
                (Datum::Text(haystack), Datum::Text(needle)) => haystack.contains(needle.as_str()),
                _ => false,
            },
            ComparisonOp::Equal => left.equals(&right),
            ComparisonOp::NotEqual => {
                !matches!(left, Datum::List(_) | Datum::Object)
                    && !matches!(right, Datum::List(_) | Datum::Object)
                    && !left.equals(&right)
            }
            _ => {
                let (Datum::Number(left), Datum::Number(right)) = (&left, &right) else {
                    return false;
                };
                let Some(ordering) = left.compare(*right) else {
                    return false;
                };
                match op {
                    ComparisonOp::Less => ordering.is_lt(),
                    ComparisonOp::LessOrEqual => ordering.is_le(),
                    ComparisonOp::Greater => ordering.is_gt(),
                    ComparisonOp::GreaterOrEqual => ordering.is_ge(),
                    _ => false,
                }
            }
        }
    }

    /// SPEC §6.7: presence on compiled data, never the filesystem.
    fn exists(&self, operand: &Operand, object: &AuthoredObject) -> bool {
        match operand {
            Operand::Path(path) => {
                let slots = self.resolve(path, object);
                if slots.is_empty() {
                    return false;
                }
                if slots.len() == 1 {
                    if let SyntaxValue::List(items, _) = &slots[0].value {
                        return !items.is_empty();
                    }
                }
                true
            }
            Operand::Variable { name, .. } => {
                match self.loops.iter().rev().find(|item| &item.name == name) {
                    None => false,
                    Some(binding) => match &binding.value {
                        SyntaxValue::List(items, _) => !items.is_empty(),
                        _ => true,
                    },
                }
            }
            Operand::Group(inner) => self.condition(inner, object),
            _ => true,
        }
    }

    /// The single value an operand reads, or `None` when it reads none.
    fn value_of(&self, operand: &Operand, object: &AuthoredObject) -> Option<Datum> {
        match operand {
            Operand::Path(path) => {
                let slots = self.resolve(path, object);
                if slots.is_empty() {
                    return None;
                }
                if slots.len() == 1 {
                    return Some(slots[0].datum());
                }
                // A projection is a list value; only `contains` and `exists`
                // accept it, and P3 rejected every other operator (E519).
                Some(Datum::List(slots.iter().map(Slot::datum).collect()))
            }
            Operand::Variable { name, .. } => self
                .loops
                .iter()
                .rev()
                .find(|item| &item.name == name)
                .map(|binding| read_datum(&binding.value, binding.shape)),
            Operand::Length { arg, .. } => {
                Some(Datum::Number(Number::Int(self.length(arg, object) as i64)))
            }
            Operand::Version { .. } => Some(Datum::Number(Number::Int(i64::from(
                self.validation.version,
            )))),
            Operand::Literal { value, .. } => Some(literal_datum(value)),
            Operand::Group(inner) => Some(Datum::Bool(self.condition(inner, object))),
        }
    }

    /// SPEC §6.10: elements of a list, Unicode scalar values of a string, the
    /// number of projected values, or `0` for a path with no value.
    fn length(&self, arg: &LengthArg, object: &AuthoredObject) -> usize {
        let datum = match arg {
            LengthArg::Path(path) => {
                let slots = self.resolve(path, object);
                match slots.len() {
                    0 => return 0,
                    1 => slots[0].datum(),
                    count => return count,
                }
            }
            LengthArg::Variable { name, .. } => {
                match self.loops.iter().rev().find(|item| &item.name == name) {
                    None => return 0,
                    Some(binding) => read_datum(&binding.value, binding.shape),
                }
            }
        };
        match datum {
            Datum::List(items) => items.len(),
            Datum::Text(text) => text.chars().count(),
            _ => 0,
        }
    }

    // -------------------------------------------------------- resolution

    /// Follows one logic path through the current object (SPEC §6.5). An
    /// absent field, a dynamic segment naming nothing, an out-of-range index
    /// and a field that does not exist in this version all resolve to no
    /// value, and none of them is an error.
    fn resolve(&self, path: &LogicPath, object: &AuthoredObject) -> Vec<Slot<'t>> {
        struct Cursor<'v, 't> {
            entries: &'v [(String, SyntaxValue)],
            fields: &'t [FieldDecl],
        }

        let mut cursors: Vec<Cursor<'_, 't>> = match &path.root {
            None => vec![Cursor {
                entries: &object.fields,
                fields: &self.schema.fields,
            }],
            Some(name) => {
                let Some(binding) = self.loops.iter().rev().find(|item| &item.name == name) else {
                    return Vec::new();
                };
                let (SyntaxValue::Object(entries), Some(fields)) =
                    (&binding.value, binding.shape.fields)
                else {
                    return Vec::new();
                };
                vec![Cursor { entries, fields }]
            }
        };

        let mut slots: Vec<Slot<'t>> = Vec::new();
        let last = path.segments.len().saturating_sub(1);
        for (index, segment) in path.segments.iter().enumerate() {
            let name = match segment.variable {
                false => segment.name.clone(),
                true => {
                    let Some(binding) = self
                        .loops
                        .iter()
                        .rev()
                        .find(|item| item.name == segment.name)
                    else {
                        return Vec::new();
                    };
                    match scalar_text(&binding.value) {
                        Some(text) => normalise(&text),
                        None => return Vec::new(),
                    }
                }
            };

            let mut next: Vec<Cursor<'_, 't>> = Vec::new();
            for cursor in &cursors {
                let Some(field) = cursor.fields.iter().find(|field| field.name == name) else {
                    continue;
                };
                if !field.exists_in(self.validation.version, self.validation.tables.versions) {
                    continue;
                }
                let Some((_, value)) = cursor.entries.iter().find(|(key, _)| key == &name) else {
                    continue;
                };
                let inner = element_fields(self.validation.tables, field);
                let shape = Shape {
                    ty: field.type_expr(),
                    fields: inner,
                    whole_list: false,
                    word: field_word(field, true),
                };

                // The values this segment selects, before the walk continues.
                let selected: Vec<&SyntaxValue> = match segment.index {
                    Some(wanted) => match value {
                        SyntaxValue::List(items, _) => items.get(wanted).into_iter().collect(),
                        single if wanted == 0 && field.is_list() => vec![single],
                        _ => Vec::new(),
                    },
                    None if index < last && field.is_list() => match value {
                        SyntaxValue::List(items, _) => items.iter().collect(),
                        single => vec![single],
                    },
                    None => vec![value],
                };

                if index == last {
                    // An unindexed list field reads as its whole list, even
                    // when it still holds the single value §5.10 will coerce.
                    let whole_list = field.is_list() && segment.index.is_none();
                    for value in selected {
                        slots.push(Slot {
                            value: value.clone(),
                            shape: Shape {
                                whole_list,
                                word: field_word(field, !whole_list),
                                ..shape
                            },
                        });
                    }
                    continue;
                }
                let Some(inner) = inner else {
                    continue;
                };
                for value in selected {
                    if let SyntaxValue::Object(entries) = value {
                        next.push(Cursor {
                            entries,
                            fields: inner,
                        });
                    }
                }
            }
            if index == last {
                break;
            }
            cursors = next;
            if cursors.is_empty() {
                return Vec::new();
            }
        }
        slots
    }

    /// The variable table of SPEC §6.9: the loop variables in force, innermost
    /// first, then the root scalar fields of the object as it now stands.
    fn variables(&self, object: &AuthoredObject) -> VariableTable {
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut non_scalars: Vec<(String, &'static str)> = Vec::new();
        let bound = |name: &str, entries: &Vec<(String, String)>, non: &Vec<(String, &str)>| {
            entries.iter().any(|(key, _)| key == name) || non.iter().any(|(key, _)| key == name)
        };
        for binding in self.loops.iter().rev() {
            if bound(&binding.name, &entries, &non_scalars) {
                continue;
            }
            match scalar_text(&binding.value) {
                Some(text) => entries.push((binding.name.clone(), text)),
                // A loop over a list of groups binds objects. Interpolation
                // inserts text, so the name is recorded as bound-but-not-text
                // and a reference to it is E522 (SPEC §6.9). Dropping it here
                // would let the reference fall through to a root field of the
                // same name, or be reported as a misspelling.
                None => non_scalars.push((binding.name.clone(), value_word(&binding.value))),
            }
        }
        let table = variable_table(object, self.binding.id);
        for (name, value) in &object.fields {
            if bound(name, &entries, &non_scalars) || table.get(name).is_some() {
                continue;
            }
            if scalar_text(value).is_none() {
                non_scalars.push((name.clone(), value_word(value)));
            }
        }
        entries.extend(table.entries);
        VariableTable {
            entries,
            non_scalars,
        }
    }
}

/// One value a `derive` is about to write.
struct Produced {
    value: SyntaxValue,
    kind: Kind,
    word: &'static str,
    /// True when the value came from already-interpreted data, so that §5.10
    /// must not expand it a second time.
    interpreted: bool,
}

/// Reads one value through the declared type of the field it came from.
fn read_datum(value: &SyntaxValue, shape: Shape<'_>) -> Datum {
    if shape.whole_list {
        let element = Shape {
            whole_list: false,
            ..shape
        };
        return match value {
            SyntaxValue::List(items, _) => {
                Datum::List(items.iter().map(|item| read_datum(item, element)).collect())
            }
            single => Datum::List(vec![read_datum(single, element)]),
        };
    }
    match shape.ty {
        None => literal_datum(value),
        Some(TypeExpr::Int { .. }) => match value {
            SyntaxValue::Bare(text) => match text.parse::<i64>() {
                Ok(number) => Datum::Number(Number::Int(number)),
                Err(_) => Datum::Unreadable,
            },
            _ => Datum::Unreadable,
        },
        Some(TypeExpr::Float { .. }) => match value {
            SyntaxValue::Bare(text) => match text.parse::<f64>() {
                Ok(number) if number.is_finite() => Datum::Number(Number::Float(number)),
                _ => Datum::Unreadable,
            },
            _ => Datum::Unreadable,
        },
        Some(TypeExpr::Bool) => match value {
            SyntaxValue::Bare(text) if text == "true" => Datum::Bool(true),
            SyntaxValue::Bare(text) if text == "false" => Datum::Bool(false),
            _ => Datum::Unreadable,
        },
        Some(TypeExpr::Nested { .. }) => match value {
            SyntaxValue::Object(_) => Datum::Object,
            _ => Datum::Unreadable,
        },
        Some(_) => match value {
            SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => Datum::Text(text.clone()),
            SyntaxValue::Object(_) => Datum::Object,
            SyntaxValue::List(_, _) => Datum::Unreadable,
            SyntaxValue::Tag { .. } => Datum::Unreadable,
        },
    }
}

/// Reads one value with no declared type, by the literal grammars of §3.5.
fn literal_datum(value: &SyntaxValue) -> Datum {
    match value {
        SyntaxValue::Quoted(text) => Datum::Text(text.clone()),
        SyntaxValue::Bare(text) => {
            if is_int_literal(text) {
                match text.parse::<i64>() {
                    Ok(number) => Datum::Number(Number::Int(number)),
                    Err(_) => Datum::Text(text.clone()),
                }
            } else if is_float_literal(text) {
                match text.parse::<f64>() {
                    Ok(number) if number.is_finite() => Datum::Number(Number::Float(number)),
                    _ => Datum::Text(text.clone()),
                }
            } else if text == "true" {
                Datum::Bool(true)
            } else if text == "false" {
                Datum::Bool(false)
            } else {
                Datum::Text(text.clone())
            }
        }
        SyntaxValue::List(items, _) => Datum::List(items.iter().map(literal_datum).collect()),
        SyntaxValue::Object(_) => Datum::Object,
        SyntaxValue::Tag { .. } => Datum::Unreadable,
    }
}

/// SPEC §6.2: a bare literal-list element is a string, normalised when it is
/// an identifier; numbers and booleans keep their literal spelling.
fn normalised_literal(value: &SyntaxValue) -> SyntaxValue {
    match value {
        SyntaxValue::Bare(text)
            if !is_int_literal(text) && !is_float_literal(text) && is_identifier(text) =>
        {
            SyntaxValue::Bare(normalise(text))
        }
        other => other.clone(),
    }
}

/// The `{kind}` substitution of E522 for a value that carries no text.
fn value_word(value: &SyntaxValue) -> &'static str {
    match value {
        SyntaxValue::List(_, _) => "a list",
        SyntaxValue::Tag { .. } => "a tag object",
        SyntaxValue::Object(_) => "an object",
        SyntaxValue::Bare(_) | SyntaxValue::Quoted(_) => "text",
    }
}

/// The text a scalar value contributes to a path segment or to interpolation.
fn scalar_text(value: &SyntaxValue) -> Option<String> {
    match value {
        SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => Some(text.clone()),
        _ => None,
    }
}

/// Resolves every `$` reference in a value a `derive` is about to write
/// (SPEC §5.11, §6.4).
fn interpolate_value(
    value: &SyntaxValue,
    variables: &VariableTable,
    at: &Located,
) -> Result<SyntaxValue, Diagnostics> {
    Ok(match value {
        SyntaxValue::Bare(text) => SyntaxValue::Bare(substitute(text, variables, Some(at))?.0),
        SyntaxValue::Quoted(text) => SyntaxValue::Quoted(substitute(text, variables, Some(at))?.0),
        SyntaxValue::List(items, spelling) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(interpolate_value(item, variables, at)?);
            }
            SyntaxValue::List(out, *spelling)
        }
        SyntaxValue::Object(fields) => {
            let mut out = Vec::with_capacity(fields.len());
            for (name, item) in fields {
                out.push((name.clone(), interpolate_value(item, variables, at)?));
            }
            SyntaxValue::Object(out)
        }
        SyntaxValue::Tag { name, args } => SyntaxValue::Tag {
            name: name.clone(),
            args: args.clone(),
        },
    })
}

/// Writes one value at a dotted path, creating the intermediate group and
/// `$(Schema)` objects a `derive` needs (SPEC §6.4).
fn write_at(fields: &mut Vec<(String, SyntaxValue)>, segments: &[String], value: SyntaxValue) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    if rest.is_empty() {
        match fields.iter_mut().find(|(key, _)| key == first) {
            Some((_, slot)) => *slot = value,
            None => fields.push((first.clone(), value)),
        }
        return;
    }
    let position = match fields.iter().position(|(key, _)| key == first) {
        Some(index) => index,
        None => {
            fields.push((first.clone(), SyntaxValue::Object(Vec::new())));
            fields.len() - 1
        }
    };
    // An intermediate segment always names a group or a `$(Schema)` field, so
    // a value that is not an object was already rejected by §7.3 step 3.
    if !matches!(fields[position].1, SyntaxValue::Object(_)) {
        fields[position].1 = SyntaxValue::Object(Vec::new());
    }
    if let SyntaxValue::Object(inner) = &mut fields[position].1 {
        write_at(inner, rest, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    #[test]
    fn operator_spellings_match_the_catalogue() {
        assert_eq!(ComparisonOp::Equal.spelling(), "==");
        assert_eq!(ComparisonOp::NotEqual.spelling(), "!=");
        assert_eq!(ComparisonOp::Contains.spelling(), "contains");
    }

    fn tables(text: &str) -> Result<TemplateTables, Diagnostics> {
        let source = SourceFile::new("data/T.abt", text);
        let tokens = crate::lexer::tokenize(&source)?;
        let file = crate::schema::parse_template("data/T.abt", &tokens)?;
        crate::schema::build_tables(std::slice::from_ref(&file))
    }

    fn check(text: &str) -> Diagnostics {
        match tables(text) {
            Ok(tables) => check_logic_blocks(&tables),
            Err(errors) => errors,
        }
    }

    fn ids(errors: &Diagnostics) -> Vec<ErrorId> {
        errors.iter().map(|item| item.id).collect()
    }

    const SCHEMA: &str = "schema T {\n\
        \x20   name: text(1..40)\n\
        \x20   count: int\n\
        \x20   ready: bool @optional\n\
        \x20   tags[]: text(1..20)\n\
        \x20   owner {\n\
        \x20       team: text(1..40)\n\
        \x20   }\n\
        \x20   caps[] {\n\
        \x20       id: enum(search, sync, banned) @tag\n\
        \x20   }\n\
        }\n";

    fn with_logic(body: &str) -> String {
        format!("{SCHEMA}\nlogic T {{\n{body}}}\n")
    }

    #[test]
    fn a_block_parses_every_statement_form() {
        let tables = tables(&with_logic(
            "    derive .name = \"a\"\n\
             \x20   derive? .ready = true\n\
             \x20   require .count > 0 else throw \"positive\"\n\
             \x20   if .ready == true {\n\
             \x20       derive .name = \"b\"\n\
             \x20   } else if .count == 1 {\n\
             \x20       derive .name = \"c\"\n\
             \x20   } else {\n\
             \x20       derive .name = \"d\"\n\
             \x20   }\n\
             \x20   for $tag in .tags {\n\
             \x20       derive .name = $tag\n\
             \x20   }\n",
        ))
        .expect("the block parses");
        let block = tables.logic_for("T").expect("a block");
        assert_eq!(block.statements.len(), 5);
        assert!(matches!(
            block.statements[1],
            LogicStatement::Derive {
                if_missing: true,
                ..
            }
        ));
        assert!(check_logic_blocks(&tables).is_empty());
    }

    #[test]
    fn precedence_binds_and_tighter_than_or_and_not_over_a_comparison() {
        let tables = tables(&with_logic(
            "    require .ready == true or .count == 1 and .name == \"a\" else throw \"x\"\n\
             \x20   require not .name == \"a\" else throw \"y\"\n",
        ))
        .expect("the block parses");
        let block = tables.logic_for("T").expect("a block");
        let LogicStatement::Require { condition, .. } = &block.statements[0] else {
            panic!("a require");
        };
        let Condition::Or(parts) = condition else {
            panic!("or is loosest");
        };
        assert_eq!(parts.len(), 2);
        assert!(matches!(parts[1], Condition::And(_)));

        let LogicStatement::Require { condition, .. } = &block.statements[1] else {
            panic!("a require");
        };
        let Condition::Not(inner) = condition else {
            panic!("not wraps the comparison");
        };
        assert!(matches!(**inner, Condition::Comparison { .. }));
    }

    #[test]
    fn a_chained_comparison_is_e512_and_an_unknown_operator_is_e511() {
        assert_eq!(
            ids(&check(&with_logic(
                "    require .count < 1 < 2 else throw \"x\"\n"
            ))),
            [ErrorId::E512]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    require .name = \"a\" else throw \"x\"\n"
            ))),
            [ErrorId::E511]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    require .name equals \"a\" else throw \"x\"\n"
            ))),
            [ErrorId::E511]
        );
    }

    #[test]
    fn a_require_needs_a_quoted_else_throw() {
        assert_eq!(
            ids(&check(&with_logic("    require .ready\n"))),
            [ErrorId::E507]
        );
        assert_eq!(
            ids(&check(&with_logic("    require .ready else throw 42\n"))),
            [ErrorId::E508]
        );
    }

    #[test]
    fn a_derive_target_is_checked_against_the_schema() {
        assert_eq!(
            ids(&check(&with_logic("    derive .id = \"x\"\n"))),
            [ErrorId::E504]
        );
        assert_eq!(
            ids(&check(&with_logic("    derive .template = \"x\"\n"))),
            [ErrorId::E504]
        );
        assert_eq!(
            ids(&check(&with_logic("    derive .nope = \"x\"\n"))),
            [ErrorId::E505]
        );
        assert_eq!(
            ids(&check(&with_logic("    derive .name.team = \"x\"\n"))),
            [ErrorId::E505]
        );
        assert_eq!(
            ids(&check(&with_logic("    derive .caps[0].id = search\n"))),
            [ErrorId::E506]
        );
        assert_eq!(
            ids(&check(&with_logic("    derive .caps.id = search\n"))),
            [ErrorId::E506]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    for $s in [a] {\n        derive .owner.$s = \"x\"\n    }\n"
            ))),
            [ErrorId::E520]
        );
    }

    #[test]
    fn an_unknown_derive_target_names_the_declared_fields() {
        let errors = check(&with_logic("    derive .nam = \"x\"\n"));
        let text = errors.to_string();
        assert!(text.contains("Did you mean 'name'?"), "{text}");
        assert!(text.contains("declared fields: name, count"), "{text}");
    }

    #[test]
    fn a_projected_operand_is_e519_and_a_list_comparison_is_e513() {
        assert_eq!(
            ids(&check(&with_logic(
                "    require .caps.id != \"banned\" else throw \"x\"\n"
            ))),
            [ErrorId::E519]
        );
        assert!(check(&with_logic(
            "    require not .caps.id contains \"banned\" else throw \"x\"\n"
        ))
        .is_empty());
        assert_eq!(
            ids(&check(&with_logic(
                "    require .tags == .tags else throw \"x\"\n"
            ))),
            [ErrorId::E513]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    require .name < .count else throw \"x\"\n"
            ))),
            [ErrorId::E513]
        );
        assert_eq!(
            ids(&check(&with_logic("    require .name else throw \"x\"\n"))),
            [ErrorId::E513]
        );
        // SPEC §6.6: a bare condition is valid only when it names a `bool`
        // field, so a bare literal is E513 even when it is a boolean literal.
        for literal in ["true", "false", "not true", "1", "\"a\""] {
            assert_eq!(
                ids(&check(&with_logic(&format!(
                    "    require {literal} else throw \"x\"\n"
                )))),
                [ErrorId::E513],
                "{literal}"
            );
        }
        assert!(check(&with_logic("    require .ready else throw \"x\"\n")).is_empty());
    }

    #[test]
    fn length_needs_a_list_or_text_argument() {
        assert!(check(&with_logic(
            "    require length(.tags) > 0 else throw \"x\"\n"
        ))
        .is_empty());
        assert!(check(&with_logic(
            "    require length(.name) > 0 else throw \"x\"\n"
        ))
        .is_empty());
        assert_eq!(
            ids(&check(&with_logic(
                "    require length(.count) > 0 else throw \"x\"\n"
            ))),
            [ErrorId::E514]
        );
    }

    #[test]
    fn loop_variables_must_be_bound_and_never_shadow() {
        assert_eq!(
            ids(&check(&with_logic(
                "    require $slot == \"a\" else throw \"x\"\n"
            ))),
            [ErrorId::E510]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    for $t in .tags {\n        for $t in .tags {\n        }\n    }\n"
            ))),
            [ErrorId::E516]
        );
        assert_eq!(
            ids(&check(&with_logic("    for $t in .name {\n    }\n"))),
            [ErrorId::E509]
        );
        assert_eq!(
            ids(&check(&with_logic("    for t in .tags {\n    }\n"))),
            [ErrorId::E210]
        );
    }

    #[test]
    fn a_variable_in_a_value_or_a_message_names_a_root_field() {
        assert!(check(&with_logic("    derive .name = \"all $name\"\n")).is_empty());
        assert!(check(&with_logic("    derive .name = $id\n")).is_empty());
        assert_eq!(
            ids(&check(&with_logic("    derive .name = \"all $names\"\n"))),
            [ErrorId::E503]
        );
        assert_eq!(
            ids(&check(&with_logic(
                "    require .ready == true else throw \"$nope\"\n"
            ))),
            [ErrorId::E503]
        );
    }

    #[test]
    fn an_unknown_read_path_is_e503_with_a_suggestion() {
        let errors = check(&with_logic("    require .nam == \"a\" else throw \"x\"\n"));
        assert_eq!(ids(&errors), [ErrorId::E503]);
        let text = errors.to_string();
        assert!(
            text.contains("Unknown path '.nam' in logic for 'T'"),
            "{text}"
        );
        assert!(text.contains("Did you mean 'name'?"), "{text}");
    }

    #[test]
    fn a_misplaced_else_and_a_one_line_block_are_reported() {
        assert_eq!(
            ids(&check(&with_logic("    else {\n    }\n"))),
            [ErrorId::E517]
        );
        assert_eq!(
            ids(&check(
                "schema T {\n    name: text\n}\nlogic T { derive .name = \"a\" }\n"
            )),
            [ErrorId::E210]
        );
    }

    #[test]
    fn block_nesting_is_bounded() {
        let mut body = String::new();
        for _ in 0..200 {
            body.push_str("    if .ready == true {\n");
        }
        for _ in 0..200 {
            body.push_str("    }\n");
        }
        let errors = check(&with_logic(&body));
        assert_eq!(ids(&errors), [ErrorId::E209]);
    }

    #[test]
    fn a_dynamic_segment_is_checked_against_the_union_of_the_types_it_reaches() {
        // SPEC §6.5: after a dynamic segment the scope is the declared types
        // of the fields at that position, and a following literal segment must
        // be declared by at least one of them.
        let template = "schema G {\n\
             \x20   slots {\n\
             \x20       head {\n            mode: text(1..20) @optional\n        }\n\
             \x20       chest {\n            mode: text(1..20) @optional\n        }\n\
             \x20   }\n\
             }\n\
             logic G {\n    for $s in [head] {\n%STATEMENT%\n    }\n}\n";
        assert!(check(&template.replace(
            "%STATEMENT%",
            "        require .slots.$s.mode exists else throw \"x\"",
        ))
        .is_empty());
        assert_eq!(
            ids(&check(&template.replace(
                "%STATEMENT%",
                "        require .slots.$s.mod exists else throw \"x\"",
            ))),
            [ErrorId::E503]
        );
    }

    #[test]
    fn a_loop_variable_at_the_head_of_a_derive_path_must_be_bound() {
        // Conformance LOGIC-14: `$owner.team` is a loop path, so an unbound
        // `$owner` is E510 even in a derive value.
        assert_eq!(
            ids(&check(&with_logic("    derive .name = $owner.team\n"))),
            [ErrorId::E510]
        );
        assert!(check(&with_logic("    derive .name = .owner.team\n")).is_empty());
    }

    #[test]
    fn length_of_a_loop_variable_follows_the_element_type() {
        assert!(check(&with_logic(
            "    for $t in .tags {\n        require length($t) > 0 else throw \"x\"\n    }\n"
        ))
        .is_empty());
        assert_eq!(
            ids(&check(
                "schema T {\n    counts[]: int\n}\n\
                 logic T {\n    for $n in .counts {\n        require length($n) > 0 else throw \"x\"\n    }\n}\n"
            )),
            [ErrorId::E514]
        );
    }

    #[test]
    fn a_derive_on_a_defaulted_field_is_e518() {
        let text = "schema T {\n    wave: int = 1\n}\nlogic T {\n    derive? .wave = 3\n}\n";
        assert_eq!(ids(&check(text)), [ErrorId::E518]);
    }

    #[test]
    fn blank_lines_and_comments_may_sit_between_a_block_and_its_else() {
        // SPEC §3.6 rule (b), §6.3: the rule tests the next *token*, and
        // neither a blank line nor a comment line produces one, so any
        // number of them may separate a `}` from its `else`, a `}` from its
        // `else if`, and a `require` condition from its `else throw`.
        let tables = tables(&with_logic(
            "    if .ready == true {\n\
             \x20       derive .name = \"a\"\n\
             \x20   }\n\
             \n\
             \x20   // a comment between the brace and the else if\n\
             \x20   else if .count == 1 {\n\
             \x20       derive .name = \"b\"\n\
             \x20   }\n\
             \n\
             \x20   else {\n\
             \x20       derive .name = \"c\"\n\
             \x20   }\n\
             \n\
             \x20   require .count > 0\n\
             \n\
             \x20       // and between a require and its else\n\
             \x20       else throw \"positive\"\n",
        ))
        .expect("the block parses");
        let block = tables.logic_for("T").expect("a block");
        assert_eq!(block.statements.len(), 2);
        let LogicStatement::If {
            branches,
            otherwise,
            ..
        } = &block.statements[0]
        else {
            panic!("an if");
        };
        assert_eq!(branches.len(), 2);
        assert!(otherwise.is_some());
        assert!(matches!(
            block.statements[1],
            LogicStatement::Require { .. }
        ));
        assert!(check_logic_blocks(&tables).is_empty());
    }

    #[test]
    fn a_statement_between_a_block_and_its_else_is_malformed() {
        // Only blank lines and comments carry no token. Rule (b) does not
        // ask what precedes the `else`, so a statement in between is joined
        // to it and the joined logical line is malformed where it is
        // malformed — not silently accepted (SPEC §3.6 rule (b), §6.3).
        let errors = check(&with_logic(
            "    if .ready == true {\n\
             \x20       derive .name = \"a\"\n\
             \x20   }\n\
             \x20   derive .count = 1\n\
             \x20   else {\n\
             \x20       derive .name = \"c\"\n\
             \x20   }\n",
        ));
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E210));

        // An `else` that does begin a logical line is E517.
        assert_eq!(
            check(&with_logic("    else {\n    }\n"))
                .first()
                .map(|item| item.id),
            Some(ErrorId::E517)
        );
    }

    #[test]
    fn e511_reports_the_two_reachable_malformed_conditions() {
        // SPEC §6.6: a token that cannot begin an operand, and a token
        // that is not an operator where one is due. There is no third.
        let errors = check(&with_logic(
            "    require .name === \"a\" else throw \"x\"\n",
        ));
        assert_eq!(ids(&errors), [ErrorId::E511]);
        let text = errors.to_string();
        assert!(text.contains("'=' is not an operand"), "{text}");

        let errors = check(&with_logic(
            "    require (.ready == true) xor (.count == 1) else throw \"x\"\n",
        ));
        assert_eq!(ids(&errors), [ErrorId::E511]);
        let text = errors.to_string();
        assert!(text.contains("'xor' is not an operator"), "{text}");
    }

    #[test]
    fn an_unbalanced_parenthesis_is_lexical_and_never_e511() {
        // SPEC §6.6: `(` and `)` ride the value-bracket stack of §3.6, so
        // the lexer reports E203 and E205 before a condition is parsed.
        let errors = check(&with_logic(
            "    require (.ready == true else throw \"x\"\n",
        ));
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E203));
        assert!(!ids(&errors).contains(&ErrorId::E511));

        let errors = check(&with_logic(
            "    require .ready == true) else throw \"x\"\n",
        ));
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E205));
        assert!(!ids(&errors).contains(&ErrorId::E511));
    }
}

/// Evaluation, run through the whole of P2 to P4 so that the order of SPEC
/// §7.3 is what is exercised rather than a hand-built object.
#[cfg(test)]
mod evaluation_tests {
    use super::*;
    use crate::ast::{parse, SourceUnit};
    use crate::output::Value;
    use crate::resolve::{check_instances, collect_instances, InstanceTable};
    use crate::source::SourceFile;
    use crate::validate::AssetChecks;
    use crate::versions::materialise;
    use std::path::Path;

    fn compile(template: &str, instance: &str) -> Result<Vec<Vec<Value>>, Diagnostics> {
        let source = SourceFile::new("data/schema.abt", template);
        let SourceUnit::Template(file) = parse(&source)? else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file])?;
        // P3 checks the schemas and the logic blocks together (SPEC §7.2).
        crate::schema::validate_schemas(&tables)?;

        let source = SourceFile::new("data/items.ab", instance);
        let SourceUnit::Instance(file) = parse(&source)? else {
            panic!("an instance file");
        };
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables)?;
        let instances = InstanceTable::new(declarations);
        materialise(
            &tables,
            &instances,
            None,
            Path::new("assets"),
            AssetChecks::Skipped,
        )
    }

    fn last(template: &str, instance: &str) -> Value {
        let documents = compile(template, instance)
            .unwrap_or_else(|errors| panic!("the project compiles:\n{errors}"));
        documents
            .last()
            .and_then(|document| document.first())
            .cloned()
            .expect("one object")
    }

    fn failure(template: &str, instance: &str) -> Diagnostics {
        compile(template, instance).expect_err("the project is rejected")
    }

    #[test]
    fn a_derive_writes_a_literal_a_path_a_length_and_the_version() {
        let object = last(
            "schema T {\n\
             \x20   title: text(1..40)\n\
             \x20   tags[]: text(1..20)\n\
             \x20   label: text(1..40) @optional\n\
             \x20   count: int @optional\n\
             \x20   schema_version: int @optional\n\
             }\n\
             logic T {\n\
             \x20   derive .label = .title\n\
             \x20   derive .count = length(.tags)\n\
             \x20   derive .schema_version = version\n\
             }\n",
            "T :: @id.one\n    title: Atlas\n    tags: a, b, c\n",
        );
        assert_eq!(object.get("label"), Some(&Value::Text("Atlas".to_string())));
        assert_eq!(object.get("count"), Some(&Value::Int(3)));
        assert_eq!(object.get("schema_version"), Some(&Value::Int(1)));
    }

    #[test]
    fn a_derive_result_must_be_assignable_to_its_target() {
        // SPEC §6.4: length() yields a number, which a text field refuses.
        let errors = failure(
            "schema T {\n    tags[]: text(1..20)\n    len: text(1..40) @optional\n}\n\
             logic T {\n    derive .len = length(.tags)\n}\n",
            "T :: @id.one\n    tags: a\n",
        );
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E412));
        let text = errors.to_string();
        assert!(
            text.contains("Type mismatch at one.len: expected text, found int."),
            "{text}"
        );
    }

    #[test]
    fn exists_is_presence_and_never_truthiness() {
        // SPEC §6.7, conformance LOGIC-06: false, 0 and "" are present.
        let object = last(
            "schema T {\n\
             \x20   featured: bool @optional\n\
             \x20   count: int @optional\n\
             \x20   note: text @optional\n\
             \x20   empty[]: text @optional\n\
             \x20   seen: bool @optional\n\
             }\n\
             logic T {\n\
             \x20   if not .featured exists {\n        derive .featured = true\n    }\n\
             \x20   if not .count exists {\n        derive .count = 99\n    }\n\
             \x20   if not .note exists {\n        derive .note = \"filled-in\"\n    }\n\
             \x20   if .empty exists {\n        derive .seen = true\n    }\n\
             }\n",
            "T :: @id.one\n    featured: false\n    count: 0\n    note: \"\"\n    empty: []\n",
        );
        assert_eq!(object.get("featured"), Some(&Value::Bool(false)));
        assert_eq!(object.get("count"), Some(&Value::Int(0)));
        assert_eq!(object.get("note"), Some(&Value::Text(String::new())));
        // An empty list is not present, so the guard did not fire.
        assert_eq!(object.get("seen"), None);
    }

    #[test]
    fn contains_is_a_substring_on_text_and_membership_on_a_list() {
        // SPEC §6.8, conformance LOGIC-17.
        let errors = failure(
            "schema Doc {\n    body: text(1..200)\n}\n\
             logic Doc {\n    require not .body contains \"forbidden\" else throw \"no.\"\n}\n",
            "Doc :: @id.one\n    body: \"this text is forbidden by policy\"\n",
        );
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E515));
        assert!(errors.to_string().contains("Doc logic: no."), "{errors}");

        // A projected path over a list of groups is the supported spelling of
        // "no element is X" (SPEC §6.5).
        let errors = failure(
            "schema T {\n    caps[] {\n        id: enum(search, banned) @tag\n    }\n}\n\
             logic T {\n    require not .caps.id contains \"banned\" else throw \"banned.\"\n}\n",
            "T :: @id.one\n    caps: [#search, #banned]\n",
        );
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E515));
    }

    #[test]
    fn string_comparison_is_exact_and_a_number_never_equals_a_string() {
        let template = "schema T {\n\
             \x20   status: enum(active, disabled)\n\
             \x20   count: int\n\
             \x20   hit: bool @optional\n\
             \x20   numeric: bool @optional\n\
             }\n\
             logic T {\n\
             \x20   if .status == \"Active\" {\n        derive .hit = true\n    }\n\
             \x20   if .count == \"2\" {\n        derive .numeric = true\n    }\n\
             }\n";
        let object = last(template, "T :: @id.one\n    status: active\n    count: 2\n");
        assert_eq!(object.get("hit"), None);
        assert_eq!(object.get("numeric"), None);
    }

    #[test]
    fn a_for_loop_binds_each_element_and_the_last_write_wins() {
        // SPEC §6.4: a derive inside a for executes once per element.
        let object = last(
            "schema T {\n    tags[]: text(1..20)\n    seen: text(1..20) @optional\n}\n\
             logic T {\n    for $t in .tags {\n        derive .seen = $t\n    }\n}\n",
            "T :: @id.one\n    tags: alpha, beta, gamma\n",
        );
        assert_eq!(object.get("seen"), Some(&Value::Text("gamma".to_string())));
    }

    #[test]
    fn a_dynamic_segment_reads_the_field_a_loop_variable_names() {
        // Conformance LOGIC-19: the read side and the write side agree.
        let object = last(
            "schema T {\n\
             \x20   slots {\n        head: text(1..20) @optional\n    }\n\
             \x20   note: text(1..80) @optional\n\
             }\n\
             logic T {\n\
             \x20   for $Slot in [head] {\n\
             \x20       require .slots.$Slot exists else throw \"slot $Slot is empty\"\n\
             \x20       derive .note = \"filled slot $Slot\"\n\
             \x20   }\n\
             }\n",
            "T :: @id.one\n    slots.head: manual\n",
        );
        assert_eq!(
            object.get("note"),
            Some(&Value::Text("filled slot head".to_string()))
        );
    }

    #[test]
    fn a_derive_value_resolves_variables_against_the_current_object_once() {
        // Conformance LOGIC-24: `$$` is a literal `$` and the result of one
        // substitution is never rescanned.
        let object = last(
            "schema T {\n    name: text(1..40)\n    note: text(1..80) @optional\n}\n\
             logic T {\n    derive .note = \"name is $name\"\n}\n",
            "T :: @id.one\n    name: \"$$id\"\n",
        );
        assert_eq!(object.get("name"), Some(&Value::Text("$id".to_string())));
        assert_eq!(
            object.get("note"),
            Some(&Value::Text("name is $id".to_string()))
        );

        // Conformance LOGIC-05: a loop variable bound to the text `$b` is
        // inserted verbatim, whatever `$b` would mean.
        let object = last(
            "schema T {\n    label: text(1..40) @optional\n}\n\
             logic T {\n\
             \x20   for $a in [\"$b\"] {\n\
             \x20       for $b in [7] {\n\
             \x20           derive .label = \"value=$a\"\n\
             \x20       }\n\
             \x20   }\n\
             }\n",
            "T :: @id.one\n",
        );
        assert_eq!(
            object.get("label"),
            Some(&Value::Text("value=$b".to_string()))
        );
    }

    #[test]
    fn derive_question_writes_only_where_no_value_stands() {
        let template = "schema T {\n    wave: int @optional\n}\n\
             logic T {\n    derive? .wave = 9\n}\n";
        assert_eq!(
            last(template, "T :: @id.one\n    wave: 3\n").get("wave"),
            Some(&Value::Int(3))
        );
        assert_eq!(
            last(template, "T :: @id.one\n").get("wave"),
            Some(&Value::Int(9))
        );
    }

    #[test]
    fn a_derive_may_satisfy_a_required_field() {
        // Conformance LOGIC-18: the required-field check runs after logic.
        let object = last(
            "schema T {\n    kind: enum(basic, pro)\n    shipping_class: text(1..40)\n}\n\
             logic T {\n\
             \x20   if .kind == \"pro\" {\n        derive .shipping_class = \"express\"\n    }\n\
             \x20   else {\n        derive .shipping_class = \"standard\"\n    }\n\
             }\n",
            "T :: @id.one\n    kind: pro\n",
        );
        assert_eq!(
            object.get("shipping_class"),
            Some(&Value::Text("express".to_string()))
        );
    }

    #[test]
    fn a_derive_into_an_absent_optional_group_makes_it_present() {
        let object = last(
            "schema T {\n\
             \x20   owner @optional {\n        team: text(1..40)\n        rank: int = 3\n    }\n\
             }\n\
             logic T {\n    derive .owner.team = \"Knowledge Systems\"\n}\n",
            "T :: @id.one\n",
        );
        let owner = object.get("owner").expect("the group is present");
        assert_eq!(
            owner.get("team"),
            Some(&Value::Text("Knowledge Systems".to_string()))
        );
        // The group's own default is filled and checked afterwards.
        assert_eq!(owner.get("rank"), Some(&Value::Int(3)));
    }

    #[test]
    fn the_version_builtin_guards_a_write_and_an_unguarded_one_is_e521() {
        let unguarded = "versions 1..2\n\
             schema T {\n    glow: bool @optional @since(2)\n}\n\
             logic T {\n    derive .glow = true\n}\n";
        let errors = failure(unguarded, "T :: @id.one\n");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E521));
        let text = errors.to_string();
        assert!(
            text.contains(
                "derive cannot write one.glow in version 1; the field exists in versions 2."
            ),
            "{text}"
        );
        assert!(text.contains("if version >= 2"), "{text}");

        let guarded = "versions 1..2\n\
             schema T {\n    glow: bool @optional @since(2)\n}\n\
             logic T {\n    if version >= 2 {\n        derive .glow = true\n    }\n}\n";
        let documents = compile(guarded, "T :: @id.one\n").expect("the guard holds");
        assert_eq!(documents[0][0].get("glow"), None);
        assert_eq!(documents[1][0].get("glow"), Some(&Value::Bool(true)));
    }

    #[test]
    fn a_failed_require_names_the_instance_the_version_and_the_loop_index() {
        let errors = failure(
            "schema T {\n    tags[]: text(1..20)\n}\n\
             logic T {\n\
             \x20   for $t in .tags {\n\
             \x20       require $t != \"bad\" else throw \"tag $t is not allowed\"\n\
             \x20   }\n\
             }\n",
            "T :: @id.one\n    tags: good, bad\n",
        );
        assert_eq!(errors.len(), 1);
        let text = errors.to_string();
        assert!(text.contains("T logic: tag bad is not allowed"), "{text}");
        assert!(text.contains("instance 'one'."), "{text}");
        assert!(text.contains("compiling version 1."), "{text}");
        assert!(text.contains("at index 1, item bad."), "{text}");
    }

    #[test]
    fn length_counts_scalar_values_of_a_string_and_elements_of_a_list() {
        // Conformance LOGIC-22.
        let object = last(
            "schema T {\n    name: text\n    short: bool @optional\n}\n\
             logic T {\n\
             \x20   require length(.name) >= 4 else throw \"too short\"\n\
             \x20   if length(.name) == 1 {\n        derive .short = true\n    }\n\
             }\n",
            "T :: @id.one\n    name: a-very-long-product-name\n",
        );
        assert_eq!(object.get("short"), None);
    }

    #[test]
    fn an_absent_operand_makes_every_comparison_false_including_inequality() {
        let object = last(
            "schema T {\n\
             \x20   maybe: text(1..20) @optional\n\
             \x20   equal: bool @optional\n\
             \x20   different: bool @optional\n\
             }\n\
             logic T {\n\
             \x20   if .maybe == \"x\" {\n        derive .equal = true\n    }\n\
             \x20   if .maybe != \"x\" {\n        derive .different = true\n    }\n\
             }\n",
            "T :: @id.one\n",
        );
        assert_eq!(object.get("equal"), None);
        assert_eq!(object.get("different"), None);
    }

    #[test]
    fn a_statement_that_never_executes_writes_nothing_and_reports_nothing() {
        // SPEC §6.4: E521 is raised at execution, so an untaken branch and a
        // loop over an empty list report nothing.
        let template = "versions 1..2\n\
             schema T {\n\
             \x20   tags[]: text(1..20) @optional\n\
             \x20   glow: bool @optional @since(2)\n\
             }\n\
             logic T {\n\
             \x20   if version >= 2 {\n        derive .glow = true\n    }\n\
             \x20   for $t in .tags {\n        derive .glow = true\n    }\n\
             }\n";
        let documents = compile(template, "T :: @id.one\n    tags: []\n")
            .unwrap_or_else(|errors| panic!("nothing executes in version 1:\n{errors}"));
        assert_eq!(documents[0][0].get("glow"), None);
        assert_eq!(documents[1][0].get("glow"), Some(&Value::Bool(true)));
    }

    #[test]
    fn a_parenthesised_condition_is_an_operand_and_a_bool_field_is_a_condition() {
        let object = last(
            "schema T {\n\
             \x20   ready: bool\n\
             \x20   spare: bool\n\
             \x20   same: bool @optional\n\
             \x20   plain: bool @optional\n\
             }\n\
             logic T {\n\
             \x20   if (not .ready) == .spare {\n        derive .same = true\n    }\n\
             \x20   if .ready {\n        derive .plain = true\n    }\n\
             }\n",
            "T :: @id.one\n    ready: true\n    spare: false\n",
        );
        assert_eq!(object.get("same"), Some(&Value::Bool(true)));
        assert_eq!(object.get("plain"), Some(&Value::Bool(true)));
    }

    #[test]
    fn and_binds_tighter_than_or_when_the_condition_runs() {
        // `a or b and c` is `a or (b and c)` (SPEC §6.6).
        let template = "schema T {\n\
             \x20   a: bool\n    b: bool\n    c: bool\n    hit: bool @optional\n\
             }\n\
             logic T {\n\
             \x20   if .a == true or .b == true and .c == true {\n\
             \x20       derive .hit = true\n\
             \x20   }\n\
             }\n";
        let object = last(
            template,
            "T :: @id.one\n    a: false\n    b: true\n    c: false\n",
        );
        assert_eq!(object.get("hit"), None);
        let object = last(
            template,
            "T :: @id.one\n    a: true\n    b: true\n    c: false\n",
        );
        assert_eq!(object.get("hit"), Some(&Value::Bool(true)));
    }

    #[test]
    fn a_derive_writes_a_whole_list_and_a_loop_variable_keeps_its_type() {
        let object = last(
            "schema T {\n\
             \x20   tags[]: text(1..20) @optional\n\
             \x20   slot_count: int @optional\n\
             }\n\
             logic T {\n\
             \x20   derive .tags = [alpha, beta]\n\
             \x20   for $n in [7] {\n        derive .slot_count = $n\n    }\n\
             }\n",
            "T :: @id.one\n",
        );
        assert_eq!(
            object.get("tags"),
            Some(&Value::List(vec![
                Value::Text("alpha".to_string()),
                Value::Text("beta".to_string()),
            ]))
        );
        assert_eq!(object.get("slot_count"), Some(&Value::Int(7)));
    }

    #[test]
    fn a_derived_value_is_validated_at_the_statement_that_wrote_it() {
        let errors = failure(
            "schema T {\n    count: int(0..9) @optional\n}\n\
             logic T {\n    derive .count = 42\n}\n",
            "T :: @id.one\n",
        );
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E413));
        let text = errors.to_string();
        assert!(text.contains("data/schema.abt:5:5"), "{text}");
    }

    #[test]
    fn evaluation_is_deterministic_and_reports_the_same_first_failure() {
        let template = "schema T {\n    tags[]: text(1..20)\n}\n\
             logic T {\n\
             \x20   for $t in .tags {\n\
             \x20       require $t != \"bad\" else throw \"first: $t\"\n\
             \x20   }\n\
             }\n";
        let instance = "T :: @id.one\n    tags: a, bad, b, bad\n";
        let first = failure(template, instance).to_string();
        let second = failure(template, instance).to_string();
        assert_eq!(first, second);
        assert!(first.contains("at index 1, item bad."), "{first}");
    }

    /// A logic block of `widths.len()` nested `for` statements, the outer
    /// one over a literal list of `widths[0]` elements and so on inward.
    /// It executes `sum over k of (widths[0] * .. * widths[k])` bodies,
    /// which is what SPEC §3.7 charges.
    fn nested_loops(widths: &[usize]) -> String {
        let mut text = String::from("schema T {\n    seen: int @optional\n}\nlogic T {\n");
        for (level, width) in widths.iter().enumerate() {
            let list = (1..=*width)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let pad = "    ".repeat(level + 1);
            text.push_str(&format!("{pad}for $v{level} in [{list}] {{\n"));
        }
        let pad = "    ".repeat(widths.len() + 1);
        text.push_str(&format!("{pad}derive .seen = $v{}\n", widths.len() - 1));
        for level in (0..widths.len()).rev() {
            text.push_str(&format!("{}}}\n", "    ".repeat(level + 1)));
        }
        text.push_str("}\n");
        text
    }

    #[test]
    fn logic_work_past_the_limit_of_the_specification_is_e523() {
        // SPEC §3.7: seven `for` blocks over a literal list of ten demand
        // 11 111 110 iterations and are E523, reported at the 1 000 001st
        // and at the `for` whose iteration crossed the bound.
        let errors = failure(&nested_loops(&[10; 7]), "T :: @id.one\n");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E523));
        let text = errors.to_string();
        assert!(
            text.contains(
                "Logic work limit exceeded at one: the logic executed more than \
                 1000000 loop iterations in version 1."
            ),
            "{text}"
        );
        // The bound is named, and the position is the second `for`: one
        // iteration of it costs 111 111, and the tenth crosses 1 000 000.
        assert!(text.contains("data/schema.abt:6:9"), "{text}");
    }

    #[test]
    fn logic_work_under_the_limit_compiles() {
        // Five of the same blocks demand 111 110 iterations and compile.
        let object = last(&nested_loops(&[10; 5]), "T :: @id.one\n");
        assert_eq!(object.get("seen"), Some(&Value::Int(10)));
    }

    #[test]
    fn the_logic_work_budget_is_fresh_for_every_instance() {
        // SPEC §3.7: the bound is on one instance in one version, not on
        // the project. Each of these two instances spends 511 106
        // iterations; a project-wide budget would refuse the second.
        let documents = compile(
            &nested_loops(&[46, 10, 10, 10, 10]),
            "T :: @id.one\n\nT :: @id.two\n",
        )
        .unwrap_or_else(|errors| panic!("the project compiles:\n{errors}"));
        let document = documents.last().expect("one version");
        assert_eq!(document.len(), 2);
        for object in document {
            assert_eq!(object.get("seen"), Some(&Value::Int(10)));
        }
    }
}
