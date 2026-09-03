//! Building the authored object: the instance tables, clone folding,
//! interpolation and defaults (SPEC §5.7, §5.11, §7.2, §7.3 steps 1, 2 and 4).
//!
//! The order is fixed. Clones fold in source order into an initially empty
//! object; header tags and body statements are then applied by plain
//! assignment; interpolation resolves every `$` reference against the root
//! scalar fields of the result; defaults are filled afterwards and are
//! interpolated against the same table.

use std::collections::HashMap;

use crate::ast::{InstanceFile, Located};
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note};
use crate::instance::{InstanceDecl, ListSpelling, SyntaxValue};
use crate::lexer::normalise;
use crate::limits::{CLONE_CHAIN, GROUP_DEPTH};
use crate::schema::{
    suggest, DefaultValue, FieldDecl, FieldKind, SchemaDecl, TemplateTables, TieBreak, TypeExpr,
};
use crate::versions::{VersionRange, Window};

// -------------------------------------------------------------- the object

/// One step of the position of a value inside an authored object: a field
/// name, or the index of a list element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Field(String),
    Index(usize),
}

/// Where one value sits in an authored object, from the root down.
pub type ValuePath = Vec<Step>;

/// The authored object of one instance for one version: a tree of syntax
/// values keyed by normalised field name, in the order the keys were created.
///
/// Beside the values themselves the object carries two side tables no syntax
/// value can hold: the source position each path was written at, so that a
/// diagnostic about a value can name the statement that produced it, and the
/// byte ranges of each text value that interpolation inserted, so that a `{`
/// which arrived from a variable never delimits a brace group (SPEC §5.8).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthoredObject {
    pub fields: Vec<(String, SyntaxValue)>,
    /// Dotted path to the position of the construct that wrote it.
    origins: Vec<(String, Located)>,
    /// Byte ranges of text values that interpolation produced.
    inserted: Vec<(ValuePath, Vec<(usize, usize)>)>,
    /// The instance header, used when no more precise position is known.
    at: Option<Located>,
}

impl AuthoredObject {
    pub fn get(&self, name: &str) -> Option<&SyntaxValue> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut SyntaxValue> {
        self.fields
            .iter_mut()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// Writes a root field, keeping the position an existing key already has.
    pub fn set(&mut self, name: &str, value: SyntaxValue) {
        match self.get_mut(name) {
            Some(slot) => *slot = value,
            None => self.fields.push((name.to_string(), value)),
        }
    }

    /// Where the instance this object was built from was declared.
    pub fn at(&self) -> Option<&Located> {
        self.at.as_ref()
    }

    pub fn set_at(&mut self, at: Located) {
        self.at = Some(at);
    }

    /// Records that `path` was written at `at`.
    pub fn set_origin(&mut self, path: &str, at: Located) {
        match self.origins.iter_mut().find(|(key, _)| key == path) {
            Some((_, slot)) => *slot = at,
            None => self.origins.push((path.to_string(), at)),
        }
    }

    /// The position a diagnostic about `path` is reported at: where the path
    /// itself was written, or where its longest written prefix was, or the
    /// instance header.
    pub fn origin(&self, path: &str) -> Option<&Located> {
        let mut best: Option<(usize, &Located)> = None;
        for (key, at) in &self.origins {
            let covers = key == path
                || (path.len() > key.len()
                    && path.starts_with(key.as_str())
                    && path.as_bytes().get(key.len()) == Some(&b'.'));
            if !covers {
                continue;
            }
            if best.map(|(length, _)| key.len() > length).unwrap_or(true) {
                best = Some((key.len(), at));
            }
        }
        best.map(|(_, at)| at).or(self.at.as_ref())
    }

    /// The byte ranges of the text value at `path` that interpolation wrote.
    pub fn inserted_spans(&self, path: &[Step]) -> &[(usize, usize)] {
        self.inserted
            .iter()
            .find(|(key, _)| key.as_slice() == path)
            .map(|(_, spans)| spans.as_slice())
            .unwrap_or(&[])
    }

    /// Replaces the ranges recorded for one value. Interpretation calls this
    /// once a value is interpreted, so that a brace left in it is literal on
    /// every later walk (SPEC §5.8).
    pub fn set_spans(&mut self, path: ValuePath, spans: Vec<(usize, usize)>) {
        match self
            .inserted
            .iter_mut()
            .find(|(key, _)| key.as_slice() == path.as_slice())
        {
            Some((_, slot)) => *slot = spans,
            None => self.inserted.push((path, spans)),
        }
    }

    fn record_spans(&mut self, path: ValuePath, spans: Vec<(usize, usize)>) {
        if spans.is_empty() {
            return;
        }
        self.set_spans(path, spans);
    }
}

/// The interpolation variable table of SPEC §5.11: the root fields of the
/// authored object whose syntax value is `Bare` or `Quoted`, plus `id`.
#[derive(Clone, Debug, Default)]
pub struct VariableTable {
    pub entries: Vec<(String, String)>,
    /// Names that are bound but hold no text: a loop variable over a list of
    /// groups, or a root field whose value is a list, an object or a tag
    /// object. Interpolation inserts text, so a reference to one of these is
    /// E522 rather than E425 (SPEC §6.9).
    pub non_scalars: Vec<(String, &'static str)>,
}

impl VariableTable {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The `{kind}` of a bound name that carries no text, or `None` when the
    /// name is not bound that way.
    pub fn non_scalar_kind(&self, name: &str) -> Option<&'static str> {
        self.non_scalars
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, kind)| *kind)
    }

    /// The candidate set of the E425 suggestion, in table order.
    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|(name, _)| name.as_str()).collect()
    }
}

// ------------------------------------------------------------- schema paths

/// Where a dotted path lands in a schema (SPEC §5.4).
#[derive(Clone, Debug)]
pub enum PathLookup<'a> {
    /// The path names this field; `window` is its existence set with every
    /// ancestor's intersected in, `None` when it exists in no version.
    Found {
        field: &'a FieldDecl,
        window: Option<VersionRange>,
    },
    /// A non-final segment names a list field; no path may traverse one.
    CrossesList { index: usize, field: &'a FieldDecl },
    /// A non-final segment names a field that holds no object.
    CrossesScalar { index: usize, field: &'a FieldDecl },
    /// The segment at `index` names no field of the object in scope.
    Unknown {
        index: usize,
        /// The declared fields of the object in scope, in declaration order.
        declared: Vec<String>,
    },
}

/// Follows a dotted path through a schema, entering groups and `$(Schema)`
/// values (SPEC §5.4, §4.12).
pub fn lookup_path<'a>(
    tables: &'a TemplateTables,
    fields: &'a [FieldDecl],
    segments: &[String],
    project: VersionRange,
) -> PathLookup<'a> {
    let mut scope: &[FieldDecl] = fields;
    let mut window = Some(project);
    for (index, segment) in segments.iter().enumerate() {
        let Some(field) = scope.iter().find(|field| &field.name == segment) else {
            return PathLookup::Unknown {
                index,
                declared: scope.iter().map(|field| field.name.clone()).collect(),
            };
        };
        window = window.and_then(|outer| {
            field
                .window_in(project)
                .and_then(|own| own.intersect(outer))
        });
        if index + 1 == segments.len() {
            return PathLookup::Found { field, window };
        }
        if field.is_list() {
            return PathLookup::CrossesList { index, field };
        }
        match &field.kind {
            FieldKind::Group { fields } => scope = fields,
            FieldKind::Scalar { ty, .. } => match ty {
                TypeExpr::Nested { schema } => match tables.schema(schema) {
                    Some(target) => scope = &target.fields,
                    None => return PathLookup::CrossesScalar { index, field },
                },
                _ => return PathLookup::CrossesScalar { index, field },
            },
        }
    }
    // The parser never produces an empty path (E316); this closes the walk.
    PathLookup::Unknown {
        index: 0,
        declared: fields.iter().map(|field| field.name.clone()).collect(),
    }
}

/// The fields of the object a group or `$(Schema)` field holds.
pub fn element_fields<'a>(
    tables: &'a TemplateTables,
    field: &'a FieldDecl,
) -> Option<&'a [FieldDecl]> {
    match &field.kind {
        FieldKind::Group { fields } => Some(fields),
        FieldKind::Scalar { ty, .. } => match ty {
            TypeExpr::Nested { schema } => tables.schema(schema).map(|decl| decl.fields.as_slice()),
            _ => None,
        },
    }
}

/// The `@tag` field of the group or schema a field holds, when it declares one
/// (SPEC §4.8, §5.7).
pub fn tag_field<'a>(tables: &'a TemplateTables, field: &'a FieldDecl) -> Option<&'a FieldDecl> {
    element_fields(tables, field)?
        .iter()
        .find(|inner| inner.is_tag())
}

// -------------------------------------------------------- the instance table

/// The instance half of the project tables (SPEC §7.2), ordered by
/// `(template, id)` as SPEC §2.7 prescribes.
#[derive(Clone, Debug, Default)]
pub struct InstanceTable<'a> {
    entries: Vec<&'a InstanceDecl>,
}

impl<'a> InstanceTable<'a> {
    /// Builds the table from declarations collected in source order. A
    /// repeated id keeps its first declaration; P2 reports it as E402.
    pub fn new(mut entries: Vec<&'a InstanceDecl>) -> Self {
        entries.sort_by(|left, right| {
            left.template
                .cmp(&right.template)
                .then_with(|| left.id.cmp(&right.id))
        });
        entries.dedup_by(|left, right| left.id == right.id);
        Self { entries }
    }

    pub fn get(&self, id: &str) -> Option<&'a InstanceDecl> {
        self.entries.iter().copied().find(|decl| decl.id == id)
    }

    /// Every instance, in the order of SPEC §2.7.
    pub fn iter(&self) -> impl Iterator<Item = &'a InstanceDecl> + '_ {
        self.entries.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The ids declared with one template, in the order of SPEC §2.7; the
    /// candidate set of the E405 and E431 suggestions.
    pub fn ids_of(&self, template: &str) -> Vec<&'a str> {
        self.entries
            .iter()
            .filter(|decl| decl.template == template)
            .map(|decl| decl.id.as_str())
            .collect()
    }

    /// The window of one instance, intersected with the project range.
    pub fn window(&self, id: &str, project: VersionRange) -> Option<VersionRange> {
        self.get(id)?.window.resolve(project)
    }
}

/// Collects every declared instance in source order.
pub fn collect_instances(files: &[InstanceFile]) -> Vec<&InstanceDecl> {
    files
        .iter()
        .flat_map(|file| file.instances.iter())
        .collect()
}

/// P2 for instances (SPEC §7.2): every check that needs the whole project but
/// no version-by-version compilation.
///
/// Reported in source order: the template exists (E401), the id is an
/// identifier (E428), is unique (E402) and satisfies the schema's `id`
/// declaration (E413), the instance window lies in the project range (E603,
/// E604), every clone source exists (E405), shares the template (E407) and
/// outlives the cloning instance (E440), the clone graph is acyclic (E406),
/// and every body statement has a non-empty applicability set (E430, then
/// E440) with no path written twice (E429).
pub fn check_instances(
    declarations: &[&InstanceDecl],
    tables: &TemplateTables,
) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    let project = tables.versions;
    let names = tables.schema_names();

    let table = InstanceTable::new(declarations.to_vec());
    let mut seen: Vec<(&str, &Located)> = Vec::new();
    for decl in declarations {
        if tables.schema(&decl.template).is_none() {
            let mut message = format!("Unknown template '{}'.", decl.template);
            if let Some(hint) = suggest(&decl.template, &names, TieBreak::ScalarOrder) {
                message.push_str(&format!(" Did you mean '{hint}'?"));
            }
            errors.push(Diagnostic::at(
                ErrorId::E401,
                decl.at.file.clone(),
                decl.at.position,
                message,
            ));
        }
        // An id that is not an identifier names nothing, so it takes no part
        // in the uniqueness and length checks (SPEC §5.3).
        if let Some(written) = &decl.invalid_id {
            errors.push(Diagnostic::at(
                ErrorId::E428,
                decl.id_at.file.clone(),
                decl.id_at.position,
                format!("Invalid instance id '{written}'; ids are identifiers (A-Z a-z 0-9 _ -)."),
            ));
        } else {
            match seen.iter().find(|(id, _)| *id == decl.id.as_str()) {
                Some((_, first)) => errors.push(
                    Diagnostic::at(
                        ErrorId::E402,
                        decl.at.file.clone(),
                        decl.at.position,
                        format!("Duplicate instance id '{}'.", decl.id),
                    )
                    .with_note(
                        Note::new("first declared here.").at(first.file.clone(), first.position),
                    ),
                ),
                None => seen.push((decl.id.as_str(), &decl.at)),
            }
            check_id_length(decl, tables, &mut errors);
        }
        check_instance_window(decl, project, &mut errors);
        check_clones(decl, &table, project, &mut errors);
        check_statements(decl, tables, project, &mut errors);
    }
    check_clone_cycles(&table, &mut errors);
    check_clone_depth(&table, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_instance_window(decl: &InstanceDecl, project: VersionRange, errors: &mut Diagnostics) {
    check_window_numbers(decl.window, project, &decl.at, errors);
}

/// The two rules SPEC §4.12 states for every version annotation: each number is
/// inside the project range (E603), and `@removed` is strictly greater than the
/// effective `@since` (E604). SPEC §5.14 restates both for an instance header;
/// SPEC §5.13 restates neither for a body statement, but §4.12 states them of
/// the annotations themselves, and SPEC §11.1's "E603 before E430 and E440"
/// can only be about a body statement, because E430 is defined for no other
/// construct.
///
/// Returns false when a number is wrong, so that the caller does not go on to
/// report the empty applicability set that follows from it.
fn check_window_numbers(
    window: Window,
    project: VersionRange,
    at: &Located,
    errors: &mut Diagnostics,
) -> bool {
    let mut ok = true;
    for number in [window.since, window.removed].into_iter().flatten() {
        if project.contains(number) {
            continue;
        }
        ok = false;
        errors.push(Diagnostic::at(
            ErrorId::E603,
            at.file.clone(),
            at.position,
            format!(
                "Version {number} is outside the project range {}..{}.",
                project.min, project.max
            ),
        ));
    }
    if !ok {
        return false;
    }
    let since = window.since.unwrap_or(project.min);
    if let Some(removed) = window.removed {
        if removed <= since {
            errors.push(Diagnostic::at(
                ErrorId::E604,
                at.file.clone(),
                at.position,
                format!("@removed({removed}) must be greater than @since({since})."),
            ));
            return false;
        }
    }
    true
}

/// The number of Unicode scalar values in an id must satisfy the schema's `id`
/// declaration, or the implicit `id: text(1..64)` (SPEC §4.11, E413).
fn check_id_length(decl: &InstanceDecl, tables: &TemplateTables, errors: &mut Diagnostics) {
    let Some(schema) = tables.schema(&decl.template) else {
        return;
    };
    let ranges = schema.id_ranges();
    if ranges.is_empty() {
        return;
    }
    let length = decl.id.chars().count() as i64;
    if ranges.iter().any(|range| range.contains(length)) {
        return;
    }
    let list = ranges
        .iter()
        .map(|range| range.describe())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(Diagnostic::at(
        ErrorId::E413,
        decl.id_at.file.clone(),
        decl.id_at.position,
        format!(
            "Range mismatch at {}.id: {length} is not in {list}.",
            decl.id
        ),
    ));
}

fn check_clones(
    decl: &InstanceDecl,
    table: &InstanceTable<'_>,
    project: VersionRange,
    errors: &mut Diagnostics,
) {
    let Some(window) = decl.window.resolve(project) else {
        return;
    };
    for clone in &decl.clones {
        let Some(source) = table.get(&clone.source) else {
            let candidates = table.ids_of(&decl.template);
            let mut message = format!("Unknown clone target '{}'.", clone.source);
            if let Some(hint) = suggest(&clone.source, &candidates, TieBreak::ScalarOrder) {
                message.push_str(&format!(" Did you mean '{hint}'?"));
            }
            errors.push(Diagnostic::at(
                ErrorId::E405,
                clone.at.file.clone(),
                clone.at.position,
                message,
            ));
            continue;
        };
        if source.template != decl.template {
            errors.push(Diagnostic::at(
                ErrorId::E407,
                clone.at.file.clone(),
                clone.at.position,
                format!(
                    "Instance '{}' is a '{}'; a clone source must use the same template '{}'.",
                    source.id, source.template, decl.template
                ),
            ));
            continue;
        }
        let source_window = source.window.resolve(project);
        let contains = source_window
            .map(|range| range.contains_range(window))
            .unwrap_or(false);
        if contains {
            continue;
        }
        let spelled = source_window
            .map(|range| range.to_string())
            .unwrap_or_else(|| "none".to_string());
        errors.push(
            Diagnostic::at(
                ErrorId::E440,
                clone.at.file.clone(),
                clone.at.position,
                format!(
                    "Clone source '{}' exists in versions {spelled}, but instance '{}' exists in versions {window}.",
                    source.id, decl.id
                ),
            )
            .with_note(
                Note::new(format!("instance '{}' is declared here.", decl.id))
                    .at(decl.at.file.clone(), decl.at.position),
            ),
        );
    }
}

/// A clone cycle is E406, naming the cycle (SPEC §5.7). The walk is iterative
/// so that a hostile source can never exhaust the stack.
fn check_clone_cycles(table: &InstanceTable<'_>, errors: &mut Diagnostics) {
    let decls: Vec<&InstanceDecl> = table.iter().collect();
    let ids: Vec<&str> = decls.iter().map(|decl| decl.id.as_str()).collect();

    #[derive(Clone, Copy, PartialEq)]
    enum Colour {
        White,
        Grey,
        Black,
    }
    let mut colour = vec![Colour::White; ids.len()];
    let mut reported = vec![false; ids.len()];

    for root in 0..ids.len() {
        if colour[root] != Colour::White {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        let mut trail: Vec<usize> = vec![root];
        colour[root] = Colour::Grey;
        while let Some((node, edge)) = stack.pop() {
            let clones = &decls[node].clones;
            if edge >= clones.len() {
                colour[node] = Colour::Black;
                trail.pop();
                continue;
            }
            stack.push((node, edge + 1));
            let clone = &clones[edge];
            let Some(next) = ids.iter().position(|item| *item == clone.source) else {
                continue;
            };
            match colour[next] {
                Colour::White => {
                    colour[next] = Colour::Grey;
                    trail.push(next);
                    stack.push((next, 0));
                }
                Colour::Grey => {
                    if reported[next] {
                        continue;
                    }
                    reported[next] = true;
                    let start = trail.iter().position(|item| *item == next).unwrap_or(0);
                    let mut cycle: Vec<&str> = trail
                        .get(start..)
                        .unwrap_or(&[])
                        .iter()
                        .map(|item| ids[*item])
                        .collect();
                    cycle.push(ids[next]);
                    errors.push(Diagnostic::at(
                        ErrorId::E406,
                        clone.at.file.clone(),
                        clone.at.position,
                        format!("Clone cycle detected: {}.", cycle.join(" -> ")),
                    ));
                }
                Colour::Black => {}
            }
        }
    }
}

/// The clone-chain limit of SPEC §3.7, checked as a property of the clone
/// graph: SPEC §5.7 states it as "chains deeper than 64 are E209", so whether
/// it fires must not depend on the order the resolver happens to visit
/// instances in, which memoisation makes depend on how the ids sort.
///
/// The transitive clone depth of an instance is the number of clone edges on
/// the longest chain leaving it, so a chain of exactly 64 clone statements has
/// depth 64 and is legal. Only the instances that first exceed the limit are
/// reported, so one over-long chain is one diagnostic rather than one per
/// instance above the limit.
fn check_clone_depth(table: &InstanceTable<'_>, errors: &mut Diagnostics) {
    let decls: Vec<&InstanceDecl> = table.iter().collect();
    let ids: Vec<&str> = decls.iter().map(|decl| decl.id.as_str()).collect();
    let mut depth = vec![0usize; ids.len()];
    let mut done = vec![false; ids.len()];
    let mut open = vec![false; ids.len()];

    // An explicit stack rather than recursion: the walk must terminate and
    // stay bounded on any graph, including one whose depth is what is being
    // rejected. Each edge is examined twice at most — once to descend into its
    // source, once to fold the source's depth in — so the walk is linear.
    for root in 0..ids.len() {
        if done[root] {
            continue;
        }
        open[root] = true;
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((node, edge)) = stack.pop() {
            let clones = &decls[node].clones;
            let Some(clone) = clones.get(edge) else {
                done[node] = true;
                open[node] = false;
                continue;
            };
            let Some(next) = ids.iter().position(|item| *item == clone.source) else {
                // E405 names a clone of an instance that does not exist.
                stack.push((node, edge + 1));
                continue;
            };
            if done[next] {
                depth[node] = depth[node].max(depth[next] + 1);
                stack.push((node, edge + 1));
                continue;
            }
            if open[next] {
                // A clone cycle, reported as E406; a cycle has no finite depth,
                // so the edge counts for one and the walk does not follow it.
                depth[node] = depth[node].max(1);
                stack.push((node, edge + 1));
                continue;
            }
            // Descend, and revisit this edge once its source has a depth.
            stack.push((node, edge));
            open[next] = true;
            stack.push((next, 0));
        }
    }

    for (index, decl) in decls.iter().enumerate() {
        if depth[index] == CLONE_CHAIN + 1 {
            errors.push(Diagnostic::at(
                ErrorId::E209,
                decl.at.file.clone(),
                decl.at.position,
                format!("Clone chain exceeds the limit of {CLONE_CHAIN}."),
            ));
        }
    }
}

/// Every body statement's applicability set must be non-empty (E430, then
/// E440), and no two assignments may write one path in a shared version
/// (E429) — SPEC §5.13.
/// The versions in which the field a path names exists, with every ancestor's
/// existence set intersected in (SPEC §4.12, §5.13).
///
/// A path that names no field yields the whole project range: it is E409 or
/// E443, both of which are reported by applying the statement and interpreting
/// its value, so such a statement must still be applied in every version.
fn target_window(
    tables: &TemplateTables,
    schema: Option<&SchemaDecl>,
    segments: &[String],
    project: VersionRange,
) -> Option<VersionRange> {
    let Some(schema) = schema else {
        return Some(project);
    };
    match lookup_path(tables, &schema.fields, segments, project) {
        PathLookup::Found { window, .. } => window,
        _ => Some(project),
    }
}

fn check_statements(
    decl: &InstanceDecl,
    tables: &TemplateTables,
    project: VersionRange,
    errors: &mut Diagnostics,
) {
    let Some(schema) = tables.schema(&decl.template) else {
        return;
    };
    let Some(instance) = decl.window.resolve(project) else {
        return;
    };
    let mut seen: Vec<(String, VersionRange, Located)> = Vec::new();
    for tag in &decl.tags {
        // A header tag is an assignment (SPEC §5.2), so SPEC §5.13's
        // applicability rule reaches it. It carries no annotation of its own
        // (E437), so its applicability set is the field's existence set
        // clipped to the instance's window; an empty set means the tag writes
        // nothing in any version, which is E440 exactly as it is for the body
        // statement that assigns the same field. Nothing in the language is
        // silent.
        let field_window = target_window(
            tables,
            Some(schema),
            std::slice::from_ref(&tag.name),
            project,
        );
        // An unknown field is E409 in P4, and a field that exists in no
        // version at all is E605 in P3; neither is this check's business, so
        // both fall through carrying the instance's own window.
        let applicable = match field_window {
            None => instance,
            Some(field) => match field.intersect(instance) {
                Some(shared) => shared,
                None => {
                    errors.push(
                        Diagnostic::at(
                            ErrorId::E440,
                            tag.at.file.clone(),
                            tag.at.position,
                            format!(
                                "This statement applies to versions {field}, but instance '{}' exists only in versions {instance}.",
                                decl.id
                            ),
                        )
                        .with_note(
                            Note::new(format!("instance '{}' is declared here.", decl.id))
                                .at(decl.at.file.clone(), decl.at.position),
                        ),
                    );
                    continue;
                }
            },
        };
        record_assignment(
            &mut seen,
            tag.name.clone(),
            applicable,
            tag.at.clone(),
            errors,
        );
    }
    for statement in &decl.statements {
        // A statement that names an envelope key is refused outright in P4
        // (E410, SPEC §5.4), so it writes nothing and takes no part in the
        // duplicate-assignment and applicability checks.
        if statement.path.starts_at_envelope_key() {
            continue;
        }
        if !check_window_numbers(statement.window, project, &statement.at, errors) {
            continue;
        }
        let field_window = target_window(tables, Some(schema), &statement.path.segments, project);
        let annotated = statement.window.resolve(project);
        let applicable = annotated
            .zip(field_window)
            .and_then(|(left, right)| left.intersect(right));
        let Some(applicable) = applicable else {
            let exists = field_window
                .map(|range| range.to_string())
                .unwrap_or_else(|| "none".to_string());
            let written = annotated
                .map(|range| range.to_string())
                .unwrap_or_else(|| "none".to_string());
            errors.push(Diagnostic::at(
                ErrorId::E430,
                statement.at.file.clone(),
                statement.at.position,
                format!(
                    "This statement can never apply: {} exists in versions {exists}, the statement is annotated {written}.",
                    statement.path.text()
                ),
            ));
            continue;
        };
        let Some(shared) = applicable.intersect(instance) else {
            errors.push(
                Diagnostic::at(
                    ErrorId::E440,
                    statement.at.file.clone(),
                    statement.at.position,
                    format!(
                        "This statement applies to versions {applicable}, but instance '{}' exists only in versions {instance}.",
                        decl.id
                    ),
                )
                .with_note(
                    Note::new(format!("instance '{}' is declared here.", decl.id))
                        .at(decl.at.file.clone(), decl.at.position),
                ),
            );
            continue;
        };
        record_assignment(
            &mut seen,
            statement.path.text(),
            shared,
            statement.at.clone(),
            errors,
        );
    }
}

fn record_assignment(
    seen: &mut Vec<(String, VersionRange, Located)>,
    path: String,
    range: VersionRange,
    at: Located,
    errors: &mut Diagnostics,
) {
    for (earlier_path, earlier_range, earlier_at) in seen.iter() {
        if *earlier_path != path {
            continue;
        }
        let Some(shared) = earlier_range.intersect(range) else {
            continue;
        };
        errors.push(
            Diagnostic::at(
                ErrorId::E429,
                at.file.clone(),
                at.position,
                format!("{path} is assigned twice for version(s) {shared}."),
            )
            .with_note(
                Note::new("first assigned here.").at(earlier_at.file.clone(), earlier_at.position),
            ),
        );
        break;
    }
    seen.push((path, range, at));
}

// --------------------------------------------------------------- the resolver

/// Builds authored objects, memoised per `(instance, version)` so that a clone
/// DAG never costs more than linear time (SPEC §5.7).
pub struct Resolver<'a> {
    tables: &'a TemplateTables,
    instances: &'a InstanceTable<'a>,
    memo: HashMap<(String, u32), AuthoredObject>,
    active: Vec<String>,
}

impl<'a> Resolver<'a> {
    pub fn new(tables: &'a TemplateTables, instances: &'a InstanceTable<'a>) -> Self {
        Self {
            tables,
            instances,
            memo: HashMap::new(),
            active: Vec::new(),
        }
    }

    pub fn tables(&self) -> &'a TemplateTables {
        self.tables
    }

    pub fn instances(&self) -> &'a InstanceTable<'a> {
        self.instances
    }

    /// The authored object of one instance for one version (SPEC §7.3 step 1):
    /// clones folded in source order, then header tags, then the body
    /// statements that apply to `version`.
    pub fn authored(&mut self, id: &str, version: u32) -> Result<AuthoredObject, Diagnostics> {
        let key = (id.to_string(), version);
        if let Some(found) = self.memo.get(&key) {
            return Ok(found.clone());
        }
        let Some(decl) = self.instances.get(id) else {
            return Ok(AuthoredObject::default());
        };
        if self.active.iter().any(|item| item == id) {
            let mut cycle = self.active.clone();
            cycle.push(id.to_string());
            return Err(Diagnostics::one(Diagnostic::at(
                ErrorId::E406,
                decl.at.file.clone(),
                decl.at.position,
                format!("Clone cycle detected: {}.", cycle.join(" -> ")),
            )));
        }
        // A backstop on the resolver's own recursion. The conformance check is
        // `check_clone_depth`, which runs in P2 over the whole clone graph; a
        // chain of exactly CLONE_CHAIN edges is legal and reaches this depth.
        if self.active.len() > CLONE_CHAIN {
            return Err(Diagnostics::one(Diagnostic::at(
                ErrorId::E209,
                decl.at.file.clone(),
                decl.at.position,
                format!("Clone chain exceeds the limit of {CLONE_CHAIN}."),
            )));
        }

        self.active.push(id.to_string());
        let built = self.build(decl, version);
        self.active.pop();
        let object = built?;
        self.memo.insert(key, object.clone());
        Ok(object)
    }

    fn build(
        &mut self,
        decl: &'a InstanceDecl,
        version: u32,
    ) -> Result<AuthoredObject, Diagnostics> {
        let mut errors = Diagnostics::new();
        let mut object = AuthoredObject::default();
        object.set_at(decl.at.clone());
        let schema = self.tables.schema(&decl.template);
        let declared = schema.map(|decl| decl.fields.as_slice()).unwrap_or(&[]);

        for clone in &decl.clones {
            let source = match self.authored(&clone.source, version) {
                Ok(source) => source,
                Err(reported) => {
                    errors.extend(reported);
                    continue;
                }
            };
            match &clone.path {
                None => {
                    merge_fields(&mut object.fields, &source.fields, declared, self.tables, 0);
                    for (path, _) in &source.origins {
                        object.set_origin(path, clone.at.clone());
                    }
                }
                Some(path) => {
                    let Some(value) = read_path(&source.fields, &path.segments) else {
                        continue;
                    };
                    let wrapped = wrap_path(&path.segments, value.clone());
                    if let SyntaxValue::Object(entries) = wrapped {
                        merge_fields(&mut object.fields, &entries, declared, self.tables, 0);
                    }
                    object.set_origin(&path.text(), clone.at.clone());
                }
            }
        }

        let project = self.tables.versions;
        for tag in &decl.tags {
            if tag.name == "id" || tag.name == "template" {
                continue;
            }
            // A header tag is an assignment (SPEC §5.2) and so has the
            // applicability set of SPEC §5.13 with no annotation of its own:
            // it writes nothing in a version in which its field does not
            // exist, and therefore names no variable there (SPEC §5.11).
            let applies = target_window(
                self.tables,
                schema,
                std::slice::from_ref(&tag.name),
                project,
            )
            .map(|range| range.contains(version))
            .unwrap_or(false);
            if !applies {
                continue;
            }
            assign(
                &mut object,
                std::slice::from_ref(&tag.name),
                tag.value.clone(),
                &tag.at,
                schema,
                self.tables,
                &mut errors,
            );
        }

        for statement in &decl.statements {
            // The applicability set of SPEC §5.13: the annotation range, the
            // project range and the existence set of the field written. The
            // instance's own window is already fixed, because `build` runs
            // only for versions in it. A statement that does not apply writes
            // nothing, so the field it names is not a variable in this version
            // (SPEC §5.11).
            let annotated = statement
                .window
                .resolve(project)
                .map(|range| range.contains(version))
                .unwrap_or(false);
            let exists = target_window(self.tables, schema, &statement.path.segments, project)
                .map(|range| range.contains(version))
                .unwrap_or(false);
            if !annotated || !exists {
                continue;
            }
            assign(
                &mut object,
                &statement.path.segments,
                statement.value.clone(),
                &statement.at,
                schema,
                self.tables,
                &mut errors,
            );
        }

        if errors.is_empty() {
            Ok(object)
        } else {
            Err(errors)
        }
    }
}

/// The versions of `instance` in which a partial clone finds nothing are
/// reported as E408 only when the path can never exist (SPEC §5.7).
pub fn check_clone_paths(
    resolver: &mut Resolver<'_>,
    decl: &InstanceDecl,
    project: VersionRange,
) -> Diagnostics {
    let mut errors = Diagnostics::new();
    let Some(window) = decl.window.resolve(project) else {
        return errors;
    };
    for clone in &decl.clones {
        let Some(path) = &clone.path else {
            continue;
        };
        // SPEC §5.7: `&other.id` and `&other.template` are E410 — the path does
        // exist on the source, but it is a reserved envelope key. The envelope
        // keys are not part of the authored object searched below, so without
        // this the absence test would fire first and its message would deny its
        // own condition.
        if path.starts_at_envelope_key() {
            continue;
        }
        let mut found = false;
        for version in window.iter() {
            // A failure here is reported by P4 when the version is compiled;
            // it says nothing about whether the path exists.
            let Ok(source) = resolver.authored(&clone.source, version) else {
                found = true;
                break;
            };
            if read_path(&source.fields, &path.segments).is_some() {
                found = true;
                break;
            }
        }
        if found {
            continue;
        }
        errors.push(
            Diagnostic::at(
                ErrorId::E408,
                clone.at.file.clone(),
                clone.at.position,
                format!(
                    "Clone path '{}' does not exist on instance '{}' in any version.",
                    path.text(),
                    clone.source
                ),
            )
            .with_note_text(format!("versions checked: {window}.")),
        );
    }
    errors
}

/// Reads the subtree a dotted path names, or `None` when it is absent.
fn read_path<'v>(
    fields: &'v [(String, SyntaxValue)],
    segments: &[String],
) -> Option<&'v SyntaxValue> {
    let mut current: Option<&SyntaxValue> = None;
    let mut scope: &[(String, SyntaxValue)] = fields;
    for segment in segments {
        let found = scope.iter().find(|(key, _)| key == segment)?;
        current = Some(&found.1);
        scope = match &found.1 {
            SyntaxValue::Object(entries) => entries.as_slice(),
            _ => &[],
        };
    }
    current
}

/// `a.b.c` and a value become `{a: {b: {c: value}}}`.
fn wrap_path(segments: &[String], value: SyntaxValue) -> SyntaxValue {
    let mut current = value;
    for segment in segments.iter().rev() {
        current = SyntaxValue::Object(vec![(segment.clone(), current)]);
    }
    current
}

// ------------------------------------------------------------- clone merging

/// Folds one authored object onto another, key by key, in `source` order
/// (SPEC §5.7). The merge is schema-directed: a field the schema declares as a
/// keyed list merges by key, every other list replaces wholesale.
fn merge_fields(
    target: &mut Vec<(String, SyntaxValue)>,
    source: &[(String, SyntaxValue)],
    fields: &[FieldDecl],
    tables: &TemplateTables,
    depth: usize,
) {
    if depth > GROUP_DEPTH {
        return;
    }
    for (key, value) in source {
        let declared = fields.iter().find(|field| &field.name == key);
        let Some(index) = target.iter().position(|(name, _)| name == key) else {
            target.push((key.clone(), value.clone()));
            continue;
        };
        target[index].1 = merge_value(&target[index].1, value, declared, tables, depth);
    }
}

fn merge_value(
    target: &SyntaxValue,
    source: &SyntaxValue,
    declared: Option<&FieldDecl>,
    tables: &TemplateTables,
    depth: usize,
) -> SyntaxValue {
    match (target, source) {
        (SyntaxValue::Object(left), SyntaxValue::Object(right)) => {
            let inner = declared
                .and_then(|field| element_fields(tables, field))
                .unwrap_or(&[]);
            let mut merged = left.clone();
            merge_fields(&mut merged, right, inner, tables, depth + 1);
            SyntaxValue::Object(merged)
        }
        (SyntaxValue::List(left, _), SyntaxValue::List(right, spelling)) => {
            let Some(field) = declared.filter(|field| field.is_list()) else {
                return source.clone();
            };
            let Some(tag) = tag_field(tables, field) else {
                return source.clone();
            };
            merge_keyed_lists(left, right, field, tag, tables, depth, *spelling)
        }
        _ => source.clone(),
    }
}

/// Merges two keyed lists by the normalised text of their tag field: the order
/// of `target` is preserved, same-key elements merge recursively, and new keys
/// are appended in `source` order (SPEC §5.7).
#[allow(clippy::too_many_arguments)]
fn merge_keyed_lists(
    target: &[SyntaxValue],
    source: &[SyntaxValue],
    field: &FieldDecl,
    tag: &FieldDecl,
    tables: &TemplateTables,
    depth: usize,
    spelling: ListSpelling,
) -> SyntaxValue {
    let inner = element_fields(tables, field).unwrap_or(&[]);
    let mut merged: Vec<SyntaxValue> = expand_element_keys(target, tag);
    for element in expand_element_keys(source, tag) {
        let Some(key) = element_key(&element, tag) else {
            merged.push(element);
            continue;
        };
        let position = merged
            .iter()
            .position(|item| element_key(item, tag).as_deref() == Some(key.as_str()));
        match position {
            Some(index) => {
                let mut left = object_form(&merged[index], tag);
                let right = object_form(&element, tag);
                merge_fields(&mut left, &right, inner, tables, depth + 1);
                merged[index] = SyntaxValue::Object(left);
            }
            None => merged.push(element),
        }
    }
    SyntaxValue::List(merged, spelling)
}

/// An enum wildcard in an element key names several members, and folding sees
/// the members rather than the pattern (SPEC §5.6, §5.7).
fn expand_element_keys(elements: &[SyntaxValue], tag: &FieldDecl) -> Vec<SyntaxValue> {
    let members = match tag.type_expr() {
        Some(TypeExpr::Enum { members }) => members.clone(),
        _ => return elements.to_vec(),
    };
    let mut out = Vec::new();
    for element in elements {
        let matched = element_key(element, tag)
            .as_deref()
            .and_then(|key| key.strip_suffix('*').map(str::to_string))
            .map(|prefix| {
                members
                    .iter()
                    .filter(|member| member.starts_with(&prefix))
                    .cloned()
                    .collect::<Vec<String>>()
            });
        match matched {
            // A wildcard that matches nothing is E415, raised when the value
            // is interpreted; folding leaves it untouched.
            Some(members) if !members.is_empty() => {
                for member in members {
                    out.push(with_key(element, tag, &member));
                }
            }
            _ => out.push(element.clone()),
        }
    }
    out
}

/// The merge key of one list element: the normalised text of its tag field.
fn element_key(element: &SyntaxValue, tag: &FieldDecl) -> Option<String> {
    match element {
        SyntaxValue::Tag { name, .. } => Some(normalise(name)),
        SyntaxValue::Object(fields) => match fields.iter().find(|(key, _)| key == &tag.name) {
            Some((_, SyntaxValue::Bare(text))) | Some((_, SyntaxValue::Quoted(text))) => {
                Some(normalise(text))
            }
            _ => None,
        },
        _ => None,
    }
}

fn with_key(element: &SyntaxValue, tag: &FieldDecl, key: &str) -> SyntaxValue {
    match element {
        SyntaxValue::Tag { args, .. } => SyntaxValue::Tag {
            name: key.to_string(),
            args: args.clone(),
        },
        SyntaxValue::Object(fields) => {
            let mut copy = fields.clone();
            for (name, value) in copy.iter_mut() {
                if name == &tag.name {
                    *value = SyntaxValue::Bare(key.to_string());
                }
            }
            SyntaxValue::Object(copy)
        }
        other => other.clone(),
    }
}

/// A `#tag(...)` element as the object it stands for, so that two elements
/// under one key can be merged field by field (SPEC §5.5, §5.7).
fn object_form(element: &SyntaxValue, tag: &FieldDecl) -> Vec<(String, SyntaxValue)> {
    match element {
        SyntaxValue::Object(fields) => fields.clone(),
        SyntaxValue::Tag { name, args } => {
            let mut fields = vec![(tag.name.clone(), SyntaxValue::Bare(name.clone()))];
            for argument in args {
                let Some(head) = argument.path.segments.first() else {
                    continue;
                };
                let value = wrap_path(
                    argument.path.segments.get(1..).unwrap_or(&[]),
                    argument.value.clone(),
                );
                match fields.iter_mut().find(|(key, _)| key == head) {
                    Some((_, slot)) => *slot = value,
                    None => fields.push((head.clone(), value)),
                }
            }
            fields
        }
        other => vec![(tag.name.clone(), other.clone())],
    }
}

// ----------------------------------------------------------- plain assignment

/// Applies one header tag or body statement by plain assignment: the value
/// written at a path replaces whatever a clone left there, with no merging
/// (SPEC §5.7, §7.3 step 1).
fn assign(
    object: &mut AuthoredObject,
    segments: &[String],
    value: SyntaxValue,
    at: &Located,
    schema: Option<&SchemaDecl>,
    tables: &TemplateTables,
    errors: &mut Diagnostics,
) {
    let path = segments.join(".");
    if let Some(schema) = schema {
        if let Some(diagnostic) = crossing_error(schema, tables, segments, &path, at) {
            errors.push(diagnostic);
            return;
        }
    }
    let mut scope: &mut Vec<(String, SyntaxValue)> = &mut object.fields;
    for (index, segment) in segments.iter().enumerate() {
        let position = scope.iter().position(|(key, _)| key == segment);
        if index + 1 == segments.len() {
            match position {
                Some(found) => scope[found].1 = value,
                None => scope.push((segment.clone(), value)),
            }
            break;
        }
        let found = match position {
            Some(found) => found,
            None => {
                scope.push((segment.clone(), SyntaxValue::Object(Vec::new())));
                scope.len() - 1
            }
        };
        if !matches!(scope[found].1, SyntaxValue::Object(_)) {
            let prefix = segments
                .get(..=index)
                .map(|head| head.join("."))
                .unwrap_or_default();
            let kind = value_kind(&scope[found].1);
            errors.push(Diagnostic::at(
                ErrorId::E443,
                at.file.clone(),
                at.position,
                format!("Cannot assign '{path}': '{prefix}' already holds {kind}."),
            ));
            return;
        }
        let SyntaxValue::Object(entries) = &mut scope[found].1 else {
            return;
        };
        scope = entries;
    }
    object.set_origin(&path, at.clone());
}

/// A path may not traverse a list field (SPEC §5.4, E443).
fn crossing_error(
    schema: &SchemaDecl,
    tables: &TemplateTables,
    segments: &[String],
    path: &str,
    at: &Located,
) -> Option<Diagnostic> {
    match lookup_path(tables, &schema.fields, segments, tables.versions) {
        PathLookup::CrossesList { index, field } => {
            let prefix = segments.get(..=index)?.join(".");
            Some(
                Diagnostic::at(
                    ErrorId::E443,
                    at.file.clone(),
                    at.position,
                    format!("Cannot assign '{path}': '{prefix}' is a list."),
                )
                .with_note_text(format!(
                    "'{}' is a list; write its elements with a tuple array or with '#tag' shorthand.",
                    field.name
                )),
            )
        }
        _ => None,
    }
}

/// The `{kind}` substitution for a syntax value already in the object.
fn value_kind(value: &SyntaxValue) -> &'static str {
    match value {
        SyntaxValue::List(_, _) => "a list",
        SyntaxValue::Tag { .. } => "a tag object",
        SyntaxValue::Object(_) => "an object",
        SyntaxValue::Bare(_) | SyntaxValue::Quoted(_) => "text",
    }
}

// ------------------------------------------------------------- interpolation

/// The variable table of SPEC §5.11: the root fields whose syntax value is
/// `Bare` or `Quoted`, in field order, with `id` always present.
pub fn variable_table(object: &AuthoredObject, id: &str) -> VariableTable {
    let mut entries = vec![("id".to_string(), id.to_string())];
    for (name, value) in &object.fields {
        if name == "id" {
            continue;
        }
        let text = match value {
            SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => text.clone(),
            _ => continue,
        };
        entries.push((name.clone(), text));
    }
    // SPEC §5.11: a nested, list, tag or object field is not a variable at
    // all, so a reference to one is E425 here. E522 belongs to logic, where
    // §6.9 binds names that do hold a value but not a text one.
    VariableTable {
        entries,
        non_scalars: Vec::new(),
    }
}

/// Resolves every `$name`, `${name}` and `$$` in an object's text values in
/// one left-to-right pass (SPEC §5.11).
pub fn interpolate(
    object: &mut AuthoredObject,
    variables: &VariableTable,
) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    let mut recorded: Vec<(ValuePath, Vec<(usize, usize)>)> = Vec::new();
    let mut fields = std::mem::take(&mut object.fields);
    // `fields` was moved out, so the object itself may be read for the origin
    // of each path while the values are rewritten in place.
    let source: &AuthoredObject = object;
    let mut walk = Interpolation {
        variables,
        source,
        recorded: &mut recorded,
        errors: &mut errors,
    };
    for (name, value) in fields.iter_mut() {
        let mut path = vec![Step::Field(name.clone())];
        walk.value(value, &mut path, name);
    }
    object.fields = fields;
    for (path, spans) in recorded {
        object.record_spans(path, spans);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// One pass of SPEC §5.11 over an authored object.
struct Interpolation<'a> {
    variables: &'a VariableTable,
    /// Read for the position of the construct that wrote each path, so that
    /// E425, E426 and E427 name the statement rather than the instance.
    source: &'a AuthoredObject,
    recorded: &'a mut Vec<(ValuePath, Vec<(usize, usize)>)>,
    errors: &'a mut Diagnostics,
}

impl Interpolation<'_> {
    /// `dotted` is the field path of the value, which is how origins are
    /// keyed; a list index does not extend it, because no statement writes
    /// through one (SPEC §5.4).
    fn value(&mut self, value: &mut SyntaxValue, path: &mut ValuePath, dotted: &str) {
        match value {
            SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => {
                let at = self.source.origin(dotted).cloned();
                match substitute(text, self.variables, at.as_ref()) {
                    Ok((rewritten, spans)) => {
                        *text = rewritten;
                        if !spans.is_empty() {
                            self.recorded.push((path.clone(), spans));
                        }
                    }
                    Err(reported) => self.errors.extend(reported),
                }
            }
            SyntaxValue::List(items, _) => {
                for (index, item) in items.iter_mut().enumerate() {
                    path.push(Step::Index(index));
                    self.value(item, path, dotted);
                    path.pop();
                }
            }
            SyntaxValue::Object(fields) => {
                for (name, item) in fields.iter_mut() {
                    let child = format!("{dotted}.{name}");
                    path.push(Step::Field(name.clone()));
                    self.value(item, path, &child);
                    path.pop();
                }
            }
            SyntaxValue::Tag { args, .. } => {
                for argument in args.iter_mut() {
                    let mut child = dotted.to_string();
                    for segment in &argument.path.segments {
                        child.push('.');
                        child.push_str(segment);
                        path.push(Step::Field(segment.clone()));
                    }
                    self.value(&mut argument.value, path, &child);
                    for _ in &argument.path.segments {
                        path.pop();
                    }
                }
            }
        }
    }
}

/// One left-to-right pass over a text value (SPEC §5.11). Substituted content
/// is never rescanned. The second half of the result is the byte range of each
/// inserted run, which brace expansion needs (SPEC §5.8).
pub fn substitute(
    text: &str,
    variables: &VariableTable,
    at: Option<&Located>,
) -> Result<(String, Vec<(usize, usize)>), Diagnostics> {
    if !text.contains('$') {
        return Ok((text.to_string(), Vec::new()));
    }
    let mut out = String::with_capacity(text.len());
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        let current = characters[index];
        if current != '$' {
            out.push(current);
            index += 1;
            continue;
        }
        match characters.get(index + 1).copied() {
            Some('$') => {
                out.push('$');
                index += 2;
            }
            Some('{') => {
                let mut end = index + 2;
                while end < characters.len() && characters[end] != '}' {
                    end += 1;
                }
                if end >= characters.len() {
                    return Err(Diagnostics::one(positioned(
                        ErrorId::E427,
                        at,
                        "Unterminated '${'.".to_string(),
                    )));
                }
                let name: String = characters
                    .get(index + 2..end)
                    .map(|slice| slice.iter().collect())
                    .unwrap_or_default();
                let value = lookup_variable(&name, variables, at)?;
                push_substitution(&mut out, &mut spans, &value);
                index = end + 1;
            }
            Some(character) if is_identifier_character(character) => {
                let mut end = index + 1;
                while end < characters.len() && is_identifier_character(characters[end]) {
                    end += 1;
                }
                let name: String = characters
                    .get(index + 1..end)
                    .map(|slice| slice.iter().collect())
                    .unwrap_or_default();
                let value = lookup_variable(&name, variables, at)?;
                push_substitution(&mut out, &mut spans, &value);
                index = end;
            }
            _ => {
                return Err(Diagnostics::one(positioned(
                    ErrorId::E426,
                    at,
                    "'$' must be followed by a name, '{' or '$'.".to_string(),
                )));
            }
        }
    }
    Ok((out, spans))
}

fn push_substitution(out: &mut String, spans: &mut Vec<(usize, usize)>, value: &str) {
    let start = out.len();
    out.push_str(value);
    if value.contains('{') || value.contains('}') {
        spans.push((start, out.len()));
    }
}

fn lookup_variable(
    name: &str,
    variables: &VariableTable,
    at: Option<&Located>,
) -> Result<String, Diagnostics> {
    let normalised = normalise(name);
    if let Some(value) = variables.get(&normalised) {
        return Ok(value.to_string());
    }
    // A name that is bound but holds no text is E522, never E425: the author
    // did not misspell anything, and the reference must not fall through to a
    // root field of the same name (SPEC §6.9).
    if let Some(kind) = variables.non_scalar_kind(&normalised) {
        return Err(Diagnostics::one(
            positioned(
                ErrorId::E522,
                at,
                format!("Variable '${name}' is not a scalar; it is {kind}."),
            )
            .with_note_text("interpolation inserts text; read a scalar field of it instead."),
        ));
    }
    let candidates = variables.names();
    let mut message = format!("Unknown variable '${name}'.");
    if let Some(hint) = suggest(name, &candidates, TieBreak::DeclarationOrder) {
        message.push_str(&format!(" Did you mean '{hint}'?"));
    }
    Err(Diagnostics::one(
        positioned(ErrorId::E425, at, message)
            .with_note_text("variables are the root scalar fields of this instance."),
    ))
}

/// E425, E426 and E427 are reported at the construct that carries the text:
/// the body statement, the header tag or the default declaration. The parser
/// records no position for the value itself, and an offset counted from the
/// start of the statement would land on a character of the path rather than on
/// the `$`, so no offset is added.
fn positioned(id: ErrorId, at: Option<&Located>, message: String) -> Diagnostic {
    match at {
        Some(at) => Diagnostic::at(id, at.file.clone(), at.position, message),
        None => Diagnostic::new(id, message),
    }
}

/// The identifier characters a `$name` reference consumes (SPEC §3.3).
fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

// ------------------------------------------------------------------ defaults

/// Fills every absent field that declares a default, top-down, skipping absent
/// groups (SPEC §7.3 step 4).
///
/// A default is interpolated at the moment it is filled in, against the
/// variable table built from the instance's authored values (SPEC §4.10).
pub fn apply_defaults(
    object: &mut AuthoredObject,
    schema: &SchemaDecl,
    variables: &VariableTable,
    tables: &TemplateTables,
    version: u32,
    instance: &str,
) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    let mut fields = std::mem::take(&mut object.fields);
    fill(
        &mut fields,
        &schema.fields,
        tables,
        variables,
        version,
        instance,
        0,
        &mut errors,
    );
    object.fields = fields;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
fn fill(
    values: &mut Vec<(String, SyntaxValue)>,
    declarations: &[FieldDecl],
    tables: &TemplateTables,
    variables: &VariableTable,
    version: u32,
    instance: &str,
    depth: usize,
    errors: &mut Diagnostics,
) {
    if depth > GROUP_DEPTH {
        return;
    }
    let project = tables.versions;
    for field in declarations {
        if !field.exists_in(version, project) {
            continue;
        }
        let Some(index) = values.iter().position(|(key, _)| key == &field.name) else {
            let Some(default) = field.default() else {
                continue;
            };
            match interpolate_default(default, variables, instance) {
                Ok(value) => values.push((field.name.clone(), value)),
                Err(reported) => errors.extend(reported),
            }
            continue;
        };
        // A present group or `$(Schema)` value is walked into; an absent one is
        // not, so the defaults inside it are not filled (SPEC §4.7, §7.3).
        let Some(inner) = element_fields(tables, field) else {
            continue;
        };
        for object in objects_of(&mut values[index].1) {
            fill(
                object,
                inner,
                tables,
                variables,
                version,
                instance,
                depth + 1,
                errors,
            );
        }
    }
}

/// Every object a value holds: itself when it is one, or its elements when it
/// is a list. A `#tag` element has been normalised into an object by
/// interpretation before defaults run (SPEC §7.3 steps 3 and 4).
fn objects_of(value: &mut SyntaxValue) -> Vec<&mut Vec<(String, SyntaxValue)>> {
    match value {
        SyntaxValue::Object(fields) => vec![fields],
        SyntaxValue::List(items, _) => items
            .iter_mut()
            .filter_map(|item| match item {
                SyntaxValue::Object(fields) => Some(fields),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn interpolate_default(
    default: &DefaultValue,
    variables: &VariableTable,
    instance: &str,
) -> Result<SyntaxValue, Diagnostics> {
    let mut value = default.value.clone();
    let mut errors = Diagnostics::new();
    rewrite_default(&mut value, variables, &default.at, instance, &mut errors);
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}

fn rewrite_default(
    value: &mut SyntaxValue,
    variables: &VariableTable,
    at: &Located,
    instance: &str,
    errors: &mut Diagnostics,
) {
    match value {
        SyntaxValue::Bare(text) | SyntaxValue::Quoted(text) => {
            match substitute(text, variables, Some(at)) {
                Ok((rewritten, _)) => *text = rewritten,
                Err(reported) => {
                    for diagnostic in reported {
                        errors.push(
                            diagnostic
                                .with_note_text(format!("filled into instance '{instance}'.")),
                        );
                    }
                }
            }
        }
        SyntaxValue::List(items, _) => {
            for item in items.iter_mut() {
                rewrite_default(item, variables, at, instance, errors);
            }
        }
        SyntaxValue::Object(fields) => {
            for (_, item) in fields.iter_mut() {
                rewrite_default(item, variables, at, instance, errors);
            }
        }
        SyntaxValue::Tag { args, .. } => {
            for argument in args.iter_mut() {
                rewrite_default(&mut argument.value, variables, at, instance, errors);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{parse, SourceUnit};
    use crate::diagnostics::Position;
    use crate::source::SourceFile;

    fn template(text: &str) -> TemplateTables {
        let source = SourceFile::new("data/schema.abt", text);
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file]).expect("tables build");
        crate::schema::validate_schemas(&tables).expect("the schemas are valid");
        tables
    }

    fn instances(path: &str, text: &str) -> InstanceFile {
        let source = SourceFile::new(path, text);
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
            panic!("an instance file");
        };
        file
    }

    fn located() -> Located {
        Located::new("data/a.ab", Position::new(1, 1))
    }

    #[test]
    fn a_variable_table_holds_the_root_scalars_and_the_id() {
        let mut object = AuthoredObject::default();
        object.set("name", SyntaxValue::Bare("Torch".to_string()));
        object.set(
            "tags",
            SyntaxValue::List(
                vec![SyntaxValue::Bare("a".to_string())],
                ListSpelling::Brackets,
            ),
        );
        let variables = variable_table(&object, "torch");
        assert_eq!(variables.get("id"), Some("torch"));
        assert_eq!(variables.get("name"), Some("Torch"));
        assert_eq!(variables.get("tags"), None);
    }

    #[test]
    fn substitution_is_one_pass_and_never_rescans() {
        let variables = VariableTable {
            entries: vec![
                ("id".to_string(), "atlas".to_string()),
                ("raw".to_string(), "$id".to_string()),
            ],
            ..Default::default()
        };
        let (text, _) = substitute("./textures/$id.png", &variables, None).expect("resolved");
        assert_eq!(text, "./textures/atlas.png");
        let (text, _) = substitute("${id}_display", &variables, None).expect("resolved");
        assert_eq!(text, "atlas_display");
        let (text, _) = substitute("$$99 special", &variables, None).expect("resolved");
        assert_eq!(text, "$99 special");
        let (text, _) = substitute("$raw", &variables, None).expect("resolved");
        assert_eq!(text, "$id");
    }

    #[test]
    fn a_malformed_reference_is_reported() {
        let variables = VariableTable {
            entries: vec![("id".to_string(), "atlas".to_string())],
            ..Default::default()
        };
        let first = |text: &str| {
            substitute(text, &variables, Some(&located()))
                .expect_err("rejected")
                .first()
                .map(|item| item.id)
        };
        assert_eq!(first("5 $ each"), Some(ErrorId::E426));
        assert_eq!(first("${id"), Some(ErrorId::E427));
        assert_eq!(first("$slugg"), Some(ErrorId::E425));
    }

    #[test]
    fn an_unknown_variable_suggests_a_root_field() {
        let variables = VariableTable {
            entries: vec![
                ("id".to_string(), "atlas".to_string()),
                ("name".to_string(), "Atlas".to_string()),
            ],
            ..Default::default()
        };
        let error = substitute("$nane", &variables, Some(&located())).expect_err("rejected");
        let message = error.first().expect("one diagnostic").to_string();
        assert!(message.contains("Did you mean 'name'?"), "{message}");
    }

    #[test]
    fn interpolation_records_the_braces_a_variable_inserted() {
        let mut object = AuthoredObject::default();
        object.set("dir", SyntaxValue::Bare("a{b}".to_string()));
        object.set("icon", SyntaxValue::Bare("$dir/x.png".to_string()));
        let variables = variable_table(&object, "one");
        interpolate(&mut object, &variables).expect("resolved");
        assert_eq!(
            object.get("icon"),
            Some(&SyntaxValue::Bare("a{b}/x.png".to_string()))
        );
        assert_eq!(
            object.inserted_spans(&[Step::Field("icon".to_string())]),
            [(0, 4)]
        );
    }

    #[test]
    fn clones_fold_in_source_order_and_own_statements_replace() {
        let tables =
            template("schema Item {\n    name: text(1..40)\n    note: text(1..40) @optional\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.base\n    name: Base\n    note: kept\n\nItem :: @id.child\n&base.*\n    name: Child\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("the instances are consistent");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let object = resolver.authored("child", 1).expect("built");
        assert_eq!(
            object.get("name"),
            Some(&SyntaxValue::Bare("Child".to_string()))
        );
        assert_eq!(
            object.get("note"),
            Some(&SyntaxValue::Bare("kept".to_string()))
        );
    }

    #[test]
    fn a_keyed_list_merges_between_clone_sources() {
        let tables = template(
            "schema Pack {\n    caps[] {\n        id: enum(search, sync) @tag\n        level: int(0..9) @optional\n    }\n}\n",
        );
        let file = instances(
            "data/packs.ab",
            "Pack :: @id.one\n    caps: [#search(level: 1)]\n\nPack :: @id.two\n    caps: [#sync]\n\nPack :: @id.three\n&one.*\n&two.*\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("the instances are consistent");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let object = resolver.authored("three", 1).expect("built");
        let Some(SyntaxValue::List(items, _)) = object.get("caps") else {
            panic!("caps is a list");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn a_partial_clone_copies_one_subtree() {
        let tables = template(
            "schema Item {\n    owner {\n        team: text(1..40)\n        contact: text(1..40) @optional\n    }\n    name: text(1..40)\n}\n",
        );
        let file = instances(
            "data/items.ab",
            "Item :: @id.base\n    name: Base\n    owner.team: Systems\n\nItem :: @id.child\n&base.owner\n    name: Child\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("the instances are consistent");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let object = resolver.authored("child", 1).expect("built");
        let Some(SyntaxValue::Object(owner)) = object.get("owner") else {
            panic!("owner is an object");
        };
        assert_eq!(owner.len(), 1);
        assert_eq!(owner[0].0, "team");
    }

    #[test]
    fn a_clone_cycle_and_an_unknown_target_are_reported() {
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.a\n&b.*\n    name: A\n\nItem :: @id.b\n&a.*\n    name: B\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("a cycle");
        assert!(errors.iter().any(|item| item.id == ErrorId::E406));

        let file = instances("data/items.ab", "Item :: @id.a\n&missing.*\n    name: A\n");
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("an unknown target");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E405));
    }

    #[test]
    fn a_partial_clone_of_a_path_that_never_exists_is_reported() {
        let tables =
            template("schema Item {\n    name: text(1..40)\n    note: text(1..40) @optional\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.base\n    name: Base\n\nItem :: @id.child\n&base.note\n    name: Child\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("consistent");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let child = table.get("child").expect("the instance");
        let errors = check_clone_paths(&mut resolver, child, tables.versions);
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E408));
    }

    #[test]
    fn a_path_that_traverses_a_list_is_reported() {
        let tables =
            template("schema Item {\n    caps[] {\n        id: enum(a, b) @tag\n    }\n}\n");
        let file = instances("data/items.ab", "Item :: @id.one\n    caps.id: a\n");
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("P2 says nothing about this");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let errors = resolver
            .authored("one", 1)
            .expect_err("a list is not traversable");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E443));
    }

    #[test]
    fn a_duplicate_id_and_an_unknown_template_are_reported() {
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.a\n    name: A\n\nItem :: @id.a\n    name: B\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("a duplicate");
        assert!(errors.iter().any(|item| item.id == ErrorId::E402));

        let file = instances("data/items.ab", "Itemm :: @id.a\n    name: A\n");
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("an unknown template");
        let message = errors.first().expect("one diagnostic").to_string();
        assert!(message.contains("Did you mean 'Item'?"), "{message}");
    }

    #[test]
    fn an_unusable_id_is_e428_in_p2_and_never_a_duplicate() {
        // SPEC §5.3 makes the id something P2 computes, so a duplicate id in
        // one file and an unusable one in another are reported in one run,
        // in the source order of SPEC §2.4.
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        let first = instances("data/a.ab", "Item :: @id.x\n    name: A\n");
        let second = instances("data/b.ab", "Item :: @id.x\n    name: B\n");
        let third = instances("data/c.ab", "Item :: @id\n    name: C\n");
        let files = [first, second, third];
        let declarations = collect_instances(&files);
        let errors = check_instances(&declarations, &tables).expect_err("two defects");
        let ids: Vec<ErrorId> = errors.iter().map(|item| item.id).collect();
        assert_eq!(ids, [ErrorId::E402, ErrorId::E428]);

        // Two unusable ids never collide with each other.
        let files = [
            instances("data/a.ab", "Item :: @id\n    name: A\n"),
            instances("data/b.ab", "Item :: @id\n    name: B\n"),
        ];
        let declarations = collect_instances(&files);
        let errors = check_instances(&declarations, &tables).expect_err("two unusable ids");
        let ids: Vec<ErrorId> = errors.iter().map(|item| item.id).collect();
        assert_eq!(ids, [ErrorId::E428, ErrorId::E428]);
    }

    #[test]
    fn defaults_are_filled_and_interpolated() {
        let tables = template(
            "schema Item {\n    name: text(1..40)\n    label: text(1..40) = ${id}_label\n}\n",
        );
        let file = instances("data/items.ab", "Item :: @id.torch\n    name: Torch\n");
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("consistent");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let mut object = resolver.authored("torch", 1).expect("built");
        let variables = variable_table(&object, "torch");
        interpolate(&mut object, &variables).expect("resolved");
        let schema = tables.schema("Item").expect("the schema");
        apply_defaults(&mut object, schema, &variables, &tables, 1, "torch").expect("filled");
        assert_eq!(
            object.get("label"),
            Some(&SyntaxValue::Bare("torch_label".to_string()))
        );
    }

    #[test]
    fn a_statement_outside_its_field_or_window_is_reported() {
        let tables = template(
            "versions 1..3\n\nschema Item {\n    name: text(1..40)\n    legacy: int(0..9) @removed(3) @optional\n}\n",
        );
        let file = instances(
            "data/items.ab",
            "Item :: @id.one\n    name: One\n    legacy: 1  @since(3)\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("E430");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E430));

        let file = instances(
            "data/items.ab",
            "Item :: @id.one @removed(2)\n    name: One\n    legacy: 1  @since(2)\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("E440");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E440));
    }

    #[test]
    fn two_statements_writing_one_path_in_one_version_are_reported() {
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.one\n    name: A\n    name: B\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("E429");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E429));
    }

    #[test]
    fn an_id_longer_than_the_declared_range_is_reported() {
        let tables = template("schema Item {\n    id: text(1..3)\n    name: text(1..40)\n}\n");
        let file = instances("data/items.ab", "Item :: @id.abcdef\n    name: A\n");
        let declarations = collect_instances(std::slice::from_ref(&file));
        let errors = check_instances(&declarations, &tables).expect_err("E413");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E413));
    }

    #[test]
    fn an_interpolation_error_names_the_statement_that_wrote_the_value() {
        // SPEC 5.11 fixes no column inside a value, so E425 is reported at the
        // construct that carries it, however deep the value sits.
        let tables = template(
            "schema Item {\n    name: text(1..40)\n    owner {\n        team: text(1..40)\n    }\n}\n",
        );
        let file = instances(
            "data/items.ab",
            "Item :: @id.one\n    name: Widget\n    owner.team: $nope\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let mut object = resolver.authored("one", 1).expect("the object builds");
        let variables = variable_table(&object, "one");
        let error = interpolate(&mut object, &variables).expect_err("$nope is unknown");
        let first = error.first().expect("one diagnostic");
        assert_eq!(first.id, ErrorId::E425);
        assert_eq!(
            first.position,
            Some(Position::new(3, 5)),
            "reported at 'owner.team:', not at the instance header"
        );
    }

    #[test]
    fn an_origin_falls_back_to_the_longest_written_prefix() {
        let mut object = AuthoredObject::default();
        object.set_at(Located::new("data/a.ab", Position::new(1, 1)));
        object.set_origin("owner", Located::new("data/a.ab", Position::new(4, 5)));
        assert_eq!(
            object.origin("owner.team").expect("a position").position,
            Position::new(4, 5)
        );
        assert_eq!(
            object.origin("other").expect("the header").position,
            Position::new(1, 1)
        );
    }

    /// A chain of `edges` clone statements, in the id order named by `ascending`.
    /// Resolution runs in `(template, id)` order (SPEC §2.7) and is memoised, so
    /// the two orders exercise the two sides of the check.
    fn clone_chain(edges: usize, ascending: bool) -> String {
        let mut text = String::new();
        for index in 0..=edges {
            let own = if ascending { index } else { edges - index };
            let has_source = if ascending { index < edges } else { own > 0 };
            text.push_str(&format!("Item :: @id.q{own:05}\n"));
            if has_source {
                let next = if ascending { own + 1 } else { own - 1 };
                text.push_str(&format!("&q{next:05}.*\n"));
            }
            text.push_str("    name: N\n\n");
        }
        text
    }

    #[test]
    fn the_clone_chain_limit_counts_edges_and_not_resolver_recursion() {
        // SPEC §5.7: "Chains deeper than 64 are E209." SPEC §7.6 makes the
        // diagnostic a property of the program, so it cannot depend on how the
        // instances happen to be named.
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        for ascending in [true, false] {
            let file = instances("data/items.ab", &clone_chain(CLONE_CHAIN, ascending));
            let declarations = collect_instances(std::slice::from_ref(&file));
            check_instances(&declarations, &tables)
                .unwrap_or_else(|_| panic!("a chain of {CLONE_CHAIN} edges is legal"));

            let file = instances("data/items.ab", &clone_chain(CLONE_CHAIN + 1, ascending));
            let declarations = collect_instances(std::slice::from_ref(&file));
            let errors = check_instances(&declarations, &tables).expect_err("one edge too deep");
            let ids: Vec<ErrorId> = errors.iter().map(|item| item.id).collect();
            assert_eq!(ids, [ErrorId::E209], "ascending: {ascending}");
        }
    }

    #[test]
    fn a_statement_annotation_is_range_checked() {
        // SPEC §4.12 states both rules of every version annotation, and SPEC
        // §11.1's "E603 before E430 and E440" can only be about a statement.
        let tables = template(
            "versions 1..3\n\nschema Item {\n    name: text(1..40)\n\
             \x20   v: text(1..40) @optional\n}\n",
        );
        let reported = |body: &str| -> Vec<ErrorId> {
            let text = format!("Item :: @id.x\n    name: N\n    {body}\n");
            let file = instances("data/items.ab", &text);
            let declarations = collect_instances(std::slice::from_ref(&file));
            check_instances(&declarations, &tables)
                .expect_err("the annotation is wrong")
                .iter()
                .map(|item| item.id)
                .collect()
        };
        assert_eq!(reported("v: a  @since(9)"), [ErrorId::E603]);
        assert_eq!(reported("v: a  @removed(9)"), [ErrorId::E603]);
        assert_eq!(reported("v: a  @since(0)"), [ErrorId::E603]);
        assert_eq!(reported("v: a  @since(3) @removed(2)"), [ErrorId::E604]);
    }

    #[test]
    fn cloning_an_envelope_key_is_reported_as_a_reserved_key() {
        // SPEC §5.7: `&other.id` is E410, and the E408 condition — the path is
        // absent on the source — is untrue of it.
        let tables = template("schema Item {\n    name: text(1..40)\n}\n");
        let file = instances(
            "data/items.ab",
            "Item :: @id.src\n    name: S\n\nItem :: @id.dst\n&src.id\n    name: D\n",
        );
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("P2 leaves E410 to P4");
        let table = InstanceTable::new(declarations);
        let mut resolver = Resolver::new(&tables, &table);
        let dst = table.get("dst").expect("the instance");
        assert!(check_clone_paths(&mut resolver, dst, tables.versions).is_empty());
    }
}
