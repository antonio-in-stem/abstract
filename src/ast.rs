//! The syntax tree produced by parsing (SPEC chapters 4, 5 and 6).
//!
//! Two file kinds parse into two roots: a template file holds schema
//! declarations, logic blocks and at most one `versions` declaration; an
//! instance file holds instances. Every node carries the position the parser
//! read it at, so every later phase can report a diagnostic with `file:line:col`.
//!
//! The bodies of the nodes below are filled in by the parser stages; this
//! module owns only the shapes they share, and the dispatch from a source file
//! to the parser its extension selects.

use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Position};
use crate::limits::PATH_SEGMENTS;
use crate::source::SourceFile;

/// The envelope keys, which no schema declares and no statement assigns
/// (SPEC §4.11, §5.4).
pub const ENVELOPE_KEYS: [&str; 2] = ["template", "id"];

/// A node's source position: the file it came from and where in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    /// Project-relative path, `/`-separated.
    pub file: String,
    pub position: Position,
}

impl Located {
    pub fn new(file: impl Into<String>, position: Position) -> Self {
        Self {
            file: file.into(),
            position,
        }
    }
}

/// The parsed form of one source file.
#[derive(Clone, Debug)]
pub enum SourceUnit {
    /// A `.abt` file: schemas, logic blocks and at most one `versions`.
    Template(TemplateFile),
    /// A `.ab` file: instance declarations.
    Instance(InstanceFile),
}

impl SourceUnit {
    /// The project-relative path this unit was parsed from.
    pub fn file(&self) -> &str {
        match self {
            SourceUnit::Template(template) => &template.file,
            SourceUnit::Instance(instance) => &instance.file,
        }
    }
}

/// A parsed template file (SPEC §4.1).
#[derive(Clone, Debug, Default)]
pub struct TemplateFile {
    pub file: String,
    pub items: Vec<TemplateItem>,
}

impl TemplateFile {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            items: Vec::new(),
        }
    }
}

/// One top-level item of a template file. Anything else is E210.
#[derive(Clone, Debug)]
pub enum TemplateItem {
    Schema(crate::schema::SchemaDecl),
    Logic(crate::logic::LogicBlock),
    Versions(crate::schema::VersionsDecl),
}

/// A parsed instance file (SPEC §5.1).
#[derive(Clone, Debug, Default)]
pub struct InstanceFile {
    pub file: String,
    pub instances: Vec<crate::instance::InstanceDecl>,
}

impl InstanceFile {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            instances: Vec::new(),
        }
    }
}

/// A dotted path, already normalised segment by segment (SPEC §3.3, §5.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<String>,
    pub at: Located,
}

impl Path {
    pub fn new(segments: Vec<String>, at: Located) -> Self {
        Self { segments, at }
    }

    /// The path as it is spelled in a diagnostic, `owner.team`.
    pub fn text(&self) -> String {
        self.segments.join(".")
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// The first segment, which decides whether the path names an envelope key
    /// (SPEC §5.4).
    pub fn first(&self) -> Option<&str> {
        self.segments.first().map(String::as_str)
    }

    /// The final segment, which names the field written.
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// This path with `prefix` in front, which is what a body block means
    /// (SPEC §5.4). The position stays the one this path was read at.
    pub fn prefixed(&self, prefix: &Path) -> Path {
        let mut segments = Vec::with_capacity(prefix.segments.len() + self.segments.len());
        segments.extend(prefix.segments.iter().cloned());
        segments.extend(self.segments.iter().cloned());
        Path::new(segments, self.at.clone())
    }

    /// True when the leading segment is `template` or `id`, which a statement
    /// may never assign (E410, SPEC §5.4).
    pub fn starts_at_envelope_key(&self) -> bool {
        self.first()
            .map(|segment| ENVELOPE_KEYS.contains(&segment))
            .unwrap_or(false)
    }

    /// True when the path has more segments than SPEC §3.7 allows, which is
    /// E209 at whichever construct produced it.
    pub fn exceeds_segment_limit(&self) -> bool {
        self.segments.len() > PATH_SEGMENTS
    }
}

/// Tokenizes and parses one source file, choosing the parser its extension
/// names (SPEC §2.1). A file that is neither `.ab` nor `.abt` never reaches
/// here: discovery collects only those two (SPEC §2.4).
pub fn parse(source: &SourceFile) -> Result<SourceUnit, Diagnostics> {
    if !source.is_template() && !source.is_instance() {
        return Err(Diagnostics::one(Diagnostic::in_file(
            ErrorId::E806,
            source.path.clone(),
            format!("'{}' is not an .ab or .abt file.", source.path),
        )));
    }

    // Lexing and parsing are one phase (SPEC §7.1 P1), so a defective stream
    // is still parsed and the two diagnostic lists are merged: within a phase
    // SPEC §11.1 orders diagnostics by source position, and a parse error
    // early in the file outranks a lexical error late in it.
    let (tokens, lexical) = crate::lexer::tokenize_recovering(source);
    let parsed = if source.is_template() {
        crate::schema::parse_template(&source.path, &source.text, &tokens).map(SourceUnit::Template)
    } else {
        crate::instance::parse_instances(&source.path, &tokens).map(SourceUnit::Instance)
    };

    match (parsed, lexical.is_empty()) {
        (Ok(unit), true) => Ok(unit),
        (Ok(_), false) => Err(lexical),
        (Err(syntactic), true) => Err(syntactic),
        (Err(syntactic), false) => Err(merge_by_position(lexical, syntactic)),
    }
}

/// Merges the two diagnostic lists of P1 into source-position order
/// (SPEC §11.1 rule 3), keeping the lexical one first at equal positions.
///
/// A line the lexer could not tokenize contributes only its lexical
/// diagnostic: whatever the parser then makes of the recovered tokens is a
/// consequence of that defect, not a second one.
fn merge_by_position(lexical: Diagnostics, syntactic: Diagnostics) -> Diagnostics {
    // Sorted, and searched rather than scanned: a hostile file can carry one
    // lexical defect per line, and the filter below runs once per parse
    // diagnostic.
    let mut spoilt: Vec<u32> = lexical
        .iter()
        .filter_map(|item| item.position.map(|position| position.line))
        .collect();
    spoilt.sort_unstable();
    let mut items: Vec<Diagnostic> = lexical.iter().cloned().collect();
    items.extend(
        syntactic
            .iter()
            .filter(|item| match item.position {
                Some(position) => spoilt.binary_search(&position.line).is_err(),
                None => true,
            })
            .cloned(),
    );
    items.sort_by_key(|item| {
        item.position
            .map(|position| (position.line, position.col))
            .unwrap_or((0, 0))
    });
    let mut merged = Diagnostics::new();
    for item in items {
        merged.push(item);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn located() -> Located {
        Located::new("data/a.ab", Position::new(3, 5))
    }

    fn ids(path: &str, text: &str) -> Vec<ErrorId> {
        let source = SourceFile::new(path, text);
        parse(&source)
            .expect_err("the source is rejected")
            .iter()
            .map(|item| item.id)
            .collect()
    }

    #[test]
    fn p1_reports_lexical_and_parse_diagnostics_in_position_order() {
        // Lexing and parsing are one phase (SPEC §7.1), and within a phase
        // SPEC §11.1 rule (3) orders by source position: the unknown top-level
        // construct on line 1 outranks the identifier defect on line 4.
        let reported = ids("data/a.abt", "schemma W {\n}\n\nmigrate W 1 -> 2 {\n}\n");
        assert_eq!(reported.first(), Some(&ErrorId::E210));
        assert!(reported.contains(&ErrorId::E207), "{reported:?}");
    }

    #[test]
    fn a_line_the_lexer_rejected_carries_no_second_diagnostic() {
        // The parser's view of a line the lexer could not tokenize is a
        // consequence of the same defect, not a second one.
        assert_eq!(
            ids("data/a.abt", "schema A {\n    caf\u{e9}: text\n}\n"),
            [ErrorId::E206]
        );
    }

    #[test]
    fn a_path_prints_its_segments_and_finds_its_ends() {
        let path = Path::new(vec!["owner".to_string(), "team".to_string()], located());
        assert_eq!(path.text(), "owner.team");
        assert_eq!(path.first(), Some("owner"));
        assert_eq!(path.last(), Some("team"));
        assert_eq!(path.len(), 2);
        assert!(!path.is_empty());
        assert!(!path.exceeds_segment_limit());
    }

    #[test]
    fn a_body_block_prefixes_the_paths_inside_it() {
        let prefix = Path::new(vec!["owner".to_string()], located());
        let inner = Path::new(vec!["team".to_string()], located());
        assert_eq!(inner.prefixed(&prefix).text(), "owner.team");
    }

    #[test]
    fn the_envelope_keys_are_recognised_at_the_head_of_a_path() {
        let template = Path::new(vec!["template".to_string()], located());
        let identity = Path::new(vec!["id".to_string()], located());
        let ordinary = Path::new(vec!["owner".to_string(), "id".to_string()], located());
        assert!(template.starts_at_envelope_key());
        assert!(identity.starts_at_envelope_key());
        assert!(!ordinary.starts_at_envelope_key());
    }

    #[test]
    fn a_path_longer_than_the_limit_is_reported() {
        let segments: Vec<String> = (0..=PATH_SEGMENTS).map(|index| index.to_string()).collect();
        assert!(Path::new(segments, located()).exceeds_segment_limit());
    }

    #[test]
    fn parsing_dispatches_on_the_file_extension() {
        // What is asserted here is the dispatch: each extension must reach the
        // parser it names rather than fail in the lexer.
        let template = SourceFile::new("data/Item.abt", "schema Item {\n}\n");
        let unit = parse(&template).expect("a template file parses");
        assert!(matches!(unit, SourceUnit::Template(_)));
        assert_eq!(unit.file(), "data/Item.abt");

        let instance = SourceFile::new("data/item.ab", "Item :: @id.x\n");
        let unit = parse(&instance).expect("an instance file parses");
        assert!(matches!(unit, SourceUnit::Instance(_)));
        assert_eq!(unit.file(), "data/item.ab");

        // A file that is neither is E806, which discovery never produces.
        let other = SourceFile::new("data/notes.txt", "");
        let error = parse(&other).expect_err("only .ab and .abt parse");
        assert_eq!(error.first().map(|item| item.id), Some(ErrorId::E806));
    }
}
