//! Release-facing regression coverage for the filesystem boundary.
//!
//! These tests intentionally use `compile_project`, rather than the in-memory
//! compiler helper, so asset resolution and rendered JSON exercise real paths.

use abstract_lang::{compile_project, CompileOptions, ErrorId, Format};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("abstract_disk_{name}_{nanos}_{unique}"));
        fs::create_dir_all(&root).expect("project directory");
        Self { root }
    }

    fn write(&self, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("relative file has parent"))
            .expect("parent directory");
        fs::write(path, bytes).expect("project file");
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes
}

fn jpeg(width: u16, height: u16) -> Vec<u8> {
    vec![
        0xFF,
        0xD8, // SOI
        0xFF,
        0xC0,
        0x00,
        0x07, // baseline SOF, five-byte body
        0x08,
        height.to_be_bytes()[0],
        height.to_be_bytes()[1],
        width.to_be_bytes()[0],
        width.to_be_bytes()[1],
    ]
}

#[test]
fn project_compilation_checks_disk_png_and_jpg_then_renders_json() {
    let project = Project::new("images_json");
    project.write(
        "data/templates/Asset.abt",
        "schema Asset {\n    icon: image(png 3x2)\n    photo: image(jpg 3x2)\n    metadata: file(json)\n}\n",
    );
    project.write(
        "data/items/asset.ab",
        "Asset :: @id.disk\n    icon: textures/icon.png\n    photo: textures/photo.jpg\n    metadata: metadata/item.json\n",
    );
    project.write("assets/textures/icon.png", png(3, 2));
    project.write("assets/textures/photo.jpg", jpeg(3, 2));
    project.write("assets/metadata/item.json", "{\"release\": true}\n");

    let document = compile_project(&project.root, CompileOptions::default())
        .expect("real assets should satisfy their declarations");
    let json = document.render(Format::Json).expect("JSON output");
    assert!(json.contains("\"icon\": \"textures/icon.png\""), "{json}");
    assert!(json.contains("\"photo\": \"textures/photo.jpg\""), "{json}");
    assert!(
        json.contains("\"metadata\": \"metadata/item.json\""),
        "{json}"
    );
}

#[test]
fn malformed_png_header_is_an_image_content_diagnostic() {
    let project = Project::new("malformed_png");
    project.write(
        "data/templates/Asset.abt",
        "schema Asset {\n    icon: image(png 3x2)\n}\n",
    );
    project.write(
        "data/items/asset.ab",
        "Asset :: @id.disk\n    icon: icon.png\n",
    );
    let mut malformed = png(3, 2);
    malformed[12..16].copy_from_slice(b"BAD!");
    project.write("assets/icon.png", malformed);

    let diagnostics = compile_project(&project.root, CompileOptions::default())
        .expect_err("a PNG signature without IHDR is not a readable PNG");
    assert_eq!(
        diagnostics.first().map(|diagnostic| diagnostic.id),
        Some(ErrorId::E422)
    );
    assert!(diagnostics.to_string().contains("missing its IHDR chunk"));
}

#[test]
fn asset_paths_reject_lexical_escape_forms_before_disk_access() {
    for (name, value) in [
        ("parent", "../outside.png"),
        ("absolute", "/outside.png"),
        ("drive", "C:\\\\outside.png"),
    ] {
        let project = Project::new(name);
        project.write(
            "data/templates/Asset.abt",
            "schema Asset {\n    icon: file(png)\n}\n",
        );
        project.write(
            "data/items/asset.ab",
            format!("Asset :: @id.disk\n    icon: {value}\n"),
        );
        let diagnostics = compile_project(&project.root, CompileOptions::default())
            .expect_err("escaped asset path must be rejected");
        assert_eq!(
            diagnostics.first().map(|diagnostic| diagnostic.id),
            Some(ErrorId::E424)
        );
    }
}

#[cfg(unix)]
#[test]
fn source_symlink_target_is_discovered_once_under_the_documented_link_contract() {
    use std::os::unix::fs::symlink;

    let project = Project::new("source_link");
    project.write("data/templates/Asset.abt", "schema Asset {\n}\n");
    project.write("outside/linked.ab", "Asset :: @id.linked\n");
    symlink(
        project.root.join("outside"),
        project.root.join("data/linked"),
    )
    .expect("source-directory symlink");

    let document = compile_project(&project.root, CompileOptions::default())
        .expect("a linked source directory is part of discovery");
    assert!(document
        .render(Format::Json)
        .unwrap()
        .contains("\"id\": \"linked\""));
}
