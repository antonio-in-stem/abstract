//! The worked examples printed in the documentation, checked mechanically.
//!
//! `docs/examples/<name>/` and each `examples/exercises/<name>/solution/` are
//! complete projects whose compiled bytes the documentation quotes;
//! `expected.json` — and, for `formats`, `expected.yml` and `expected.abraw` —
//! hold those bytes. Each one is compiled through the binary and compared
//! after the one normalisation SPEC §11.2 permits.
//!
//! An example that drifts from the compiler is a documentation defect, so it
//! fails the suite rather than being repaired silently.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The `abstract.compiler` value both sides are normalised to (SPEC §11.2).
const NORMALISED_COMPILER: &str = "0.0.0";

#[test]
fn every_documentation_example_compiles_to_its_recorded_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    assert!(root.is_dir(), "docs/examples is missing");

    let mut names: Vec<String> = fs::read_dir(&root)
        .expect("docs/examples is readable")
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(!names.is_empty(), "docs/examples holds no project");

    let mut compared = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for name in &names {
        let project = root.join(name);
        for (file, format) in [
            ("expected.json", "JSON"),
            ("expected.yml", "YML"),
            ("expected.abraw", "RAW"),
        ] {
            let expected_path = project.join(file);
            if !expected_path.is_file() {
                continue;
            }
            compared += 1;
            let expected = fs::read_to_string(&expected_path).unwrap_or_default();
            match compile(&project, format) {
                Err(reason) => failures.push(format!("{name}/{file}: {reason}")),
                Ok(actual) => {
                    if normalise(&actual) != normalise(&expected) {
                        let line = first_difference(&normalise(&actual), &normalise(&expected));
                        failures.push(format!("{name}/{file}: bytes differ at line {line}"));
                    }
                }
            }
        }
    }

    let exercises_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/exercises");
    assert!(exercises_root.is_dir(), "examples/exercises is missing");
    let mut exercise_names: Vec<String> = fs::read_dir(&exercises_root)
        .expect("examples/exercises is readable")
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    exercise_names.sort();
    assert!(
        !exercise_names.is_empty(),
        "examples/exercises holds no exercise"
    );

    for name in &exercise_names {
        let project = exercises_root.join(name).join("solution");
        let expected_path = project.join("expected.json");
        if !expected_path.is_file() {
            failures.push(format!("{name}/solution/expected.json: missing golden"));
            continue;
        }
        compared += 1;
        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        match compile(&project, "JSON") {
            Err(reason) => failures.push(format!("{name}/solution/expected.json: {reason}")),
            Ok(actual) => {
                if normalise(&actual) != normalise(&expected) {
                    let line = first_difference(&normalise(&actual), &normalise(&expected));
                    failures.push(format!(
                        "{name}/solution/expected.json: bytes differ at line {line}"
                    ));
                }
            }
        }
    }

    println!(
        "docs examples: {} project(s), {} exercise solution(s), {compared} comparison(s)",
        names.len(),
        exercise_names.len()
    );
    assert!(
        failures.is_empty(),
        "documentation examples out of date:\n{}",
        failures.join("\n")
    );
}

/// Runs `abstract compile . <FORMAT>` in the project and returns stdout.
fn compile(project: &Path, format: &str) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_abstract"))
        .args(["compile", ".", format])
        .current_dir(project)
        .output()
        .map_err(|error| format!("cannot run the compiler: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "exit {}:\n{}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The two normalisations SPEC §11.2 permits, and no others.
fn normalise(text: &str) -> String {
    normalise_compiler(&text.replace("\r\n", "\n"))
}

/// Replaces the string value that follows the first `compiler` key.
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
