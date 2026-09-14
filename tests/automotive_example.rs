//! End-to-end proof that the rich automotive corpus and its teaching failures
//! stay synchronized with the real CLI in every specified output format.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::process::{Command, Output};

fn run(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_abstract"))
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("the Abstract compiler runs")
}

fn analyze(root: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_abstract"))
        .args(["analyze", "examples/automotive", "--stdio", "--symbols"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the analysis process starts");
    let mut frame = b"ABANLZ01".to_vec();
    frame.extend_from_slice(&41_u32.to_be_bytes());
    frame.extend_from_slice(&0_u32.to_be_bytes());
    child.stdin.take().unwrap().write_all(&frame).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn automotive_project_matches_json_yaml_and_raw_goldens() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (format, expected) in [
        ("JSON", "expected.json"),
        ("YML", "expected.yml"),
        ("RAW", "expected.raw"),
    ] {
        let output = run(&repository, &["compile", "examples/automotive", format]);
        assert!(
            output.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{format} wrote stderr");
        assert_eq!(
            output.stdout,
            fs::read(repository.join("examples/automotive").join(expected)).unwrap(),
            "{format} golden drifted"
        );
    }
}

#[test]
fn automotive_teaching_failures_report_the_documented_diagnostic() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "range",
        "enum",
        "ref",
        "missing-asset",
        "steering",
        "performance",
    ] {
        let project = format!("examples/automotive/negative-cases/{name}");
        let output = run(&repository, &["compile", &project, "JSON"]);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(output.stdout.is_empty(), "{name} wrote a partial document");
        let expected = fs::read_to_string(repository.join(&project).join("expected-error.txt"))
            .unwrap()
            .trim()
            .to_string();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(&format!("error[{expected}]")),
            "{name}: {stderr}"
        );
    }
}

#[test]
fn automotive_semantic_binding_graph_is_complete() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = analyze(&repository);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\"complete\": true"), "{text}");
    for kind in ["schema", "field", "instance", "loop"] {
        assert!(
            text.contains(&format!("\"kind\": \"{kind}\"")),
            "missing {kind}"
        );
    }
    assert!(text.contains("\"qualifiedName\": \"Car.features.notes.detail\""));
    assert!(text.contains("\"spelling\": \"aurora_demo\""));
    assert!(text.contains("\"name\": \"corner\""));
    let at = text
        .find("\"qualifiedName\": \"Car.wheel_checks.front_left\"")
        .unwrap();
    let end = (at + 600).min(text.len());
    assert!(
        text[at..end].contains("\"renamable\": false"),
        "a field selected by a dynamic path must refuse partial Rename"
    );
}
