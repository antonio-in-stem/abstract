//! The command line (SPEC chapter 9).
//!
//! The argument parser is strict: every token must be recognised, nothing is
//! ever silently ignored, and stdout carries only the rendered document, the
//! schema list or the usage text — never a diagnostic.
//!
//! Exit codes (SPEC §9.6): 0 success, 1 a compilation diagnostic in the range
//! `E1xx`–`E7xx`, 2 a usage error (any `E8xx` but E810), 3 output could not be
//! written (E810). A reader that closes stdout — `abstract compile . | head` —
//! ends the process quietly with code 0.
//!
//! `bundle` and `unbundle` are optional tooling (SPEC §9.9, Appendix D). The
//! specification defines neither their arguments nor an identifier for their
//! failures, so their diagnostics are printed as `abstract: error: …` with no
//! catalogue identifier, which is the calling convention `bundle.rs` documents.

use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use abstract_lang::bundle::{self, BundleInvocation};
use abstract_lang::project::{self, ProjectLayout};
use abstract_lang::source::{has_extension, normalise_display_path};
use abstract_lang::{
    compile_paths, project_templates, CompileOptions, Diagnostic, Diagnostics, ErrorId, Format,
    Note, DEFAULT_MAX_ERRORS, MAX_MAX_ERRORS,
};

/// The exit code of a usage error (SPEC §9.6). The other three codes are
/// carried by the diagnostics themselves, through [`ErrorId::exit_code`].
const EXIT_USAGE: i32 = 2;

/// The usage text, printed by `--help`, `-h` and `help` (SPEC §9.2).
const USAGE: &str = "\
abstract <command> [positional ...] [flags ...]

Commands:
  compile <path>... [FORMAT] [--out <file>] [--skip-assets] [--max-errors <n>]
  lint <path>... [--skip-assets] [--max-errors <n>]
  templates <path>
  init <directory>
  --help, -h, help        print this text
  --version, -V, version  print the compiler version

FORMAT is JSON, YML, YAML or RAW, compared case-insensitively. It defaults to
JSON. A path is the project root, the data directory, or one or more .ab files.

Flags:
  --out <file>        write the document to <file> instead of stdout
  --skip-assets       skip the on-disk asset checks
  --max-errors <n>    report at most n diagnostics (1 to 10000; default 20)

Optional tooling, outside the language specification:
  analyze --capabilities | analyze <project> --stdio [--symbols]
  public-contract --capabilities | public-contract <path>... [--out <file>] [--skip-assets]
  bundle <path>... [--key <k> | --plain] [--out <file>]
  unbundle <file.abx> [--key <k>] [--out <file>]";

/// How a command failed.
///
/// Every failure the specification names is a [`Diagnostics`], which carries
/// its own exit code. The optional bundle tooling of SPEC §9.9 has no
/// catalogue identifiers, so its failures carry a message and a code instead.
#[derive(Debug)]
enum Failure {
    Reported(Diagnostics),
    Plain { message: String, code: i32 },
}

impl From<Diagnostics> for Failure {
    fn from(diagnostics: Diagnostics) -> Self {
        Failure::Reported(diagnostics)
    }
}

impl From<Diagnostic> for Failure {
    fn from(diagnostic: Diagnostic) -> Self {
        Failure::Reported(Diagnostics::one(diagnostic))
    }
}

/// Runs one invocation and returns the process exit code (SPEC §9.6).
pub fn run(args: &[String]) -> i32 {
    match dispatch(args) {
        Ok(code) => code,
        Err(Failure::Reported(diagnostics)) => report(diagnostics),
        Err(Failure::Plain { message, code }) => {
            let _ = writeln!(io::stderr(), "abstract: error: {message}");
            code
        }
    }
}

fn dispatch(args: &[String]) -> Result<i32, Failure> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        // A command word is always required; there is no direct form
        // (SPEC §9.1, Appendix A53).
        None => return Err(unknown_command("").into()),
    };

    match command {
        "compile" => command_compile(rest),
        "lint" => command_lint(rest),
        "templates" => command_templates(rest),
        "init" => command_init(rest),
        "bundle" => command_bundle(rest),
        "unbundle" => command_unbundle(rest),
        "analyze" => command_analyze(rest),
        "public-contract" => command_public_contract(rest),
        "--help" | "-h" | "help" => {
            expect_no_arguments(rest)?;
            write_stdout(&format!("{USAGE}\n"))
        }
        "--version" | "-V" | "version" => {
            expect_no_arguments(rest)?;
            write_stdout(&format!("abstract {}\n", abstract_lang::COMPILER_VERSION))
        }
        other => Err(unknown_command(other).into()),
    }
}

fn command_analyze(args: &[String]) -> Result<i32, Failure> {
    let plain = |message: String| Failure::Plain {
        message,
        code: EXIT_USAGE,
    };
    let response = match args {
        [capabilities] if capabilities == "--capabilities" => {
            abstract_lang::analysis::capabilities().map_err(plain)?
        }
        [root, mode] if mode == "--stdio" && !root.starts_with('-') => {
            let request =
                abstract_lang::analysis::read_request(io::stdin().lock()).map_err(plain)?;
            abstract_lang::analysis::analyze(Path::new(root), request).map_err(plain)?
        }
        [root, mode, symbols] if mode == "--stdio" && symbols == "--symbols" && !root.starts_with('-') => {
            let request = abstract_lang::analysis::read_request(io::stdin().lock()).map_err(plain)?;
            abstract_lang::analysis::analyze_symbols(Path::new(root), request).map_err(plain)?
        }
        _ => {
            return Err(plain(
                "Usage: abstract analyze --capabilities | abstract analyze <project> --stdio [--symbols]"
                    .into(),
            ))
        }
    };
    write_stdout(&response)
}

fn unknown_command(text: &str) -> Diagnostic {
    Diagnostic::new(
        ErrorId::E801,
        format!("Unknown command '{text}'; expected compile, lint, templates or init."),
    )
    .with_note(Note::new("run 'abstract --help' for usage."))
}

/// `--help` and `--version` take nothing; a trailing token is not ignored.
fn expect_no_arguments(rest: &[String]) -> Result<(), Failure> {
    match rest.first() {
        None => Ok(()),
        Some(text) => {
            Err(Diagnostic::new(ErrorId::E804, format!("Unexpected argument '{text}'.")).into())
        }
    }
}

// ---------------------------------------------------------------------------
// Argument parsing (SPEC §9.1, §9.3)
// ---------------------------------------------------------------------------

/// Which flags one command accepts. A flag this command does not accept is
/// E802 like any other unknown flag: `templates --skip-assets` is refused,
/// where 0.2.0 accepted and ignored it (Appendix A71).
#[derive(Clone, Copy)]
struct FlagSet {
    out: bool,
    skip_assets: bool,
    max_errors: bool,
}

impl FlagSet {
    const NONE: FlagSet = FlagSet {
        out: false,
        skip_assets: false,
        max_errors: false,
    };
    const COMPILE: FlagSet = FlagSet {
        out: true,
        skip_assets: true,
        max_errors: true,
    };
    const LINT: FlagSet = FlagSet {
        out: false,
        skip_assets: true,
        max_errors: true,
    };
}

/// The positionals and flags of one invocation, after validation.
#[derive(Debug, Default)]
struct Invocation {
    positionals: Vec<String>,
    out: Option<String>,
    skip_assets: bool,
    max_errors: Option<usize>,
}

impl Invocation {
    /// The compiler options this invocation asks for (SPEC §9.3, §9.5).
    fn options(&self) -> CompileOptions {
        CompileOptions {
            skip_asset_checks: self.skip_assets,
            max_errors: self.max_errors.unwrap_or(DEFAULT_MAX_ERRORS),
        }
    }
}

/// Splits the tokens after the command word into positionals and flags.
///
/// Flags may appear anywhere among the positionals (SPEC §9.3). Flag errors
/// are decided in one left-to-right pass, before any positional is examined.
fn parse_args(args: &[String], flags: FlagSet) -> Result<Invocation, Diagnostic> {
    let mut invocation = Invocation::default();
    let mut index = 0;

    while index < args.len() {
        let token = args[index].as_str();
        index += 1;
        let (name, inline) = split_flag(token);

        match name {
            "--out" if flags.out => {
                if invocation.out.is_some() {
                    return Err(repeated_flag("--out"));
                }
                invocation.out = Some(take_value("--out", inline, args, &mut index)?);
            }
            "--max-errors" if flags.max_errors => {
                if invocation.max_errors.is_some() {
                    return Err(repeated_flag("--max-errors"));
                }
                let text = take_value("--max-errors", inline, args, &mut index)?;
                invocation.max_errors = Some(parse_max_errors(&text)?);
            }
            "--skip-assets" if flags.skip_assets => {
                if inline.is_some() {
                    return Err(unknown_flag(token));
                }
                if invocation.skip_assets {
                    return Err(repeated_flag("--skip-assets"));
                }
                invocation.skip_assets = true;
            }
            _ if is_flag_token(token) => return Err(unknown_flag(token)),
            _ => invocation.positionals.push(token.to_string()),
        }
    }

    Ok(invocation)
}

/// Splits `--flag=value` into its name and its inline value. Only a token that
/// begins with `--` can carry one, so a positional containing `=` is untouched.
fn split_flag(token: &str) -> (&str, Option<&str>) {
    match token.split_once('=') {
        Some((name, value)) if name.starts_with("--") => (name, Some(value)),
        _ => (token, None),
    }
}

/// True when a token is meant as a flag. A lone `-` is a positional, so a
/// destination or path named `-` stays expressible.
fn is_flag_token(token: &str) -> bool {
    token.starts_with('-') && token != "-"
}

/// Reads a flag's value, from `--flag=value` or from the next token.
///
/// In the separate-token form the value must not begin with `-`, so
/// `--out --skip-assets` is a missing value and never a file named
/// `--skip-assets` (SPEC §9.3). A value that genuinely begins with `-` is
/// written `--out=-name` or `--out ./-name`.
fn take_value(
    flag: &str,
    inline: Option<&str>,
    args: &[String],
    index: &mut usize,
) -> Result<String, Diagnostic> {
    if let Some(value) = inline {
        if value.is_empty() {
            return Err(missing_value(flag));
        }
        return Ok(value.to_string());
    }
    let Some(value) = args.get(*index) else {
        return Err(missing_value(flag));
    };
    if value.is_empty() || value.starts_with('-') {
        return Err(missing_value(flag));
    }
    *index += 1;
    Ok(value.clone())
}

/// `--max-errors` takes a decimal integer from 1 to `MAX_MAX_ERRORS`
/// inclusive (SPEC §9.3). The bound is part of the flag's own rule, so a
/// rejection never has to deny the condition its message states.
fn parse_max_errors(text: &str) -> Result<usize, Diagnostic> {
    let decimal = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let value = decimal.then(|| text.parse::<usize>().ok()).flatten();
    match value {
        Some(value) if (1..=MAX_MAX_ERRORS).contains(&value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorId::E812,
            format!(
                "Flag '--max-errors' requires a decimal integer from 1 to {MAX_MAX_ERRORS}; \
                 found '{text}'."
            ),
        )),
    }
}

fn unknown_flag(token: &str) -> Diagnostic {
    Diagnostic::new(ErrorId::E802, format!("Unknown flag '{token}'."))
}

fn missing_value(flag: &str) -> Diagnostic {
    Diagnostic::new(ErrorId::E803, format!("Flag '{flag}' requires a value."))
}

fn repeated_flag(flag: &str) -> Diagnostic {
    Diagnostic::new(ErrorId::E811, format!("Flag '{flag}' was given twice."))
}

// ---------------------------------------------------------------------------
// Positionals (SPEC §9.2)
// ---------------------------------------------------------------------------

/// Separates a trailing FORMAT keyword from the input paths of `compile`.
///
/// The last positional is the FORMAT when, and only when, at least one other
/// positional is present and it compares case-insensitively equal to `JSON`,
/// `YML`, `YAML` or `RAW`. A sole positional is always an input path, so
/// `abstract compile json` compiles the directory `json`.
fn split_format(positionals: &[String]) -> Result<(Vec<PathBuf>, Format), Diagnostic> {
    let Some((last, head)) = positionals.split_last() else {
        return Err(no_input_path());
    };
    if head.is_empty() {
        return Ok((to_paths(positionals), Format::Json));
    }
    if let Some(format) = Format::parse(last) {
        return Ok((to_paths(head), format));
    }
    if Path::new(last).exists() {
        return Ok((to_paths(positionals), Format::Json));
    }
    Err(Diagnostic::new(
        ErrorId::E805,
        format!("Unknown output format '{last}'; expected JSON, YML, YAML or RAW."),
    ))
}

/// The input paths of a command that takes no FORMAT.
fn input_paths(positionals: &[String]) -> Result<Vec<PathBuf>, Diagnostic> {
    if positionals.is_empty() {
        return Err(no_input_path());
    }
    Ok(to_paths(positionals))
}

/// The single positional of `templates`, `init` and `unbundle`. `missing`
/// builds the diagnostic for an empty command line, which differs by command:
/// only a command that reads a project offers the `.ab` files lying about.
fn only_positional(
    positionals: &[String],
    missing: fn() -> Diagnostic,
) -> Result<&String, Diagnostic> {
    match positionals.split_first() {
        None => Err(missing()),
        Some((first, [])) => Ok(first),
        Some((_, rest)) => Err(Diagnostic::new(
            ErrorId::E804,
            format!("Unexpected argument '{}'.", rest[0]),
        )),
    }
}

fn to_paths(positionals: &[String]) -> Vec<PathBuf> {
    positionals
        .iter()
        .map(|text| PathBuf::from(strip_extended_length_prefix(text)))
        .collect()
}

/// Removes the Windows extended-length prefix from an input path (SPEC §9.1).
///
/// The prefix is a request to the platform's path resolver, not part of the
/// path an author wrote, so it is stripped before anything else looks at the
/// argument. Doing it here — and not in the project resolver — is what keeps
/// it out of every diagnostic (SPEC §9.8). The same spelling inside a `file`
/// or `image` value is an absolute path and stays E424 (SPEC §5.9).
fn strip_extended_length_prefix(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        // `\\?\UNC\host\share` names the share `\\host\share`; the leading
        // pair of separators has to come back or the path changes meaning.
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(text).to_string()
}

/// E806 for a command that reads a project and was given no path, with the
/// note SPEC §10.4 gives it. The note is omitted when the working directory
/// holds no `.ab` file, rather than printing an empty list.
fn no_input_path() -> Diagnostic {
    match instance_files_here() {
        Some(list) => {
            no_path_given().with_note(Note::new(format!("available .ab files here: {list}.")))
        }
        None => no_path_given(),
    }
}

/// E806 for a command whose path is a destination rather than a project to
/// read, so that no list of `.ab` files is offered.
fn no_path_given() -> Diagnostic {
    Diagnostic::new(ErrorId::E806, "No input path was given.")
}

/// The `.ab` files in the working directory, in Unicode scalar order.
fn instance_files_here() -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(".")
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| has_extension(name, "ab"))
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort();
    Some(names.join(", "))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn command_compile(args: &[String]) -> Result<i32, Failure> {
    let invocation = parse_args(args, FlagSet::COMPILE)?;
    let (paths, format) = split_format(&invocation.positionals)?;
    let options = invocation.options();

    let layout = resolve_project(&paths)?;
    // A destination is refused before anything is compiled: a compiler never
    // overwrites its own inputs, whatever the project turns out to contain
    // (SPEC §9.4).
    if let Some(destination) = &invocation.out {
        refuse_unsafe_destination(destination, &layout)?;
        check_destination_parent(destination)?;
    }

    let document =
        compile_paths(&paths, options).map_err(|errors| limit(errors, options.max_errors))?;
    let rendered = document
        .render(format)
        .map_err(|errors| limit(errors, options.max_errors))?;

    match &invocation.out {
        Some(destination) => {
            write_output_file(destination, rendered.as_bytes())?;
            note_written(destination, format.name());
            Ok(0)
        }
        None => write_stdout(&rendered),
    }
}

/// One private interchange envelope from the same discovered build snapshot.
/// This command never installs overrides or turns an unbound fragment into grants.
fn command_public_contract(args: &[String]) -> Result<i32, Failure> {
    if matches!(args, [flag] if flag == "--capabilities") {
        let capability = abstract_lang::Value::Object(vec![
            (
                "protocol".into(),
                abstract_lang::Value::Text("abstract-public-compilation".into()),
            ),
            ("version".into(), abstract_lang::Value::Int(1)),
            (
                "compiler".into(),
                abstract_lang::Value::Text(abstract_lang::COMPILER_VERSION.into()),
            ),
            (
                "profiles".into(),
                abstract_lang::Value::List(vec![abstract_lang::Value::Text(
                    "independent-scalars-v1".into(),
                )]),
            ),
            (
                "bindingStatus".into(),
                abstract_lang::Value::Text("unbound".into()),
            ),
        ]);
        return write_stdout(&abstract_lang::output::render(&capability, Format::Json)?);
    }
    let invocation = parse_args(args, FlagSet::COMPILE)?;
    let paths = input_paths(&invocation.positionals)?;
    let options = invocation.options();
    let layout = resolve_project(&paths)?;
    if let Some(destination) = &invocation.out {
        refuse_unsafe_destination(destination, &layout)?;
        check_destination_parent(destination)?;
    }
    let compilation = abstract_lang::compile_public_layout(&layout, options)
        .map_err(|errors| limit(errors, options.max_errors))?;
    let rendered = compilation
        .render()
        .map_err(|errors| limit(errors, options.max_errors))?;
    match &invocation.out {
        Some(destination) => {
            write_output_file(destination, rendered.as_bytes())?;
            note_written(destination, "public compilation JSON");
            Ok(0)
        }
        None => write_stdout(&rendered),
    }
}

fn command_lint(args: &[String]) -> Result<i32, Failure> {
    let invocation = parse_args(args, FlagSet::LINT)?;
    let paths = input_paths(&invocation.positionals)?;
    let options = invocation.options();

    // lint does exactly the work compile does and renders nothing (SPEC §9.2).
    resolve_project(&paths)?;
    compile_paths(&paths, options).map_err(|errors| limit(errors, options.max_errors))?;
    let _ = writeln!(io::stderr(), "abstract: ok");
    Ok(0)
}

fn command_templates(args: &[String]) -> Result<i32, Failure> {
    let invocation = parse_args(args, FlagSet::NONE)?;
    let path = only_positional(&invocation.positionals, no_input_path)?;

    let names = project_templates(&[PathBuf::from(strip_extended_length_prefix(path))])?;
    let mut text = String::new();
    for name in &names {
        text.push_str(name);
        text.push('\n');
    }
    write_stdout(&text)
}

fn command_init(args: &[String]) -> Result<i32, Failure> {
    let invocation = parse_args(args, FlagSet::NONE)?;
    let target = only_positional(&invocation.positionals, no_path_given)?;

    let written = normalise_display_path(target);
    crate::scaffold::create(Path::new(target), &written)?;
    let _ = writeln!(io::stderr(), "abstract: created {written}");
    Ok(0)
}

// ---------------------------------------------------------------------------
// Optional tooling: bundle and unbundle (SPEC §9.9, Appendix D.1)
// ---------------------------------------------------------------------------

fn command_bundle(args: &[String]) -> Result<i32, Failure> {
    let invocation = bundle_invocation(args)?;
    let paths = input_paths(&invocation.positionals)?;
    let key = bundle_key(invocation.flags.key.as_deref())?;

    let layout = resolve_project(&paths)?;
    if let Some(destination) = &invocation.flags.out {
        refuse_unsafe_destination(destination, &layout)?;
        check_destination_parent(destination)?;
    }

    let document = compile_paths(&paths, CompileOptions::default())?;
    let json = document.render(Format::Json)?;
    let container = bundle::encode(json.as_bytes(), key.as_ref());

    match &invocation.flags.out {
        Some(destination) => {
            write_output_file(destination, &container)?;
            let seal = if key.is_some() { "sealed" } else { "plain" };
            let _ = writeln!(
                io::stderr(),
                "abstract: wrote {} ({} bytes, {seal})",
                normalise_display_path(destination),
                container.len()
            );
            Ok(0)
        }
        None => write_stdout_bytes(&container),
    }
}

fn command_unbundle(args: &[String]) -> Result<i32, Failure> {
    let invocation = bundle_invocation(args)?;
    if invocation.flags.plain {
        return Err(Failure::Plain {
            message: "Flag '--plain' writes a container; unbundle only reads one.".to_string(),
            code: EXIT_USAGE,
        });
    }
    let source = only_positional(&invocation.positionals, no_path_given)?;
    let key = bundle_key(invocation.flags.key.as_deref())?;
    if let Some(destination) = &invocation.flags.out {
        check_destination_parent(destination)?;
    }

    let bytes = fs::read(source).map_err(|_| {
        Diagnostic::new(
            ErrorId::E806,
            format!("Input not found: '{}'.", normalise_display_path(source)),
        )
    })?;
    let payload = bundle::decode(&bytes, key.as_ref()).map_err(|message| Failure::Plain {
        message: format!("{}: {message}.", normalise_display_path(source)),
        code: 1,
    })?;
    let text = String::from_utf8(payload).map_err(|_| Failure::Plain {
        message: format!(
            "{}: the container payload is not UTF-8.",
            normalise_display_path(source)
        ),
        code: 1,
    })?;

    match &invocation.flags.out {
        Some(destination) => {
            write_output_file(destination, text.as_bytes())?;
            note_written(destination, "JSON");
            Ok(0)
        }
        None => write_stdout(&text),
    }
}

/// Validates the flags of `bundle` and `unbundle`, which SPEC §9.9 leaves
/// outside the catalogue: the wording and the exit code come from
/// [`BundleFlagError`] itself, as `bundle.rs` documents.
fn bundle_invocation(args: &[String]) -> Result<BundleInvocation, Failure> {
    bundle::validate_bundle_flags(args).map_err(|error| Failure::Plain {
        message: error.to_string(),
        code: error.exit_code(),
    })
}

/// Derives the bundle key. The key material itself is never echoed: a
/// diagnostic that quoted it would leak a passphrase into build logs.
fn bundle_key(material: Option<&str>) -> Result<Option<[u8; 32]>, Failure> {
    match material {
        None => Ok(None),
        Some(material) => match bundle::derive_key(material) {
            Ok(key) => Ok(Some(key)),
            Err(reason) => Err(Failure::Plain {
                message: format!("Flag '--key' requires valid key material: {reason}."),
                code: EXIT_USAGE,
            }),
        },
    }
}

// ---------------------------------------------------------------------------
// Output (SPEC §9.4, §9.7)
// ---------------------------------------------------------------------------

/// Resolves the project the named paths belong to (SPEC §2.3, §2.5).
///
/// E806 and E807 are command-line diagnostics with the usage exit code
/// (SPEC §9.6), so the command line decides them before any compilation
/// begins; the layout is then what `--out` is checked against.
fn resolve_project(paths: &[PathBuf]) -> Result<ProjectLayout, Diagnostics> {
    project::resolve(paths)
}

/// Refuses a `--out` destination the next run would read (SPEC §9.4).
fn refuse_unsafe_destination(destination: &str, layout: &ProjectLayout) -> Result<(), Diagnostic> {
    let written = normalise_display_path(destination);
    let Some(absolute) = absolute_destination(Path::new(destination)) else {
        // The parent directory does not exist, so the path is inside no
        // project directory; the write itself reports E810.
        return Ok(());
    };

    for source in &layout.sources {
        let Some(origin) = source.origin() else {
            continue;
        };
        if fs::canonicalize(origin)
            .map(|path| path == absolute)
            .unwrap_or(false)
        {
            return Err(Diagnostic::new(
                ErrorId::E808,
                format!("Refusing to write '{written}': it is a source file."),
            ));
        }
    }

    if let Some(data) = &layout.data_dir {
        if absolute.starts_with(data) {
            return Err(Diagnostic::new(
                ErrorId::E808,
                format!("Refusing to write '{written}': it is inside the data directory."),
            ));
        }
    }

    let is_source_name = has_extension(&written, "ab") || has_extension(&written, "abt");
    if is_source_name && absolute.starts_with(&layout.discovery_root) {
        return Err(Diagnostic::new(
            ErrorId::E808,
            format!(
                "Refusing to write '{written}': an '.ab' or '.abt' file inside the discovery \
                 root would be read on the next run."
            ),
        ));
    }
    Ok(())
}

/// The parent directory of a `--out` destination must exist (SPEC §9.4).
///
/// It is checked with the other destination rules, before compiling, so that a
/// project never compiles only to discover at the end that it has nowhere to
/// go. Every other write failure is reported by the write itself.
fn check_destination_parent(destination: &str) -> Result<(), Diagnostic> {
    let path = Path::new(destination);
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if parent.is_dir() {
        return Ok(());
    }
    Err(Diagnostic::new(
        ErrorId::E810,
        format!(
            "Cannot write '{}': the parent directory does not exist.",
            normalise_display_path(destination)
        ),
    ))
}

/// The destination as a path comparable with the canonical directories of a
/// [`ProjectLayout`]. `None` when the parent directory does not exist, which
/// is the one case in which no containment test can be made.
fn absolute_destination(destination: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = fs::canonicalize(destination) {
        return Some(canonical);
    }
    let name = destination.file_name()?;
    let parent = match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some(fs::canonicalize(parent).ok()?.join(name))
}

fn write_output_file(destination: &str, bytes: &[u8]) -> Result<(), Diagnostic> {
    fs::write(destination, bytes).map_err(|error| {
        Diagnostic::new(
            ErrorId::E810,
            format!(
                "Cannot write '{}': {}.",
                normalise_display_path(destination),
                write_reason(&error)
            ),
        )
    })
}

fn note_written(destination: &str, format: &str) {
    let _ = writeln!(
        io::stderr(),
        "abstract: wrote {} ({format})",
        normalise_display_path(destination)
    );
}

/// The `{reason}` of E810. The two conditions an author can act on are named
/// in words; anything else falls back to the platform's own text.
pub fn write_reason(error: &io::Error) -> String {
    match error.kind() {
        ErrorKind::NotFound => "the parent directory does not exist".to_string(),
        ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string().trim_end_matches('.').to_string(),
    }
}

/// Writes the one thing this run puts on stdout.
///
/// A closed reader — a broken pipe, as with `| head` — ends the process
/// quietly with exit code 0 and reports nothing (SPEC §9.7, Appendix A60).
fn write_stdout(text: &str) -> Result<i32, Failure> {
    write_stdout_bytes(text.as_bytes())
}

fn write_stdout_bytes(bytes: &[u8]) -> Result<i32, Failure> {
    let mut stream = io::stdout();
    match stream.write_all(bytes).and_then(|()| stream.flush()) {
        Ok(()) => Ok(0),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(0),
        Err(error) => Err(Diagnostic::new(
            ErrorId::E810,
            format!("Cannot write '<stdout>': {}.", write_reason(&error)),
        )
        .into()),
    }
}

/// Applies `--max-errors` to what a phase reported (SPEC §9.3).
///
/// The compiler is asked to stop at the limit, so this normally changes
/// nothing; it is the guarantee that the limit holds whatever a phase returns.
fn limit(diagnostics: Diagnostics, max_errors: usize) -> Diagnostics {
    if diagnostics.len() <= max_errors {
        return diagnostics;
    }
    let mut limited = Diagnostics::new();
    for diagnostic in diagnostics.iter().take(max_errors) {
        limited.push(diagnostic.clone());
    }
    limited.set_truncated_at(max_errors);
    limited
}

/// Prints diagnostics to stderr and returns the exit code they imply. stdout
/// is never written to (SPEC §9.7).
fn report(diagnostics: Diagnostics) -> i32 {
    let code = diagnostics.exit_code();
    let text = diagnostics.to_string();
    if !text.is_empty() {
        let _ = writeln!(io::stderr(), "{text}");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_string()).collect()
    }

    fn flag_error(tokens: &[&str], flags: FlagSet) -> Diagnostic {
        parse_args(&args(tokens), flags).expect_err("expected a flag error")
    }

    #[test]
    fn flags_may_appear_anywhere_among_the_positionals() {
        let invocation =
            parse_args(&args(&["--skip-assets", "data", "JSON"]), FlagSet::COMPILE).expect("parse");
        assert!(invocation.skip_assets);
        assert_eq!(invocation.positionals, ["data", "JSON"]);
    }

    #[test]
    fn both_spellings_of_out_are_accepted() {
        let spaced = parse_args(&args(&["data", "--out", "x.json"]), FlagSet::COMPILE).expect("a");
        let inline = parse_args(&args(&["--out=x.json", "data"]), FlagSet::COMPILE).expect("b");
        assert_eq!(spaced.out.as_deref(), Some("x.json"));
        assert_eq!(inline.out.as_deref(), Some("x.json"));

        let leading_dash =
            parse_args(&args(&["data", "--out=-name.json"]), FlagSet::COMPILE).expect("c");
        assert_eq!(leading_dash.out.as_deref(), Some("-name.json"));
    }

    #[test]
    fn a_flag_value_never_swallows_the_next_flag() {
        let error = flag_error(&["data", "--out", "--skip-assets"], FlagSet::COMPILE);
        assert_eq!(error.id, ErrorId::E803);
        assert_eq!(error.message, "Flag '--out' requires a value.");

        assert_eq!(
            flag_error(&["data", "--out"], FlagSet::COMPILE).id,
            ErrorId::E803
        );
        assert_eq!(
            flag_error(&["data", "--out="], FlagSet::COMPILE).id,
            ErrorId::E803
        );
    }

    #[test]
    fn a_repeated_flag_is_e811() {
        for tokens in [
            vec!["data", "--out", "a", "--out", "b"],
            vec!["data", "--skip-assets", "--skip-assets"],
            vec!["data", "--max-errors", "1", "--max-errors=2"],
        ] {
            assert_eq!(flag_error(&tokens, FlagSet::COMPILE).id, ErrorId::E811);
        }
    }

    #[test]
    fn an_unknown_flag_is_e802_and_a_command_never_ignores_one_it_lacks() {
        let unknown = flag_error(&["data", "--nonsense"], FlagSet::COMPILE);
        assert_eq!(unknown.id, ErrorId::E802);
        assert_eq!(unknown.message, "Unknown flag '--nonsense'.");

        // Appendix A71: `templates --skip-assets` was accepted and ignored.
        assert_eq!(
            flag_error(&["data", "--skip-assets"], FlagSet::NONE).id,
            ErrorId::E802
        );
        assert_eq!(
            flag_error(&["data", "--out", "x"], FlagSet::LINT).id,
            ErrorId::E802
        );
        // A value on a flag that takes none is not a flag this command knows.
        assert_eq!(
            flag_error(&["data", "--skip-assets=yes"], FlagSet::COMPILE).id,
            ErrorId::E802
        );
    }

    /// SPEC §9.1: an input path may carry the Windows extended-length prefix,
    /// which is a request to the platform's path resolver and not part of the
    /// path an author wrote. Stripping it here — and not in the project
    /// resolver — is what keeps it out of every diagnostic (SPEC §9.8).
    #[test]
    fn an_extended_length_input_path_loses_its_prefix() {
        assert_eq!(
            strip_extended_length_prefix(r"\\?\C:\packs\winter"),
            r"C:\packs\winter"
        );
        assert_eq!(
            strip_extended_length_prefix(r"\\?\UNC\host\share\packs"),
            r"\\host\share\packs"
        );
        // Everything else is passed through exactly as written.
        assert_eq!(strip_extended_length_prefix("data"), "data");
        assert_eq!(
            strip_extended_length_prefix(r"\\host\share"),
            r"\\host\share"
        );
        assert_eq!(
            strip_extended_length_prefix("./data/items/a.ab"),
            "./data/items/a.ab"
        );

        let paths = to_paths(&[r"\\?\C:\packs\winter".to_string(), "data".to_string()]);
        assert_eq!(
            paths,
            [PathBuf::from(r"C:\packs\winter"), PathBuf::from("data")]
        );
    }

    #[test]
    fn max_errors_takes_a_decimal_integer_inside_the_bound() {
        let one = parse_args(&args(&["data", "--max-errors", "1"]), FlagSet::COMPILE).expect("1");
        assert_eq!(one.options().max_errors, 1);
        let five = parse_args(&args(&["data", "--max-errors=5"]), FlagSet::COMPILE).expect("5");
        assert_eq!(five.options().max_errors, 5);
        let most = parse_args(&args(&["data", "--max-errors=10000"]), FlagSet::COMPILE)
            .expect("the bound itself");
        assert_eq!(most.options().max_errors, MAX_MAX_ERRORS);

        let default = parse_args(&args(&["data"]), FlagSet::COMPILE).expect("default");
        assert_eq!(default.options().max_errors, DEFAULT_MAX_ERRORS);

        // SPEC §9.3 bounds the flag at 1..=10000, so `0` and anything above
        // the bound are E812 like any other value the flag cannot take. The
        // message names the range, so it never denies its own condition.
        for bad in [
            "x",
            "1.5",
            "+1",
            "12a",
            "",
            "0",
            "10001",
            "99999999999999999999",
        ] {
            let tokens = format!("--max-errors={bad}");
            let error = flag_error(&["data", &tokens], FlagSet::COMPILE);
            let expected = if bad.is_empty() {
                ErrorId::E803
            } else {
                ErrorId::E812
            };
            assert_eq!(error.id, expected, "for '{bad}'");
        }
        // A negative value in the separate-token form is a missing value: a
        // flag's value may not begin with '-'.
        assert_eq!(
            flag_error(&["data", "--max-errors", "-1"], FlagSet::COMPILE).id,
            ErrorId::E803
        );
        assert_eq!(
            flag_error(&["data", "--max-errors=-1"], FlagSet::COMPILE).id,
            ErrorId::E812
        );
    }

    #[test]
    fn a_lone_dash_is_a_positional() {
        let invocation = parse_args(&args(&["-"]), FlagSet::COMPILE).expect("parse");
        assert_eq!(invocation.positionals, ["-"]);
    }

    #[test]
    fn the_last_positional_is_a_format_only_beside_another_one() {
        let (paths, format) = split_format(&args(&["pack", "JSON"])).expect("format");
        assert_eq!(paths, [PathBuf::from("pack")]);
        assert_eq!(format, Format::Json);

        let (paths, format) = split_format(&args(&["a.ab", "b.ab", "raw"])).expect("format");
        assert_eq!(paths.len(), 2);
        assert_eq!(format, Format::Raw);

        // A sole positional is always an input path (SPEC §9.2).
        let (paths, format) = split_format(&args(&["json"])).expect("path");
        assert_eq!(paths, [PathBuf::from("json")]);
        assert_eq!(format, Format::Json);

        let (paths, format) = split_format(&args(&["pack"])).expect("path");
        assert_eq!(paths, [PathBuf::from("pack")]);
        assert_eq!(format, Format::Json);
    }

    #[test]
    fn yml_and_yaml_name_one_format_and_case_does_not_matter() {
        for keyword in ["YML", "yaml", "Yml", "YAML"] {
            let (_, format) = split_format(&args(&["pack", keyword])).expect("format");
            assert_eq!(format, Format::Yaml);
        }
    }

    #[test]
    fn a_last_positional_that_is_neither_a_format_nor_a_path_is_e805() {
        let error = split_format(&args(&["pack", "XML"])).expect_err("E805");
        assert_eq!(error.id, ErrorId::E805);
        assert_eq!(
            error.message,
            "Unknown output format 'XML'; expected JSON, YML, YAML or RAW."
        );
        assert_eq!(
            split_format(&args(&["pack", "RAWW"])).expect_err("E805").id,
            ErrorId::E805
        );
    }

    #[test]
    fn no_positional_at_all_is_e806() {
        assert_eq!(split_format(&[]).expect_err("E806").id, ErrorId::E806);
        assert_eq!(input_paths(&[]).expect_err("E806").id, ErrorId::E806);
        assert_eq!(
            only_positional(&[], no_input_path).expect_err("E806").id,
            ErrorId::E806
        );
        assert_eq!(
            split_format(&[]).expect_err("E806").message,
            "No input path was given."
        );
    }

    #[test]
    fn a_second_positional_is_e804_where_one_is_expected() {
        let error = only_positional(&args(&["a", "b"]), no_input_path).expect_err("E804");
        assert_eq!(error.id, ErrorId::E804);
        assert_eq!(error.message, "Unexpected argument 'b'.");
    }

    #[test]
    fn help_and_version_take_no_arguments() {
        assert!(expect_no_arguments(&[]).is_ok());
        let Err(Failure::Reported(errors)) = expect_no_arguments(&args(&["compile"])) else {
            panic!("expected a diagnostic");
        };
        assert_eq!(errors.first().map(|item| item.id), Some(ErrorId::E804));
    }

    #[test]
    fn an_unknown_command_is_e801() {
        let error = unknown_command("frobnicate");
        assert_eq!(error.id, ErrorId::E801);
        assert_eq!(
            error.message,
            "Unknown command 'frobnicate'; expected compile, lint, templates or init."
        );
        assert_eq!(error.exit_code(), EXIT_USAGE);
    }

    #[test]
    fn the_max_errors_limit_cuts_the_list_and_adds_the_note() {
        let mut diagnostics = Diagnostics::new();
        for index in 0..5 {
            diagnostics.push(Diagnostic::new(ErrorId::E411, format!("problem {index}.")));
        }

        let limited = limit(diagnostics.clone(), 2);
        assert_eq!(limited.len(), 2);
        assert!(limited
            .to_string()
            .ends_with("abstract: note: stopping after 2 errors; more may remain."));

        // A list already inside the limit is untouched.
        assert_eq!(limit(diagnostics.clone(), 5).len(), 5);
        assert_eq!(limit(diagnostics, 20).len(), 5);
    }

    #[test]
    fn a_broken_pipe_is_not_a_write_failure() {
        assert_eq!(
            write_reason(&io::Error::from(ErrorKind::NotFound)),
            "the parent directory does not exist"
        );
        assert_eq!(
            write_reason(&io::Error::from(ErrorKind::PermissionDenied)),
            "permission denied"
        );
    }

    #[test]
    fn exit_codes_follow_the_specification() {
        assert_eq!(
            Diagnostics::one(unknown_command("x")).exit_code(),
            EXIT_USAGE
        );
        assert_eq!(
            Diagnostics::one(Diagnostic::new(ErrorId::E810, "x")).exit_code(),
            3
        );
        assert_eq!(
            Diagnostics::one(Diagnostic::new(ErrorId::E411, "x")).exit_code(),
            1
        );
    }

    #[test]
    fn bundle_flag_errors_keep_their_wording_and_exit_code() {
        let error = bundle::validate_bundle_flags(&args(&["data", "--key", "k", "--plain"]))
            .expect_err("--key with --plain is refused");
        assert!(error.to_string().contains("cannot be combined"));
        assert_eq!(error.exit_code(), EXIT_USAGE);
    }

    #[test]
    fn empty_key_material_is_refused_without_echoing_it() {
        let Err(Failure::Plain { message, code }) = bundle_key(Some("   ")) else {
            panic!("empty key material is refused");
        };
        assert_eq!(code, EXIT_USAGE);
        assert!(!message.contains("   "));
        assert!(bundle_key(Some("passphrase")).expect("derives").is_some());
        assert!(bundle_key(None).expect("no key").is_none());
    }
}
