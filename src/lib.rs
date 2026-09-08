//! Abstract 1.x — a schema-backed data language compiler.
//!
//! A project is a set of template files (`.abt`) declaring schemas, logic
//! blocks and at most one `versions` range, and instance files (`.ab`)
//! declaring objects validated against those schemas. The compiler validates
//! every instance, evaluates logic, and emits one compiled document in JSON,
//! YAML or RAW form.
//!
//! The normative definition of the language is `docs/SPEC.md`, with the
//! grammar in `docs/GRAMMAR.ebnf`. Every rule this crate implements cites the
//! section it comes from; where the code and the specification disagree, the
//! specification wins.
//!
//! The crate has no dependencies, by design. Four modules extend the compiler
//! core: [`media`] probes image headers without decoding pixels, [`crypto`]
//! implements the RFC 8439 AEAD used by bundles, [`bundle`] seals compiled
//! data into `.abx` containers, and [`output`] renders the three formats.

#![deny(warnings)]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

pub mod analysis;
pub mod ast;
pub mod bundle;
pub mod crypto;
pub mod diagnostics;
pub mod instance;
pub mod lexer;
pub mod logic;
pub mod media;
pub mod output;
pub mod project;
pub mod public_contract;
pub mod resolve;
pub mod schema;
pub mod source;
pub mod validate;
pub mod versions;

pub use diagnostics::{Diagnostic, Diagnostics, ErrorId, Note, Position};
pub use output::{Format, Value};
pub use source::SourceFile;
pub use versions::{Overlay, VersionRange, Window};

/// The compiler version reported as `abstract.compiler` in every document
/// (SPEC §8.1). Golden comparison normalises it (SPEC §11.2).
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The document format number, exactly `1` (SPEC §8.1).
pub const DOCUMENT_FORMAT: i64 = 1;

/// The default `--max-errors` limit (SPEC §9.3).
pub const DEFAULT_MAX_ERRORS: usize = 20;

/// The largest value `--max-errors` accepts (SPEC §9.3). The flag is bounded
/// so that every value it refuses is one its own message describes as
/// refusable; there is no "no limit" spelling.
pub const MAX_MAX_ERRORS: usize = 10_000;

/// The nesting and size limits of SPEC §3.7. Exceeding one is E209; they exist
/// so that a hostile or corrupted source can never exhaust the stack.
pub mod limits {
    /// Bracket nesting depth in any value.
    pub const BRACKET_DEPTH: usize = 64;
    /// Segments in an assignment path, clone path or logic path.
    pub const PATH_SEGMENTS: usize = 64;
    /// Nesting depth of schema groups, and of `$(Schema)` values in one object.
    pub const GROUP_DEPTH: usize = 64;
    /// Nesting depth of `if` / `for` blocks in one logic block.
    pub const LOGIC_BLOCK_DEPTH: usize = 64;
    /// Length of a clone chain (transitive clone depth).
    pub const CLONE_CHAIN: usize = 64;
    /// Nesting depth of the emitted document.
    pub const DOCUMENT_DEPTH: usize = 64;
    /// Loop iterations the logic of one instance may execute for one
    /// version. This is the one limit of SPEC §3.7 that bounds work
    /// rather than depth, and exceeding it is E523, not E209.
    pub const LOGIC_WORK: u64 = 1_000_000;
}

/// Compiler behaviour switches.
///
/// There is no lenient mode in Abstract 1.0: `--allow-unknown` does not exist
/// (SPEC §5.12, Appendix A52).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    /// Skip the on-disk asset checks E421, E422 and E423 (SPEC §9.5). The
    /// extension check (E420) and the path-confinement check (E424) still run,
    /// and the compiled bytes are identical either way.
    pub skip_asset_checks: bool,
    /// Report at most this many diagnostics; `0` means no limit (SPEC §9.3).
    pub max_errors: usize,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            skip_asset_checks: false,
            max_errors: DEFAULT_MAX_ERRORS,
        }
    }
}

/// One compiled document: the envelope of SPEC §8.1, the base `data` array
/// for the project's maximum version, and the overlays that carry every
/// earlier version that differs from it (SPEC §7.5).
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledDocument {
    compiler: String,
    versions: VersionRange,
    data: Vec<Value>,
    overlays: Vec<Overlay>,
}

impl CompiledDocument {
    /// Builds a document from its parts. `data` must already be ordered by
    /// `(template, id)` (SPEC §2.7) and `overlays` by SPEC §7.5 step 2.
    pub fn new(versions: VersionRange, data: Vec<Value>, overlays: Vec<Overlay>) -> Self {
        Self {
            compiler: COMPILER_VERSION.to_string(),
            versions,
            data,
            overlays,
        }
    }

    /// The `abstract.compiler` string of this document.
    pub fn compiler(&self) -> &str {
        &self.compiler
    }

    /// The project version range (SPEC §4.12).
    pub fn versions(&self) -> VersionRange {
        self.versions
    }

    /// The base instance objects, for version `versions().max`.
    pub fn data(&self) -> &[Value] {
        &self.data
    }

    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    /// The whole document as a value tree, in envelope order (SPEC §8.1).
    pub fn envelope(&self) -> Value {
        Value::Object(vec![
            (
                "abstract".to_string(),
                Value::Object(vec![
                    ("format".to_string(), Value::Int(DOCUMENT_FORMAT)),
                    ("compiler".to_string(), Value::Text(self.compiler.clone())),
                    ("versions".to_string(), version_range_value(self.versions)),
                ]),
            ),
            ("data".to_string(), Value::List(self.data.clone())),
            (
                "overlays".to_string(),
                Value::List(self.overlays.iter().map(overlay_value).collect()),
            ),
        ])
    }

    /// Renders the document. The result ends with exactly one `LF`.
    pub fn render(&self, format: Format) -> Result<String, Diagnostics> {
        output::render(&self.envelope(), format)
    }

    /// Renders as JSON, or an empty string when rendering fails. Rendering can
    /// only fail on E701, which is unreachable in a conforming document
    /// (SPEC §8.7); use [`CompiledDocument::render`] to observe the failure.
    pub fn to_json_string(&self) -> String {
        self.render(Format::Json).unwrap_or_default()
    }

    /// Renders as YAML. See [`CompiledDocument::to_json_string`].
    pub fn to_yaml_string(&self) -> String {
        self.render(Format::Yaml).unwrap_or_default()
    }

    /// Renders as RAW. See [`CompiledDocument::to_json_string`].
    pub fn to_raw_string(&self) -> String {
        self.render(Format::Raw).unwrap_or_default()
    }
}

fn version_range_value(range: VersionRange) -> Value {
    Value::Object(vec![
        ("min".to_string(), Value::Int(i64::from(range.min))),
        ("max".to_string(), Value::Int(i64::from(range.max))),
    ])
}

fn overlay_value(overlay: &Overlay) -> Value {
    Value::Object(vec![
        (
            "versions".to_string(),
            version_range_value(overlay.versions),
        ),
        ("data".to_string(), Value::List(overlay.data.clone())),
        (
            "removed".to_string(),
            Value::List(
                overlay
                    .removed
                    .iter()
                    .map(|id| Value::Text(id.clone()))
                    .collect(),
            ),
        ),
    ])
}

/// Compiles a whole project rooted at `root`, which must be the project root
/// or the data directory (SPEC §2.3).
pub fn compile_project(
    root: &Path,
    options: CompileOptions,
) -> Result<CompiledDocument, Diagnostics> {
    compile_paths(&[root.to_path_buf()], options)
}

/// Compiles the paths named on a command line: exactly one directory, or one
/// or more `.ab` files (SPEC §2.5).
///
/// P0 resolves the project and collects its sources; the remaining phases are
/// [`compile_layout`]. Every discovered instance is compiled whichever form
/// was used, and the named files only select what reaches the document, so
/// single-file mode and whole-project mode agree (SPEC §7.6).
pub fn compile_paths(
    paths: &[PathBuf],
    options: CompileOptions,
) -> Result<CompiledDocument, Diagnostics> {
    let layout = project::resolve(paths)?;
    compile_layout(&layout, options)
}

/// Compiles in-memory sources, with no filesystem discovery.
///
/// There is no project root, so the on-disk asset checks E421, E422 and E423
/// cannot run and are skipped exactly as `--skip-assets` skips them; the
/// extension check (E420) and path confinement (E424) still run, and SPEC §7.6
/// makes the compiled bytes identical either way. Every other phase is the one
/// [`compile_paths`] runs.
pub fn compile_sources(
    sources: Vec<SourceFile>,
    options: CompileOptions,
) -> Result<CompiledDocument, Diagnostics> {
    let mut sources = sources;
    project::sort_sources(&mut sources);
    compile_collected(
        &sources,
        None,
        Path::new(project::ASSETS_DIR),
        validate::AssetChecks::Skipped,
        options,
    )
}

/// Compiles the selected paths and exports an unbound public-data fragment.
/// The fragment is not an authorization to modify a runtime document.
pub fn compile_public_paths(
    paths: &[PathBuf],
    options: CompileOptions,
) -> Result<public_contract::PublicCompilation, Diagnostics> {
    let layout = project::resolve(paths)?;
    compile_public_layout(&layout, options)
}

/// Uses one discovered source snapshot for compilation and public export.
/// Asset checks still access the layout's assets directory as ordinary compilation does.
pub fn compile_public_layout(
    layout: &project::ProjectLayout,
    options: CompileOptions,
) -> Result<public_contract::PublicCompilation, Diagnostics> {
    let checks = if options.skip_asset_checks {
        validate::AssetChecks::Skipped
    } else {
        validate::AssetChecks::Enabled
    };
    on_compiler_stack(
        compile_public_phases,
        (
            &layout.sources,
            layout.selected.as_deref(),
            &layout.assets_dir,
            checks,
            options,
        ),
    )
}

/// In-memory counterpart of [`compile_public_paths`], with the same asset
/// behavior as [`compile_sources`]. Public export never evaluates a second build.
pub fn compile_public_sources(
    mut sources: Vec<SourceFile>,
    options: CompileOptions,
) -> Result<public_contract::PublicCompilation, Diagnostics> {
    project::sort_sources(&mut sources);
    on_compiler_stack(
        compile_public_phases,
        (
            &sources,
            None,
            Path::new(project::ASSETS_DIR),
            validate::AssetChecks::Skipped,
            options,
        ),
    )
}

/// Phases P1 to P5 over a resolved project layout (SPEC §7.1).
fn compile_layout(
    layout: &project::ProjectLayout,
    options: CompileOptions,
) -> Result<CompiledDocument, Diagnostics> {
    let checks = if options.skip_asset_checks {
        validate::AssetChecks::Skipped
    } else {
        validate::AssetChecks::Enabled
    };
    compile_collected(
        &layout.sources,
        layout.selected.as_deref(),
        &layout.assets_dir,
        checks,
        options,
    )
}

/// Phases P1 to P5 over an already-collected source list.
///
/// `selected` names the project-relative source paths whose instances reach
/// the document; `None` emits every instance (SPEC §2.5). A phase runs only if
/// every earlier phase succeeded (SPEC §7.1).
fn compile_collected(
    sources: &[SourceFile],
    selected: Option<&[String]>,
    assets_dir: &Path,
    asset_checks: validate::AssetChecks,
    options: CompileOptions,
) -> Result<CompiledDocument, Diagnostics> {
    on_compiler_stack(
        compile_phases,
        (sources, selected, assets_dir, asset_checks, options),
    )
}

/// The stack the compiler's phases run on.
///
/// SPEC §3.7 bounds every recursion at 64 levels and forbids a crash on any
/// input. The bound is on levels, not on bytes: an unoptimised build spends
/// several kilobytes of frame per level, which a source at the documented
/// limits can push past the stack a process's first thread is given. Sizing
/// the stack for the deepest input the limits allow makes the guarantee hold
/// in every profile rather than only in an optimised one.
const COMPILER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// The arguments of one compile, all `Copy`, so that the work can be run on
/// either stack without being moved into the thread first.
type CompileArgs<'a> = (
    &'a [SourceFile],
    Option<&'a [String]>,
    &'a Path,
    validate::AssetChecks,
    CompileOptions,
);

fn on_compiler_stack<A, T>(work: fn(A) -> T, args: A) -> T
where
    A: Copy + Send,
    T: Send,
{
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .stack_size(COMPILER_STACK_BYTES)
            .spawn_scoped(scope, move || work(args));
        match spawned {
            Ok(handle) => match handle.join() {
                Ok(value) => value,
                Err(payload) => std::panic::resume_unwind(payload),
            },
            // A platform that refuses the thread is no reason to refuse the
            // compile: it runs here instead, on the caller's own stack.
            Err(_) => work(args),
        }
    })
}

fn compile_phases(args: CompileArgs<'_>) -> Result<CompiledDocument, Diagnostics> {
    compile_with_projection(args, |_, _, _, _, document| Ok(document))
}

fn compile_public_phases(
    args: CompileArgs<'_>,
) -> Result<public_contract::PublicCompilation, Diagnostics> {
    compile_with_projection(args, public_contract::export)
}

/// Both products use the same P1–P5 result. The public projection sees every
/// materialized version before overlay reduction discards equal versions.
fn compile_with_projection<T>(
    (sources, selected, assets_dir, asset_checks, _options): CompileArgs<'_>,
    finish: impl FnOnce(
        &schema::TemplateTables,
        &resolve::InstanceTable<'_>,
        Option<&[String]>,
        &[Vec<Value>],
        CompiledDocument,
    ) -> Result<T, Diagnostics>,
) -> Result<T, Diagnostics> {
    // P1: lex and parse every source.
    let (templates, instance_files) = parse_sources(sources)?;

    // P2: the project tables, then every whole-project instance check.
    let tables = schema::build_tables(&templates)?;
    let declarations = resolve::collect_instances(&instance_files);
    resolve::check_instances(&declarations, &tables)?;
    let instances = resolve::InstanceTable::new(declarations);

    // P3: schemas and logic blocks. It depends on no instance, but SPEC §11.1
    // fixes P2 before P3, so an unknown template outranks a broken schema.
    schema::validate_schemas(&tables)?;

    // P4: compile every instance for every version in which it exists.
    let emitted = selected.map(|paths| emitted_ids(&instances, paths));
    let documents = versions::materialise(
        &tables,
        &instances,
        emitted.as_deref(),
        assets_dir,
        asset_checks,
    )?;

    // P5: the base document plus the overlays that differ from it.
    let overlays = versions::reduce_overlays(tables.versions, &documents);
    let base = documents.last().cloned().unwrap_or_default();
    finish(
        &tables,
        &instances,
        emitted.as_deref(),
        &documents,
        CompiledDocument::new(tables.versions, base, overlays),
    )
}

/// The ids of the instances declared in the selected files (SPEC §2.5).
///
/// Selection is by source file, so an id is emitted when the file that
/// declares it was named on the command line. The result keeps the table's
/// `(template, id)` order, which is the order the document uses (SPEC §2.7).
fn emitted_ids(instances: &resolve::InstanceTable<'_>, selected: &[String]) -> Vec<String> {
    instances
        .iter()
        .filter(|decl| selected.iter().any(|path| path == &decl.at.file))
        .map(|decl| decl.id.clone())
        .collect()
}

/// The schema names a project declares, sorted as sequences of Unicode scalar
/// values — the output of `abstract templates` (SPEC §9.2).
///
/// Phases P0 to P3 all run (SPEC §7.1): discovery, parsing, the project tables
/// and schema validation. A project whose schemas or instances are broken
/// therefore fails instead of printing a partial list.
pub fn project_templates(paths: &[PathBuf]) -> Result<Vec<String>, Diagnostics> {
    let layout = project::resolve(paths)?;
    let (templates, instances) = parse_sources(&layout.sources)?;

    // P2 before P3, the order SPEC §7.1 fixes and SPEC §11.1 makes observable:
    // an unknown template (E401) outranks a malformed schema field (E302).
    let tables = schema::build_tables(&templates)?;
    let declarations = resolve::collect_instances(&instances);
    resolve::check_instances(&declarations, &tables)?;
    schema::validate_schemas(&tables)?;

    let mut names: Vec<String> = tables
        .schemas
        .iter()
        .map(|schema| schema.name.clone())
        .collect();
    // Rust orders `String` by UTF-8 bytes, which is Unicode scalar order.
    names.sort();
    Ok(names)
}

/// P1 (SPEC §7.1): parses every discovered source and splits the result into
/// template files and instance files.
///
/// Parsing continues past a file that fails, so one run reports every parse
/// error in the project, in the source order of SPEC §2.4.
fn parse_sources(
    sources: &[SourceFile],
) -> Result<(Vec<ast::TemplateFile>, Vec<ast::InstanceFile>), Diagnostics> {
    let mut templates = Vec::new();
    let mut instances = Vec::new();
    let mut errors = Diagnostics::new();

    for source in sources {
        match ast::parse(source) {
            Ok(ast::SourceUnit::Template(file)) => templates.push(file),
            Ok(ast::SourceUnit::Instance(file)) => instances.push(file),
            Err(diagnostics) => errors.extend(diagnostics),
        }
    }

    if errors.is_empty() {
        Ok((templates, instances))
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_has_three_keys_in_order() {
        let document = CompiledDocument::new(VersionRange::DEFAULT, Vec::new(), Vec::new());
        let envelope = document.envelope();
        let Value::Object(entries) = &envelope else {
            panic!("the envelope is an object");
        };
        let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["abstract", "data", "overlays"]);

        let meta = envelope.get("abstract").expect("abstract object");
        let Value::Object(meta_entries) = meta else {
            panic!("abstract is an object");
        };
        let meta_keys: Vec<&str> = meta_entries.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(meta_keys, ["format", "compiler", "versions"]);
        assert_eq!(meta.get("format"), Some(&Value::Int(1)));
        assert_eq!(
            meta.get("compiler"),
            Some(&Value::Text(COMPILER_VERSION.to_string()))
        );
    }

    #[test]
    fn default_options_carry_the_documented_error_limit() {
        let options = CompileOptions::default();
        assert!(!options.skip_asset_checks);
        assert_eq!(options.max_errors, DEFAULT_MAX_ERRORS);
    }

    #[test]
    fn an_empty_source_list_is_a_project_with_no_sources() {
        // P0 is the caller's job for `compile_sources`, so an empty list is
        // simply a project with nothing in it: zero schemas, zero instances,
        // the default version range and an empty document.
        let document =
            compile_sources(Vec::new(), CompileOptions::default()).expect("an empty project");
        assert_eq!(document.versions(), VersionRange::DEFAULT);
        assert!(document.data().is_empty());
        assert!(document.overlays().is_empty());
    }

    #[test]
    fn the_pipeline_compiles_in_memory_sources() {
        let sources = vec![
            SourceFile::new("Item.abt", "schema Item {\n    name: text(1..40)\n}\n"),
            SourceFile::new("torch.ab", "Item :: @id.torch\n    name: Torch\n"),
        ];
        let document = compile_sources(sources, CompileOptions::default()).expect("a document");
        assert_eq!(document.data().len(), 1);
        let object = &document.data()[0];
        assert_eq!(object.get("template"), Some(&Value::Text("Item".into())));
        assert_eq!(object.get("id"), Some(&Value::Text("torch".into())));
        assert_eq!(object.get("name"), Some(&Value::Text("Torch".into())));
    }
}
