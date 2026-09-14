//! Editor provenance accompanies a complete compiler export; it is not a second
//! eligibility evaluator. Catalogue/response limits never yield partial success.
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::ast::Located;
use crate::lexer::{self, Token};
use crate::public_contract;
use crate::resolve::InstanceTable;
use crate::schema::{FieldDecl, FieldKind, TemplateTables, TypeExpr};
use crate::versions::VersionRange;

const MAX_SOURCES: usize = 1024;
const MAX_DECLARATIONS: usize = 16384;
const MAX_VISITS: usize = 262144;
const MAX_PATH_COPY_BYTES: usize = 4 * 1024 * 1024;
const MAX_INVENTORY_BYTES: usize = MAX_RESPONSE_BYTES - 32768;

pub(super) fn unavailable(reason: &str) -> Value {
    object(vec![
        ("version", number(1)),
        ("status", text("unavailable")),
        ("reason", text(reason)),
        ("catalogComplete", Value::Bool(false)),
        ("sources", Value::List(vec![])),
        ("declarations", Value::List(vec![])),
        ("roots", Value::List(vec![])),
        ("exportDiagnostics", Value::List(vec![])),
    ])
}

pub(super) fn analyze(
    layout: &project::ProjectLayout,
    request: &Request,
) -> Result<String, String> {
    // The projection runs on the compiler stack. It receives tables and every
    // materialized version from one P1-P5 run, before those borrowed inputs die.
    let result = crate::on_compiler_stack(compile, (layout, &request.overlays));
    let (diagnostics, inventory, truncated) = match result {
        Ok(report) => (Diagnostics::default(), report.value, report.truncated),
        Err(errors) => (
            errors,
            unavailable("Fix compiler diagnostics before inspecting public fields."),
            false,
        ),
    };
    respond(layout, request, &diagnostics, inventory, truncated)
}

fn respond(
    layout: &project::ProjectLayout,
    request: &Request,
    diagnostics: &Diagnostics,
    inventory: Value,
    truncated: bool,
) -> Result<String, String> {
    let rendered = response(
        request.id,
        diagnostics,
        &layout.sources,
        &request.overlays,
        true,
        Some(("publicInventory", inventory)),
        truncated,
    );
    // Nesting adds renderer indentation. The final envelope, not a standalone
    // fragment estimate, is the last authority on response size.
    if rendered
        .as_ref()
        .is_err_and(|error| error == "Analysis response exceeds protocol limit.")
    {
        response(
            request.id,
            diagnostics,
            &layout.sources,
            &request.overlays,
            true,
            Some((
                "publicInventory",
                unavailable("Public inventory exceeds the complete analysis response budget."),
            )),
            false,
        )
    } else {
        rendered
    }
}

struct Report {
    value: Value,
    truncated: bool,
}

fn compile(
    (layout, overlays): (&project::ProjectLayout, &HashMap<PathBuf, String>),
) -> Result<Report, Diagnostics> {
    crate::compile_with_projection(
        (
            &layout.sources,
            layout.selected.as_deref(),
            &layout.assets_dir,
            crate::validate::AssetChecks::Enabled,
            CompileOptions::default(),
        ),
        |tables, instances, emitted, documents, document| {
            // Catalogue failure is an editor tooling limit, not a language error.
            let report = collect(tables, instances, &layout.sources, overlays, || {
                public_contract::export(tables, instances, emitted, documents, document)
            })
            .unwrap_or_else(|reason| Report {
                value: unavailable(&reason),
                truncated: false,
            });
            Ok(report)
        },
    )
}

fn collect(
    tables: &TemplateTables,
    instances: &InstanceTable<'_>,
    sources: &[SourceFile],
    overlays: &HashMap<PathBuf, String>,
    export: impl FnOnce() -> Result<public_contract::PublicCompilation, Diagnostics>,
) -> Result<Report, String> {
    let mut budget = Budget::default();
    let (mut locations, source_values) = Locations::new(sources, overlays, &mut budget)?;
    let mut declarations = BTreeMap::new();
    for schema in &tables.schemas {
        catalogue(
            &schema.fields,
            &schema.name,
            &[],
            tables.versions,
            &mut locations,
            &mut declarations,
            &mut budget,
        )?;
    }
    let mut roots = Vec::new();
    let mut export_diagnostics = Vec::new();
    let mut truncated = false;
    let (status, fragment) = match export() {
        Ok(product) => {
            // PublicCompilation also owns the private document/envelope. Neither
            // enters this Value tree. Clone only its bounded unbound fragment.
            let fragment = product.fragment().clone();
            let entries = list(fragment.get("entries"))?;
            let mut root_keys = BTreeSet::new();
            for entry in entries {
                let key = (
                    string(entry.get("declaringSchema"))?.to_string(),
                    strings(entry.get("declaringPath"))?,
                );
                if !declarations.contains_key(&key) {
                    return Err("Export target has no unique nominated declaration.".into());
                }
                root_keys.insert((
                    string(entry.get("rootSchema"))?.to_string(),
                    string(entry.get("rootInstanceId"))?.to_string(),
                ));
            }
            for (schema, id) in root_keys {
                let instance = instances
                    .get(&id)
                    .filter(|instance| instance.template == schema)
                    .ok_or("Export target has no root instance declaration.")?;
                let declaration = locations.token(&instance.at, &schema)?;
                let value = object(vec![
                    ("rootSchema", text(schema)),
                    ("rootInstanceId", text(id)),
                    ("declaration", declaration),
                ]);
                budget.keep(&value)?;
                roots.push(value);
            }
            ("export-admitted", Some(fragment))
        }
        Err(errors) => {
            truncated = errors.len() > MAX_DIAGNOSTICS;
            for error in errors.iter().take(MAX_DIAGNOSTICS) {
                let value = diagnostic_value(error, sources, overlays, &mut truncated);
                budget.keep(&value)?;
                export_diagnostics.push(value);
            }
            ("export-rejected", None)
        }
    };
    let mut fields = vec![
        ("version", number(1)),
        ("status", text(status)),
        ("catalogComplete", Value::Bool(true)),
        ("projectVersions", range(tables.versions)),
        ("sources", Value::List(source_values)),
        (
            "declarations",
            Value::List(declarations.into_values().collect()),
        ),
        ("roots", Value::List(roots)),
        ("exportDiagnostics", Value::List(export_diagnostics)),
    ];
    if let Some(fragment) = fragment {
        fields.push(("fragment", fragment));
    }
    let value = object(fields);
    ensure_response_size(&value)?;
    Ok(Report { value, truncated })
}

type Declarations = BTreeMap<(String, Vec<String>), Value>;

fn catalogue(
    fields: &[FieldDecl],
    schema: &str,
    prefix: &[String],
    project: VersionRange,
    locations: &mut Locations<'_>,
    out: &mut Declarations,
    budget: &mut Budget,
) -> Result<(), String> {
    if prefix.len() > crate::limits::GROUP_DEPTH {
        return Err("Public catalogue group depth exceeds 64.".into());
    }
    for field in fields {
        budget.visit()?;
        if !field.is_public() && !matches!(field.kind, FieldKind::Group { .. }) {
            continue;
        }
        // Bound strings before constructing qualified paths, even for private
        // fields. This applies to added catalogue work, not earlier P1-P5 work.
        if field.name.len() > MAX_INVENTORY_BYTES || schema.len() > MAX_INVENTORY_BYTES {
            return Err("Public catalogue identifier exceeds its text budget.".into());
        }
        // One temporary path for groups; public paths are also retained in the
        // declaration value and map key. Account before any prefix clone.
        budget.path(prefix, &field.name, if field.is_public() { 3 } else { 1 })?;
        let mut path = prefix.to_vec();
        path.push(field.name.clone());
        if field.is_public() {
            if out.len() >= MAX_DECLARATIONS {
                return Err("Public catalogue exceeds 16384 declarations.".into());
            }
            let declaration = locations.token(&field.at, &field.spelled)?;
            let at = Located::new(
                &field.at.file,
                field
                    .modifiers
                    .public_at
                    .ok_or("Nomination has no source position.")?,
            );
            let (nomination, spelling) = locations.nomination(&at)?;
            let window = field
                .window_in(project)
                .ok_or("Validated nomination has an empty declaration window.")?;
            let fields = vec![
                ("declaringSchema", text(schema)),
                (
                    "declaringPath",
                    Value::List(path.iter().map(text).collect()),
                ),
                ("spelling", text(&field.spelled)),
                ("type", text(field_type(field))),
                ("list", Value::Bool(field.is_list())),
                ("optional", Value::Bool(field.is_optional())),
                ("tag", Value::Bool(field.is_tag())),
                ("declaration", declaration),
                ("nomination", nomination),
                ("nominationSpelling", text(spelling)),
                ("versions", range(window)),
            ];
            // Own annotation window only; no group/nested/root intersection.
            // P3 already rejects empty own windows, including unused schemas.
            let value = object(fields);
            budget.keep(&value)?;
            if out.insert((schema.into(), path.clone()), value).is_some() {
                return Err("Public declaration identity is ambiguous.".into());
            }
        }
        if let FieldKind::Group { fields } = &field.kind {
            catalogue(fields, schema, &path, project, locations, out, budget)?;
        }
        // A nested schema is catalogued once at its own declaration, not once
        // per possible occurrence. Lists/unused schemas still retain nominations.
    }
    Ok(())
}

fn field_type(field: &FieldDecl) -> &'static str {
    match field.type_expr() {
        None => "group",
        Some(TypeExpr::Nested { .. }) => "nested",
        Some(ty) => ty.kind(),
    }
}

#[derive(Default)]
struct Budget {
    visits: usize,
    bytes: usize,
    path_bytes: usize,
}
impl Budget {
    fn path(&mut self, prefix: &[String], name: &str, copies: usize) -> Result<(), String> {
        let size = prefix
            .iter()
            .try_fold(name.len(), |sum, item| sum.checked_add(item.len()))
            .and_then(|sum| sum.checked_mul(copies))
            .ok_or("Public catalogue path-copy size overflow.")?;
        if size > MAX_PATH_COPY_BYTES - self.path_bytes {
            return Err("Public catalogue exceeds 4 MiB cumulative path copies.".into());
        }
        self.path_bytes += size;
        Ok(())
    }
    fn visit(&mut self) -> Result<(), String> {
        if self.visits >= MAX_VISITS {
            return Err("Public catalogue exceeds 262144 field visits.".into());
        }
        self.visits += 1;
        Ok(())
    }
    fn keep(&mut self, value: &Value) -> Result<(), String> {
        let size = json(value.clone())?.len();
        if size > MAX_INVENTORY_BYTES - self.bytes {
            return Err("Public catalogue exceeds its response budget.".into());
        }
        self.bytes += size;
        Ok(())
    }
}

struct SourceLocation<'a> {
    source: &'a SourceFile,
    path: String,
    preserve_bom: bool,
    tokens: Option<Vec<Token>>,
}

struct Locations<'a> {
    sources: BTreeMap<&'a str, SourceLocation<'a>>,
}

impl<'a> Locations<'a> {
    fn new(
        sources: &'a [SourceFile],
        overlays: &HashMap<PathBuf, String>,
        budget: &mut Budget,
    ) -> Result<(Self, Vec<Value>), String> {
        if sources.len() > MAX_SOURCES {
            return Err("Public inventory exceeds 1024 sources.".into());
        }
        let mut index = BTreeMap::new();
        let mut files = BTreeMap::new();
        let mut text_bytes = 0;
        for source in sources {
            let origin = source
                .origin()
                .ok_or("Source has no filesystem identity.")?;
            let canonical = fs::canonicalize(origin).unwrap_or_else(|_| origin.to_path_buf());
            let path = normalise_display_path(&canonical.to_string_lossy());
            if path.len() > MAX_PATH_BYTES {
                return Err("Public inventory source path exceeds 32768 bytes.".into());
            }
            let preserve_bom = overlays.contains_key(&canonical) && source.has_leading_bom();
            let size = source.text.len() + if preserve_bom { 3 } else { 0 };
            if size > MAX_TEXT_BYTES || size > MAX_REQUEST_BYTES - text_bytes {
                return Err(
                    "Public inventory exceeds 4 MiB/source or 16 MiB aggregate source text.".into(),
                );
            }
            text_bytes += size;
            let editor_text = if preserve_bom {
                format!("\u{feff}{}", source.text)
            } else {
                source.text.clone()
            };
            let digest = crate::crypto::sha256(editor_text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let value = object(vec![("path", text(&path)), ("sha256", text(digest))]);
            budget.keep(&value)?;
            if files.insert(path.clone(), value).is_some()
                || index
                    .insert(
                        source.path.as_str(),
                        SourceLocation {
                            source,
                            path,
                            preserve_bom,
                            tokens: None,
                        },
                    )
                    .is_some()
            {
                return Err("Public inventory has duplicate source identities.".into());
            }
        }
        Ok((Self { sources: index }, files.into_values().collect()))
    }

    fn at(&mut self, at: &Located) -> Result<(&SourceLocation<'a>, usize), String> {
        let source = self
            .sources
            .get_mut(at.file.as_str())
            .ok_or("Declaration source is outside the captured snapshot.")?;
        if source.tokens.is_none() {
            source.tokens = Some(
                lexer::tokenize(source.source)
                    .map_err(|_| "Validated source could not be tokenized for provenance.")?,
            );
        }
        let tokens = source.tokens.as_ref().expect("tokens initialized");
        let index = tokens
            .binary_search_by_key(&(at.position.line, at.position.col), |token| {
                (token.position.line, token.position.col)
            })
            .map_err(|_| "Declaration position has no exact source token.")?;
        Ok((source, index))
    }

    fn token(&mut self, at: &Located, spelling: &str) -> Result<Value, String> {
        let (source, index) = self.at(at)?;
        let token = &source.tokens.as_ref().expect("tokens initialized")[index];
        if token.identifier_text() != Some(spelling)
            || source.source.text.get(token.span.0..token.span.1) != Some(spelling)
        {
            return Err("Declaration spelling does not match its source token.".into());
        }
        Ok(source.location(token.span.0, token.span.1))
    }

    fn nomination(&mut self, at: &Located) -> Result<(Value, String), String> {
        let (source, index) = self.at(at)?;
        let tokens = source.tokens.as_ref().expect("tokens initialized");
        let sign = &tokens[index];
        let word = tokens
            .get(index + 1)
            .ok_or("Incomplete nomination token span.")?;
        if !sign.is_punctuation("@") || !word.is_keyword("public") {
            return Err("Nomination position does not identify @public tokens.".into());
        }
        let spelling = source
            .source
            .text
            .get(sign.span.0..word.span.1)
            .ok_or("Nomination span is outside its source.")?;
        Ok((source.location(sign.span.0, word.span.1), spelling.into()))
    }
}

impl SourceLocation<'_> {
    fn location(&self, start: usize, end: usize) -> Value {
        let (sl, sc, _) = utf16_range(
            self.source,
            self.source.position_at(start),
            self.preserve_bom,
        );
        let (el, ec, _) = utf16_range(self.source, self.source.position_at(end), self.preserve_bom);
        object(vec![
            ("path", text(&self.path)),
            (
                "range",
                object(vec![
                    (
                        "start",
                        object(vec![("line", number(sl)), ("character", number(sc))]),
                    ),
                    (
                        "end",
                        object(vec![("line", number(el)), ("character", number(ec))]),
                    ),
                ]),
            ),
        ])
    }
}

fn range(window: VersionRange) -> Value {
    object(vec![
        ("min", number(window.min as usize)),
        ("max", number(window.max as usize)),
    ])
}
fn string(value: Option<&Value>) -> Result<&str, String> {
    match value {
        Some(Value::Text(text)) => Ok(text),
        _ => Err("Export identity is not text.".into()),
    }
}
fn list(value: Option<&Value>) -> Result<&[Value], String> {
    match value {
        Some(Value::List(values)) => Ok(values),
        _ => Err("Export identity list is missing.".into()),
    }
}
fn strings(value: Option<&Value>) -> Result<Vec<String>, String> {
    list(value)?
        .iter()
        .map(|value| string(Some(value)).map(str::to_string))
        .collect()
}
fn ensure_response_size(value: &Value) -> Result<(), String> {
    if json(value.clone())?.len() > MAX_INVENTORY_BYTES {
        return Err("Public inventory exceeds the 4 MiB analysis response budget.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(schema: &str, instances: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "abstract_inventory_{now}_{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("data")).unwrap();
            fs::write(root.join("data/model.abt"), schema).unwrap();
            fs::write(root.join("data/items.ab"), instances).unwrap();
            Self(root)
        }
        fn report(&self, overlays: &HashMap<PathBuf, String>) -> Report {
            let layout = project::resolve_with_overlays(&[self.0.clone()], overlays).unwrap();
            compile((&layout, overlays))
                .unwrap_or_else(|errors| panic!("ordinary errors: {errors:?}"))
        }
        fn saved(&self) -> Value {
            self.report(&HashMap::new()).value
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn values<'a>(value: &'a Value, key: &str) -> &'a [Value] {
        list(value.get(key)).unwrap()
    }
    fn entries(value: &Value) -> &[Value] {
        values(value.get("fragment").unwrap(), "entries")
    }
    fn named<'a>(value: &'a Value, name: &str) -> &'a Value {
        values(value, "declarations")
            .iter()
            .find(|decl| decl.get("spelling") == Some(&text(name)))
            .unwrap()
    }
    fn get_text<'a>(value: &'a Value, key: &str) -> &'a str {
        string(value.get(key)).unwrap()
    }
    fn assert_admitted(value: &Value) {
        assert_eq!(value.get("status"), Some(&text("export-admitted")));
        assert_eq!(value.get("catalogComplete"), Some(&Value::Bool(true)));
        assert!(values(value, "exportDiagnostics").is_empty());
    }

    #[test]
    fn added_work_limits_accept_exact_and_reject_next() {
        let mut budget = Budget {
            visits: MAX_VISITS - 1,
            bytes: 0,
            path_bytes: MAX_PATH_COPY_BYTES - 6,
        };
        budget.path(&["ab".into()], "c", 2).unwrap();
        assert!(budget.path(&[], "x", 1).is_err());
        budget.visit().unwrap();
        assert!(budget.visit().is_err());
        let item = text("x");
        let size = json(item.clone()).unwrap().len();
        budget.bytes = MAX_INVENTORY_BYTES - size;
        budget.keep(&item).unwrap();
        assert!(budget.keep(&item).is_err());
        assert!(ensure_response_size(&text("x".repeat(MAX_INVENTORY_BYTES))).is_err());
    }

    #[test]
    fn complete_envelope_budget_failure_returns_unavailable_without_partial_data() {
        let fixture = Fixture::new("schema Settings {}\n", "");
        let layout = project::resolve(&[fixture.0.clone()]).unwrap();
        let request = Request {
            id: 1,
            overlays: HashMap::new(),
        };
        let output = respond(
            &layout,
            &request,
            &Diagnostics::default(),
            object(vec![("fragment", text("x".repeat(MAX_RESPONSE_BYTES)))]),
            false,
        )
        .unwrap();
        assert!(output.len() < MAX_RESPONSE_BYTES);
        assert!(output.contains("\"status\": \"unavailable\""));
        assert!(!output.contains("\"fragment\""));
        assert!(output.contains("\"catalogComplete\": false"));
    }

    #[test]
    fn private_scalar_descendants_do_not_spend_repeated_group_path_copies() {
        let fixture = Fixture::new(
            "schema Unused {\n group {\n a: int\n b: int\n c: int\n }\n}\n",
            "",
        );
        let layout = project::resolve(&[fixture.0.clone()]).unwrap();
        let (templates, _) = crate::parse_sources(&layout.sources).unwrap();
        let tables = crate::schema::build_tables(&templates).unwrap();
        let mut budget = Budget {
            path_bytes: MAX_PATH_COPY_BYTES - "group".len(),
            ..Budget::default()
        };
        let (mut locations, _) =
            Locations::new(&layout.sources, &HashMap::new(), &mut budget).unwrap();
        let mut out = BTreeMap::new();
        catalogue(
            &tables.schemas[0].fields,
            "Unused",
            &[],
            tables.versions,
            &mut locations,
            &mut out,
            &mut budget,
        )
        .unwrap();
        assert!(out.is_empty());
        assert_eq!(budget.path_bytes, MAX_PATH_COPY_BYTES);
        assert_eq!(budget.visits, 4);
    }

    #[test]
    fn max_version_keeps_unannotated_objects_in_ordinary_export_and_inventory() {
        for min in [u32::MAX, u32::MAX - 1] {
            let fixture = Fixture::new(
                &format!(
                    "versions {min}..{}\nschema Settings {{\n n: int @public = 1\n}}\n",
                    u32::MAX
                ),
                "Settings :: @id.main\n",
            );
            let expected = object(vec![
                ("template", text("Settings")),
                ("id", text("main")),
                ("n", Value::Int(1)),
            ]);
            let ordinary = crate::compile_paths(&[fixture.0.clone()], CompileOptions::default())
                .expect("u32 maximum is a valid project version");
            assert_eq!(ordinary.data(), &[expected]);
            assert!(ordinary.overlays().is_empty());
            let export =
                crate::compile_public_paths(&[fixture.0.clone()], CompileOptions::default())
                    .unwrap();
            assert_eq!(export.document, ordinary);
            let value = fixture.saved();
            assert_admitted(&value);
            assert_eq!(value.get("fragment"), Some(export.fragment()));
            let window = range(VersionRange { min, max: u32::MAX });
            assert_eq!(named(&value, "n").get("versions"), Some(&window));
            assert_eq!(entries(&value)[0].get("declarationVersions"), Some(&window));
            assert_eq!(
                values(&entries(&value)[0], "variants")[0].get("versions"),
                Some(&window)
            );
            assert_eq!(values(&value, "roots").len(), 1);
        }
    }

    #[test]
    fn max_version_explicit_removal_and_changed_values_preserve_exclusive_windows() {
        let max = u32::MAX;
        let previous = max - 1;
        let fixture = Fixture::new(&format!("versions {previous}..{max}\nschema Settings {{\n gone: int @public @removed({max}) = 1\n zero: float @public\n label: text @public @optional\n}}\n"), &format!("Settings :: @id.main\n zero: -0.0 @removed({max})\n zero: 0.0 @since({max})\n label: latest @since({max})\n"));
        let ordinary =
            crate::compile_paths(&[fixture.0.clone()], CompileOptions::default()).unwrap();
        assert_eq!(ordinary.data().len(), 1);
        assert_eq!(ordinary.data()[0].get("gone"), None);
        assert_eq!(ordinary.data()[0].get("label"), Some(&text("latest")));
        assert_eq!(ordinary.overlays().len(), 1);
        assert_eq!(
            ordinary.overlays()[0].versions,
            VersionRange {
                min: previous,
                max: previous
            }
        );
        assert_eq!(
            ordinary.overlays()[0].data[0].get("gone"),
            Some(&Value::Int(1))
        );
        let value = fixture.saved();
        assert_admitted(&value);
        let entry = |name: &str| {
            entries(&value)
                .iter()
                .find(|e| e.get("path") == Some(&Value::List(vec![text(name)])))
                .unwrap()
        };
        assert_eq!(
            entry("gone").get("declarationVersions"),
            Some(&range(VersionRange {
                min: previous,
                max: previous
            }))
        );
        let zero = values(entry("zero"), "variants");
        assert_eq!(zero.len(), 2);
        assert_eq!(zero[0].get("default"), Some(&text("8000000000000000")));
        assert_eq!(zero[1].get("default"), Some(&text("0000000000000000")));
        assert_eq!(
            values(entry("label"), "variants")[0].get("presence"),
            Some(&text("absent"))
        );
        assert_eq!(
            values(entry("label"), "variants")[1].get("presence"),
            Some(&text("present"))
        );
        assert_eq!(
            crate::versions::render_version_set(&[max, previous, max]),
            format!("{previous}..{max}")
        );
    }

    #[test]
    fn unused_nominations_and_non_scalar_kinds_are_catalogued_without_fake_targets() {
        let fixture = Fixture::new("schema Unused {\n group @public {\n count: int @public\n }\n list[]: int @public\n file: file(png) @public\n ref: ref(Used) @public\n nested: $(Used) @public\n}\nschema Used {\n private_value: int = 1\n}\n", "Used :: @id.main\n");
        let value = fixture.saved();
        assert_admitted(&value);
        assert_eq!(values(&value, "declarations").len(), 6);
        assert!(entries(&value).is_empty());
        assert!(values(&value, "roots").is_empty());
        assert_eq!(named(&value, "group").get("type"), Some(&text("group")));
        assert_eq!(named(&value, "nested").get("type"), Some(&text("nested")));
        assert_eq!(named(&value, "list").get("list"), Some(&Value::Bool(true)));
        assert_eq!(
            named(&value, "count").get("declaringPath"),
            Some(&Value::List(vec![text("group"), text("count")]))
        );
        assert_eq!(
            named(&value, "count").get("versions"),
            Some(&range(VersionRange::DEFAULT))
        );
    }

    #[test]
    fn repeated_nested_occurrences_bind_once_to_declaration_and_exact_roots() {
        let fixture = Fixture::new("schema Inner {\n n: int @public = 4\n}\nschema Settings {\n left: $(Inner)\n right: $(Inner)\n}\n", "Settings :: @id.z\n left.n: 1\n right.n: 2\nSettings :: @id.a\n left.n: 3\n right.n: 4\n");
        let value = fixture.saved();
        assert_admitted(&value);
        assert_eq!(values(&value, "declarations").len(), 1);
        assert_eq!(entries(&value).len(), 4);
        let roots = values(&value, "roots");
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].get("rootInstanceId"), Some(&text("a")));
        assert_eq!(
            roots[0]
                .get("declaration")
                .unwrap()
                .get("range")
                .unwrap()
                .get("start")
                .unwrap()
                .get("line"),
            Some(&Value::Int(3))
        );
        for entry in entries(&value) {
            assert_eq!(entry.get("declaringSchema"), Some(&text("Inner")));
            assert_eq!(
                entry.get("declaringPath"),
                named(&value, "n").get("declaringPath")
            );
        }
        let exported =
            crate::compile_public_paths(&[fixture.0.clone()], CompileOptions::default()).unwrap();
        assert_eq!(value.get("fragment"), Some(exported.fragment()));
    }

    #[test]
    fn own_windows_remain_distinct_from_root_and_group_intersections() {
        let fixture = Fixture::new("versions 1..5\nschema Settings {\n group @since(3) @removed(5) {\n n: int @public @since(2) @removed(5) = 7\n }\n}\n", "Settings :: @id.main @since(4)\n group.n: 7 @removed(5)\n");
        let value = fixture.saved();
        assert_admitted(&value);
        assert_eq!(
            named(&value, "n").get("versions"),
            Some(&range(VersionRange { min: 2, max: 4 }))
        );
        assert_eq!(entries(&value).len(), 1);
        assert_eq!(
            entries(&value)[0].get("declarationVersions"),
            Some(&range(VersionRange { min: 4, max: 4 }))
        );
    }

    #[test]
    fn lossless_scalars_and_optional_absence_are_the_exporters_exact_variants() {
        let fixture = Fixture::new("versions 1..3\nschema Settings {\n f: float @public\n n: int @public = -9223372036854775808\n max: int @public = 9223372036854775807\n big: int @public = 9007199254740993\n optional: text @optional @public\n}\n", "Settings :: @id.main\n f: -0.0 @removed(2)\n f: 0.0 @since(2) @removed(3)\n f: -0.0 @since(3)\n optional: visible @since(2)\n");
        let value = fixture.saved();
        assert_admitted(&value);
        let entry = |name: &str| {
            entries(&value)
                .iter()
                .find(|e| e.get("path") == Some(&Value::List(vec![text(name)])))
                .unwrap()
        };
        assert_eq!(
            values(entry("n"), "variants")[0].get("default"),
            Some(&text("-9223372036854775808"))
        );
        assert_eq!(
            values(entry("max"), "variants")[0].get("default"),
            Some(&text("9223372036854775807"))
        );
        assert_eq!(
            values(entry("big"), "variants")[0].get("default"),
            Some(&text("9007199254740993"))
        );
        let zero = values(entry("f"), "variants");
        assert_eq!(zero.len(), 3);
        for (v, bits) in
            zero.iter()
                .zip(["8000000000000000", "0000000000000000", "8000000000000000"])
        {
            assert_eq!(v.get("default"), Some(&text(bits)));
        }
        let absent = &values(entry("optional"), "variants")[0];
        assert_eq!(absent.get("presence"), Some(&text("absent")));
        assert_eq!(absent.get("writable"), Some(&Value::Bool(false)));
        assert!(absent.get("default").is_none());
    }

    #[test]
    fn unexecuted_dependency_rejection_retains_catalogue_but_no_fragment() {
        let fixture = Fixture::new("schema Settings {\n n: int @public = 4\n safe: bool @public = true\n choose: bool = false\n result: int = 0\n}\nlogic Settings {\n if .choose {\n derive .result = .n\n }\n}\n", "Settings :: @id.main\n");
        let value = fixture.saved();
        assert_eq!(value.get("status"), Some(&text("export-rejected")));
        assert_eq!(values(&value, "declarations").len(), 2);
        assert!(value.get("fragment").is_none());
        assert!(values(&value, "roots").is_empty());
        let error = &values(&value, "exportDiagnostics")[0];
        assert_eq!(error.get("code"), Some(&text("E702")));
        assert!(error.get("range").is_some());
        let notes = values(error, "notes");
        assert!(!notes.is_empty());
        assert!(notes[0].get("range").is_some());
    }

    #[test]
    fn dirty_bom_crlf_astral_and_normalized_names_have_exact_provenance() {
        let fixture = Fixture::new("not valid", "Settings :: @id.main\n");
        let file = fs::canonicalize(fixture.0.join("data/model.abt")).unwrap();
        let source = "\u{feff}schema Settings {\r\n\tMax-Count: int @public = 4\r\n text: text @public = \"A😀é\"\r\n}\r\n";
        let overlay = HashMap::from([(file.clone(), source.to_string())]);
        let value = fixture.report(&overlay).value;
        assert_admitted(&value);
        assert_eq!(fs::read_to_string(&file).unwrap(), "not valid");
        let field = named(&value, "Max-Count");
        assert_eq!(
            field.get("declaringPath"),
            Some(&Value::List(vec![text("max_count")]))
        );
        assert_eq!(get_text(field, "nominationSpelling"), "@public");
        let span = field.get("nomination").unwrap().get("range").unwrap();
        assert_eq!(span.get("start").unwrap().get("line"), Some(&Value::Int(1)));
        assert_eq!(
            span.get("start").unwrap().get("character"),
            Some(&Value::Int(16))
        );
        assert_eq!(
            span.get("end").unwrap().get("character"),
            Some(&Value::Int(23))
        );
        let expected = crate::crypto::sha256(source.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let canonical = normalise_display_path(&file.to_string_lossy());
        let saved = values(&value, "sources")
            .iter()
            .find(|item| item.get("path") == Some(&text(&canonical)))
            .unwrap();
        assert_eq!(get_text(saved, "sha256"), expected);
        // Direct provenance range on a line with an astral scalar before its
        // marker exercises UTF-16 conversion independently of grammar layout.
        let source = SourceFile::new("a.abt", "\u{feff}😀 @public\r\n");
        let entry = SourceLocation {
            source: &source,
            path: "a.abt".into(),
            preserve_bom: true,
            tokens: None,
        };
        let at = entry.location(5, 12);
        assert_eq!(
            at.get("range")
                .unwrap()
                .get("start")
                .unwrap()
                .get("character"),
            Some(&Value::Int(4))
        );
        assert_eq!(
            at.get("range")
                .unwrap()
                .get("end")
                .unwrap()
                .get("character"),
            Some(&Value::Int(11))
        );
    }

    #[test]
    fn oversized_catalogue_source_is_unavailable_not_a_partial_admission() {
        let source = format!(
            "//{}\nschema Settings {{\n n: int @public = 1\n}}\n",
            "x".repeat(MAX_TEXT_BYTES)
        );
        let fixture = Fixture::new(&source, "Settings :: @id.main\n");
        let value = fixture.saved();
        assert_eq!(value.get("status"), Some(&text("unavailable")));
        assert_eq!(value.get("catalogComplete"), Some(&Value::Bool(false)));
        for key in ["sources", "declarations", "roots", "exportDiagnostics"] {
            assert!(values(&value, key).is_empty());
        }
        assert!(value.get("fragment").is_none());
    }

    #[test]
    fn real_assets_are_checked_and_new_overlay_sources_are_not_written() {
        let fixture = Fixture::new(
            "schema Settings {\n n: int @public = 1\n asset: file(txt)\n}\n",
            "Settings :: @id.main\n asset: note.txt\n",
        );
        fs::create_dir(fixture.0.join("assets")).unwrap();
        fs::write(fixture.0.join("assets/note.txt"), "asset").unwrap();
        let new_file = fs::canonicalize(fixture.0.join("data"))
            .unwrap()
            .join("unused.abt");
        let overlay = HashMap::from([(
            new_file.clone(),
            "schema Unused {\n u: text @public\n}\n".into(),
        )]);
        let value = fixture.report(&overlay).value;
        assert_admitted(&value);
        assert_eq!(values(&value, "declarations").len(), 2);
        assert_eq!(values(&value, "sources").len(), 3);
        assert!(!new_file.exists());
        fs::remove_file(fixture.0.join("assets/note.txt")).unwrap();
        let request = Request {
            id: 77,
            overlays: overlay,
        };
        let output = super::super::analyze_public(&fixture.0, request).unwrap();
        assert!(output.contains("E421"), "{output}");
        assert!(output.contains("\"status\": \"unavailable\""));
    }
}
