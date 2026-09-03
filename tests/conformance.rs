//! The conformance-corpus runner.
//!
//! The corpus lives at `tests/conformance/cases/<area>/<id>/`.
//! Each case directory holds the project tree it compiles plus a `case.toml`
//! describing the case. A case states its expectation in one of three ways:
//!
//! - `expected.json`, `expected.yml` or `expected.abraw` — the compile must
//!   succeed and the rendered bytes must match, after the two normalisations
//!   of SPEC §11.2 (`CRLF` becomes `LF`, and `abstract.compiler` becomes
//!   `0.0.0`);
//! - `expected-error.txt` — the compile must fail and the diagnostic
//!   identifiers it reports must begin with the identifiers listed there, one
//!   per line, in report order. The corpus writes identifiers rather than
//!   rendered messages because SPEC §7.1 leaves parser recovery open: the
//!   first identifier is the one a conforming compiler MUST report first, and
//!   the later lines are the further independent diagnostics it must also
//!   report, but a compiler may report more than the file pins;
//! - `expect = "ok"` in `case.toml` — the compile must succeed; its bytes are
//!   not compared.
//!
//! A case whose `status` is `pending` carries no expectation yet: it is
//! counted and listed, never failed. Set `ABSTRACT_CONFORMANCE_RUN_PENDING=1`
//! to attempt them anyway, which is how a stage checks its own progress.
//!
//! Every case is compiled **through the binary**, as SPEC §11.3 requires, with
//! the case directory as the working directory. The argument vector is the one
//! the case names in `command_1_0` or on the first line of `expectation_1_0`;
//! a case that names none is compiled with `compile . <FORMAT>`, where the
//! format follows the expectation file's suffix. A case whose argument vector
//! writes with `--out` is run over a copy of its tree in a temporary
//! directory, so a run never modifies the corpus, and the written file is what
//! the expectation is compared against.
//!
//! `case.toml` is read by the small flat-key reader below rather than by a
//! TOML crate: the compiler has no dependencies, and neither do its tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use abstract_lang::Format;

/// Overrides the corpus location.
const ENV_CORPUS_DIR: &str = "ABSTRACT_CONFORMANCE_DIR";
/// Set to `1` to run cases whose `status` is `pending`.
const ENV_RUN_PENDING: &str = "ABSTRACT_CONFORMANCE_RUN_PENDING";
/// The `abstract.compiler` value every comparison is normalised to (SPEC §11.2).
const NORMALISED_COMPILER: &str = "0.0.0";
/// Depth limit for the corpus walk; the corpus is two levels deep.
const MAX_WALK_DEPTH: usize = 8;
/// Depth limit for the copy a `--out` case runs over.
const MAX_COPY_DEPTH: usize = 16;
/// The commands the runner knows how to take an argument vector for.
const COMMANDS: [&str; 6] = ["compile", "lint", "templates", "init", "bundle", "unbundle"];

#[test]
fn conformance_corpus() {
    let Some(corpus) = corpus_dir() else {
        println!("conformance: no corpus directory found; set {ENV_CORPUS_DIR} to run the suite.");
        return;
    };

    let mut cases = Vec::new();
    collect_cases(&corpus, 0, &mut cases);
    cases.sort();

    assert!(
        !cases.is_empty(),
        "conformance: no case.toml found under {}",
        corpus.display()
    );

    let run_pending = std::env::var(ENV_RUN_PENDING)
        .map(|v| v == "1")
        .unwrap_or(false);

    let mut passed = 0usize;
    let mut pending: Vec<String> = Vec::new();
    let mut unspecified: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for case_dir in &cases {
        let name = relative_name(&corpus, case_dir);
        let toml_path = case_dir.join("case.toml");
        let text = match fs::read_to_string(&toml_path) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{name}: cannot read case.toml: {error}"));
                continue;
            }
        };
        let fields = parse_flat_toml(&text);

        let is_pending = fields.get("status").map(String::as_str) == Some("pending");
        if is_pending && !run_pending {
            pending.push(name);
            continue;
        }

        match expectation(case_dir, &fields) {
            None => unspecified.push(name),
            Some(expectation) => match run_case(case_dir, &fields, &expectation) {
                Ok(()) => passed += 1,
                Err(reason) => failures.push(format!("{name}: {reason}")),
            },
        }
    }

    println!(
        "conformance: {} case(s): {passed} passed, {} pending, {} without an expectation, {} failed.",
        cases.len(),
        pending.len(),
        unspecified.len(),
        failures.len()
    );
    for name in &pending {
        println!("  pending: {name}");
    }
    for name in &unspecified {
        println!("  no expectation: {name}");
    }

    assert!(
        failures.is_empty(),
        "conformance failures:\n{}",
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Case discovery
// ---------------------------------------------------------------------------

fn corpus_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var(ENV_CORPUS_DIR) {
        let path = PathBuf::from(value);
        return path.is_dir().then_some(path);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest.join("tests").join("conformance").join("cases");
    candidate.is_dir().then_some(candidate)
}

/// Collects every directory that directly contains a `case.toml`.
fn collect_cases(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    if dir.join("case.toml").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        collect_cases(&path, depth + 1, out);
    }
}

fn relative_name(corpus: &Path, case_dir: &Path) -> String {
    let relative = case_dir.strip_prefix(corpus).unwrap_or(case_dir);
    relative.to_string_lossy().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Expectations
// ---------------------------------------------------------------------------

enum Expectation {
    /// The compile succeeds and renders these exact bytes.
    Document { format: Format, expected: String },
    /// The compile fails and the identifiers it reports begin with these.
    Error { expected: Vec<String> },
    /// The compile succeeds; the bytes are not compared.
    Ok,
}

impl Expectation {
    /// The format keyword a default argument vector must carry.
    fn format(&self) -> Format {
        match self {
            Expectation::Document { format, .. } => *format,
            _ => Format::Json,
        }
    }
}

/// What one run of the binary produced.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn expectation(case_dir: &Path, fields: &BTreeMap<String, String>) -> Option<Expectation> {
    for (file, format) in [
        ("expected.json", Format::Json),
        ("expected.yml", Format::Yaml),
        ("expected.yaml", Format::Yaml),
        ("expected.abraw", Format::Raw),
    ] {
        let path = case_dir.join(file);
        if path.is_file() {
            let expected = fs::read_to_string(&path).unwrap_or_default();
            return Some(Expectation::Document { format, expected });
        }
    }

    let error_path = case_dir.join("expected-error.txt");
    if error_path.is_file() {
        let text = fs::read_to_string(&error_path).unwrap_or_default();
        let expected: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        return Some(Expectation::Error { expected });
    }

    match fields.get("expect").map(String::as_str) {
        Some("ok") => Some(Expectation::Ok),
        _ => None,
    }
}

fn run_case(
    case_dir: &Path,
    fields: &BTreeMap<String, String>,
    expectation: &Expectation,
) -> Result<(), String> {
    let invocation = Invocation::of(case_dir, fields, expectation);
    let run = invocation.execute()?;

    match expectation {
        Expectation::Ok => {
            if run.code == 0 {
                Ok(())
            } else {
                Err(format!(
                    "expected a successful compile, got exit {}:\n{}",
                    run.code, run.stderr
                ))
            }
        }
        Expectation::Document { format, expected } => {
            if run.code != 0 {
                return Err(format!(
                    "expected a document, got exit {}:\n{}",
                    run.code, run.stderr
                ));
            }
            compare(&normalise(&run.stdout), &normalise(expected), format.name())
        }
        Expectation::Error { expected } => {
            if run.code == 0 {
                return Err("expected the compile to fail, but it succeeded".to_string());
            }
            let actual = identifiers(&run.stderr);
            compare_identifiers(&actual, expected, &run.stderr)
        }
    }
}

/// The command line one case runs, and where it runs it.
struct Invocation {
    directory: PathBuf,
    args: Vec<String>,
    /// The `--out` destination, when the argument vector names one. Its
    /// content, not stdout, carries the document (SPEC §9.4).
    out: Option<String>,
}

impl Invocation {
    fn of(
        case_dir: &Path,
        fields: &BTreeMap<String, String>,
        expectation: &Expectation,
    ) -> Invocation {
        let named = command_line(fields);
        let (directory, args) = match named {
            Some((cwd, args)) => (resolve_cwd(case_dir, cwd.as_deref()), args),
            None => (
                case_dir.to_path_buf(),
                vec![
                    "compile".to_string(),
                    ".".to_string(),
                    expectation.format().name().to_string(),
                ],
            ),
        };
        let out = out_destination(&args);
        Invocation {
            directory,
            args,
            out,
        }
    }

    fn execute(&self) -> Result<Run, String> {
        // A run that writes must never touch the corpus, so it happens over a
        // copy. A run that only reads is executed in place.
        let scratch = match self.out {
            Some(_) => Some(copy_tree(&self.directory)?),
            None => None,
        };
        let directory = scratch.as_deref().unwrap_or(&self.directory);
        let output = Command::new(env!("CARGO_BIN_EXE_abstract"))
            .args(&self.args)
            .current_dir(directory)
            .output()
            .map_err(|error| format!("cannot run the compiler: {error}"))?;
        let mut run = Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        if let (Some(destination), 0) = (self.out.as_deref(), run.code) {
            let written = directory.join(destination);
            run.stdout = fs::read_to_string(&written)
                .map_err(|error| format!("cannot read '{destination}': {error}"))?;
        }
        if let Some(scratch) = scratch {
            let _ = fs::remove_dir_all(scratch);
        }
        Ok(run)
    }
}

/// The argument vector a case names, with the working directory it names.
///
/// `command_1_0` holds it directly; `expectation_1_0` holds it on its first
/// line, followed by the expectation file it produces. Both may carry
/// parenthesised prose, which is stripped, except for a `(cwd = …)` note,
/// which selects the working directory. A case that lists a second invocation
/// separates it from the first by two or more spaces, so the argument vector
/// ends at the first such run.
fn command_line(fields: &BTreeMap<String, String>) -> Option<(Option<String>, Vec<String>)> {
    let text = fields
        .get("command_1_0")
        .or_else(|| fields.get("expectation_1_0"))?;
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let line = line.split("->").next().unwrap_or(line);

    let mut cwd = None;
    let mut stripped = String::new();
    let mut depth = 0usize;
    let mut note = String::new();
    for ch in line.chars() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(value) = note.trim().strip_prefix("cwd =") {
                        cwd = Some(value.trim().to_string());
                    }
                    note.clear();
                }
            }
            _ if depth > 0 => note.push(ch),
            _ => stripped.push(ch),
        }
    }

    let stripped = stripped.trim_start();
    let first_run = stripped.find("  ").unwrap_or(stripped.len());
    let args: Vec<String> = stripped[..first_run]
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if args.is_empty() {
        return None;
    }
    // A placeholder such as `<passphrase>` is not a runnable argument vector.
    if args.iter().any(|arg| arg.starts_with('<')) {
        return None;
    }
    // A line may introduce the vector in prose ("cmd under 1.0: compile …");
    // the vector starts at the command word. A line with no command word at
    // all is a deliberately invalid one (`abstract a.ab Item.abt true JSON`)
    // and is passed through as written.
    match args.iter().position(|arg| COMMANDS.contains(&arg.as_str())) {
        Some(start) => Some((cwd, args[start..].to_vec())),
        None => Some((cwd, args)),
    }
}

/// The directory a `(cwd = …)` note names, relative to the case directory.
fn resolve_cwd(case_dir: &Path, cwd: Option<&str>) -> PathBuf {
    let Some(cwd) = cwd else {
        return case_dir.to_path_buf();
    };
    let cwd = cwd.trim_matches(|ch: char| ch == '.' || ch.is_whitespace());
    if cwd.is_empty() || cwd.contains(' ') {
        // "case root", "the case directory" and anything else prose-shaped.
        return case_dir.to_path_buf();
    }
    let case_name = case_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let relative = cwd
        .strip_prefix(&format!("{case_name}/"))
        .unwrap_or(cwd)
        .to_string();
    let candidate = case_dir.join(&relative);
    if candidate.is_dir() {
        candidate
    } else {
        case_dir.to_path_buf()
    }
}

/// The value of a `--out` flag in either of its two spellings (SPEC §9.3).
fn out_destination(args: &[String]) -> Option<String> {
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--out=") {
            return Some(value.to_string());
        }
        if arg == "--out" {
            return args.get(index + 1).cloned();
        }
    }
    None
}

/// Copies a case tree into a fresh temporary directory.
///
/// Only regular files and directories are copied: a case that uses a junction
/// or a symbolic link never writes, so it is never copied.
fn copy_tree(source: &Path) -> Result<PathBuf, String> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "case".to_string());
    let destination = std::env::temp_dir().join(format!("abstract_conf_{name}_{suffix}"));
    copy_into(source, &destination, 0)?;
    Ok(destination)
}

fn copy_into(source: &Path, destination: &Path, depth: usize) -> Result<(), String> {
    if depth > MAX_COPY_DEPTH {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("cannot create '{}': {error}", destination.display()))?;
    let entries = fs::read_dir(source)
        .map_err(|error| format!("cannot read '{}': {error}", source.display()))?;
    for entry in entries.flatten() {
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if kind.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_into(&entry.path(), &target, depth + 1)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("cannot copy '{}': {error}", target.display()))?;
        }
    }
    Ok(())
}

/// The diagnostic identifiers on stderr, in report order (SPEC §9.8).
fn identifiers(stderr: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let Some(open) = line.find("error[") else {
            continue;
        };
        let rest = &line[open + "error[".len()..];
        let Some(close) = rest.find(']') else {
            continue;
        };
        out.push(rest[..close].to_string());
    }
    out
}

/// The identifiers a case pins must be the first ones reported, in order.
///
/// A compiler may report more: SPEC §7.1 lets a phase report several
/// diagnostics before stopping and fixes no recovery rule, so the corpus pins
/// the prefix every conforming compiler must produce.
fn compare_identifiers(actual: &[String], expected: &[String], stderr: &str) -> Result<(), String> {
    let matches = actual.len() >= expected.len()
        && expected
            .iter()
            .zip(actual.iter())
            .all(|(want, got)| want == got);
    if matches {
        return Ok(());
    }
    Err(format!(
        "diagnostic identifiers differ\n--- expected (a prefix) ---\n{}\n--- actual ---\n{}\n--- reported ---\n{stderr}",
        expected.join("\n"),
        actual.join("\n")
    ))
}

fn compare(actual: &str, expected: &str, what: &str) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    let line = first_difference(actual, expected);
    Err(format!(
        "{what} differ at line {line}\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
    ))
}

fn first_difference(actual: &str, expected: &str) -> usize {
    let mut actual_lines = actual.lines();
    let mut expected_lines = expected.lines();
    let mut line = 1usize;
    loop {
        match (actual_lines.next(), expected_lines.next()) {
            (None, None) => return line,
            (a, b) if a == b => line += 1,
            _ => return line,
        }
    }
}

/// The two normalisations SPEC §11.2 permits, and no others.
fn normalise(text: &str) -> String {
    normalise_compiler(&text.replace("\r\n", "\n"))
}

/// Replaces the string value that follows the first `compiler` key with
/// `0.0.0`, in whichever of the three formats the text is written.
fn normalise_compiler(text: &str) -> String {
    let Some(key) = text.find("compiler") else {
        return text.to_string();
    };
    let after_key = key + "compiler".len();
    let Some(colon_offset) = text.get(after_key..).and_then(|rest| rest.find(':')) else {
        return text.to_string();
    };
    let after_colon = after_key + colon_offset + 1;
    let Some(quote_offset) = text.get(after_colon..).and_then(|rest| rest.find('"')) else {
        return text.to_string();
    };
    let value_start = after_colon + quote_offset + 1;
    let Some(end_offset) = text.get(value_start..).and_then(|rest| rest.find('"')) else {
        return text.to_string();
    };
    let value_end = value_start + end_offset;
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..value_start]);
    out.push_str(NORMALISED_COMPILER);
    out.push_str(&text[value_end..]);
    out
}

// ---------------------------------------------------------------------------
// A flat-key TOML reader
// ---------------------------------------------------------------------------

/// Reads the flat `key = value` pairs of a `case.toml`.
///
/// Supported values: `'''literal'''` (possibly spanning lines), `'literal'`,
/// `"basic"` with the five escapes Abstract itself defines, and bare tokens,
/// which are kept as written. Table headers and anything else are skipped;
/// nothing here can panic on malformed input.
fn parse_flat_toml(text: &str) -> BTreeMap<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut fields = BTreeMap::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        index += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        let Some(equals) = trimmed.find('=') else {
            continue;
        };
        let key = trimmed[..equals].trim().to_string();
        if key.is_empty() {
            continue;
        }
        let rest = trimmed[equals + 1..].trim_start();

        if let Some(open) = rest.strip_prefix("'''") {
            if let Some(end) = open.find("'''") {
                fields.insert(key, open[..end].to_string());
                continue;
            }
            let mut value = String::from(open);
            let mut closed = false;
            while index < lines.len() {
                let next = lines[index];
                index += 1;
                if let Some(end) = next.find("'''") {
                    if !value.is_empty() {
                        value.push('\n');
                    }
                    value.push_str(&next[..end]);
                    closed = true;
                    break;
                }
                if !value.is_empty() {
                    value.push('\n');
                }
                value.push_str(next);
            }
            if closed || !value.is_empty() {
                fields.insert(key, value);
            }
            continue;
        }

        if let Some(open) = rest.strip_prefix('\'') {
            let value = match open.find('\'') {
                Some(end) => open[..end].to_string(),
                None => open.to_string(),
            };
            fields.insert(key, value);
            continue;
        }

        if let Some(open) = rest.strip_prefix('"') {
            fields.insert(key, read_basic_string(open));
            continue;
        }

        let bare = match rest.find('#') {
            Some(index) => rest[..index].trim(),
            None => rest.trim(),
        };
        fields.insert(key, bare.to_string());
    }

    fields
}

/// Reads a double-quoted value up to its unescaped closing quote.
fn read_basic_string(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod reader_tests {
    use super::*;

    #[test]
    fn reads_the_flat_keys_the_corpus_uses() {
        let text = "\
# a comment
id = '''cli-01'''
severity = '''minor'''
title = '''line one
line two'''
expect = \"ok\"
count = 3
";
        let fields = parse_flat_toml(text);
        assert_eq!(fields.get("id").map(String::as_str), Some("cli-01"));
        assert_eq!(fields.get("severity").map(String::as_str), Some("minor"));
        assert_eq!(
            fields.get("title").map(String::as_str),
            Some("line one\nline two")
        );
        assert_eq!(fields.get("expect").map(String::as_str), Some("ok"));
        assert_eq!(fields.get("count").map(String::as_str), Some("3"));
    }

    #[test]
    fn an_unterminated_value_does_not_panic() {
        let fields = parse_flat_toml("a = '''open\nstill open\n");
        assert_eq!(
            fields.get("a").map(String::as_str),
            Some("open\nstill open")
        );
        assert!(parse_flat_toml("= 1\n[table]\nnot a pair\n").is_empty());
    }

    #[test]
    fn normalisation_replaces_crlf_and_the_compiler_string() {
        let text = "{\r\n  \"compiler\": \"1.0.0\"\r\n}\r\n";
        assert_eq!(normalise(text), "{\n  \"compiler\": \"0.0.0\"\n}\n");
        assert_eq!(normalise("no keys here\n"), "no keys here\n");
    }
}
