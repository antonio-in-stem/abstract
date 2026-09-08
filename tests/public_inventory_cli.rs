//! Real CLI transport, dirty-buffer admission and separation of diagnostic modes.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new(schema: &str, instance: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "abstract_inventory_cli_{now}_{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("data")).unwrap();
        fs::write(root.join("data/model.abt"), schema).unwrap();
        fs::write(root.join("data/item.ab"), instance).unwrap();
        Self(root)
    }
    fn run(&self, mode: &[&str], input: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_abstract"))
            .args(mode)
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn frame(overlays: &[(&Path, &str)]) -> Vec<u8> {
    let mut bytes = b"ABANLZ01".to_vec();
    bytes.extend_from_slice(&17_u32.to_be_bytes());
    bytes.extend_from_slice(&(overlays.len() as u32).to_be_bytes());
    for (file, text) in overlays {
        let path = file.to_str().unwrap().as_bytes();
        bytes.extend_from_slice(&(path.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
        bytes.extend_from_slice(path);
        bytes.extend_from_slice(text.as_bytes());
    }
    bytes
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn capability_is_explicit_and_existing_analysis_modes_stay_separate() {
    let fixture = Fixture::new(
        "schema Settings {\n n: int @public = 1\n}\n",
        "Settings :: @id.main\n",
    );
    let capability = success(fixture.run(&["analyze", "--capabilities"], &[]));
    assert!(capability.contains("\"publicInventory\": 1"));
    assert!(capability.contains("\"schemaBindings\": 1"));
    let ordinary = success(fixture.run(&["analyze", ".", "--stdio"], &frame(&[])));
    assert!(!ordinary.contains("publicInventory"));
    let symbols = success(fixture.run(&["analyze", ".", "--stdio", "--symbols"], &frame(&[])));
    assert!(symbols.contains("\"bindings\""));
    assert!(!symbols.contains("publicInventory"));
    let invalid = fixture.run(&["analyze", ".", "--stdio", "--symbols", "--public"], &[]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
}

#[test]
fn dirty_schema_repairs_disk_and_never_publishes_private_compiled_body() {
    let fixture = Fixture::new("invalid source", "Settings :: @id.main\n");
    let path = fs::canonicalize(fixture.0.join("data/model.abt")).unwrap();
    let source = "schema Settings {\n n: int @public = 9007199254740993\n private: text = \"PRIVATE_BODY_SENTINEL\"\n}\n";
    let output = success(fixture.run(
        &["analyze", ".", "--stdio", "--public"],
        &frame(&[(&path, source)]),
    ));
    assert!(output.contains("\"requestId\": 17"));
    assert!(output.contains("\"status\": \"export-admitted\""));
    assert!(output.contains("\"default\": \"9007199254740993\""));
    for private in [
        "PRIVATE_BODY_SENTINEL",
        "documentJson",
        "documentSha256",
        "abstract.public-compilation",
    ] {
        assert!(!output.contains(private), "leaked {private}");
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), "invalid source");
}

#[test]
fn spaced_marker_is_language_error_while_dependency_error_is_export_only() {
    let fixture = Fixture::new("schema Settings {\n n: int @public = 1\n}\nlogic Settings {\n require .n > 0 else throw \"positive\"\n}\n", "Settings :: @id.main\n");
    let rejected = success(fixture.run(&["analyze", ".", "--stdio", "--public"], &frame(&[])));
    assert!(rejected.contains("\"status\": \"export-rejected\""));
    assert!(rejected.contains("\"diagnostics\": []"));
    assert!(rejected.contains("\"code\": \"E702\""));
    assert!(!rejected.contains("\"fragment\""));
    let path = fs::canonicalize(fixture.0.join("data/model.abt")).unwrap();
    let invalid = success(fixture.run(
        &["analyze", ".", "--stdio", "--public"],
        &frame(&[(&path, "schema Settings {\n n: int @ public = 1\n}\n")]),
    ));
    assert!(invalid.contains("\"code\": \"E210\""));
    assert!(invalid.contains("\"status\": \"unavailable\""));
    assert!(invalid.contains("\"catalogComplete\": false"));
    for key in ["sources", "declarations", "roots", "exportDiagnostics"] {
        assert!(invalid.contains(&format!("\"{key}\": []")));
    }
    assert!(!invalid.contains("\"fragment\""));
}

#[test]
fn malformed_frames_and_other_project_overlays_never_produce_an_inventory() {
    let fixture = Fixture::new(
        "schema Settings {\n n: int @public = 1\n}\n",
        "Settings :: @id.main\n",
    );
    let other = Fixture::new(
        "schema Other {\n n: int @public = 2\n}\n",
        "Other :: @id.foreign\n",
    );
    let path = fs::canonicalize(other.0.join("data/model.abt")).unwrap();
    for bytes in [b"ABANLZ02".to_vec(), frame(&[(&path, "schema Other {}\n")])] {
        let output = fixture.run(&["analyze", ".", "--stdio", "--public"], &bytes);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let other_output = success(other.run(&["analyze", ".", "--stdio", "--public"], &frame(&[])));
    assert!(other_output.contains("\"rootSchema\": \"Other\""));
    assert!(!other_output.contains("Settings"));
}
