//! Type-directed interpretation and validation (SPEC §5.10, §7.3 steps 3 to 8).
//!
//! Interpreting a syntax value against a declared type is one deterministic
//! function that performs, in this order: brace expansion, enum wildcard
//! expansion, `#tag` expansion and normalisation, per-element type, range and
//! enum checking, and finally single-value-to-list coercion and the
//! cardinality check. Interpretation writes the interpreted value back over
//! the authored one and is idempotent: running it again returns the same value
//! and the same compiled tree.
//!
//! The same walk produces the compiled [`Value`] in the key order of SPEC
//! §8.3, so the bytes that are emitted and the values that were checked can
//! never disagree.

use std::cell::Cell;
use std::path::{Component, Path, PathBuf};

use crate::ast::Located;
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note, Position};
use crate::instance::{InstanceDecl, ListSpelling, SyntaxValue};
use crate::lexer::{is_float_literal, is_int_literal, normalise};
use crate::limits::{DOCUMENT_DEPTH, LOGIC_WORK};
use crate::media::{probe_image, ImageFormat, ProbeError};
use crate::output::Value;
use crate::resolve::{
    apply_defaults, element_fields, interpolate, tag_field, variable_table, AuthoredObject,
    InstanceTable, Resolver, Step, ValuePath,
};
use crate::schema::{
    canonical_extension, suggest, FieldDecl, FieldKind, ImageAlt, IntRange, SchemaDecl,
    TemplateTables, TieBreak, TypeExpr,
};
use crate::versions::VersionRange;

/// What the on-disk asset checks of SPEC §9.5 may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetChecks {
    /// Existence, header probing and dimension checks all run.
    Enabled,
    /// `--skip-assets`: E421, E422 and E423 are skipped. Extension checks
    /// (E420) and path confinement (E424) still run, and the compiled bytes
    /// are unchanged.
    Skipped,
}

/// Which of the two interpretations of SPEC §7.3 is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Step 3: the authored values are checked and replaced by their
    /// interpreted form. An absent field is not yet an error and no file is
    /// touched.
    Authored,
    /// Step 7: the whole object, including everything logic wrote, is checked
    /// again, required fields must have values and assets are probed.
    Final,
}

/// Everything validation needs that is not the value itself.
#[derive(Clone, Debug)]
pub struct ValidationContext<'a> {
    pub tables: &'a TemplateTables,
    pub instances: &'a InstanceTable<'a>,
    /// The version being compiled.
    pub version: u32,
    /// `<project root>/assets`.
    pub assets_dir: &'a Path,
    pub asset_checks: AssetChecks,
    /// Loop iterations spent so far by the logic of this instance in this
    /// version (SPEC §3.7, E523). The budget belongs to the instance and
    /// not to one logic block, because a nested `$(Schema)` value runs a
    /// block of its own (SPEC §6.11); this context is built once per
    /// instance and per version, so holding it here gives it exactly that
    /// lifetime.
    pub work: Cell<u64>,
}

impl ValidationContext<'_> {
    fn project(&self) -> VersionRange {
        self.tables.versions
    }

    /// Charges one unit of logic work — one execution of a `for` body —
    /// and reports whether the budget of SPEC §3.7 is still unspent.
    pub fn charge_logic_work(&self) -> bool {
        let spent = self.work.get().saturating_add(1);
        self.work.set(spent);
        spent <= LOGIC_WORK
    }
}

/// The note SPEC §5.9 requires on E421 and E424.
const ASSETS_NOTE: &str = "assets live in '<project root>/assets'; the project root is the parent of the 'data' directory when one exists, and the compiled directory otherwise.";

// --------------------------------------------------------------- the driver

/// Compiles one instance for one version (SPEC §7.3), returning its object in
/// the key order of SPEC §8.3.
///
/// The steps run in the order the specification fixes: authored object,
/// interpolation, interpretation, defaults, logic, then the required-field
/// check and the full re-validation. Defaults are filled a second time after
/// logic, because an object that logic made present is defaulted and
/// required-checked in step 7.
pub fn compile_instance(
    resolver: &mut Resolver<'_>,
    decl: &InstanceDecl,
    version: u32,
    assets_dir: &Path,
    asset_checks: AssetChecks,
) -> Result<Value, Diagnostics> {
    let tables = resolver.tables();
    let Some(schema) = tables.schema(&decl.template) else {
        return Err(Diagnostics::one(Diagnostic::at(
            ErrorId::E401,
            decl.at.file.clone(),
            decl.at.position,
            format!("Unknown template '{}'.", decl.template),
        )));
    };
    envelope_assignments(decl)?;
    let mut object = resolver.authored(&decl.id, version)?;
    let context = ValidationContext {
        tables,
        instances: resolver.instances(),
        version,
        assets_dir,
        asset_checks,
        work: Cell::new(0),
    };

    header_tag_targets(decl, schema, &context)?;

    let variables = variable_table(&object, &decl.id);
    interpolate(&mut object, &variables)?;
    interpret_object(&mut object, schema, &context, Stage::Authored, &decl.id)?;
    apply_defaults(&mut object, schema, &variables, tables, version, &decl.id)?;

    if evaluate_logic(&mut object, schema, &context, &decl.id)? {
        // Step 7 re-runs step 4 over the whole tree, so a group that logic
        // made present is defaulted before it is required-checked
        // (SPEC §7.3 step 7, §6.11 step 4).
        apply_defaults(&mut object, schema, &variables, tables, version, &decl.id)?;
    }

    let fields = interpret_object(&mut object, schema, &context, Stage::Final, &decl.id)?;
    let mut entries = Vec::with_capacity(fields.len() + 2);
    entries.push(("template".to_string(), Value::Text(decl.template.clone())));
    entries.push(("id".to_string(), Value::Text(decl.id.clone())));
    entries.extend(fields);
    Ok(Value::Object(entries))
}

/// `template` and `id` are written by the compiler, so a header tag, a clone
/// path or a body statement that names one is E410 (SPEC §5.2, §5.4, §5.7).
///
/// The check belongs to step 1 of SPEC §7.3, where the authored object is
/// built: a schema that declares `template` as a field fails P3 first (E314),
/// and P4 never runs.
fn envelope_assignments(decl: &InstanceDecl) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    let mut report = |name: &str, at: &Located| {
        errors.push(Diagnostic::at(
            ErrorId::E410,
            at.file.clone(),
            at.position,
            format!("'{name}' is a reserved envelope key and cannot be assigned."),
        ));
    };

    for tag in &decl.tags {
        // `@id` is the instance's own identity and is not an assignment.
        if tag.name == "template" {
            report(&tag.name, &tag.at);
        }
    }
    for clone in &decl.clones {
        if let Some(path) = &clone.path {
            if path.starts_at_envelope_key() {
                let name = path.first().unwrap_or_default().to_string();
                report(&name, &path.at);
            }
        }
    }
    for statement in &decl.statements {
        if statement.path.starts_at_envelope_key() {
            let name = statement.path.first().unwrap_or_default().to_string();
            report(&name, &statement.path.at);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Step 5 of SPEC §7.3, in the order of SPEC §6.11: logic for one object runs
/// for every nested `$(Schema)` value first — each field of type `$(Other)` in
/// schema declaration order, and each element of a list of `$(Other)` in
/// element order — and then for the object's own schema.
///
/// Inline groups are not named schemas and carry no block of their own, but a
/// `$(Schema)` value inside a group is visited like any other, because §6.11
/// treats every present nested object as "one object".
///
/// This is the seam [`crate::logic`] plugs into: it evaluates one block
/// against one object bound to `.`, and this function decides which objects
/// exist, in which order, and with which schema in scope. The two halves of
/// validation around it are [`interpret_object`] with [`Stage::Authored`]
/// before and [`Stage::Final`] after, with [`apply_defaults`] on both sides.
///
/// Returns whether any block ran, so that a project with no logic performs no
/// second pass of defaults and its compiled bytes are reached by the shortest
/// path.
fn evaluate_logic<'t>(
    object: &mut AuthoredObject,
    schema: &'t SchemaDecl,
    context: &ValidationContext<'t>,
    instance: &str,
) -> Result<bool, Diagnostics> {
    let at = object.at().cloned();
    let mut fields = std::mem::take(&mut object.fields);
    let nested = evaluate_nested(
        &mut fields,
        &schema.fields,
        context,
        at.as_ref(),
        instance,
        instance,
        0,
    );
    object.fields = fields;
    let mut ran = nested?;
    if let Some(block) = context.tables.logic_for(&schema.name) {
        let binding = crate::logic::Binding {
            context: instance,
            id: instance,
            at: at.as_ref(),
        };
        crate::logic::evaluate(block, object, schema, context, &binding)?;
        ran = true;
    }
    Ok(ran)
}

/// The `$(Schema)` values reachable from one object, visited depth first in
/// declaration and element order (SPEC §6.11).
#[allow(clippy::too_many_arguments)]
fn evaluate_nested<'t>(
    fields: &mut [(String, SyntaxValue)],
    declarations: &'t [FieldDecl],
    context: &ValidationContext<'t>,
    at: Option<&Located>,
    instance: &str,
    path: &str,
    depth: usize,
) -> Result<bool, Diagnostics> {
    // The document depth limit of SPEC §3.7 bounds this walk exactly as it
    // bounds the one that builds the object; a legal `$(Schema)` cycle is
    // finite only because some edge of it is absent.
    if depth >= DOCUMENT_DEPTH {
        return Ok(false);
    }
    let mut ran = false;
    for declaration in declarations {
        if !declaration.exists_in(context.version, context.project()) {
            continue;
        }
        let Some(inner) = element_fields(context.tables, declaration) else {
            continue;
        };
        let target = match &declaration.kind {
            FieldKind::Scalar {
                ty: TypeExpr::Nested { schema },
                ..
            } => context.tables.schema(schema),
            _ => None,
        };
        let Some(index) = fields
            .iter()
            .position(|(name, _)| name == &declaration.name)
        else {
            continue;
        };
        // The `{context}` substitution of SPEC §9.8: a group or `$(Schema)`
        // value is `atlas.owner`, and an element of a list is `atlas.copy[2]`.
        let own = format!("{path}.{}", declaration.name);
        let objects: Vec<(String, &mut Vec<(String, SyntaxValue)>)> = match &mut fields[index].1 {
            SyntaxValue::Object(entries) => vec![(own.clone(), entries)],
            SyntaxValue::List(items, _) => items
                .iter_mut()
                .enumerate()
                .filter_map(|(position, item)| match item {
                    SyntaxValue::Object(entries) => Some((format!("{own}[{position}]"), entries)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let step = if declaration.is_list() { 2 } else { 1 };
        for (child, entries) in objects {
            if evaluate_nested(entries, inner, context, at, instance, &child, depth + step)? {
                ran = true;
            }
            let Some(nested) = target else {
                continue;
            };
            let Some(block) = context.tables.logic_for(&nested.name) else {
                continue;
            };
            // `logic::evaluate` binds `.` to a whole object, so the nested
            // fields are lent to one for the length of the call and handed
            // back whether it succeeded or not.
            let mut bound = AuthoredObject::default();
            if let Some(at) = at {
                bound.set_at(at.clone());
            }
            bound.fields = std::mem::take(entries);
            let binding = crate::logic::Binding {
                context: &child,
                id: instance,
                at,
            };
            let outcome = crate::logic::evaluate(block, &mut bound, nested, context, &binding);
            *entries = bound.fields;
            outcome?;
            ran = true;
        }
    }
    Ok(ran)
}

/// A header tag assigns one root field (SPEC §5.2): a tag on a group, a list
/// group or a `$(Schema)` field is E438 because it can never reach a nested
/// field, and a bare flag is valid only on a `bool` field (E412).
///
/// A scalar list field is not nested, so a tag on one is legal: its single
/// value is coerced into a one-element list exactly as a body value would be
/// (SPEC §4.6, §5.10).
fn header_tag_targets(
    decl: &InstanceDecl,
    schema: &SchemaDecl,
    context: &ValidationContext<'_>,
) -> Result<(), Diagnostics> {
    let mut errors = Diagnostics::new();
    for tag in &decl.tags {
        if tag.name == "id" || tag.name == "template" {
            continue;
        }
        let Some(field) = schema.field(&tag.name) else {
            continue; // E409 is raised where the value is validated.
        };
        if !field.exists_in(context.version, context.project()) {
            continue;
        }
        if element_fields(context.tables, field).is_some() {
            errors.push(Diagnostic::at(
                ErrorId::E438,
                tag.at.file.clone(),
                tag.at.position,
                format!(
                    "Header tag '@{}' targets {} '{}'; a header tag assigns one root scalar field. Write a body statement.",
                    tag.spelled,
                    field.kind_word(),
                    field.name
                ),
            ));
            continue;
        }
        if tag.flag && !matches!(field.type_expr(), Some(TypeExpr::Bool)) {
            // SPEC §5.2: the bare `@name` form sets a boolean, so `{found}` is
            // `bool`; substituting `text` renders "expected text, found text"
            // on a `text` field.
            errors.push(Diagnostic::at(
                ErrorId::E412,
                tag.at.file.clone(),
                tag.at.position,
                format!(
                    "Type mismatch at {}.{}: expected {}, found bool.",
                    decl.id,
                    field.name,
                    field.kind_word()
                ),
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Interprets every present value of an object against its declared type,
/// writes the interpreted values back and returns the compiled fields in the
/// key order of SPEC §8.3. `context` is the `{context}` substitution of the
/// root object: the instance id (SPEC §9.8).
pub fn interpret_object(
    object: &mut AuthoredObject,
    schema: &SchemaDecl,
    validation: &ValidationContext<'_>,
    stage: Stage,
    context: &str,
) -> Result<Vec<(String, Value)>, Diagnostics> {
    let mut fields = std::mem::take(&mut object.fields);
    let root = object.at().cloned();
    let (compiled, errors, masks) = {
        let mut walk = Walk {
            validation,
            source: &*object,
            stage,
            errors: Diagnostics::new(),
            masks: Vec::new(),
            root,
        };
        let mut path: ValuePath = Vec::new();
        let compiled = walk.object(&mut fields, &schema.fields, context, "", &mut path, 0);
        (compiled, walk.errors, walk.masks)
    };
    object.fields = fields;
    for (path, spans) in masks {
        object.set_spans(path, spans);
    }
    if errors.is_empty() {
        Ok(compiled)
    } else {
        Err(errors)
    }
}

// ------------------------------------------------------------------ the walk

struct Walk<'a, 'b> {
    validation: &'a ValidationContext<'b>,
    source: &'a AuthoredObject,
    stage: Stage,
    errors: Diagnostics,
    masks: Vec<(ValuePath, Vec<(usize, usize)>)>,
    root: Option<Located>,
}

impl Walk<'_, '_> {
    fn at(&self, dotted: &str) -> Located {
        self.source
            .origin(dotted)
            .cloned()
            .or_else(|| self.root.clone())
            .unwrap_or_else(|| Located::new("", Position::new(1, 1)))
    }

    fn error(&mut self, id: ErrorId, dotted: &str, message: String) {
        let at = self.at(dotted);
        self.errors
            .push(Diagnostic::at(id, at.file, at.position, message));
    }

    fn error_with_note(&mut self, id: ErrorId, dotted: &str, message: String, note: &str) {
        let at = self.at(dotted);
        self.errors
            .push(Diagnostic::at(id, at.file, at.position, message).with_note(Note::new(note)));
    }

    /// One object: the unknown fields, then every declared field in
    /// declaration order (SPEC §5.12, §8.3).
    fn object(
        &mut self,
        fields: &mut [(String, SyntaxValue)],
        declarations: &[FieldDecl],
        context: &str,
        dotted: &str,
        path: &mut ValuePath,
        depth: usize,
    ) -> Vec<(String, Value)> {
        // SPEC §3.7: the instance object is level 1, and every object and list
        // below it is one further level, so an object reached with `depth`
        // ancestors stands at level `depth + 1`.
        if depth >= DOCUMENT_DEPTH {
            self.error(
                ErrorId::E209,
                dotted,
                format!("Instance nesting depth exceeds the limit of {DOCUMENT_DEPTH}."),
            );
            return Vec::new();
        }
        let version = self.validation.version;
        let project = self.validation.project();
        let declared: Vec<&str> = declarations
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let unknown: Vec<String> = fields
            .iter()
            .map(|(name, _)| name.clone())
            .filter(|name| !declared.contains(&name.as_str()))
            .collect();
        for name in unknown {
            let child = join(dotted, &name);
            let mut message = format!("Unknown field '{name}' in {context}.");
            if let Some(hint) = suggest(&name, &declared, TieBreak::DeclarationOrder) {
                message.push_str(&format!(" Did you mean '{hint}'?"));
            }
            let note = format!("declared fields: {}.", declared.join(", "));
            self.error_with_note(ErrorId::E409, &child, message, &note);
        }

        let mut compiled: Vec<(String, Value)> = Vec::new();
        for declaration in declarations {
            if !declaration.exists_in(version, project) {
                // A statement writing a field that does not exist in this
                // version is rejected before P4 (E430, E440), so no value can
                // be here to emit.
                continue;
            }
            let child = join(dotted, &declaration.name);
            let index = fields
                .iter()
                .position(|(name, _)| name == &declaration.name);
            // SPEC §3.7 counts a list as one level of its own, so a list field
            // inside an object that already stands at the limit is one level
            // too deep whatever its elements are.
            if declaration.is_list() && index.is_some() && depth + 1 >= DOCUMENT_DEPTH {
                self.error(
                    ErrorId::E209,
                    &child,
                    format!("Instance nesting depth exceeds the limit of {DOCUMENT_DEPTH}."),
                );
                continue;
            }
            let before = self.errors.len();
            let value = match index {
                Some(index) => {
                    path.push(Step::Field(declaration.name.clone()));
                    let (_, slot) = &mut fields[index];
                    let value = self.field(declaration, slot, context, &child, path, depth);
                    path.pop();
                    value
                }
                None => None,
            };
            // A field whose own value was rejected is not also missing: one
            // defect produces one diagnostic.
            let rejected = self.errors.len() > before;
            match value {
                Some(value) => compiled.push((declaration.name.clone(), value)),
                None if !rejected && self.stage == Stage::Final && declaration.is_required() => {
                    let window = declaration
                        .window_in(project)
                        .map(|range| range.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    self.error_with_note(
                        ErrorId::E411,
                        &child,
                        format!("Missing required field {context}.{}.", declaration.name),
                        &format!(
                            "this field exists in versions {window} and has no value in version {version}."
                        ),
                    );
                }
                None => {}
            }
        }
        compiled
    }

    /// One field: the whole of SPEC §5.10 for one declared type.
    fn field(
        &mut self,
        declaration: &FieldDecl,
        value: &mut SyntaxValue,
        context: &str,
        dotted: &str,
        path: &mut ValuePath,
        depth: usize,
    ) -> Option<Value> {
        match element_fields(self.validation.tables, declaration) {
            Some(inner) => {
                self.object_field(declaration, inner, value, context, dotted, path, depth)
            }
            None => self.scalar_field(declaration, value, context, dotted, path),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn object_field(
        &mut self,
        declaration: &FieldDecl,
        inner: &[FieldDecl],
        value: &mut SyntaxValue,
        context: &str,
        dotted: &str,
        path: &mut ValuePath,
        depth: usize,
    ) -> Option<Value> {
        let own = format!("{context}.{}", declaration.name);
        if !declaration.is_list() {
            let mut fields = self.as_object(declaration, value, context, dotted)?;
            let compiled = self.object(&mut fields, inner, &own, dotted, path, depth + 1);
            *value = SyntaxValue::Object(fields);
            if compiled.is_empty() {
                // A group with no values is absent, never an empty object
                // (SPEC §8.9).
                return None;
            }
            return Some(Value::Object(compiled));
        }

        let before = self.errors.len();
        let spelling = list_spelling(value);
        let elements = take_elements(value);
        let elements = self.expand_object_elements(declaration, inner, elements, &own, dotted);
        let mut interpreted: Vec<SyntaxValue> = Vec::with_capacity(elements.len());
        let mut compiled: Vec<Value> = Vec::with_capacity(elements.len());
        for element in elements {
            let mut element = element;
            let element_context = format!("{own}[{}]", interpreted.len());
            let Some(mut fields) = self.as_object(declaration, &mut element, context, dotted)
            else {
                continue;
            };
            path.push(Step::Index(interpreted.len()));
            // The list itself is one level (SPEC §3.7), so an element object
            // stands two levels below the object that declares the field.
            let values = self.object(
                &mut fields,
                inner,
                &element_context,
                dotted,
                path,
                depth + 2,
            );
            path.pop();
            interpreted.push(SyntaxValue::Object(fields));
            compiled.push(Value::Object(values));
        }
        self.duplicate_keys(declaration, &interpreted, context, dotted);
        *value = SyntaxValue::List(interpreted, spelling);
        if self.errors.len() == before {
            self.cardinality(declaration, compiled.len(), context, dotted);
        }
        Some(Value::List(compiled))
    }

    /// A group value is written as an object, or with the `#tag` shorthand
    /// (SPEC §5.5, §5.10).
    fn as_object(
        &mut self,
        declaration: &FieldDecl,
        value: &mut SyntaxValue,
        context: &str,
        dotted: &str,
    ) -> Option<Vec<(String, SyntaxValue)>> {
        match value {
            SyntaxValue::Object(fields) => Some(std::mem::take(fields)),
            SyntaxValue::Tag { name, args } => {
                let Some(tag) = tag_field(self.validation.tables, declaration) else {
                    let message = format!(
                        "{context}.{} has no @tag field, so '#{name}' shorthand cannot be used here.",
                        declaration.name
                    );
                    self.error(ErrorId::E418, dotted, message);
                    return None;
                };
                let inner = element_fields(self.validation.tables, declaration).unwrap_or(&[]);
                let mut fields = vec![(tag.name.clone(), SyntaxValue::Bare(name.clone()))];
                for argument in args.iter() {
                    let Some(head) = argument.path.segments.first() else {
                        continue;
                    };
                    // SPEC §5.5: a bare `flag` argument sets the field `flag`
                    // to `true` and is valid only when `flag` is a `bool`
                    // field. The check belongs here because the argument is
                    // folded into a bare `true` below, after which the flag
                    // form is no longer visible. A field no declaration names
                    // is left to E409, raised where the value is validated.
                    if argument.flag {
                        let declared = inner.iter().find(|field| &field.name == head);
                        if let Some(field) = declared {
                            if !matches!(field.type_expr(), Some(TypeExpr::Bool)) {
                                let message = format!(
                                    "Type mismatch at {context}.{}.{head}: expected {}, found bool.",
                                    declaration.name,
                                    field.kind_word()
                                );
                                self.error(ErrorId::E412, dotted, message);
                                continue;
                            }
                        }
                    }
                    let nested = wrap(
                        argument.path.segments.get(1..).unwrap_or(&[]),
                        argument.value.clone(),
                    );
                    match fields.iter_mut().find(|(key, _)| key == head) {
                        Some((_, slot)) => *slot = nested,
                        None => fields.push((head.clone(), nested)),
                    }
                }
                Some(fields)
            }
            other => {
                let found = other.shape();
                let message = format!(
                    "Type mismatch at {context}.{}: expected object, found {found}.",
                    declaration.name
                );
                self.error(ErrorId::E412, dotted, message);
                None
            }
        }
    }

    /// An enum wildcard replicates a whole list element: the name of a `#tag`
    /// shorthand and a tuple cell alike (SPEC §5.6).
    fn expand_object_elements(
        &mut self,
        declaration: &FieldDecl,
        inner: &[FieldDecl],
        elements: Vec<SyntaxValue>,
        context: &str,
        dotted: &str,
    ) -> Vec<SyntaxValue> {
        let mut out: Vec<SyntaxValue> = Vec::with_capacity(elements.len());
        for element in elements {
            match &element {
                SyntaxValue::Tag { name, args } => {
                    let members =
                        tag_field(self.validation.tables, declaration).and_then(enum_members);
                    let expanded =
                        members.and_then(|members| self.match_wildcard(name, &members, dotted));
                    match expanded {
                        Some(matches) => {
                            let args = args.clone();
                            out.extend(matches.into_iter().map(|member| SyntaxValue::Tag {
                                name: member,
                                args: args.clone(),
                            }));
                        }
                        None => out.push(element),
                    }
                }
                SyntaxValue::Object(fields) => {
                    let mut rows = vec![fields.clone()];
                    for field in inner {
                        let Some(members) = enum_members(field) else {
                            continue;
                        };
                        let mut next: Vec<Vec<(String, SyntaxValue)>> = Vec::new();
                        for row in rows {
                            let text = match row.iter().find(|(key, _)| key == &field.name) {
                                Some((_, SyntaxValue::Bare(text))) => text.clone(),
                                _ => {
                                    next.push(row);
                                    continue;
                                }
                            };
                            match self.match_wildcard(&text, &members, dotted) {
                                Some(matches) => {
                                    for member in matches {
                                        let mut copy = row.clone();
                                        for (key, value) in copy.iter_mut() {
                                            if key == &field.name {
                                                *value = SyntaxValue::Bare(member.clone());
                                            }
                                        }
                                        next.push(copy);
                                    }
                                }
                                None => next.push(row),
                            }
                        }
                        rows = next;
                    }
                    out.extend(rows.into_iter().map(SyntaxValue::Object));
                }
                _ => out.push(element),
            }
        }
        let _ = context;
        out
    }

    /// The members a prefix wildcard names, or `None` when the text is not a
    /// wildcard. A wildcard that matches nothing is E415 (SPEC §5.6).
    fn match_wildcard(
        &mut self,
        text: &str,
        members: &[String],
        dotted: &str,
    ) -> Option<Vec<String>> {
        let normalised = normalise(text);
        let prefix = normalised.strip_suffix('*')?.to_string();
        let matched: Vec<String> = members
            .iter()
            .filter(|member| member.starts_with(&prefix))
            .cloned()
            .collect();
        if matched.is_empty() {
            self.error(
                ErrorId::E415,
                dotted,
                format!(
                    "Wildcard '{prefix}*' matches no member of {}.",
                    members.join(", ")
                ),
            );
        }
        Some(matched)
    }

    /// Within one list value, two elements whose tag field normalises alike
    /// are E444 (SPEC §5.5).
    fn duplicate_keys(
        &mut self,
        declaration: &FieldDecl,
        elements: &[SyntaxValue],
        context: &str,
        dotted: &str,
    ) {
        let Some(tag) = tag_field(self.validation.tables, declaration) else {
            return;
        };
        let mut seen: Vec<String> = Vec::new();
        for element in elements {
            let SyntaxValue::Object(fields) = element else {
                continue;
            };
            let key = match fields.iter().find(|(name, _)| name == &tag.name) {
                Some((_, SyntaxValue::Bare(text))) | Some((_, SyntaxValue::Quoted(text))) => {
                    normalise(text)
                }
                _ => continue,
            };
            if seen.contains(&key) {
                self.error_with_note(
                    ErrorId::E444,
                    dotted,
                    format!(
                        "{context}.{} already has an element whose {} is '{key}'.",
                        declaration.name, tag.name
                    ),
                    "an earlier element of this value already has that key.",
                );
                continue;
            }
            seen.push(key);
        }
    }

    fn cardinality(&mut self, declaration: &FieldDecl, count: usize, context: &str, dotted: &str) {
        let cardinality = declaration.cardinality();
        if cardinality.accepts(u32::try_from(count).unwrap_or(u32::MAX)) {
            return;
        }
        self.error(
            ErrorId::E445,
            dotted,
            format!(
                "{context}.{} has {count} elements; {} declares {}.",
                declaration.name,
                declaration.name,
                cardinality.describe()
            ),
        );
    }

    // --------------------------------------------------------------- scalars

    fn scalar_field(
        &mut self,
        declaration: &FieldDecl,
        value: &mut SyntaxValue,
        context: &str,
        dotted: &str,
        path: &mut ValuePath,
    ) -> Option<Value> {
        let ty = declaration.type_expr()?.clone();
        if !declaration.is_list() {
            return self.element(declaration, &ty, value, context, dotted, path, false);
        }

        let before = self.errors.len();
        let indexed = matches!(value, SyntaxValue::List(_, _));
        let spelling = list_spelling(value);
        let items = take_elements(value);
        let mut interpreted: Vec<SyntaxValue> = Vec::new();
        let mut compiled: Vec<Value> = Vec::new();
        for (written, item) in items.into_iter().enumerate() {
            if matches!(item, SyntaxValue::List(_, _)) {
                self.error(
                    ErrorId::E441,
                    dotted,
                    "Nested lists are not supported.".to_string(),
                );
                continue;
            }
            let mut source = path.clone();
            if indexed {
                source.push(Step::Index(written));
            }
            for mut expanded in self.expand_element(&ty, item, &source, dotted) {
                path.push(Step::Index(interpreted.len()));
                let value =
                    self.element(declaration, &ty, &mut expanded, context, dotted, path, true);
                path.pop();
                if let Some(value) = value {
                    interpreted.push(expanded);
                    compiled.push(value);
                }
            }
        }
        *value = SyntaxValue::List(interpreted, spelling);
        if self.errors.len() == before {
            self.cardinality(declaration, compiled.len(), context, dotted);
        }
        Some(Value::List(compiled))
    }

    /// Brace expansion and enum wildcard expansion, the two rules that turn
    /// one written element into several (SPEC §5.6, §5.8).
    fn expand_element(
        &mut self,
        ty: &TypeExpr,
        item: SyntaxValue,
        source: &ValuePath,
        dotted: &str,
    ) -> Vec<SyntaxValue> {
        match (&item, ty) {
            (SyntaxValue::Bare(text), TypeExpr::File { .. } | TypeExpr::Image { .. }) => {
                let mask = self.source.inserted_spans(source).to_vec();
                if !has_group(text, &mask) {
                    return vec![item];
                }
                match expand_braces(text, &mask) {
                    Ok(paths) => paths.into_iter().map(SyntaxValue::Bare).collect(),
                    Err(reason) => {
                        self.error(
                            ErrorId::E435,
                            dotted,
                            format!("Invalid brace pattern '{text}': {reason}."),
                        );
                        Vec::new()
                    }
                }
            }
            (SyntaxValue::Bare(text), TypeExpr::Enum { members }) => {
                let text = text.clone();
                let members = members.clone();
                match self.match_wildcard(&text, &members, dotted) {
                    Some(matched) => matched.into_iter().map(SyntaxValue::Bare).collect(),
                    None => vec![item],
                }
            }
            _ => vec![item],
        }
    }

    /// One element against one declared type: the table of SPEC §5.10.
    #[allow(clippy::too_many_arguments)]
    fn element(
        &mut self,
        declaration: &FieldDecl,
        ty: &TypeExpr,
        value: &mut SyntaxValue,
        context: &str,
        dotted: &str,
        path: &ValuePath,
        in_list: bool,
    ) -> Option<Value> {
        let field = declaration.name.clone();
        let (text, quoted) = match value {
            SyntaxValue::Bare(text) => (text.clone(), false),
            SyntaxValue::Quoted(text) => (text.clone(), true),
            SyntaxValue::List(_, spelling) => {
                let message = format!(
                    "Type mismatch at {context}.{field}: expected {}, found list.",
                    ty.kind()
                );
                // The note offers to quote a literal comma, which is advice
                // about the bare spelling alone: it would be a non-sequitur on
                // `[a, b]`, on a tuple array, or on a list that a default or a
                // `derive` produced.
                match spelling {
                    ListSpelling::Commas => self.error_with_note(
                        ErrorId::E412,
                        dotted,
                        message,
                        "a ',' outside brackets separates list items; quote the value to include a literal comma.",
                    ),
                    ListSpelling::Brackets => self.error(ErrorId::E412, dotted, message),
                }
                return None;
            }
            SyntaxValue::Tag { .. } => {
                self.error_with_note(
                    ErrorId::E412,
                    dotted,
                    format!(
                        "Type mismatch at {context}.{field}: expected {}, found tag object.",
                        ty.kind()
                    ),
                    "a leading '#' starts a tag object; quote the value to write a literal '#'.",
                );
                return None;
            }
            SyntaxValue::Object(_) => {
                self.error(
                    ErrorId::E412,
                    dotted,
                    format!(
                        "Type mismatch at {context}.{field}: expected {}, found object.",
                        ty.kind()
                    ),
                );
                return None;
            }
        };

        let compiled = match ty {
            TypeExpr::Text { ranges } => {
                let length = text.chars().count() as i64;
                if !ranges.is_empty() && !ranges.iter().any(|range| range.contains(length)) {
                    self.range_error(
                        context,
                        &field,
                        dotted,
                        length.to_string(),
                        &describe(ranges),
                    );
                    return None;
                }
                Value::Text(text.clone())
            }
            TypeExpr::Int { ranges } => {
                if quoted || !is_int_literal(&text) {
                    self.type_error(context, &field, dotted, "int", "text");
                    return None;
                }
                let Ok(number) = text.parse::<i64>() else {
                    self.error(
                        ErrorId::E211,
                        dotted,
                        format!("'{text}' is not a valid int literal."),
                    );
                    return None;
                };
                if !ranges.is_empty() && !ranges.iter().any(|range| range.contains(number)) {
                    self.range_error(
                        context,
                        &field,
                        dotted,
                        number.to_string(),
                        &describe(ranges),
                    );
                    return None;
                }
                Value::Int(number)
            }
            TypeExpr::Float { ranges } => {
                if quoted || !(is_float_literal(&text) || is_int_literal(&text)) {
                    self.type_error(context, &field, dotted, "float", "text");
                    return None;
                }
                // SPEC §3.5 classifies `-0` as an integer literal, and §4.4.3
                // widens an integer literal to the float it denotes, so the
                // value is zero. Parsing it as a float directly would yield
                // negative zero, which §8.7 renders as `-0.0` and §7.5 treats
                // as a different value from `0.0`.
                let integer = is_int_literal(&text)
                    .then(|| text.parse::<i64>().ok())
                    .flatten()
                    .map(|value| value as f64);
                let Some(number) = integer
                    .or_else(|| text.parse::<f64>().ok())
                    .filter(|value| value.is_finite())
                else {
                    self.error(
                        ErrorId::E211,
                        dotted,
                        format!("'{text}' is not a valid float literal."),
                    );
                    return None;
                };
                if !ranges.is_empty() && !ranges.iter().any(|range| range.contains(number)) {
                    let list = ranges
                        .iter()
                        .map(|range| {
                            format!("{}..{}", render_float(range.min), render_float(range.max))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.range_error(context, &field, dotted, render_float(number), &list);
                    return None;
                }
                Value::Float(number)
            }
            TypeExpr::Bool => {
                if quoted || (text != "true" && text != "false") {
                    self.type_error(context, &field, dotted, "bool", "text");
                    return None;
                }
                Value::Bool(text == "true")
            }
            TypeExpr::Enum { members } => {
                let normalised = normalise(&text);
                if !members.contains(&normalised) {
                    let candidates: Vec<&str> =
                        members.iter().map(|member| member.as_str()).collect();
                    let mut message = format!(
                        "Enum mismatch at {context}.{field}: '{normalised}' is not one of {}.",
                        members.join(", ")
                    );
                    if let Some(hint) =
                        suggest(&normalised, &candidates, TieBreak::DeclarationOrder)
                    {
                        message.push_str(&format!(" Did you mean '{hint}'?"));
                    }
                    self.error(ErrorId::E414, dotted, message);
                    return None;
                }
                *value = SyntaxValue::Bare(normalised.clone());
                Value::Text(normalised)
            }
            TypeExpr::File { .. } | TypeExpr::Image { .. } => {
                self.asset(
                    declaration,
                    ty,
                    &text,
                    context,
                    dotted,
                    path,
                    in_list,
                    quoted,
                )?;
                Value::Text(text.clone())
            }
            TypeExpr::Ref { schema } => {
                let target = normalise(&text);
                self.reference(&target, schema, context, &field, dotted)?;
                *value = SyntaxValue::Bare(target.clone());
                Value::Text(target)
            }
            TypeExpr::Nested { .. } => {
                self.type_error(context, &field, dotted, "object", "text");
                return None;
            }
        };

        // Any brace left in an interpreted text value is literal: either the
        // field expands no braces, or expansion has already run and what is
        // left arrived from a variable (SPEC §5.8). Recording it keeps
        // interpretation idempotent when the object is walked again.
        if let Value::Text(text) = &compiled {
            if text.contains('{') || text.contains('}') {
                self.masks.push((path.clone(), vec![(0, text.len())]));
            }
        }
        Some(compiled)
    }

    fn type_error(
        &mut self,
        context: &str,
        field: &str,
        dotted: &str,
        expected: &str,
        found: &str,
    ) {
        self.error(
            ErrorId::E412,
            dotted,
            format!("Type mismatch at {context}.{field}: expected {expected}, found {found}."),
        );
    }

    fn range_error(
        &mut self,
        context: &str,
        field: &str,
        dotted: &str,
        value: String,
        ranges: &str,
    ) {
        self.error(
            ErrorId::E413,
            dotted,
            format!("Range mismatch at {context}.{field}: {value} is not in {ranges}."),
        );
    }

    /// `ref(Schema)`: the target must exist, carry that schema and exist in
    /// the version being compiled (SPEC §4.4.8).
    fn reference(
        &mut self,
        target: &str,
        schema: &str,
        context: &str,
        field: &str,
        dotted: &str,
    ) -> Option<()> {
        let Some(decl) = self.validation.instances.get(target) else {
            let candidates = self.validation.instances.ids_of(schema);
            let mut message = format!("Unknown reference '{target}' at {context}.{field}.");
            if let Some(hint) = suggest(target, &candidates, TieBreak::ScalarOrder) {
                message.push_str(&format!(" Did you mean '{hint}'?"));
            }
            self.error(ErrorId::E431, dotted, message);
            return None;
        };
        if decl.template != schema {
            self.error(
                ErrorId::E432,
                dotted,
                format!(
                    "Reference '{target}' at {context}.{field} is a '{}', expected a '{schema}'.",
                    decl.template
                ),
            );
            return None;
        }
        let window = decl.window.resolve(self.validation.project());
        if window
            .map(|range| range.contains(self.validation.version))
            .unwrap_or(false)
        {
            return Some(());
        }
        let versions = window
            .map(|range| range.to_string())
            .unwrap_or_else(|| "none".to_string());
        let version = self.validation.version;
        self.error_with_note(
            ErrorId::E431,
            dotted,
            format!(
                "Reference '{target}' at {context}.{field} does not exist in version {version}."
            ),
            &format!("instance '{target}' exists in versions {versions}."),
        );
        None
    }

    /// A `file` or `image` value: confinement, extension and, in step 7, the
    /// on-disk checks (SPEC §4.4.6, §4.4.7, §5.9).
    #[allow(clippy::too_many_arguments)]
    fn asset(
        &mut self,
        declaration: &FieldDecl,
        ty: &TypeExpr,
        text: &str,
        context: &str,
        dotted: &str,
        path: &ValuePath,
        in_list: bool,
        quoted: bool,
    ) -> Option<()> {
        let field = declaration.name.clone();
        if !in_list && !quoted && has_group(text, self.source.inserted_spans(path)) {
            self.error(
                ErrorId::E434,
                dotted,
                format!(
                    "Brace patterns expand into several paths and require a list field; {context}.{field} is not a list."
                ),
            );
            return None;
        }
        let Some(relative) = relative_asset(text) else {
            let at = self.at(dotted);
            self.errors.push(
                Diagnostic::at(
                    ErrorId::E424,
                    at.file,
                    at.position,
                    format!(
                        "Asset path '{}' must be relative to assets/ and must not be absolute or contain '..'.",
                        display_asset(text)
                    ),
                )
                .with_note(Note::new(ASSETS_NOTE)),
            );
            return None;
        };

        let allowed = ty.extensions();
        let extension = text
            .rsplit_once('.')
            .map(|(_, extension)| canonical_extension(extension))
            .unwrap_or_default();
        if !allowed.contains(&extension) {
            self.error(
                ErrorId::E420,
                dotted,
                format!(
                    "Extension '{extension}' is not allowed at {context}.{field}; allowed: {}.",
                    allowed.join(", ")
                ),
            );
            return None;
        }
        if self.stage != Stage::Final || self.validation.asset_checks == AssetChecks::Skipped {
            return Some(());
        }

        let resolved = self.validation.assets_dir.join(&relative);
        if !resolved.is_file() {
            let at = self.at(dotted);
            self.errors.push(
                Diagnostic::at(
                    ErrorId::E421,
                    at.file,
                    at.position,
                    format!(
                        "File not found for {context}.{field}: '{}' (resolved to 'assets/{relative}').",
                        display_asset(text)
                    ),
                )
                .with_note(Note::new(ASSETS_NOTE)),
            );
            return None;
        }
        let TypeExpr::Image { alternatives } = ty else {
            return Some(());
        };
        let alternatives = alternatives.clone();
        self.image(
            &alternatives,
            &extension,
            &resolved,
            text,
            context,
            &field,
            dotted,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        alternatives: &[ImageAlt],
        extension: &str,
        resolved: &Path,
        text: &str,
        context: &str,
        field: &str,
        dotted: &str,
    ) -> Option<()> {
        let expected = ImageFormat::from_extension(extension)?;
        let info = match probe_image(resolved) {
            Ok(info) => info,
            Err(error) => {
                let actual = probe_subject(&error);
                let at = self.at(dotted);
                let mut diagnostic = Diagnostic::at(
                    ErrorId::E422,
                    at.file,
                    at.position,
                    format!(
                        "Image content mismatch at {context}.{field}: '{}' is {actual}, not {expected}.",
                        display_asset(text)
                    ),
                );
                if let Some(note) = error.note() {
                    diagnostic = diagnostic.with_note(Note::new(note));
                }
                self.errors.push(diagnostic);
                return None;
            }
        };
        if info.format != expected {
            self.error(
                ErrorId::E422,
                dotted,
                format!(
                    "Image content mismatch at {context}.{field}: '{}' is {}, not {expected}.",
                    display_asset(text),
                    info.format
                ),
            );
            return None;
        }
        let matching: Vec<&ImageAlt> = alternatives
            .iter()
            .filter(|alternative| alternative.extension == extension)
            .collect();
        let fits = matching.iter().any(|alternative| {
            alternative
                .width
                .map(|width| width == info.width)
                .unwrap_or(true)
                && alternative
                    .height
                    .map(|height| height == info.height)
                    .unwrap_or(true)
        });
        if fits {
            return Some(());
        }
        let list = matching
            .iter()
            .map(|alternative| alternative.describe())
            .collect::<Vec<_>>()
            .join(", ");
        self.error(
            ErrorId::E423,
            dotted,
            format!(
                "Image size mismatch at {context}.{field}: '{}' is {}x{}, expected one of {list}.",
                display_asset(text),
                info.width,
                info.height
            ),
        );
        None
    }
}

/// The `{actual}` substitution of E422 for a failed probe.
///
/// When the signature matched but the header did not decode, the file is no
/// readable image at all, so naming the matched format would render the message
/// as "is bmp, not bmp". The note beside it names the check that failed, and
/// the declared format is already the `{expected}` half (SPEC §4.4.7.1).
fn probe_subject(error: &ProbeError) -> String {
    match error {
        ProbeError::Truncated { .. }
        | ProbeError::MalformedBmpWidth(_)
        | ProbeError::MalformedCanvasSize { .. }
        | ProbeError::NoStartOfFrame => "unreadable".to_string(),
        ProbeError::UnrecognisedSignature(bytes) => format!("not an image ({bytes})"),
        ProbeError::Io(reason) | ProbeError::Malformed(reason) => reason.clone(),
    }
}

/// The elements of a list value, or the single value a list field is given,
/// which SPEC §5.10 coerces into a one-element array.
fn take_elements(value: &mut SyntaxValue) -> Vec<SyntaxValue> {
    match value {
        SyntaxValue::List(items, _) => std::mem::take(items),
        other => vec![std::mem::replace(other, SyntaxValue::Bare(String::new()))],
    }
}

/// How a value that is about to be coerced into a list was written, so that
/// the interpreted list keeps the spelling (SPEC §5.5, E412's note). A single
/// value promoted to a one-item list was never a comma list.
fn list_spelling(value: &SyntaxValue) -> ListSpelling {
    match value {
        SyntaxValue::List(_, spelling) => *spelling,
        _ => ListSpelling::Brackets,
    }
}

fn enum_members(field: &FieldDecl) -> Option<Vec<String>> {
    match field.type_expr() {
        Some(TypeExpr::Enum { members }) if !field.is_list() => Some(members.clone()),
        _ => None,
    }
}

// --------------------------------------------------------------- asset paths

/// Resolves a `file`/`image` value under `<project root>/assets` after the
/// confinement checks of SPEC §5.9: strip one leading `./` or `.\`, then
/// resolve what remains relative to the assets directory.
pub fn resolve_asset_path(
    value: &str,
    context: &ValidationContext<'_>,
) -> Result<PathBuf, Diagnostic> {
    match relative_asset(value) {
        Some(relative) => Ok(context.assets_dir.join(relative)),
        None => Err(Diagnostic::new(
            ErrorId::E424,
            format!(
                "Asset path '{}' must be relative to assets/ and must not be absolute or contain '..'.",
                display_asset(value)
            ),
        )
        .with_note(Note::new(ASSETS_NOTE))),
    }
}

/// The part of an asset value that resolves under `assets/`, with `/` as the
/// separator, or `None` when the value may not be resolved at all (SPEC §5.9).
fn relative_asset(value: &str) -> Option<String> {
    if value.is_empty() || value.contains('\0') {
        return None;
    }
    let stripped = value
        .strip_prefix("./")
        .or_else(|| value.strip_prefix(".\\"))
        .unwrap_or(value);
    if stripped.is_empty() || stripped.starts_with('/') || stripped.starts_with('\\') {
        return None;
    }
    let head: String = stripped.chars().take(2).collect();
    if head.chars().count() == 2 && head.ends_with(':') {
        return None;
    }
    let normalised = stripped.replace('\\', "/");
    if normalised
        .split('/')
        .any(|segment| segment == ".." || segment.is_empty())
    {
        return None;
    }
    // `Path` is consulted only to reject a platform prefix the text form
    // cannot show; the value itself is never canonicalised.
    if Path::new(&normalised)
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(normalised)
}

/// Diagnostics print an asset path in project-relative form with `/`
/// separators, never a canonical or extended-length platform path (SPEC §5.9).
fn display_asset(value: &str) -> String {
    value.replace('\\', "/")
}

// ----------------------------------------------------------- brace expansion

/// Expands the brace groups of a value into the paths they name, the leftmost
/// group varying slowest (SPEC §5.8). `mask` names the byte ranges
/// interpolation inserted; a brace inside one of them is an ordinary
/// character that never delimits a group.
fn expand_braces(text: &str, mask: &[(usize, usize)]) -> Result<Vec<String>, String> {
    let segments = split_groups(text, mask)?;
    let mut out: Vec<String> = vec![String::new()];
    for segment in &segments {
        match segment {
            Segment::Literal(start, end) => {
                let slice = text.get(*start..*end).unwrap_or("");
                for built in out.iter_mut() {
                    built.push_str(slice);
                }
            }
            Segment::Group(alternatives) => {
                let mut next: Vec<String> = Vec::with_capacity(out.len() * alternatives.len());
                for built in &out {
                    for (start, end) in alternatives {
                        let mut copy = built.clone();
                        copy.push_str(text.get(*start..*end).unwrap_or(""));
                        next.push(copy);
                    }
                }
                out = next;
            }
        }
    }
    Ok(out)
}

/// The most brace groups one pattern may hold (SPEC §5.8). The product of
/// eight groups is already the largest asset list an author writes by hand,
/// and the bound keeps a one-line source from demanding an exponential number
/// of paths.
const MAX_BRACE_GROUPS: usize = 8;

enum Segment {
    Literal(usize, usize),
    Group(Vec<(usize, usize)>),
}

/// Splits a value into literal runs and brace groups.
fn split_groups(text: &str, mask: &[(usize, usize)]) -> Result<Vec<Segment>, String> {
    let bytes = text.as_bytes();
    let mut segments = Vec::new();
    let mut literal = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'{' || masked(mask, index) {
            index += 1;
            continue;
        }
        segments.push(Segment::Literal(literal, index));
        index += 1;
        let mut alternatives: Vec<(usize, usize)> = Vec::new();
        let mut start = index;
        let mut closed = false;
        while index < bytes.len() {
            if masked(mask, index) {
                index += 1;
                continue;
            }
            match bytes[index] {
                b'{' => return Err("groups do not nest".to_string()),
                b',' => {
                    alternatives.push((start, index));
                    index += 1;
                    start = index;
                }
                b'}' => {
                    alternatives.push((start, index));
                    index += 1;
                    closed = true;
                    break;
                }
                _ => index += 1,
            }
        }
        if !closed {
            return Err("the group is not closed".to_string());
        }
        let mut trimmed = Vec::with_capacity(alternatives.len());
        for (start, end) in alternatives {
            let (start, end) = trim_range(text, start, end);
            if start >= end {
                return Err("an alternative is empty".to_string());
            }
            trimmed.push((start, end));
        }
        segments.push(Segment::Group(trimmed));
        if segments
            .iter()
            .filter(|segment| matches!(segment, Segment::Group(_)))
            .count()
            > MAX_BRACE_GROUPS
        {
            return Err(format!("a pattern holds at most {MAX_BRACE_GROUPS} groups"));
        }
        literal = index;
    }
    segments.push(Segment::Literal(literal, bytes.len()));
    Ok(segments)
}

fn trim_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let slice = text.get(start..end).unwrap_or("");
    let leading = slice.len() - slice.trim_start().len();
    let trailing = slice.len() - slice.trim_end().len();
    (start + leading, end.saturating_sub(trailing))
}

fn masked(mask: &[(usize, usize)], index: usize) -> bool {
    mask.iter()
        .any(|(start, end)| index >= *start && index < *end)
}

/// True when the value carries a brace group that expansion would see.
fn has_group(text: &str, mask: &[(usize, usize)]) -> bool {
    text.as_bytes()
        .iter()
        .enumerate()
        .any(|(index, byte)| *byte == b'{' && !masked(mask, index))
}

// ------------------------------------------------------------------ helpers

/// The `{value}` substitution of SPEC §9.8 for a float, spelled as SPEC §8.7
/// prescribes. The emitted document is rendered by [`crate::output`]; this is
/// the diagnostic spelling of the same number.
fn render_float(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let magnitude = value.abs();
    if (1e-6..1e21).contains(&magnitude) {
        let plain = format!("{value}");
        if plain.contains('.') {
            return plain;
        }
        return format!("{plain}.0");
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    match exponent.strip_prefix('-') {
        Some(rest) => format!("{mantissa}e-{rest}"),
        None => format!("{mantissa}e+{exponent}"),
    }
}

fn describe(ranges: &[IntRange]) -> String {
    ranges
        .iter()
        .map(|range| range.describe())
        .collect::<Vec<_>>()
        .join(", ")
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

fn wrap(segments: &[String], value: SyntaxValue) -> SyntaxValue {
    let mut current = value;
    for segment in segments.iter().rev() {
        current = SyntaxValue::Object(vec![(segment.clone(), current)]);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{parse, SourceUnit};
    use crate::resolve::{check_instances, collect_instances};
    use crate::source::SourceFile;
    use crate::versions::materialise;

    /// Compiles one template and one instance file, with the on-disk asset
    /// checks off so that the tests need no fixtures.
    fn compile(template: &str, instance: &str) -> Result<Vec<Vec<Value>>, Diagnostics> {
        let source = SourceFile::new("data/schema.abt", template);
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file])?;
        crate::schema::validate_schemas(&tables)?;

        let source = SourceFile::new("data/items.ab", instance);
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
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

    fn first(template: &str, instance: &str) -> Value {
        let documents = compile(template, instance).expect("the project compiles");
        documents
            .last()
            .and_then(|document| document.first())
            .cloned()
            .expect("one object")
    }

    fn ids(template: &str, instance: &str) -> Vec<ErrorId> {
        compile(template, instance)
            .expect_err("the project is rejected")
            .iter()
            .map(|item| item.id)
            .collect()
    }

    fn keys(value: &Value) -> Vec<String> {
        match value {
            Value::Object(entries) => entries.iter().map(|(key, _)| key.clone()).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn an_envelope_key_can_never_be_assigned() {
        // E410 belongs to step 1 of SPEC §7.3, so a schema that declares
        // `template` as a field fails P3 first and P4 never runs.
        let schema =
            "schema Item {\n    name: text(1..40)\n    owner {\n        id: text(1..8)\n    }\n}\n";
        assert_eq!(
            ids(schema, "Item :: @id.one\n    name: A\n    id: other\n"),
            [ErrorId::E410]
        );
        assert_eq!(
            ids(
                schema,
                "Item :: @id.one\n    name: A\n    template.x: other\n"
            ),
            [ErrorId::E410]
        );
        assert_eq!(
            ids(schema, "Item :: @id.one, @template.Other\n    name: A\n"),
            [ErrorId::E410]
        );
        // A nested `id` is an ordinary field of its group.
        let object = first(schema, "Item :: @id.one\n    name: A\n    owner.id: keep\n");
        assert_eq!(object.get("id"), Some(&Value::Text("one".to_string())));
    }

    #[test]
    fn every_scalar_type_is_interpreted_from_its_lexeme() {
        let object = first(
            "schema Item {\n    name: text(1..40)\n    count: int(0..99)\n    price: float(0..99)\n    live: bool\n    status: enum(draft, active)\n}\n",
            "Item :: @id.one\n    name: 1.21.5\n    count: 12\n    price: 19\n    live: true\n    status: Active\n",
        );
        assert_eq!(object.get("name"), Some(&Value::Text("1.21.5".to_string())));
        assert_eq!(object.get("count"), Some(&Value::Int(12)));
        assert_eq!(object.get("price"), Some(&Value::Float(19.0)));
        assert_eq!(object.get("live"), Some(&Value::Bool(true)));
        assert_eq!(
            object.get("status"),
            Some(&Value::Text("active".to_string()))
        );
    }

    #[test]
    fn quoting_never_coerces_a_number_or_a_boolean() {
        assert_eq!(
            ids(
                "schema Item {\n    count: int\n}\n",
                "Item :: @id.one\n    count: \"5\"\n"
            ),
            [ErrorId::E412]
        );
        assert_eq!(
            ids(
                "schema Item {\n    live: bool\n}\n",
                "Item :: @id.one\n    live: \"true\"\n"
            ),
            [ErrorId::E412]
        );
        assert_eq!(
            ids(
                "schema Item {\n    count: int\n}\n",
                "Item :: @id.one\n    count: 1.5\n"
            ),
            [ErrorId::E412]
        );
    }

    #[test]
    fn a_bare_comma_list_on_a_scalar_field_is_a_type_mismatch() {
        let error = compile(
            "schema Item {\n    caption: text(1..40)\n}\n",
            "Item :: @id.one\n    caption: Hello, world\n",
        )
        .expect_err("two items on a scalar field");
        let message = error.first().expect("one diagnostic").to_string();
        assert_eq!(error.first().map(|item| item.id), Some(ErrorId::E412));
        assert!(message.contains("separates list items"), "{message}");

        // The note offers to quote a literal comma, which is the remedy for
        // the bare spelling and a non-sequitur for any other, so the bracketed
        // form gets the same E412 without it (SPEC §5.5).
        let bracketed = compile(
            "schema Item {\n    caption: text(1..40)\n}\n",
            "Item :: @id.one\n    caption: [Hello, world]\n",
        )
        .expect_err("a bracketed list on a scalar field");
        let message = bracketed.first().expect("one diagnostic").to_string();
        assert_eq!(bracketed.first().map(|item| item.id), Some(ErrorId::E412));
        assert!(!message.contains("separates list items"), "{message}");
    }

    #[test]
    fn a_single_value_is_coerced_into_a_one_element_list() {
        let object = first(
            "schema Item {\n    tags[]: enum(core, public)\n}\n",
            "Item :: @id.one\n    tags: core\n",
        );
        assert_eq!(
            object.get("tags"),
            Some(&Value::List(vec![Value::Text("core".to_string())]))
        );
    }

    #[test]
    fn an_empty_list_is_a_value_and_a_cardinality_is_enforced() {
        let object = first(
            "schema Item {\n    tags[]: enum(core, public)\n}\n",
            "Item :: @id.one\n    tags: []\n",
        );
        assert_eq!(object.get("tags"), Some(&Value::List(Vec::new())));
        assert_eq!(
            ids(
                "schema Item {\n    tags[1..2]: enum(core, public)\n}\n",
                "Item :: @id.one\n    tags: []\n"
            ),
            [ErrorId::E445]
        );
    }

    #[test]
    fn an_absent_optional_field_is_omitted_and_a_required_one_is_reported() {
        let object = first(
            "schema Item {\n    name: text(1..40)\n    tags[]: enum(core) @optional\n}\n",
            "Item :: @id.one\n    name: One\n",
        );
        assert_eq!(keys(&object), ["template", "id", "name"]);
        assert_eq!(
            ids(
                "schema Item {\n    name: text(1..40)\n}\n",
                "Item :: @id.one\n"
            ),
            [ErrorId::E411]
        );
    }

    #[test]
    fn keys_follow_declaration_order_and_never_assignment_order() {
        let object = first(
            "schema Owner {\n    team: text(1..40)\n    contact: text(1..40)\n}\n\nschema Item {\n    owner: $(Owner)\n    name: text(1..40)\n}\n",
            "Item :: @id.one\n    name: One\n    owner.contact: c\n    owner.team: t\n",
        );
        assert_eq!(keys(&object), ["template", "id", "owner", "name"]);
        let owner = object.get("owner").expect("the nested object");
        assert_eq!(keys(owner), ["team", "contact"]);
    }

    #[test]
    fn tag_shorthand_expands_into_the_group_it_names() {
        let object = first(
            "schema Item {\n    caps[] {\n        id: enum(search, sync, export) @tag\n        level: int(0..9) = 0\n    }\n}\n",
            "Item :: @id.one\n    caps: [#search, #export(level: 3)]\n",
        );
        let Some(Value::List(items)) = object.get("caps") else {
            panic!("caps is a list");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(keys(&items[0]), ["id", "level"]);
        assert_eq!(items[0].get("level"), Some(&Value::Int(0)));
        assert_eq!(items[1].get("id"), Some(&Value::Text("export".to_string())));
        assert_eq!(items[1].get("level"), Some(&Value::Int(3)));
    }

    #[test]
    fn a_tag_on_a_group_without_one_and_a_duplicate_key_are_reported() {
        assert!(ids(
            "schema Item {\n    caps[] {\n        name: text(1..9)\n    }\n}\n",
            "Item :: @id.one\n    caps: [#search]\n"
        )
        .contains(&ErrorId::E418));
        assert_eq!(
            ids(
                "schema Item {\n    caps[] {\n        id: enum(search) @tag\n    }\n}\n",
                "Item :: @id.one\n    caps: [#search, #search]\n"
            ),
            [ErrorId::E444]
        );
    }

    #[test]
    fn an_enum_wildcard_replicates_a_whole_element() {
        let object = first(
            "schema Item {\n    flags[]: enum(hat_a, hat_b, coat)\n}\n",
            "Item :: @id.one\n    flags: hat_*\n",
        );
        assert_eq!(
            object.get("flags"),
            Some(&Value::List(vec![
                Value::Text("hat_a".to_string()),
                Value::Text("hat_b".to_string()),
            ]))
        );
        assert_eq!(
            ids(
                "schema Item {\n    flags[]: enum(hat_a)\n}\n",
                "Item :: @id.one\n    flags: cor_*\n"
            ),
            [ErrorId::E415]
        );
    }

    #[test]
    fn a_tag_shorthand_wildcard_clones_the_whole_object() {
        // SPEC §5.6 position (c), bracketed and bare: interpretation expands
        // the wildcard before single-value-to-list coercion (SPEC §5.10), so
        // both spellings carry the same data.
        let schema = "schema Thing {\n    caps[] {\n        id: enum(search, sync, export) @tag\n        availability: enum(alpha, stable) = stable\n    }\n}\n";
        for value in ["[#s*]", "#s*"] {
            let object = first(schema, &format!("Thing :: @id.one\n    caps: {value}\n"));
            let Some(Value::List(rows)) = object.get("caps") else {
                panic!("caps is a list");
            };
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].get("id"), Some(&Value::Text("search".to_string())));
            assert_eq!(rows[1].get("id"), Some(&Value::Text("sync".to_string())));
            assert_eq!(
                rows[1].get("availability"),
                Some(&Value::Text("stable".to_string()))
            );
        }
        assert_eq!(
            ids(schema, "Thing :: @id.one\n    caps: [#zz*]\n"),
            [ErrorId::E415]
        );
    }

    #[test]
    fn a_tuple_row_is_cloned_once_per_wildcard_match() {
        let object = first(
            "schema Page {\n    copy[] {\n        key: enum(en_us, es_es, es_mx) @tag\n        value: text(1..80)\n    }\n}\n",
            "Page :: @id.home\n    copy(key, value): (es_*, Bienvenido)\n",
        );
        let Some(Value::List(rows)) = object.get("copy") else {
            panic!("copy is a list");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("key"), Some(&Value::Text("es_es".to_string())));
        assert_eq!(rows[1].get("key"), Some(&Value::Text("es_mx".to_string())));
        assert_eq!(
            rows[1].get("value"),
            Some(&Value::Text("Bienvenido".to_string()))
        );
    }

    #[test]
    fn brace_patterns_expand_on_list_asset_fields_only() {
        let object = first(
            "schema Item {\n    images[]: file(png)\n}\n",
            "Item :: @id.one\n    images: ./art/{red}/{small}.png\n",
        );
        assert_eq!(
            object.get("images"),
            Some(&Value::List(vec![Value::Text(
                "./art/red/small.png".to_string()
            )]))
        );
        assert_eq!(
            ids(
                "schema Item {\n    icon: file(png)\n}\n",
                "Item :: @id.one\n    icon: a{1}.png\n"
            ),
            [ErrorId::E434]
        );
        assert!(ids(
            "schema Item {\n    images[]: file(png)\n}\n",
            "Item :: @id.one\n    images: a{}.png\n"
        )
        .contains(&ErrorId::E435));
    }

    /// SPEC §5.8's own example expands `{hero,thumbnail}` into two paths, so
    /// the depth-0 comma of a brace group belongs to the value and is not a
    /// list separator: inside a bare text `{` and `}` raise and lower the
    /// run's bracket depth (SPEC §3.5). The block-brace rule of SPEC §3.6
    /// governs `{` as a token, outside a value.
    #[test]
    fn a_multi_alternative_pattern_is_one_value_that_expands() {
        let object = first(
            "schema Item {\n    images[]: file(png)\n}\n",
            "Item :: @id.one\n    images: ./art/{red,blue}.png\n",
        );
        assert_eq!(
            object.get("images"),
            Some(&Value::List(vec![
                Value::Text("./art/red.png".to_string()),
                Value::Text("./art/blue.png".to_string()),
            ]))
        );
    }

    #[test]
    fn a_brace_that_arrived_from_a_variable_never_delimits_a_group() {
        let object = first(
            "schema Item {\n    dir: text(1..40)\n    images[]: file(png)\n}\n",
            "Item :: @id.one\n    dir: a{b}\n    images: ${dir}/{hero}.png\n",
        );
        assert_eq!(
            object.get("images"),
            Some(&Value::List(vec![Value::Text("a{b}/hero.png".to_string())]))
        );
    }

    #[test]
    fn an_asset_path_is_confined_and_its_extension_checked() {
        assert_eq!(
            ids(
                "schema Item {\n    icon: file(png)\n}\n",
                "Item :: @id.one\n    icon: ../../secret.png\n"
            ),
            [ErrorId::E424]
        );
        assert_eq!(
            ids(
                "schema Item {\n    icon: file(png)\n}\n",
                "Item :: @id.one\n    icon: ./a.tga\n"
            ),
            [ErrorId::E420]
        );
    }

    #[test]
    fn a_reference_must_exist_carry_its_schema_and_resolve_to_an_id() {
        let documents = compile(
            "schema Pack {\n    name: text(1..9)\n}\n\nschema Item {\n    pack: ref(Pack)\n}\n",
            "Pack :: @id.winter\n    name: Winter\n\nItem :: @id.one\n    pack: Winter\n",
        )
        .expect("compiles");
        let item = documents[0]
            .iter()
            .find(|object| object.get("id") == Some(&Value::Text("one".to_string())))
            .expect("the item");
        assert_eq!(item.get("pack"), Some(&Value::Text("winter".to_string())));

        assert_eq!(
            ids(
                "schema Pack {\n    name: text(1..9)\n}\n\nschema Item {\n    pack: ref(Pack)\n}\n",
                "Item :: @id.one\n    pack: missing\n"
            ),
            [ErrorId::E431]
        );
        assert_eq!(
            ids(
                "schema Pack {\n    name: text(1..9)\n}\n\nschema Item {\n    pack: ref(Pack)\n    name: text(1..9)\n}\n",
                "Item :: @id.other\n    name: A\n    pack: other\n"
            ),
            [ErrorId::E432]
        );
    }

    #[test]
    fn a_reference_to_an_instance_absent_in_this_version_is_reported() {
        let errors = ids(
            "versions 1..2\n\nschema Pack {\n    name: text(1..9)\n}\n\nschema Item {\n    pack: ref(Pack)\n}\n",
            "Pack :: @id.winter @since(2)\n    name: Winter\n\nItem :: @id.one\n    pack: winter\n",
        );
        assert_eq!(errors, [ErrorId::E431]);
    }

    #[test]
    fn an_unknown_field_names_the_declared_ones() {
        let error = compile(
            "schema Item {\n    rarity: int(0..9)\n}\n",
            "Item :: @id.one\n    rarty: 3\n",
        )
        .expect_err("an unknown field");
        let text = error.to_string();
        assert!(text.contains("Unknown field 'rarty' in one."), "{text}");
        assert!(text.contains("Did you mean 'rarity'?"), "{text}");
        assert!(text.contains("declared fields: rarity."), "{text}");
    }

    #[test]
    fn a_header_tag_addresses_one_root_scalar_field() {
        assert_eq!(
            ids(
                "schema Item {\n    owner {\n        team: text(1..9)\n    }\n}\n",
                "Item :: @id.one, @owner.team\n"
            ),
            [ErrorId::E438]
        );
        assert_eq!(
            ids(
                "schema Item {\n    name: text(1..9)\n}\n",
                "Item :: @id.one, @name\n"
            ),
            [ErrorId::E412]
        );
    }

    #[test]
    fn a_statement_outside_its_field_window_names_no_variable() {
        // SPEC 5.11: the variable table is built per version, so a field whose
        // statement does not apply to the version being compiled is not a
        // variable there, whether the statement was clipped by its own
        // annotation or by the existence set of the field it writes.
        let template = concat!(
            "versions 1..2\n\n",
            "schema Item {\n",
            "    name: text(1..40)\n",
            "    tint: int(0..255) @removed(2)\n",
            "    label: text(1..40) @optional\n",
            "}\n",
        );
        assert_eq!(
            ids(
                template,
                "Item :: @id.torch\n    name: Torch\n    tint: 5\n    label: torch-$tint\n",
            ),
            [ErrorId::E425]
        );
        // Annotated to the same versions as the field it reads, it compiles,
        // and the value survives only where the variable does.
        let documents = compile(
            template,
            "Item :: @id.torch\n    name: Torch\n    tint: 5\n    label: torch-$tint  @removed(2)\n",
        )
        .expect("the annotated form compiles");
        assert_eq!(
            keys(&documents[0][0]),
            ["template", "id", "name", "tint", "label"]
        );
        assert_eq!(
            documents[0][0].get("label"),
            Some(&Value::Text("torch-5".to_string()))
        );
        assert_eq!(keys(&documents[1][0]), ["template", "id", "name"]);
    }

    #[test]
    fn a_header_tag_may_target_a_scalar_list_field() {
        // SPEC 5.2 and 10.4: E438 is a header tag on a *nested* field. A
        // scalar list is not nested, so the single value is coerced into a
        // one-element list exactly as a body value would be (SPEC 5.10).
        let object = first(
            "schema Item {\n    tags[]: enum(core, public)\n}\n",
            "Item :: @id.one, @tags.core\n",
        );
        assert_eq!(
            object.get("tags"),
            Some(&Value::List(vec![Value::Text("core".to_string())]))
        );
        // A list group stays E438: a header tag can never reach a nested field.
        assert_eq!(
            ids(
                "schema Item {\n    caps[] {\n        name: text(1..9)\n    }\n}\n",
                "Item :: @id.one, @caps.x\n",
            ),
            [ErrorId::E438]
        );
    }

    #[test]
    fn a_header_tag_writes_nothing_where_its_field_does_not_exist() {
        // A header tag carries no annotation (E437), so its applicability set
        // is the existence set of the field it writes (SPEC 5.2, 5.13).
        let documents = compile(
            "versions 1..2\n\nschema Item {\n    name: text(1..40)\n    glow: bool @since(2) @optional\n}\n",
            "Item :: @id.torch, @glow.true\n    name: Torch\n",
        )
        .expect("compiles");
        assert_eq!(keys(&documents[0][0]), ["template", "id", "name"]);
        assert_eq!(keys(&documents[1][0]), ["template", "id", "name", "glow"]);
    }

    #[test]
    fn nested_schema_logic_is_reached_before_the_object_s_own_block() {
        // SPEC 6.11: logic runs for every present nested $(Schema) value
        // before the block of the schema that holds it. What this pins is
        // which objects the walk visits.
        let template = concat!(
            "schema Part {\n",
            "    label: text(1..40)\n",
            "}\n",
            "\n",
            "schema Item {\n",
            "    name: text(1..40)\n",
            "    part: $(Part) @optional\n",
            "    spares[]: $(Part) @optional\n",
            "}\n",
            "\n",
            "logic Part {\n",
            "    require .label == \"Nut\"\n",
            "        else throw \"A part must be a nut.\"\n",
            "}\n",
        );
        // No nested value is present, so no block runs and the project
        // compiles although `logic Part` exists.
        let object = first(template, "Item :: @id.one\n    name: Widget\n");
        assert_eq!(keys(&object), ["template", "id", "name"]);

        // A present $(Part) value reaches `logic Part`, although `Item` has
        // no block of its own.
        let error = compile(
            template,
            "Item :: @id.one\n    name: Widget\n    part.label: Bolt\n",
        )
        .expect_err("the nested require fails");
        assert_eq!(error.first().map(|item| item.id), Some(ErrorId::E515));
        let text = error.to_string();
        assert!(text.contains("Part logic: A part must be a nut."), "{text}");
        assert!(text.contains("instance 'one'."), "{text}");

        // So does an element of a list of $(Part).
        assert_eq!(
            ids(
                template,
                "Item :: @id.one\n    name: Widget\n    spares(label): (Bolt), (Nut)\n",
            ),
            [ErrorId::E515]
        );
        // Every element satisfies it, so the project compiles.
        let object = first(
            template,
            "Item :: @id.one\n    name: Widget\n    spares(label): (Nut), (Nut)\n",
        );
        assert_eq!(keys(&object), ["template", "id", "name", "spares"]);
    }

    #[test]
    fn a_range_violation_names_the_measured_quantity() {
        // SPEC §9.8: against a `text` range the quantity constrained is the
        // value's length, so `{value}` is its scalar count and never its
        // text. `{ranges}` renders an integer part whose bounds are equal as
        // the bare number, so `text(2..2)` reads `2`.
        let error = compile(
            "schema Item {\n    caption: text(1..4)\n}\n",
            "Item :: @id.one\n    caption: abcdefg\n",
        )
        .expect_err("too long");
        let text = error.to_string();
        assert!(
            text.contains("Range mismatch at one.caption: 7 is not in 1..4."),
            "{text}"
        );

        let error = compile(
            "schema Item {\n    code: text(2..2)\n    tier: text(1..3, 8, 12..14)\n}\n",
            "Item :: @id.one\n    code: abc\n    tier: abcde\n",
        )
        .expect_err("both are out of range");
        let text = error.to_string();
        assert!(
            text.contains("Range mismatch at one.code: 3 is not in 2."),
            "{text}"
        );
        assert!(
            text.contains("Range mismatch at one.tier: 5 is not in 1..3, 8, 12..14."),
            "{text}"
        );

        // An astral character is one scalar value, not its bytes and not
        // its UTF-16 code units.
        let error = compile(
            "schema Item {\n    code: text(2..2)\n}\n",
            "Item :: @id.one\n    code: \u{1f600}\n",
        )
        .expect_err("one scalar value, not two or four");
        let text = error.to_string();
        assert!(
            text.contains("Range mismatch at one.code: 1 is not in 2."),
            "{text}"
        );

        // Against an `int` or a `float` range `{value}` is the number
        // itself, and a float part always renders both bounds.
        let error = compile(
            "schema Item {\n    replicas: int(1, 3, 12)\n    ratio: float(1.0..1.0)\n}\n",
            "Item :: @id.one\n    replicas: 99\n    ratio: 2.5\n",
        )
        .expect_err("both are out of range");
        let text = error.to_string();
        assert!(
            text.contains("Range mismatch at one.replicas: 99 is not in 1, 3, 12."),
            "{text}"
        );
        assert!(
            text.contains("Range mismatch at one.ratio: 2.5 is not in 1.0..1.0."),
            "{text}"
        );
    }

    #[test]
    fn interpretation_is_idempotent_over_a_whole_object() {
        // The walk of §7.3 step 7 must reproduce step 3 exactly.
        let documents = compile(
            "schema Item {\n    flags[]: enum(hat_a, hat_b)\n    images[]: file(png)\n    status: enum(draft, active)\n}\n",
            "Item :: @id.one\n    flags: hat_*\n    images: ./a/{x}.png\n    status: DRAFT\n",
        )
        .expect("compiles");
        let object = documents[0].first().expect("one object");
        assert_eq!(
            object.get("flags"),
            Some(&Value::List(vec![
                Value::Text("hat_a".to_string()),
                Value::Text("hat_b".to_string()),
            ]))
        );
        assert_eq!(
            object.get("images"),
            Some(&Value::List(vec![Value::Text("./a/x.png".to_string())]))
        );
        assert_eq!(
            object.get("status"),
            Some(&Value::Text("draft".to_string()))
        );
    }

    #[test]
    fn an_absent_optional_group_is_omitted_and_its_defaults_are_not_filled() {
        let object = first(
            "schema Item {\n    name: text(1..9)\n    owner @optional {\n        team: text(1..9) = none\n    }\n}\n",
            "Item :: @id.one\n    name: One\n",
        );
        assert_eq!(keys(&object), ["template", "id", "name"]);
    }

    #[test]
    fn brace_expansion_produces_the_cartesian_product_in_order() {
        let paths = expand_braces("a{1,2}b{x,y}", &[]).expect("expanded");
        assert_eq!(paths, ["a1bx", "a1by", "a2bx", "a2by"]);
        assert!(expand_braces("a{b", &[]).is_err());
        assert!(expand_braces("a{b,{c}}", &[]).is_err());
        assert!(expand_braces("a{ , b}", &[]).is_err());
    }

    #[test]
    fn an_asset_value_is_confined_to_the_assets_directory() {
        assert_eq!(
            relative_asset("./textures/a.png").as_deref(),
            Some("textures/a.png")
        );
        assert_eq!(
            relative_asset("textures\\a.png").as_deref(),
            Some("textures/a.png")
        );
        assert_eq!(
            relative_asset("./assets/a.png").as_deref(),
            Some("assets/a.png")
        );
        assert_eq!(relative_asset("/a.png"), None);
        assert_eq!(relative_asset("C:/a.png"), None);
        assert_eq!(relative_asset("\\\\host\\share\\a.png"), None);
        assert_eq!(relative_asset("../a.png"), None);
        assert_eq!(relative_asset(""), None);
    }

    #[test]
    fn a_value_outside_the_declared_vocabulary_is_reported_with_a_suggestion() {
        let error = compile(
            "schema Item {\n    status: enum(draft, active, retired)\n}\n",
            "Item :: @id.one\n    status: activ\n",
        )
        .expect_err("not a member");
        let text = error.to_string();
        assert_eq!(error.first().map(|item| item.id), Some(ErrorId::E414));
        assert!(
            text.contains("'activ' is not one of draft, active, retired."),
            "{text}"
        );
        assert!(text.contains("Did you mean 'active'?"), "{text}");
    }

    #[test]
    fn a_path_that_crosses_a_value_already_written_is_reported() {
        let error = compile(
            "schema Owner {\n    team: text(1..9)\n}\n\nschema Item {\n    owner: $(Owner)\n}\n",
            "Item :: @id.one\n    owner.team: t\n    owner.team.deep: x\n",
        )
        .expect_err("the prefix holds text");
        assert_eq!(error.first().map(|item| item.id), Some(ErrorId::E443));
        assert!(
            error
                .to_string()
                .contains("'owner.team' already holds text"),
            "{error}"
        );
    }

    #[test]
    fn a_clone_across_templates_or_outside_a_window_is_reported() {
        assert_eq!(
            ids(
                "schema Pack {\n    name: text(1..9)\n}\n\nschema Item {\n    name: text(1..9)\n}\n",
                "Pack :: @id.p\n    name: P\n\nItem :: @id.i\n&p.*\n    name: I\n"
            ),
            [ErrorId::E407]
        );
        assert_eq!(
            ids(
                "versions 1..3\n\nschema Item {\n    name: text(1..9)\n}\n",
                "Item :: @id.a @since(2)\n    name: A\n\nItem :: @id.b\n&a.*\n    name: B\n"
            ),
            [ErrorId::E440]
        );
    }

    #[test]
    fn only_the_selected_instances_reach_the_document() {
        let source = SourceFile::new(
            "data/schema.abt",
            "schema Item {\n    name: text(1..9)\n}\n",
        );
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file]).expect("tables build");
        let source = SourceFile::new(
            "data/items.ab",
            "Item :: @id.a\n    name: A\n\nItem :: @id.b\n    name: B\n",
        );
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
            panic!("an instance file");
        };
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("consistent");
        let instances = InstanceTable::new(declarations);
        let documents = materialise(
            &tables,
            &instances,
            Some(&["b".to_string()]),
            Path::new("assets"),
            AssetChecks::Skipped,
        )
        .expect("both compile, one is emitted");
        assert_eq!(documents[0].len(), 1);
        assert_eq!(
            documents[0][0].get("id"),
            Some(&Value::Text("b".to_string()))
        );
    }

    #[test]
    fn floats_are_spelled_as_the_output_would_spell_them() {
        assert_eq!(render_float(0.0), "0.0");
        assert_eq!(render_float(-0.0), "-0.0");
        assert_eq!(render_float(3.0), "3.0");
        assert_eq!(render_float(19.5), "19.5");
        assert_eq!(render_float(1e21), "1.0e+21");
        assert_eq!(render_float(1.5e-7), "1.5e-7");
    }

    #[test]
    fn an_integer_literal_on_a_float_field_is_the_integer_it_denotes() {
        // SPEC §3.5 makes `-0` an integer literal and SPEC §4.4.3 widens it, so
        // the value is zero; SPEC §7.5 makes `0.0` and `-0.0` different values.
        let schema = "schema Item {\n    r: float\n}\n";
        assert_eq!(
            first(schema, "Item :: @id.one\n    r: -0\n").get("r"),
            Some(&Value::Float(0.0))
        );
        assert_eq!(
            first(schema, "Item :: @id.one\n    r: -0.0\n").get("r"),
            Some(&Value::Float(-0.0))
        );
        assert_eq!(
            first(schema, "Item :: @id.one\n    r: 19\n").get("r"),
            Some(&Value::Float(19.0))
        );
    }

    #[test]
    fn a_bare_tag_flag_targets_a_bool_field() {
        // SPEC §5.5: a bare `flag` argument sets `flag` to `true` and is valid
        // only when `flag` is a `bool` field.
        let schema = "schema Item {\n    caps[] {\n        key: text(1..8) @tag\n\
             \x20       label: text(1..8) @optional\n        on: bool @optional\n    }\n}\n";
        assert_eq!(
            ids(schema, "Item :: @id.one\n    caps: [#search(label)]\n"),
            [ErrorId::E412]
        );
        assert_eq!(
            ids(schema, "Item :: @id.one\n    caps: [#search(key)]\n"),
            [ErrorId::E412]
        );
        let ok = first(schema, "Item :: @id.one\n    caps: [#search(on)]\n");
        let Some(Value::List(elements)) = ok.get("caps") else {
            panic!("caps is a list");
        };
        assert_eq!(elements[0].get("on"), Some(&Value::Bool(true)));
    }

    #[test]
    fn a_bare_header_flag_names_the_type_it_would_write() {
        // SPEC §5.2: the bare `@name` form writes a boolean, so `{found}` is
        // `bool` and the message never reads "expected text, found text".
        let schema = "schema Item {\n    name: text(1..8)\n}\n";
        let reported = compile(schema, "Item :: @id.one, @name\n").expect_err("E412");
        let first = reported.first().expect("one diagnostic");
        assert_eq!(first.id, ErrorId::E412);
        assert!(
            first.message.ends_with("expected text, found bool."),
            "{}",
            first.message
        );
    }
}

#[cfg(test)]
mod asset_tests {
    use super::*;
    use crate::ast::{parse, SourceUnit};
    use crate::resolve::{check_instances, collect_instances};
    use crate::source::SourceFile;
    use crate::versions::materialise;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh assets directory under the system temporary directory, removed
    /// when the test ends.
    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Sandbox {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("abstract_assets_{name}_{suffix}"));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("sandbox");
            Sandbox { root }
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let mut path = self.root.clone();
            for part in relative.split('/') {
                path.push(part);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(&path, bytes).expect("write");
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// The 24 bytes of a PNG header SPEC §4.4.7.1 reads: signature, chunk
    /// length, `IHDR`, then width and height as big-endian u32.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn gif(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes
    }

    fn run(
        template: &str,
        instance: &str,
        assets: &Path,
        checks: AssetChecks,
    ) -> Result<Vec<Vec<Value>>, Diagnostics> {
        let source = SourceFile::new("data/schema.abt", template);
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file])?;
        crate::schema::validate_schemas(&tables)?;
        let source = SourceFile::new("data/items.ab", instance);
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
            panic!("an instance file");
        };
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables)?;
        let instances = InstanceTable::new(declarations);
        materialise(&tables, &instances, None, assets, checks)
    }

    #[test]
    fn skipping_the_asset_checks_never_changes_the_compiled_values() {
        // SPEC 7.6: the output must not depend on whether --skip-assets was
        // passed. The checks of SPEC 9.5 read the disk; nothing they learn
        // reaches the document.
        let sandbox = Sandbox::new("determinism");
        sandbox.write("textures/icon.png", &png(128, 128));
        let template =
            "schema Item {\n    icon: image(png 128x128)\n    art[]: file(png, jpg)\n}\n";
        let instance =
            "Item :: @id.one\n    icon: ./textures/icon.png\n    art: textures/icon.png\n";
        let checked = run(template, instance, &sandbox.root, AssetChecks::Enabled)
            .expect("the assets are on disk");
        let skipped = run(template, instance, &sandbox.root, AssetChecks::Skipped)
            .expect("skipping the checks compiles too");
        assert_eq!(checked, skipped);
    }

    #[test]
    fn an_image_is_probed_for_its_format_and_its_size() {
        let sandbox = Sandbox::new("image");
        sandbox.write("textures/icon.png", &png(128, 128));
        sandbox.write("textures/small.png", &png(64, 64));
        sandbox.write("textures/lying.png", &gif(128, 128));

        let template = "schema Item {\n    icon: image(png 128x128)\n}\n";
        run(
            template,
            "Item :: @id.one\n    icon: ./textures/icon.png\n",
            &sandbox.root,
            AssetChecks::Enabled,
        )
        .expect("the image matches the declaration");

        let errors = run(
            template,
            "Item :: @id.one\n    icon: ./textures/small.png\n",
            &sandbox.root,
            AssetChecks::Enabled,
        )
        .expect_err("the size is wrong");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E423));
        assert!(errors
            .to_string()
            .contains("is 64x64, expected one of png 128x128"));

        let errors = run(
            template,
            "Item :: @id.one\n    icon: ./textures/lying.png\n",
            &sandbox.root,
            AssetChecks::Enabled,
        )
        .expect_err("the content is a gif");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E422));
        assert!(errors.to_string().contains("is gif, not png"));
    }

    #[test]
    fn a_missing_asset_is_reported_and_skip_assets_passes_it() {
        let sandbox = Sandbox::new("missing");
        let template = "schema Item {\n    icon: file(png)\n}\n";
        let instance = "Item :: @id.one\n    icon: ./textures/gone.png\n";

        let errors = run(template, instance, &sandbox.root, AssetChecks::Enabled)
            .expect_err("the file is not there");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E421));
        let text = errors.to_string();
        assert!(
            text.contains("(resolved to 'assets/textures/gone.png')"),
            "{text}"
        );
        assert!(
            text.contains("assets live in '<project root>/assets'"),
            "{text}"
        );

        // The compiled bytes never depend on --skip-assets (SPEC §9.5).
        let documents = run(template, instance, &sandbox.root, AssetChecks::Skipped)
            .expect("the on-disk checks are skipped");
        assert_eq!(
            documents[0][0].get("icon"),
            Some(&Value::Text("./textures/gone.png".to_string()))
        );
    }

    #[test]
    fn a_truncated_image_is_reported_as_truncated_never_as_unrecognised() {
        let sandbox = Sandbox::new("truncated");
        sandbox.write("a.png", &png(4, 4)[..12]);
        let errors = run(
            "schema Item {\n    icon: image(png 4x4)\n}\n",
            "Item :: @id.one\n    icon: a.png\n",
            &sandbox.root,
            AssetChecks::Enabled,
        )
        .expect_err("the header is short");
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E422));
        assert!(errors.to_string().contains("file is truncated."));
    }

    #[test]
    fn a_wildcard_side_accepts_any_extent() {
        let sandbox = Sandbox::new("wildcard");
        sandbox.write("wide.png", &png(1024, 7));
        run(
            "schema Item {\n    wide: image(png 1024x*)\n}\n",
            "Item :: @id.one\n    wide: wide.png\n",
            &sandbox.root,
            AssetChecks::Enabled,
        )
        .expect("only the width is constrained");
    }
}
