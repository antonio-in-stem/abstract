//! The command line, exercised through the built binary (SPEC chapter 9).
//!
//! Every test here runs `target/…/abstract` as a child process, so it checks
//! the three things a unit test cannot: the exit code, what reached stdout and
//! what reached stderr. SPEC §11.3 requires exactly that of a conforming
//! suite — "every command of §9.2 and every reachable `E8xx` identifier,
//! exercised through the binary".
//!
//! Every test here runs: the ones that need a compiled document exercise the
//! whole pipeline of SPEC chapter 7, and the rest cover what the argument
//! parser, the project resolver and phases P0 to P3 decide on their own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use abstract_lang::bundle;

/// What one run of the binary produced.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn assert_error(&self, id: &str) -> &Self {
        assert!(
            self.stderr.contains(&format!("error[{id}]:")),
            "expected {id}, got exit {} and stderr:\n{}",
            self.code,
            self.stderr
        );
        // stdout carries only the document, the schema list or the usage text;
        // it is never mixed with diagnostics (SPEC §9.7).
        assert_eq!(self.stdout, "", "a diagnostic reached stdout");
        self
    }

    fn assert_code(&self, code: i32) -> &Self {
        assert_eq!(
            self.code, code,
            "expected exit {code}; stderr:\n{}",
            self.stderr
        );
        self
    }
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_abstract")
}

fn run_in(directory: &Path, args: &[&str]) -> Run {
    let output = Command::new(binary())
        .args(args)
        .current_dir(directory)
        .output()
        .expect("the abstract binary runs");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    }
}

/// A fresh empty directory under the system temporary directory.
fn temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("abstract_cli_{name}_{suffix}"));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temporary directory");
    path
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    fs::write(path, text).expect("write");
}

/// A minimal two-schema project in the `data/` layout of SPEC §2.3.
fn sample_project(name: &str) -> PathBuf {
    let root = temp_dir(name);
    write(
        &root,
        "data/templates/zebra.abt",
        "schema Zebra {\n    id: text(1..40)\n    stripes: int(0..99)\n}\n",
    );
    write(
        &root,
        "data/templates/apple.abt",
        "schema Apple {\n    id: text(1..40)\n    ripe: bool\n}\n",
    );
    write(
        &root,
        "data/items/one.ab",
        "Zebra :: @id.one\n    stripes: 12\n",
    );
    root
}

#[test]
fn public_capability_is_explicit_and_does_not_accept_extra_arguments() {
    let root = temp_dir("public_capability");
    let run = run_in(&root, &["public-contract", "--capabilities"]);
    run.assert_code(0);
    assert!(run.stdout.contains("abstract-public-compilation"));
    assert!(run.stdout.contains("independent-scalars-v1"));
    assert!(run.stdout.contains("unbound"));
    assert!(run.stderr.is_empty());
    run_in(&root, &["public-contract", "--capabilities", "."])
        .assert_error("E802")
        .assert_code(2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn public_output_is_one_private_envelope_and_file_matches_stdout() {
    let root = temp_dir("public_output");
    write(
        &root,
        "data/model.abt",
        "schema Model {\ncount: int @public = 9007199254740993\n}\n",
    );
    write(&root, "data/item.ab", "Model :: @id.item\n");
    let run = run_in(&root, &["public-contract", "."]);
    run.assert_code(0);
    assert!(run.stderr.is_empty());
    for key in [
        "documentJson",
        "documentSha256",
        "publicFragment",
        "unbound",
    ] {
        assert!(run.stdout.contains(key), "missing {key}");
    }
    let saved = run_in(
        &root,
        &["public-contract", ".", "--out", "public contract.json"],
    );
    saved.assert_code(0);
    assert!(saved.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("public contract.json")).unwrap(),
        run.stdout
    );
    let before = fs::read(root.join("data/model.abt")).unwrap();
    run_in(&root, &["public-contract", ".", "--out", "data/model.abt"])
        .assert_error("E808")
        .assert_code(2);
    assert_eq!(fs::read(root.join("data/model.abt")).unwrap(), before);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unsupported_public_dependency_emits_no_partial_contract_and_preserves_output() {
    let root = temp_dir("public_dependency");
    write(&root, "data/model.abt", "schema Model {\ncount: int @public = 5\n}\nlogic Model {\nrequire .count > 0 else throw \"positive\"\n}\n");
    write(&root, "data/item.ab", "Model :: @id.item\n");
    write(&root, "existing.json", "previous successful output\n");
    run_in(&root, &["compile", "."]).assert_code(0);
    run_in(&root, &["public-contract", ".", "--out", "existing.json"])
        .assert_error("E702")
        .assert_code(1);
    assert_eq!(
        fs::read_to_string(root.join("existing.json")).unwrap(),
        "previous successful output\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn public_file_selection_emits_only_selected_targets_but_validates_the_project() {
    let root = temp_dir("public_selection");
    write(&root, "data/model.abt", "schema Good {\ncount: int @public = 5\n}\nschema Bad {\ncounts[]: int @public = [1, 2]\n}\n");
    write(&root, "data/good.ab", "Good :: @id.chosen\n");
    write(&root, "data/bad.ab", "Bad :: @id.excluded\n");
    let chosen = run_in(&root, &["public-contract", "data/good.ab"]);
    chosen.assert_code(0);
    assert!(chosen.stdout.contains("chosen"));
    assert!(!chosen.stdout.contains("excluded"));
    run_in(&root, &["public-contract", "."])
        .assert_error("E702")
        .assert_code(1);
    write(
        &root,
        "data/bad.ab",
        "Bad :: @id.excluded\ncounts: [invalid]\n",
    );
    run_in(&root, &["public-contract", "data/good.ab"])
        .assert_error("E412")
        .assert_code(1);
    let _ = fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------
// help and version (SPEC §9.2)
// ---------------------------------------------------------------------------

#[test]
fn help_prints_usage_to_stdout_and_exits_zero() {
    let root = temp_dir("help");
    for spelling in ["--help", "-h", "help"] {
        let run = run_in(&root, &[spelling]);
        run.assert_code(0);
        assert!(
            run.stdout.starts_with("abstract <command>"),
            "usage went missing for {spelling}: {}",
            run.stdout
        );
        assert!(run.stdout.contains("--max-errors"));
        assert_eq!(run.stderr, "");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn version_prints_the_compiler_version_to_stdout() {
    let root = temp_dir("version");
    let expected = format!("abstract {}\n", abstract_lang::COMPILER_VERSION);
    for spelling in ["--version", "-V", "version"] {
        let run = run_in(&root, &[spelling]);
        run.assert_code(0);
        assert_eq!(run.stdout, expected);
        assert_eq!(run.stderr, "");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn help_and_version_take_no_further_tokens() {
    let root = temp_dir("help_extra");
    run_in(&root, &["help", "compile"])
        .assert_error("E804")
        .assert_code(2);
    run_in(&root, &["--version", "x"])
        .assert_error("E804")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// E801, E802, E803, E804 — the argument parser (SPEC §9.1, §9.3)
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_command_is_e801() {
    let root = temp_dir("e801");
    let run = run_in(&root, &["frobnicate"]);
    run.assert_error("E801").assert_code(2);
    assert!(run
        .stderr
        .contains("Unknown command 'frobnicate'; expected compile, lint, templates or init."));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn no_command_word_at_all_is_a_usage_error() {
    let root = temp_dir("e801_empty");
    // There is no direct form in 1.0: a command word is always required
    // (SPEC §9.1, Appendix A53).
    run_in(&root, &[]).assert_error("E801").assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unknown_flag_is_e802_and_is_never_ignored() {
    let root = sample_project("e802");
    run_in(&root, &["compile", "data", "JSON", "--skip-asset"])
        .assert_error("E802")
        .assert_code(2);
    run_in(&root, &["compile", "data", "--nonsense"])
        .assert_error("E802")
        .assert_code(2);
    // Appendix A71: 0.2.0 accepted and ignored these two on `templates`.
    run_in(&root, &["templates", "data", "--skip-assets"])
        .assert_error("E802")
        .assert_code(2);
    run_in(&root, &["templates", "data", "--allow-unknown"])
        .assert_error("E802")
        .assert_code(2);
    // `lint` renders nothing, so it has no --out.
    run_in(&root, &["lint", "data", "--out", "x.json"])
        .assert_error("E802")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_flag_without_a_value_is_e803() {
    let root = sample_project("e803");
    run_in(&root, &["compile", "data", "--out"])
        .assert_error("E803")
        .assert_code(2);
    run_in(&root, &["compile", "data", "--out="])
        .assert_error("E803")
        .assert_code(2);
    // A flag value may not begin with '-', so this is a missing value and not
    // a request to write a file named '--skip-assets' (SPEC §9.3).
    run_in(&root, &["compile", "data", "--out", "--skip-assets"])
        .assert_error("E803")
        .assert_code(2);
    run_in(&root, &["compile", "data", "--max-errors", "-1"])
        .assert_error("E803")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_extra_positional_is_e804_where_one_is_expected() {
    let root = sample_project("e804");
    run_in(&root, &["templates", "data", "data"])
        .assert_error("E804")
        .assert_code(2);
    run_in(&root, &["init", "a", "b"])
        .assert_error("E804")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// E805, E806, E807 — inputs and the FORMAT keyword (SPEC §9.2)
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_format_keyword_is_e805() {
    let root = sample_project("e805");
    for keyword in ["XML", "RAWW", "jsonn"] {
        run_in(&root, &["compile", "data", keyword])
            .assert_error("E805")
            .assert_code(2);
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn every_format_keyword_is_accepted_in_any_case() {
    let root = sample_project("formats");
    for keyword in ["JSON", "json", "YML", "yaml", "YAML", "RAW", "raw"] {
        let run = run_in(&root, &["compile", "data", keyword]);
        assert!(
            !run.stderr.contains("error[E805]"),
            "{keyword} was refused as a format"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_sole_positional_is_always_an_input_path() {
    let root = temp_dir("sole");
    // `abstract compile json` compiles the directory `json`; a bad spelling
    // there is E806, never E805 (SPEC §9.2).
    run_in(&root, &["compile", "json"])
        .assert_error("E806")
        .assert_code(2);
    run_in(&root, &["compile", "raw"])
        .assert_error("E806")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_missing_input_path_is_e806() {
    let root = sample_project("e806");
    for command in ["compile", "lint", "templates", "init"] {
        run_in(&root, &[command])
            .assert_error("E806")
            .assert_code(2);
    }
    run_in(&root, &["compile", "nosuch"])
        .assert_error("E806")
        .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_directory_inside_the_data_directory_is_e806() {
    let root = sample_project("e806_inside");
    let run = run_in(&root, &["compile", "data/items"]);
    run.assert_error("E806").assert_code(2);
    assert!(run
        .stderr
        .contains("note: name the project root or the 'data' directory."));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn mixing_files_and_directories_is_e807() {
    let root = sample_project("e807");
    let run = run_in(&root, &["compile", "data", "data/items/one.ab"]);
    run.assert_error("E807").assert_code(2);
    assert!(run
        .stderr
        .contains("Cannot mix files and directories in one invocation."));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// E808, E810, E811, E812, E813 — destinations, flag values and init
// ---------------------------------------------------------------------------

#[test]
fn out_refuses_a_destination_the_next_run_would_read() {
    let root = sample_project("e808");
    let source = run_in(
        &root,
        &["compile", "data", "--out", "data/templates/zebra.abt"],
    );
    source.assert_error("E808").assert_code(2);
    assert!(source.stderr.contains("it is a source file."));

    let inside = run_in(&root, &["compile", "data", "--out", "data/out.json"]);
    inside.assert_error("E808").assert_code(2);
    assert!(inside.stderr.contains("it is inside the data directory."));

    // The project root is never refused (SPEC §9.4).
    let allowed = run_in(&root, &["compile", "data", "--out", "out.json"]);
    assert!(!allowed.stderr.contains("error[E808]"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn out_refuses_a_source_extension_inside_a_flat_discovery_root() {
    // A project with no data/ directory discovers its own root, so a `.ab`
    // file written there would be read on the next run (SPEC §9.4).
    let root = temp_dir("e808_flat");
    write(
        &root,
        "Zebra.abt",
        "schema Zebra {\n    id: text(1..40)\n}\n",
    );
    write(&root, "one.ab", "Zebra :: @id.one\n");

    for destination in ["out.ab", "out.ABT"] {
        let run = run_in(&root, &["compile", ".", "--out", destination]);
        run.assert_error("E808").assert_code(2);
        assert!(run.stderr.contains("would be read on the next run."));
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_missing_output_directory_is_e810_and_exits_three() {
    let root = sample_project("e810");
    let run = run_in(&root, &["compile", "data", "--out", "nosuchdir/out.json"]);
    run.assert_error("E810").assert_code(3);
    assert!(run.stderr.contains("the parent directory does not exist."));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_repeated_flag_is_e811() {
    let root = sample_project("e811");
    run_in(
        &root,
        &["compile", "data", "--out", "a.json", "--out", "b.json"],
    )
    .assert_error("E811")
    .assert_code(2);
    run_in(
        &root,
        &["compile", "data", "--skip-assets", "--skip-assets"],
    )
    .assert_error("E811")
    .assert_code(2);
    run_in(
        &root,
        &["lint", "data", "--max-errors", "1", "--max-errors=2"],
    )
    .assert_error("E811")
    .assert_code(2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_invalid_flag_value_is_e812() {
    let root = sample_project("e812");
    for value in ["x", "1.5", "--max-errors=+1", "--max-errors=-1"] {
        let token = if value.starts_with("--") {
            value.to_string()
        } else {
            format!("--max-errors={value}")
        };
        run_in(&root, &["compile", "data", &token])
            .assert_error("E812")
            .assert_code(2);
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn init_refuses_a_target_that_is_not_empty() {
    let root = sample_project("e813");
    let run = run_in(&root, &["init", "."]);
    run.assert_error("E813").assert_code(2);
    assert!(run.stderr.contains("'.' is not empty."));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// init (SPEC §9.2)
// ---------------------------------------------------------------------------

#[test]
fn init_writes_the_documented_scaffold() {
    let root = temp_dir("init");
    let run = run_in(&root, &["init", "sample"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "", "init writes nothing to stdout");
    assert!(run.stderr.contains("abstract: created sample"));

    let project = root.join("sample");
    for relative in [
        "data/templates/catalog.abt",
        "data/packs/starter.ab",
        "data/items/starter_sword.ab",
        "data/items/practice_sword.ab",
        "assets/textures/icon.png",
    ] {
        assert!(project.join(relative).is_file(), "{relative} is missing");
    }

    let schema = fs::read_to_string(project.join("data/templates/catalog.abt")).expect("read");
    for construct in [
        "versions 1..2",
        "text(",
        "int(",
        "float(",
        "bool",
        "enum(",
        "ref(Pack)",
        "@optional",
        "@tag",
        "logic Item {",
        "require ",
        "derive ",
    ] {
        assert!(schema.contains(construct), "{construct} is missing");
    }
    assert!(
        !schema.contains('\r'),
        "the scaffold must be written with LF"
    );

    let clone = fs::read_to_string(project.join("data/items/practice_sword.ab")).expect("read");
    assert!(
        clone.contains("&starter_sword.*"),
        "the second instance clones the first"
    );

    // The icon is a real 16x16 PNG, not a placeholder byte string.
    let icon = fs::read(project.join("assets/textures/icon.png")).expect("read icon");
    assert_eq!(
        &icon[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
    assert_eq!(
        u32::from_be_bytes([icon[16], icon[17], icon[18], icon[19]]),
        16
    );
    assert_eq!(
        u32::from_be_bytes([icon[20], icon[21], icon[22], icon[23]]),
        16
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_scaffold_declares_the_schemas_it_documents() {
    // `templates` runs P0 to P3, so this also proves the scaffold parses, its
    // project tables build and its schemas and logic validate (SPEC §9.2).
    let root = temp_dir("init_templates");
    run_in(&root, &["init", "sample"]).assert_code(0);

    let run = run_in(&root, &["templates", "sample"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "Item\nPack\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_fresh_scaffold_compiles_and_lints_with_no_flags() {
    // SPEC §9.2 requires exactly this of `abstract init`.
    let root = temp_dir("init_compiles");
    run_in(&root, &["init", "sample"]).assert_code(0);

    let compiled = run_in(&root, &["compile", "sample"]);
    compiled.assert_code(0);
    assert!(compiled.stdout.starts_with('{'));
    assert!(compiled.stdout.contains("\"starter_sword\""));

    let linted = run_in(&root, &["lint", "sample"]);
    linted.assert_code(0);
    assert_eq!(linted.stdout, "");
    assert_eq!(linted.stderr, "abstract: ok\n");
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// templates (SPEC §9.2)
// ---------------------------------------------------------------------------

#[test]
fn templates_prints_schema_names_in_scalar_order() {
    let root = sample_project("templates");
    let run = run_in(&root, &["templates", "data"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "Apple\nZebra\n");
    assert_eq!(run.stderr, "");

    // Every spelling of the project resolves the same source set (SPEC §2.3).
    let from_root = run_in(&root, &["templates", "."]);
    assert_eq!(from_root.stdout, run.stdout);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn templates_fails_on_a_broken_project_instead_of_printing_a_partial_list() {
    // Appendix A61: 0.2.0 ignored duplicate-schema errors and printed anyway.
    let root = temp_dir("templates_broken");
    write(
        &root,
        "data/a.abt",
        "schema Zebra {\n    id: text(1..40)\n}\n",
    );
    write(
        &root,
        "data/b.abt",
        "schema Zebra {\n    id: text(1..40)\n}\n",
    );

    let run = run_in(&root, &["templates", "data"]);
    run.assert_error("E301").assert_code(1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn templates_runs_the_instance_checks_of_p2() {
    let root = temp_dir("templates_instances");
    write(
        &root,
        "data/Zebra.abt",
        "schema Zebra {\n    id: text(1..40)\n}\n",
    );
    write(&root, "data/one.ab", "Zebra :: @id.same\n");
    write(&root, "data/two.ab", "Zebra :: @id.same\n");

    run_in(&root, &["templates", "data"])
        .assert_error("E402")
        .assert_code(1);
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// stdout, stderr and broken pipes (SPEC §9.7)
// ---------------------------------------------------------------------------

#[test]
fn a_reader_that_closes_stdout_never_crashes_the_process() {
    // Appendix A60: 0.2.0 panicked with exit 101 when piped into `head`.
    let root = temp_dir("broken_pipe");
    let mut child = Command::new(binary())
        .arg("--help")
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    drop(child.stdout.take());
    let status = child.wait().expect("wait");
    assert_eq!(
        status.code(),
        Some(0),
        "a closed reader must end the process quietly with code 0"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_usage_error_writes_nothing_to_stdout() {
    let root = sample_project("streams");
    for args in [
        vec!["compile", "data", "XML"],
        vec!["compile", "nosuch"],
        vec!["templates", "data", "extra"],
        vec!["nonsense"],
    ] {
        let run = run_in(&root, &args);
        assert_eq!(run.stdout, "", "stdout was written for {args:?}");
        assert!(!run.stderr.is_empty(), "stderr was empty for {args:?}");
    }
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Optional tooling: bundle, unbundle and keygen (SPEC §9.9, Appendix D.1)
// ---------------------------------------------------------------------------

const SAMPLE_DOCUMENT: &str = "{\n  \"data\": []\n}\n";

#[test]
fn unbundle_reads_a_plain_container() {
    let root = temp_dir("unbundle_plain");
    fs::write(
        root.join("data.abx"),
        bundle::encode(SAMPLE_DOCUMENT.as_bytes(), None),
    )
    .expect("write container");

    let run = run_in(&root, &["unbundle", "data.abx"]);
    run.assert_code(0);
    assert_eq!(run.stdout, SAMPLE_DOCUMENT);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unbundle_opens_a_sealed_container_only_with_its_key() {
    let root = temp_dir("unbundle_sealed");
    let key = bundle::derive_key("release passphrase").expect("key");
    fs::write(
        root.join("data.abx"),
        bundle::encode(SAMPLE_DOCUMENT.as_bytes(), Some(&key)),
    )
    .expect("write container");

    let opened = run_in(
        &root,
        &["unbundle", "data.abx", "--key", "release passphrase"],
    );
    opened.assert_code(0);
    assert_eq!(opened.stdout, SAMPLE_DOCUMENT);

    let wrong = run_in(&root, &["unbundle", "data.abx", "--key", "other"]);
    wrong.assert_code(1);
    assert_eq!(wrong.stdout, "");

    let keyless = run_in(&root, &["unbundle", "data.abx"]);
    keyless.assert_code(1);
    assert!(keyless.stderr.contains("a key is required"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unbundle_refuses_a_key_against_a_container_that_declares_itself_plain() {
    // Appendix D.1 item 4.
    let root = temp_dir("unbundle_downgrade");
    fs::write(
        root.join("data.abx"),
        bundle::encode(SAMPLE_DOCUMENT.as_bytes(), None),
    )
    .expect("write container");

    let run = run_in(&root, &["unbundle", "data.abx", "--key", "secret"]);
    run.assert_code(1);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("downgrade"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn bundle_refuses_a_key_together_with_plain_and_writes_nothing() {
    // Appendix D.1 item 1: the two flags are a usage error, a supplied key is
    // never silently discarded, and no unencrypted container is written while
    // a key was given. `bundle` is outside the language (SPEC §9.9), so the
    // refusal carries no chapter 10 identifier; the exit code is the usage
    // one and the destination must not exist afterwards.
    let root = sample_project("bundle_key_and_plain");
    let run = run_in(
        &root,
        &[
            "bundle",
            "data",
            "--key",
            "release passphrase",
            "--plain",
            "--out",
            "both.abx",
        ],
    );
    run.assert_code(2);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("--plain"), "{}", run.stderr);
    assert!(run.stderr.contains("--key"), "{}", run.stderr);
    assert!(!run.stderr.contains("error["), "{}", run.stderr);
    assert!(!root.join("both.abx").exists(), "a container was written");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unbundle_writes_to_out_and_says_so() {
    let root = temp_dir("unbundle_out");
    fs::write(
        root.join("data.abx"),
        bundle::encode(SAMPLE_DOCUMENT.as_bytes(), None),
    )
    .expect("write container");

    let run = run_in(&root, &["unbundle", "data.abx", "--out", "data.json"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "", "--out suppresses stdout entirely");
    assert!(run.stderr.contains("abstract: wrote data.json (JSON)"));
    let written = fs::read_to_string(root.join("data.json")).expect("read back");
    assert_eq!(written.replace("\r\n", "\n"), SAMPLE_DOCUMENT);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn bundle_refuses_a_key_together_with_plain() {
    // Appendix D.1 item 1: a supplied key is never silently discarded.
    let root = sample_project("bundle_flags");
    let run = run_in(&root, &["bundle", "data", "--key", "secret", "--plain"]);
    run.assert_code(2);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.contains("cannot be combined"));

    // Appendix D.1 item 2: a flag value never begins with '-'.
    let swallowed = run_in(&root, &["bundle", "data", "--key", "--out", "x.abx"]);
    swallowed.assert_code(2);
    assert!(swallowed.stderr.contains("Flag '--key' requires a value."));
    assert!(!root.join("x.abx").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn keygen_prints_only_cli_ready_key_material() {
    let root = temp_dir("keygen");
    let run = run_in(&root, &["keygen"]);
    run.assert_code(0);
    assert_eq!(run.stderr, "");
    let material = run.stdout.trim_end();
    assert!(material.starts_with("hex:"));
    assert_eq!(material.len(), 68);
    assert!(material[4..].bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(bundle::parse_key(material).is_ok());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn bundle_refuses_legacy_passphrases_and_writes_nothing() {
    let root = sample_project("bundle_legacy_passphrase");
    let run = run_in(
        &root,
        &[
            "bundle",
            "data",
            "--key",
            "legacy passphrase",
            "--out",
            "data.abx",
        ],
    );
    run.assert_code(2);
    assert_eq!(run.stdout, "");
    assert!(!run.stderr.contains("legacy passphrase"));
    assert!(run.stderr.contains("hex:"));
    assert!(!root.join("data.abx").exists());
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Waiting on the compilation pipeline (SPEC chapter 7)
// ---------------------------------------------------------------------------

#[test]
fn compile_renders_the_document_to_stdout() {
    let root = sample_project("compile_stdout");
    let run = run_in(&root, &["compile", "data"]);
    run.assert_code(0);
    assert!(run.stdout.starts_with("{\n  \"abstract\": {"));
    assert!(run.stdout.ends_with("}\n"));
    assert_eq!(run.stderr, "");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn compile_writes_exactly_the_stdout_bytes_to_out() {
    let root = sample_project("compile_out");
    let piped = run_in(&root, &["compile", "data", "JSON"]);
    piped.assert_code(0);

    let written = run_in(&root, &["compile", "data", "JSON", "--out", "out.json"]);
    written.assert_code(0);
    assert_eq!(written.stdout, "", "--out suppresses stdout entirely");
    assert!(written.stderr.contains("abstract: wrote out.json (JSON)"));

    let bytes = fs::read_to_string(root.join("out.json")).expect("read back");
    assert_eq!(bytes.replace("\r\n", "\n"), piped.stdout);

    // Both spellings of the flag behave identically (SPEC §9.3, A56).
    run_in(&root, &["compile", "data", "JSON", "--out=other.json"]).assert_code(0);
    assert_eq!(
        fs::read_to_string(root.join("other.json")).expect("read back"),
        fs::read_to_string(root.join("out.json")).expect("read back")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_project_with_schemas_and_no_instances_compiles_to_an_empty_document() {
    // SPEC §8.1: a whole-project compile of a project whose sources declare
    // schemas but no instance is the second way to an empty `data` array,
    // and it is a success. SPEC §7.2 checks every schema and logic block
    // against no instance at all, so `lint` reports ok.
    let root = temp_dir("schemas_only");
    write(
        &root,
        "data/templates/item.abt",
        "schema Item {\n    name: text(1..40)\n    ready: bool @optional\n}\n\
         logic Item {\n    if .name == \"a\" {\n        derive .ready = true\n    }\n}\n",
    );

    let linted = run_in(&root, &["lint", "data"]);
    linted.assert_code(0);
    assert_eq!(linted.stdout, "");
    assert_eq!(linted.stderr, "abstract: ok\n");

    let compiled = run_in(&root, &["compile", "data", "JSON"]);
    compiled.assert_code(0);
    assert!(
        compiled.stdout.contains("\"data\": []"),
        "{}",
        compiled.stdout
    );
    assert!(
        compiled.stdout.contains("\"overlays\": []"),
        "{}",
        compiled.stdout
    );

    // A project with no source files at all is the third case, and it is
    // E103 rather than an empty document (SPEC §2.4, §8.1).
    let bare = temp_dir("no_sources");
    write(&bare, "data/readme.txt", "not a source\n");
    let refused = run_in(&bare, &["compile", "data", "JSON"]);
    refused.assert_code(1);
    assert!(refused.stderr.contains("error[E103]"), "{}", refused.stderr);
    let _ = fs::remove_dir_all(&bare);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn lint_prints_ok_to_stderr_and_nothing_to_stdout() {
    let root = sample_project("lint_ok");
    let run = run_in(&root, &["lint", "data"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr, "abstract: ok\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn each_format_keyword_selects_its_renderer() {
    let root = sample_project("formats_render");
    let json = run_in(&root, &["compile", "data", "JSON"]);
    let yaml = run_in(&root, &["compile", "data", "YML"]);
    let raw = run_in(&root, &["compile", "data", "RAW"]);
    json.assert_code(0);
    yaml.assert_code(0);
    raw.assert_code(0);
    assert!(json.stdout.starts_with('{'));
    // Every YAML mapping key is double-quoted, always (SPEC §8.5).
    assert!(yaml.stdout.starts_with("\"abstract\":"), "{}", yaml.stdout);
    assert_ne!(json.stdout, raw.stdout);
    // YML and YAML name one format (SPEC §9.2).
    assert_eq!(
        yaml.stdout,
        run_in(&root, &["compile", "data", "YAML"]).stdout
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skip_assets_never_changes_the_compiled_bytes() {
    // SPEC §9.5: the flag changes which errors are reported, never what is
    // emitted.
    let root = sample_project("skip_assets");
    let plain = run_in(&root, &["compile", "data", "JSON"]);
    let skipped = run_in(&root, &["compile", "data", "JSON", "--skip-assets"]);
    plain.assert_code(0);
    skipped.assert_code(0);
    assert_eq!(plain.stdout, skipped.stdout);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn max_errors_bounds_reporting_and_names_the_limit() {
    let root = temp_dir("max_errors");
    write(
        &root,
        "data/Zebra.abt",
        "schema Zebra {\n    id: text(1..40)\n    a: int(0..9)\n    b: int(0..9)\n}\n",
    );
    write(
        &root,
        "data/one.ab",
        "Zebra :: @id.one\n    a: 40\n    b: 40\n",
    );

    let limited = run_in(&root, &["compile", "data", "--max-errors", "1"]);
    limited.assert_code(1);
    assert!(limited
        .stderr
        .contains("abstract: note: stopping after 1 errors; more may remain."));
    // The limit never changes the exit code or which diagnostic comes first.
    let full = run_in(&root, &["compile", "data"]);
    full.assert_code(1);
    assert_eq!(
        full.stderr.lines().next(),
        limited.stderr.lines().next(),
        "the first diagnostic must not depend on --max-errors"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn every_spelling_of_the_project_root_compiles_to_the_same_bytes() {
    // SPEC §7.6: the same project from its root and from its data directory.
    let root = sample_project("determinism");
    let from_root = run_in(&root, &["compile", ".", "JSON"]);
    let from_data = run_in(&root, &["compile", "data", "JSON"]);
    from_root.assert_code(0);
    from_data.assert_code(0);
    assert_eq!(from_root.stdout, from_data.stdout);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn bundle_seals_a_compiled_document_and_unbundle_opens_it() {
    let root = sample_project("bundle_round_trip");
    let key = "hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    let sealed = run_in(
        &root,
        &["bundle", "data", "--key", key, "--out", "data.abx"],
    );
    sealed.assert_code(0);
    assert!(sealed.stderr.contains("sealed"));

    let opened = run_in(&root, &["unbundle", "data.abx", "--key", key]);
    opened.assert_code(0);
    let compiled = run_in(&root, &["compile", "data", "JSON"]);
    assert_eq!(opened.stdout, compiled.stdout);
    let _ = fs::remove_dir_all(&root);
}
