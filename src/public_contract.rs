//! Opt-in, unbound public scalar export from one successful P1–P5 compile.
//!
//! This is conservative admission, not runtime override authorization. Static
//! logic paths are inspected in every branch; dynamic paths cover their known
//! prefix. Interpolation containing any `$` covers the entire instance (even
//! an escaped dollar); clone endpoints cover the copied subtree. Windows are
//! not used to eliminate dependency facts. These deliberate false positives
//! avoid claiming independence from a trace of the baked execution. Recursive
//! schema paths with reachable nominations and exposure through lists fail.
//! No consumer/effect bindings or dependent evaluation plan are produced.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::Located;
use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note};
use crate::instance::{InstanceDecl, SyntaxValue};
use crate::logic::{
    CalcExpr, Condition, DeriveExpr, Iterable, LengthArg, LogicPath, LogicStatement, Operand,
};
use crate::output::{Format, Value};
use crate::resolve::InstanceTable;
use crate::schema::{FieldDecl, FieldKind, TemplateTables, TypeExpr};
use crate::versions::VersionRange;
use crate::CompiledDocument;

pub const PROFILE: &str = "independent-scalars-v1";
pub const MAX_TARGETS: usize = 16_384;
pub const MAX_VISITS: usize = 262_144;
pub const MAX_DEPENDENCIES: usize = 65_536;
pub const MAX_FRAGMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024 * 1024;

/// Private build interchange. `fragment()` is intentionally unbound; it must
/// never be treated as an executable override contract. `render` also checks
/// that the public document field has not been replaced after export.
#[derive(Clone, Debug)]
pub struct PublicCompilation {
    pub document: CompiledDocument,
    fragment: Value,
    document_json: String,
    envelope: String,
}

impl PublicCompilation {
    pub fn fragment(&self) -> &Value {
        &self.fragment
    }

    pub fn render(&self) -> Result<String, Diagnostics> {
        check_document_size(&self.document)?;
        if self.document.render(Format::Json)? != self.document_json {
            return Err(budget("compiled document changed after public export"));
        }
        Ok(self.envelope.clone())
    }
}

/// Inputs are the SAME validated tables, complete instance table, selected IDs
/// and per-version selected materializations from one compile. This is not an
/// admission API for arbitrary constructed ASTs or foreign compiled documents.
/// All decisions finish before a successful product is returned. Existing P4
/// allocations precede this function and are not bounded by export budgets.
pub fn export(
    tables: &TemplateTables,
    instances: &InstanceTable<'_>,
    emitted: Option<&[String]>,
    documents: &[Vec<Value>],
    document: CompiledDocument,
) -> Result<PublicCompilation, Diagnostics> {
    if documents.len() != tables.versions.count() as usize || document.versions() != tables.versions
    {
        return Err(budget(
            "inconsistent materialization count or version range",
        ));
    }
    let mut work = Work::default();
    let mut targets = BTreeMap::new();
    for instance in instances.iter() {
        if emitted.is_some_and(|ids| !ids.contains(&instance.id)) {
            continue;
        }
        let schema = tables
            .schema(&instance.template)
            .ok_or_else(|| budget("missing validated root schema"))?;
        if !nominated(tables, &schema.fields, &mut BTreeSet::new(), 0, &mut work)? {
            continue;
        }
        let Some(window) = instance.window.resolve(tables.versions) else {
            continue;
        };
        let mut candidates = Vec::new();
        collect(
            tables,
            &schema.fields,
            &schema.name,
            &[],
            &[],
            window,
            false,
            &mut vec![schema.name.clone()],
            &mut candidates,
            &mut work,
        )?;
        let mut facts = Vec::new();
        collect_schema_facts(
            tables,
            &schema.fields,
            &schema.name,
            &[],
            &mut Vec::new(),
            &mut facts,
            &mut work,
        )?;
        instance_facts(instances, instance, &mut facts, &mut work)?;
        for candidate in candidates {
            if let Some(fact) = facts
                .iter()
                .find(|fact| overlaps(&candidate.path, &fact.path))
            {
                return Err(rejection(
                    candidate.field,
                    &candidate.path,
                    "dependency independence was not established",
                    Some(&fact.at),
                ));
            }
            if targets.len() >= MAX_TARGETS {
                return Err(budget("target count exceeds 16384"));
            }
            let key = (
                instance.template.clone(),
                instance.id.clone(),
                candidate.path.clone(),
            );
            if targets.contains_key(&key) {
                return Err(budget("duplicate resolved public target"));
            }
            let mut variants: Vec<Value> = Vec::new();
            for version in candidate.window.iter() {
                work.visit()?;
                let objects = &documents[(version - tables.versions.min) as usize];
                let mut matched = None;
                for object in objects {
                    work.visit()?;
                    if matches!(object.get("id"), Some(Value::Text(id)) if id == &instance.id)
                        && matches!(object.get("template"), Some(Value::Text(schema)) if schema == &instance.template)
                    {
                        if matched.replace(object).is_some() {
                            return Err(budget("duplicate materialized instance"));
                        }
                    }
                }
                let object = matched.ok_or_else(|| {
                    budget("selected instance missing from applicable materialization")
                })?;
                let value = at_path(object, &candidate.path);
                let default = match value {
                    Some(value) => Some(typed_value(candidate.field, value, &mut work)?),
                    None => None,
                };
                // Values contain only exact strings/bools here; equality does
                // not collapse f64 -0.0 and +0.0 as Value::Float equality does.
                let same = variants.last().is_some_and(|last| {
                    last.get("default") == default.as_ref()
                        && last.get("presence")
                            == Some(&text(if value.is_some() { "present" } else { "absent" }))
                });
                if same {
                    if let Some(Value::Object(fields)) = variants.last_mut() {
                        if let Some((_, Value::Object(range))) =
                            fields.iter_mut().find(|(k, _)| k == "versions")
                        {
                            if let Some((_, slot)) = range.iter_mut().find(|(k, _)| k == "max") {
                                *slot = Value::Int(i64::from(version));
                            }
                        }
                    }
                } else {
                    let mut fields = vec![
                        (
                            "versions".into(),
                            range_value(VersionRange {
                                min: version,
                                max: version,
                            }),
                        ),
                        (
                            "presence".into(),
                            text(if value.is_some() { "present" } else { "absent" }),
                        ),
                        ("writable".into(), Value::Bool(value.is_some())),
                    ];
                    if let Some(default) = default {
                        fields.push(("default".into(), default));
                    }
                    variants.push(Value::Object(fields));
                }
            }
            let entry = object(vec![
                ("rootSchema", work.string(&instance.template)?),
                ("rootInstanceId", work.string(&instance.id)?),
                ("path", strings(&candidate.path, &mut work)?),
                ("declaringSchema", work.string(&candidate.declaring_schema)?),
                (
                    "declaringPath",
                    strings(&candidate.declaring_path, &mut work)?,
                ),
                ("declarationVersions", range_value(candidate.window)),
                ("domain", domain(candidate.field, &mut work)?),
                ("variants", Value::List(variants)),
            ]);
            // Bounds serialization before retaining another target. This is
            // not a bound on total Rust heap or earlier compiler allocations.
            work.serialized = work
                .serialized
                .checked_add(json(&entry, MAX_FRAGMENT_BYTES)?.len())
                .ok_or_else(|| budget("fragment size overflow"))?;
            if work.serialized > MAX_FRAGMENT_BYTES {
                return Err(budget("fragment exceeds 4 MiB"));
            }
            targets.insert(key, entry);
        }
    }
    let fragment = object(vec![
        ("formatVersion", Value::Int(1)),
        ("dependencyProfile", text(PROFILE)),
        ("complete", Value::Bool(true)),
        ("bindingStatus", text("unbound")),
        ("entries", Value::List(targets.into_values().collect())),
    ]);
    let fragment_json = json(&fragment, MAX_FRAGMENT_BYTES)?;
    check_document_size(&document)?;
    let document_json = document.render(Format::Json)?;
    // Escaped documentJson is emitted by the bounded writer, not by an
    // unbounded intermediate escape/render. A digest is not authenticity.
    let digest = crate::crypto::sha256(document_json.as_bytes())
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    let mut writer = JsonWriter::new(MAX_ENVELOPE_BYTES);
    writer.raw("{\"format\":\"abstract.public-compilation\",\"formatVersion\":1,\"profile\":")?;
    writer.string(PROFILE)?;
    writer.raw(",\"compiler\":")?;
    writer.string(crate::COMPILER_VERSION)?;
    writer.raw(
        ",\"languageSemantics\":\"abstract-scalars-v1\",\"documentFormat\":1,\"documentJson\":",
    )?;
    writer.string(&document_json)?;
    writer.raw(",\"documentSha256\":")?;
    writer.string(&digest)?;
    writer.raw(",\"publicFragment\":")?;
    writer.raw(fragment_json.trim_end_matches('\n'))?;
    writer.raw("}\n")?;
    Ok(PublicCompilation {
        document,
        fragment,
        document_json,
        envelope: writer.out,
    })
}

#[derive(Default)]
struct Work {
    visits: usize,
    dependencies: usize,
    copied_text: usize,
    serialized: usize,
}
impl Work {
    fn visit(&mut self) -> Result<(), Diagnostics> {
        if self.visits >= MAX_VISITS {
            return Err(budget("schema/target/version visits exceed 262144"));
        }
        self.visits += 1;
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<Value, Diagnostics> {
        self.copied_text = self
            .copied_text
            .checked_add(value.len())
            .ok_or_else(|| budget("text size overflow"))?;
        if self.copied_text > MAX_FRAGMENT_BYTES {
            return Err(budget("export text-copy budget exceeds 4 MiB"));
        }
        Ok(text(value))
    }
}
struct Candidate<'a> {
    field: &'a FieldDecl,
    path: Vec<String>,
    declaring_schema: String,
    declaring_path: Vec<String>,
    window: VersionRange,
}

fn nominated(
    tables: &TemplateTables,
    fields: &[FieldDecl],
    seen: &mut BTreeSet<String>,
    depth: usize,
    work: &mut Work,
) -> Result<bool, Diagnostics> {
    if depth > crate::limits::GROUP_DEPTH {
        return Err(budget("schema nomination reachability depth exceeds 64"));
    }
    for field in fields {
        work.visit()?;
        // Bound a name before any qualified-path copy, independently of
        // whether the enclosing declaration eventually proves relevant.
        if field.name.len() > MAX_FRAGMENT_BYTES {
            return Err(budget("field name exceeds export text budget"));
        }
        if field.is_public() {
            return Ok(true);
        }
        let found = match &field.kind {
            FieldKind::Group { fields } => nominated(tables, fields, seen, depth + 1, work)?,
            FieldKind::Scalar {
                ty: TypeExpr::Nested { schema },
                ..
            } if seen.insert(schema.clone()) => {
                let target = tables
                    .schema(schema)
                    .ok_or_else(|| budget("missing nested schema"))?;
                nominated(tables, &target.fields, seen, depth + 1, work)?
            }
            _ => false,
        };
        if found {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn collect<'a>(
    tables: &'a TemplateTables,
    fields: &'a [FieldDecl],
    declaring: &str,
    prefix: &[String],
    declared_prefix: &[String],
    window: VersionRange,
    list: bool,
    stack: &mut Vec<String>,
    out: &mut Vec<Candidate<'a>>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    for field in fields {
        work.visit()?;
        let Some(window) = field
            .window_in(tables.versions)
            .and_then(|w| w.intersect(window))
        else {
            continue;
        };
        let mut path = prefix.to_vec();
        path.push(field.name.clone());
        let mut declared_path = declared_prefix.to_vec();
        declared_path.push(field.name.clone());
        if path.len() > crate::limits::PATH_SEGMENTS {
            return Err(budget("resolved public path is too deep"));
        }
        if field.is_public() {
            if list
                || field.is_list()
                || field.is_tag()
                || !matches!(
                    field.type_expr(),
                    Some(
                        TypeExpr::Text { .. }
                            | TypeExpr::Int { .. }
                            | TypeExpr::Float { .. }
                            | TypeExpr::Bool
                            | TypeExpr::Enum { .. }
                    )
                )
            {
                return Err(rejection(field, &path, "only non-tag scalar text/int/float/bool/enum fields outside lists are admitted", None));
            }
            if out.len() >= MAX_TARGETS {
                return Err(budget("target count exceeds 16384"));
            }
            out.push(Candidate {
                field,
                path: path.clone(),
                declaring_schema: declaring.into(),
                declaring_path: declared_path.clone(),
                window,
            });
        }
        match &field.kind {
            FieldKind::Group { fields } => collect(
                tables,
                fields,
                declaring,
                &path,
                &declared_path,
                window,
                list || field.is_list(),
                stack,
                out,
                work,
            )?,
            FieldKind::Scalar {
                ty: TypeExpr::Nested { schema },
                ..
            } => {
                let target = tables
                    .schema(schema)
                    .ok_or_else(|| budget("missing nested schema"))?;
                if !nominated(tables, &target.fields, &mut BTreeSet::new(), 0, work)? {
                    continue;
                }
                if stack.contains(schema) {
                    return Err(rejection(
                        field,
                        &path,
                        "recursive schema occurrence graph with public nominations is unsupported",
                        None,
                    ));
                }
                stack.push(schema.clone());
                collect(
                    tables,
                    &target.fields,
                    schema,
                    &path,
                    &[],
                    window,
                    list || field.is_list(),
                    stack,
                    out,
                    work,
                )?;
                stack.pop();
            }
            _ => (),
        }
    }
    Ok(())
}

struct Fact {
    path: Vec<String>,
    at: Located,
}
fn fact(
    out: &mut Vec<Fact>,
    path: &[String],
    at: &Located,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    if work.dependencies >= MAX_DEPENDENCIES {
        return Err(budget("dependency facts exceed 65536"));
    }
    work.dependencies += 1;
    out.push(Fact {
        path: path.to_vec(),
        at: at.clone(),
    });
    Ok(())
}
fn overlaps(a: &[String], b: &[String]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn dollars(value: &SyntaxValue) -> bool {
    match value {
        SyntaxValue::Bare(v) | SyntaxValue::Quoted(v) => v.contains('$'),
        SyntaxValue::List(v, _) => v.iter().any(dollars),
        SyntaxValue::Object(v) => v.iter().any(|(_, v)| dollars(v)),
        SyntaxValue::Tag { name, args } => {
            name.contains('$') || args.iter().any(|a| dollars(&a.value))
        }
    }
}

fn authored_dollars(
    instances: &InstanceTable<'_>,
    instance: &InstanceDecl,
    seen: &mut BTreeSet<String>,
    depth: usize,
    work: &mut Work,
) -> Result<bool, Diagnostics> {
    if depth > crate::limits::CLONE_CHAIN {
        return Err(budget("clone dependency depth exceeds 64"));
    }
    if !seen.insert(instance.id.clone()) {
        return Ok(false);
    }
    for tag in &instance.tags {
        work.visit()?;
        if dollars(&tag.value) {
            return Ok(true);
        }
    }
    for statement in &instance.statements {
        work.visit()?;
        if dollars(&statement.value) {
            return Ok(true);
        }
    }
    for clone in &instance.clones {
        work.visit()?;
        let source = instances
            .get(&clone.source)
            .ok_or_else(|| budget("missing validated clone source"))?;
        if authored_dollars(instances, source, seen, depth + 1, work)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn instance_facts(
    instances: &InstanceTable<'_>,
    instance: &InstanceDecl,
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    if authored_dollars(instances, instance, &mut BTreeSet::new(), 0, work)? {
        fact(out, &[], &instance.at, work)?;
    }
    for target in instances.iter() {
        work.visit()?;
        for clone in &target.clones {
            work.visit()?;
            if target.id == instance.id || clone.source == instance.id {
                fact(
                    out,
                    clone
                        .path
                        .as_ref()
                        .map(|p| p.segments.as_slice())
                        .unwrap_or(&[]),
                    &clone.at,
                    work,
                )?;
            }
        }
    }
    Ok(())
}

fn collect_schema_facts(
    tables: &TemplateTables,
    fields: &[FieldDecl],
    schema: &str,
    prefix: &[String],
    stack: &mut Vec<String>,
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    if stack.iter().any(|s| s == schema) {
        // Unknown recursive subtree; a separate public sibling is disjoint.
        let at = &tables
            .schema(schema)
            .ok_or_else(|| budget("missing schema"))?
            .at;
        return fact(out, prefix, at, work);
    }
    stack.push(schema.into());
    if let Some(block) = tables.logic_for(schema) {
        statements(&block.statements, prefix, out, work)?;
    }
    schema_field_facts(tables, fields, prefix, stack, out, work)?;
    stack.pop();
    Ok(())
}
fn schema_field_facts(
    tables: &TemplateTables,
    fields: &[FieldDecl],
    prefix: &[String],
    stack: &mut Vec<String>,
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    for field in fields {
        work.visit()?;
        if field.default().is_some_and(|d| dollars(&d.value)) {
            // Default interpolation uses the enclosing INSTANCE's authored
            // variable table even inside a nested schema, unlike nested logic.
            fact(
                out,
                &[],
                &field.default().expect("checked default").at,
                work,
            )?;
        }
        let mut path = prefix.to_vec();
        path.push(field.name.clone());
        if path.len() > crate::limits::PATH_SEGMENTS {
            return Err(budget("dependency schema path is too deep"));
        }
        match &field.kind {
            FieldKind::Group { fields } => {
                schema_field_facts(tables, fields, &path, stack, out, work)?
            }
            FieldKind::Scalar {
                ty: TypeExpr::Nested { schema },
                ..
            } => {
                let target = tables
                    .schema(schema)
                    .ok_or_else(|| budget("missing schema"))?;
                collect_schema_facts(tables, &target.fields, schema, &path, stack, out, work)?;
            }
            _ => (),
        }
    }
    Ok(())
}

fn path_fact(
    path: &LogicPath,
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    let mut resolved = scope.to_vec();
    if path.root.is_none() {
        for segment in &path.segments {
            if segment.variable {
                break;
            }
            resolved.push(segment.name.clone());
            // Dropping an index conservatively includes all list elements.
        }
    }
    fact(out, &resolved, &path.at, work)
}
fn length(
    arg: &LengthArg,
    at: &Located,
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    match arg {
        LengthArg::Path(p) => path_fact(p, scope, out, work),
        LengthArg::Variable { .. } => fact(out, scope, at, work),
    }
}
fn operand(
    value: &Operand,
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    match value {
        Operand::Path(p) => path_fact(p, scope, out, work),
        Operand::Variable { at, .. } => fact(out, scope, at, work),
        Operand::Length { arg, at } => length(arg, at, scope, out, work),
        Operand::Literal { value, at } if dollars(value) => fact(out, scope, at, work),
        Operand::Group(c) => condition(c, scope, out, work),
        Operand::Calc { expr, .. } => calc(expr, scope, out, work),
        _ => Ok(()),
    }
}
fn condition(
    value: &Condition,
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    match value {
        Condition::Comparison { left, right, .. } => {
            operand(left, scope, out, work)?;
            operand(right, scope, out, work)
        }
        Condition::Exists { operand: v, .. } | Condition::Truth { operand: v, .. } => {
            operand(v, scope, out, work)
        }
        Condition::Not(c) => condition(c, scope, out, work),
        Condition::And(v) | Condition::Or(v) => {
            for c in v {
                condition(c, scope, out, work)?;
            }
            Ok(())
        }
    }
}
fn statements(
    values: &[LogicStatement],
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    for statement in values {
        work.visit()?;
        match statement {
            LogicStatement::Derive { target, value, .. } => {
                path_fact(target, scope, out, work)?;
                match value {
                    DeriveExpr::Path(p) => path_fact(p, scope, out, work)?,
                    DeriveExpr::Length { arg, at } => length(arg, at, scope, out, work)?,
                    DeriveExpr::Variable { at, .. } => fact(out, scope, at, work)?,
                    DeriveExpr::Value { value, at } if dollars(value) => {
                        fact(out, scope, at, work)?
                    }
                    DeriveExpr::Calc { expr, .. } => calc(expr, scope, out, work)?,
                    _ => (),
                }
            }
            LogicStatement::Require {
                condition: c,
                message,
                at,
            } => {
                condition(c, scope, out, work)?;
                if message.contains('$') {
                    fact(out, scope, at, work)?;
                }
            }
            LogicStatement::If {
                branches,
                otherwise,
                ..
            } => {
                for (c, body) in branches {
                    condition(c, scope, out, work)?;
                    statements(body, scope, out, work)?;
                }
                if let Some(body) = otherwise {
                    statements(body, scope, out, work)?;
                }
            }
            LogicStatement::For {
                iterable, body, at, ..
            } => {
                match iterable {
                    Iterable::Path(p) => path_fact(p, scope, out, work)?,
                    Iterable::Literal(v) if v.iter().any(dollars) => fact(out, scope, at, work)?,
                    _ => (),
                }
                statements(body, scope, out, work)?;
            }
        }
    }
    Ok(())
}

fn calc(
    value: &CalcExpr,
    scope: &[String],
    out: &mut Vec<Fact>,
    work: &mut Work,
) -> Result<(), Diagnostics> {
    match value {
        CalcExpr::Path(path) => path_fact(path, scope, out, work),
        CalcExpr::Variable { at, .. } => fact(out, scope, at, work),
        CalcExpr::Length { arg, at } => length(arg, at, scope, out, work),
        CalcExpr::Unary { value, .. } => calc(value, scope, out, work),
        CalcExpr::Binary { left, right, .. } => {
            calc(left, scope, out, work)?;
            calc(right, scope, out, work)
        }
        CalcExpr::Call { args, .. } => {
            for arg in args {
                calc(arg, scope, out, work)?;
            }
            Ok(())
        }
        CalcExpr::Literal { .. } | CalcExpr::Version { .. } => Ok(()),
    }
}

fn typed_value(field: &FieldDecl, value: &Value, work: &mut Work) -> Result<Value, Diagnostics> {
    match (field.type_expr(), value) {
        (Some(TypeExpr::Text { .. } | TypeExpr::Enum { .. }), Value::Text(v)) => work.string(v),
        (Some(TypeExpr::Int { .. }), Value::Int(v)) => work.string(&v.to_string()),
        (Some(TypeExpr::Float { .. }), Value::Float(v)) if v.is_finite() => {
            work.string(&format!("{:016x}", v.to_bits()))
        }
        (Some(TypeExpr::Bool), Value::Bool(v)) => Ok(Value::Bool(*v)),
        _ => Err(budget(
            "materialized public value does not match its declared scalar type",
        )),
    }
}
fn domain(field: &FieldDecl, work: &mut Work) -> Result<Value, Diagnostics> {
    let ty = field
        .type_expr()
        .ok_or_else(|| budget("missing scalar type"))?;
    let mut fields = vec![("type", text(ty.kind()))];
    match ty {
        TypeExpr::Text { ranges } | TypeExpr::Int { ranges } => {
            let mut values = Vec::new();
            for r in ranges {
                work.visit()?;
                values.push(object(vec![
                    ("min", work.string(&r.min.to_string())?),
                    ("max", work.string(&r.max.to_string())?),
                ]));
            }
            fields.push(("ranges", Value::List(values)));
            if matches!(ty, TypeExpr::Text { .. }) {
                fields.push(("lengthUnit", text("unicode-scalars")));
            }
        }
        TypeExpr::Float { ranges } => {
            let mut values = Vec::new();
            for r in ranges {
                work.visit()?;
                if !r.min.is_finite() || !r.max.is_finite() {
                    return Err(budget("non-finite float domain"));
                }
                values.push(object(vec![
                    ("min", work.string(&format!("{:016x}", r.min.to_bits()))?),
                    ("max", work.string(&format!("{:016x}", r.max.to_bits()))?),
                ]));
            }
            fields.push(("ranges", Value::List(values)));
        }
        TypeExpr::Enum { members } => fields.push(("members", strings(members, work)?)),
        TypeExpr::Bool => (),
        _ => return Err(budget("unsupported scalar domain reached encoding")),
    }
    Ok(object(fields))
}
fn at_path<'a>(mut value: &'a Value, path: &[String]) -> Option<&'a Value> {
    for segment in path {
        value = value.get(segment)?;
    }
    Some(value)
}
fn text(v: &str) -> Value {
    Value::Text(v.into())
}
fn object(v: Vec<(&str, Value)>) -> Value {
    Value::Object(v.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn strings(values: &[String], work: &mut Work) -> Result<Value, Diagnostics> {
    let mut out = Vec::new();
    for value in values {
        work.visit()?;
        out.push(work.string(value)?);
    }
    Ok(Value::List(out))
}
fn range_value(v: VersionRange) -> Value {
    object(vec![
        ("min", Value::Int(i64::from(v.min))),
        ("max", Value::Int(i64::from(v.max))),
    ])
}
fn budget(reason: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic::new(
        ErrorId::E703,
        format!("Public contract export rejected: {reason}."),
    ))
}
fn rejection(
    field: &FieldDecl,
    path: &[String],
    reason: &str,
    dependency: Option<&Located>,
) -> Diagnostics {
    let mut d = Diagnostic::at(
        ErrorId::E702,
        field.at.file.clone(),
        field.modifiers.public_at.unwrap_or(field.at.position),
        format!(
            "Public contract profile cannot admit {}: {reason}.",
            path.join(".")
        ),
    );
    if let Some(at) = dependency {
        d = d.with_note(
            Note::new("conservative dependency originates here.").at(at.file.clone(), at.position),
        );
    }
    Diagnostics::one(d)
}

// Conservative preflight before the existing renderer clones its envelope.
// Bounds additional document rendering, not P1–P4 or global heap. The estimate
// deliberately overcounts UTF-8 text/indentation and can reject before 64 MiB.
fn check_document_size(document: &CompiledDocument) -> Result<(), Diagnostics> {
    fn charge(total: &mut usize, value: &Value, depth: usize) -> Result<(), Diagnostics> {
        let amount = match value {
            Value::Text(s) => s.len().saturating_mul(6),
            _ => 32,
        };
        *total = total
            .saturating_add(amount)
            .saturating_add(depth.saturating_mul(8));
        if *total > MAX_ENVELOPE_BYTES {
            return Err(budget("document rendering preflight exceeds 64 MiB"));
        }
        match value {
            Value::List(v) => {
                for v in v {
                    charge(total, v, depth + 1)?;
                }
            }
            Value::Object(v) => {
                for (k, v) in v {
                    *total = total.saturating_add(k.len().saturating_mul(6));
                    charge(total, v, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let mut size = 1024;
    for value in document.data() {
        charge(&mut size, value, 4)?;
    }
    for overlay in document.overlays() {
        size = size.saturating_add(256);
        for value in &overlay.data {
            charge(&mut size, value, 6)?;
        }
        for id in &overlay.removed {
            size = size
                .saturating_add(id.len().saturating_mul(6))
                .saturating_add(64);
        }
    }
    if size > MAX_ENVELOPE_BYTES {
        return Err(budget("document rendering preflight exceeds 64 MiB"));
    }
    Ok(())
}

/// Compact object order is construction order; arrays/tuples retain order,
/// target order is Unicode tuple order; UTF-8 escaping matches output §8.8.
/// Every serialized fragment/envelope ends with exactly one LF. Floats are
/// prohibited in this lossless contract tree (represented by bit strings).
fn json(value: &Value, limit: usize) -> Result<String, Diagnostics> {
    let mut w = JsonWriter::new(limit);
    w.value(value)?;
    w.raw("\n")?;
    Ok(w.out)
}
struct JsonWriter {
    out: String,
    limit: usize,
}
impl JsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            out: String::new(),
            limit,
        }
    }
    fn raw(&mut self, v: &str) -> Result<(), Diagnostics> {
        if v.len() > self.limit.saturating_sub(self.out.len()) {
            return Err(budget("serialized output exceeds its byte limit"));
        }
        self.out.push_str(v);
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<(), Diagnostics> {
        self.raw("\"")?;
        for c in value.chars() {
            match c {
                '"' => self.raw("\\\"")?,
                '\\' => self.raw("\\\\")?,
                '\u{8}' => self.raw("\\b")?,
                '\t' => self.raw("\\t")?,
                '\n' => self.raw("\\n")?,
                '\u{c}' => self.raw("\\f")?,
                '\r' => self.raw("\\r")?,
                '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                    self.raw(&format!("\\u{:04x}", c as u32))?
                }
                _ => self.raw(c.encode_utf8(&mut [0; 4]))?,
            }
        }
        self.raw("\"")
    }
    fn value(&mut self, value: &Value) -> Result<(), Diagnostics> {
        match value {
            Value::Text(v) => self.string(v),
            Value::Int(v) => self.raw(&v.to_string()),
            Value::Bool(v) => self.raw(if *v { "true" } else { "false" }),
            Value::Float(_) => Err(budget("raw float in lossless contract")),
            Value::List(values) => {
                self.raw("[")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        self.raw(",")?;
                    }
                    self.value(v)?;
                }
                self.raw("]")
            }
            Value::Object(values) => {
                self.raw("{")?;
                for (i, (k, v)) in values.iter().enumerate() {
                    if i > 0 {
                        self.raw(",")?;
                    }
                    self.string(k)?;
                    self.raw(":")?;
                    self.value(v)?;
                }
                self.raw("}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serializer_exact_limit_and_unicode_escape() {
        let value = text("é\n\u{85}\"\\");
        let expected = "\"é\\n\\u0085\\\"\\\\\"\n";
        assert_eq!(json(&value, expected.len()).unwrap(), expected);
        assert!(json(&value, expected.len() - 1).is_err());
    }
    #[test]
    fn counters_accept_exact_and_reject_next_without_increment() {
        let mut work = Work {
            visits: MAX_VISITS - 1,
            dependencies: MAX_DEPENDENCIES - 1,
            ..Work::default()
        };
        work.visit().unwrap();
        assert!(work.visit().is_err());
        assert_eq!(work.visits, MAX_VISITS);
        let mut out = Vec::new();
        let at = Located::new("test", crate::Position::new(1, 1));
        fact(&mut out, &[], &at, &mut work).unwrap();
        assert!(fact(&mut out, &[], &at, &mut work).is_err());
        assert_eq!(out.len(), 1);
    }
}
