//! Project resolution and source discovery (SPEC §2.3, §2.4, §2.5).
//!
//! A command-line root resolves to a project root and a discovery root by
//! walking upwards for a `data` directory marker. The discovery root is then
//! walked recursively, skipping dot-directories and the four vendor names,
//! resolving links and de-duplicating by canonical path.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId, Note};
use crate::source::{has_extension, normalise_display_path, SourceFile};

/// Directory names the walker always skips, compared case-insensitively
/// (SPEC §2.4). A directory whose final component begins with `.` is skipped
/// as well.
pub const IGNORED_DIRECTORIES: [&str; 4] = ["node_modules", "target", "build", "out"];

/// The case-insensitive marker that names a data directory (SPEC §2.3).
pub const DATA_MARKER: &str = "data";

/// The assets directory name under the project root (SPEC §2.3).
pub const ASSETS_DIR: &str = "assets";

/// Where the sources of one compilation live.
#[derive(Clone, Debug)]
pub struct ProjectLayout {
    /// The parent of the data directory, or the compiled directory when the
    /// project has none.
    pub project_root: PathBuf,
    /// The directory the walk starts from: the data directory when there is
    /// one, otherwise the project root.
    pub discovery_root: PathBuf,
    /// The data directory, when the project has one.
    pub data_dir: Option<PathBuf>,
    /// `<project root>/assets`. It need not exist.
    pub assets_dir: PathBuf,
    /// Every collected source, sorted by project-root-relative path.
    pub sources: Vec<SourceFile>,
    /// Single-file mode: the project-relative paths whose instances are
    /// emitted. `None` means the whole project is emitted (SPEC §2.5).
    pub selected: Option<Vec<String>>,
}

impl ProjectLayout {
    /// True when only the instances of named files are emitted.
    pub fn is_single_file(&self) -> bool {
        self.selected.is_some()
    }
}

/// True when the walker must skip a directory with this final component
/// (SPEC §2.4).
pub fn is_ignored_directory(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    IGNORED_DIRECTORIES
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

/// True when a file name ends with `.ab` or `.abt`, compared
/// case-insensitively (SPEC §2.1).
pub fn is_source_file_name(name: &str) -> bool {
    has_extension(name, "ab") || has_extension(name, "abt")
}

/// True when a directory's final component is the `data` marker, compared
/// case-insensitively (SPEC §2.3).
pub fn is_data_marker(name: &str) -> bool {
    name.eq_ignore_ascii_case(DATA_MARKER)
}

/// Orders collected sources by their project-root-relative path with `/` as
/// the separator, compared as a sequence of Unicode scalar values
/// (SPEC §2.4). This order is used for diagnostics and for nothing else.
pub fn sort_sources(sources: &mut [SourceFile]) {
    sources.sort_by(|a, b| a.path.cmp(&b.path));
}

/// Where one command-line root places the project (SPEC §2.3).
#[derive(Clone, Debug, PartialEq, Eq)]
struct RootLayout {
    project_root: PathBuf,
    discovery_root: PathBuf,
    data_dir: Option<PathBuf>,
}

/// Resolves one or more command-line roots to a project layout and collects
/// its sources (SPEC §2.3, §2.4, §2.5).
///
/// The roots are either exactly one directory or one or more `.ab` files;
/// mixing them, or naming paths that resolve to different project roots, is
/// E807 (SPEC §9.2).
pub fn resolve(roots: &[PathBuf]) -> Result<ProjectLayout, Diagnostics> {
    resolve_with_overlays(roots, &HashMap::new())
}

/// Editor analysis substitutes source bytes at discovery time, before disk
/// UTF-8 decoding. Keys are canonical paths validated by `analysis`.
pub(crate) fn resolve_with_overlays(
    roots: &[PathBuf],
    overlays: &HashMap<PathBuf, String>,
) -> Result<ProjectLayout, Diagnostics> {
    if roots.is_empty() {
        return Err(Diagnostics::one(Diagnostic::new(
            ErrorId::E806,
            "No input path was given.",
        )));
    }

    let mut directories: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for root in roots {
        let written = normalise_display_path(&root.to_string_lossy());
        let metadata = fs::metadata(root).map_err(|_| {
            Diagnostics::one(Diagnostic::new(
                ErrorId::E806,
                format!("Input not found: '{written}'."),
            ))
        })?;
        if metadata.is_dir() {
            directories.push((written, root.clone()));
        } else {
            files.push((written, root.clone()));
        }
    }

    if !directories.is_empty() && !files.is_empty() {
        return Err(Diagnostics::one(Diagnostic::new(
            ErrorId::E807,
            "Cannot mix files and directories in one invocation.",
        )));
    }

    for (written, _) in &files {
        if has_extension(written, "abt") {
            return Err(Diagnostics::one(Diagnostic::new(
                ErrorId::E806,
                format!("'{written}' is a template file and declares no instances."),
            )));
        }
        if !has_extension(written, "ab") {
            return Err(Diagnostics::one(Diagnostic::new(
                ErrorId::E806,
                format!("'{written}' is not an .ab or .abt file."),
            )));
        }
    }

    let named = if directories.is_empty() {
        &files
    } else {
        &directories
    };

    let mut layout: Option<(String, RootLayout)> = None;
    for (written, path) in named {
        let resolved =
            resolve_root(path, written, !directories.is_empty()).map_err(Diagnostics::one)?;
        match &layout {
            None => layout = Some((written.clone(), resolved)),
            Some((first_written, first)) if first.project_root != resolved.project_root => {
                return Err(Diagnostics::one(Diagnostic::new(
                    ErrorId::E807,
                    format!(
                        "Inputs belong to different projects: '{first_written}' and '{written}'."
                    ),
                )));
            }
            Some(_) => {}
        }
    }
    if directories.len() > 1 {
        let (written, _) = &directories[1];
        return Err(Diagnostics::one(Diagnostic::new(
            ErrorId::E804,
            format!("Unexpected argument '{written}'."),
        )));
    }

    let (root_written, resolved) = layout.expect("at least one root");
    let mut sources =
        discover_with_overlays(&resolved.project_root, &resolved.discovery_root, overlays)?;

    // A named file is always a source, even when the walk skipped the
    // directory that holds it: discovery collects the project, the named
    // files select the output (SPEC §2.5).
    let mut errors = Diagnostics::new();
    let mut selected: Vec<String> = Vec::new();
    for (_, path) in &files {
        let display = display_path(&resolved.project_root, &canonical_or_self(path));
        if !contains_file(&sources, path) {
            match read_source(path, &canonical_or_self(path), display.clone(), overlays) {
                Ok(source) => sources.push(source),
                Err(diagnostic) => errors.push(diagnostic),
            }
        }
        selected.push(display);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    sort_sources(&mut sources);
    if sources.is_empty() {
        return Err(Diagnostics::one(Diagnostic::new(
            ErrorId::E103,
            format!("No .ab or .abt files were found under '{root_written}'."),
        )));
    }

    selected.sort();
    selected.dedup();
    Ok(ProjectLayout {
        assets_dir: resolved.project_root.join(ASSETS_DIR),
        project_root: resolved.project_root,
        discovery_root: resolved.discovery_root,
        data_dir: resolved.data_dir,
        sources,
        selected: if files.is_empty() {
            None
        } else {
            Some(selected)
        },
    })
}

/// Applies SPEC §2.3 steps 1 to 5 to one command-line root.
fn resolve_root(root: &Path, written: &str, is_directory: bool) -> Result<RootLayout, Diagnostic> {
    let absolute = fs::canonicalize(root).map_err(|error| {
        Diagnostic::new(
            ErrorId::E806,
            format!("Input not found: '{written}': {error}."),
        )
    })?;
    let start = if is_directory {
        absolute.clone()
    } else {
        absolute
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| absolute.clone())
    };

    // Step 2: the walk goes upwards only; no directory below is examined.
    let mut data_dir = None;
    let mut current = Some(start.as_path());
    while let Some(directory) = current {
        if directory
            .file_name()
            .map(|name| is_data_marker(&name.to_string_lossy()))
            .unwrap_or(false)
        {
            data_dir = Some(directory.to_path_buf());
            break;
        }
        current = directory.parent();
    }

    // Step 3: exactly one further path, and only for a named directory.
    if data_dir.is_none() && is_directory {
        data_dir = data_child(&absolute, written)?;
    }

    let Some(data) = data_dir else {
        return Ok(RootLayout {
            project_root: start.clone(),
            discovery_root: start,
            data_dir: None,
        });
    };

    let project_root = data
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| data.clone());
    // Step 5: a named directory is the data directory or the project root.
    if is_directory && absolute != data && absolute != project_root {
        let extra = component_count(&absolute).saturating_sub(component_count(&data));
        let data_written = trim_components(written, extra);
        return Err(Diagnostic::new(
            ErrorId::E806,
            format!("'{written}' is inside the data directory '{data_written}'."),
        )
        .with_note(Note::new("name the project root or the 'data' directory.")));
    }
    Ok(RootLayout {
        project_root,
        discovery_root: data.clone(),
        data_dir: Some(data),
    })
}

/// SPEC §2.3 step 3: the child of a named directory whose final component is
/// the `data` marker. More than one such child is E806; nothing is chosen by
/// sort order.
fn data_child(root: &Path, written: &str) -> Result<Option<PathBuf>, Diagnostic> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(None);
    };
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_data_marker(&name) {
            continue;
        }
        let path = entry.path();
        if fs::metadata(&path)
            .map(|meta| meta.is_dir())
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(canonical_or_self(&found[0]))),
        _ => Err(Diagnostic::new(
            ErrorId::E806,
            format!("'{written}' contains more than one 'data' directory."),
        )),
    }
}

/// Walks a discovery root and collects every `.ab` and `.abt` file
/// (SPEC §2.4).
///
/// Directory entries are visited in name order, so that the walk — and with it
/// the path kept for a file reachable by two names — never depends on
/// filesystem enumeration order (P3).
pub fn discover(
    project_root: &Path,
    discovery_root: &Path,
) -> Result<Vec<SourceFile>, Diagnostics> {
    discover_with_overlays(project_root, discovery_root, &HashMap::new())
}

fn discover_with_overlays(
    project_root: &Path,
    discovery_root: &Path,
    overlays: &HashMap<PathBuf, String>,
) -> Result<Vec<SourceFile>, Diagnostics> {
    let mut sources = Vec::new();
    let mut errors = Diagnostics::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut collected: HashSet<PathBuf> = HashSet::new();
    let mut stack: Vec<PathBuf> = vec![discovery_root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        if !visited.insert(canonical_or_self(&directory)) {
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                errors.push(Diagnostic::new(
                    ErrorId::E101,
                    format!(
                        "Cannot read source directory '{}': {error}.",
                        display_path(project_root, &directory)
                    ),
                ));
                continue;
            }
        };

        let mut children: Vec<(String, PathBuf, bool)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // Metadata follows links and junctions, so a linked directory is a
            // directory (SPEC §2.4).
            let is_directory = fs::metadata(&path)
                .map(|meta| meta.is_dir())
                .unwrap_or(false);
            children.push((name, path, is_directory));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));

        let mut subdirectories = Vec::new();
        for (name, path, is_directory) in children {
            if is_directory {
                if !is_ignored_directory(&name) {
                    subdirectories.push(path);
                }
                continue;
            }
            if !is_source_file_name(&name) {
                continue;
            }
            let canonical = canonical_or_self(&path);
            if !collected.insert(canonical.clone()) {
                continue;
            }
            match read_source(
                &path,
                &canonical,
                display_path(project_root, &path),
                overlays,
            ) {
                Ok(source) => sources.push(source),
                Err(diagnostic) => errors.push(diagnostic),
            }
        }
        for path in subdirectories.into_iter().rev() {
            stack.push(path);
        }
    }

    // An editor can introduce an unsaved source in an existing directory.
    // Existing ignored files are never smuggled back into discovery. A new
    // source's canonical parent must be inside this discovery root; following
    // a junction outside it is not a way to add a foreign project overlay.
    for (origin, text) in overlays {
        if collected.contains(origin)
            || origin.exists()
            || !eligible_new_source(origin, discovery_root)
        {
            continue;
        }
        let mut source = SourceFile::new(display_path(project_root, origin), text.clone());
        source.set_origin(origin);
        sources.push(source);
    }
    if errors.is_empty() {
        sort_sources(&mut sources);
        Ok(sources)
    } else {
        Err(errors)
    }
}

fn read_source(
    origin: &Path,
    canonical: &Path,
    display: String,
    overlays: &HashMap<PathBuf, String>,
) -> Result<SourceFile, Diagnostic> {
    if let Some(text) = overlays.get(canonical) {
        let mut source = SourceFile::new(display, text.clone());
        source.set_origin(canonical);
        Ok(source)
    } else {
        SourceFile::read(origin, display)
    }
}

fn eligible_new_source(origin: &Path, discovery_root: &Path) -> bool {
    let Ok(relative) = origin.strip_prefix(canonical_or_self(discovery_root)) else {
        return false;
    };
    let Some(name) = relative.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !is_source_file_name(name) {
        return false;
    }
    relative.parent().map(|parent| parent.components().all(|component| {
        matches!(component, std::path::Component::Normal(name) if !is_ignored_directory(&name.to_string_lossy()))
    })).unwrap_or(false)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn contains_file(sources: &[SourceFile], path: &Path) -> bool {
    let canonical = canonical_or_self(path);
    sources
        .iter()
        .filter_map(SourceFile::origin)
        .any(|origin| canonical_or_self(origin) == canonical)
}

/// The project-root-relative path a diagnostic prints, with `/` separators
/// (SPEC §9.8). A path outside the project root keeps its file name only, so
/// that no absolute path ever reaches a message.
fn display_path(project_root: &Path, path: &Path) -> String {
    match path.strip_prefix(project_root) {
        Ok(relative) => normalise_display_path(&relative.to_string_lossy()),
        Err(_) => path
            .file_name()
            .map(|name| normalise_display_path(&name.to_string_lossy()))
            .unwrap_or_default(),
    }
}

fn component_count(path: &Path) -> usize {
    path.components().count()
}

/// Removes `count` trailing components from a written path, so that a message
/// can name an ancestor the way the author named its descendant.
fn trim_components(written: &str, count: usize) -> String {
    let trimmed = written.trim_end_matches('/');
    let mut result = trimmed;
    for _ in 0..count {
        result = match result.rfind('/') {
            Some(index) => &result[..index],
            None => return trimmed.to_string(),
        };
    }
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_directories_cover_dot_dirs_and_the_four_vendor_names() {
        assert!(is_ignored_directory(".git"));
        assert!(is_ignored_directory(".vscode"));
        assert!(is_ignored_directory("node_modules"));
        assert!(is_ignored_directory("Target"));
        assert!(is_ignored_directory("BUILD"));
        assert!(is_ignored_directory("out"));
        assert!(!is_ignored_directory("data"));
        assert!(!is_ignored_directory("outputs"));
    }

    #[test]
    fn the_data_marker_and_source_extensions_are_case_insensitive() {
        assert!(is_data_marker("Data"));
        assert!(is_data_marker("DATA"));
        assert!(!is_data_marker("database"));
        assert!(is_source_file_name("Frost.AB"));
        assert!(is_source_file_name("Item.abt"));
        assert!(!is_source_file_name("notes.txt"));
        assert!(!is_source_file_name("ab"));
    }

    #[test]
    fn sources_sort_by_project_relative_path() {
        let mut sources = vec![
            SourceFile::new("data/items/b.ab", ""),
            SourceFile::new("data/Item.abt", ""),
            SourceFile::new("data/items/a.ab", ""),
        ];
        sort_sources(&mut sources);
        let paths: Vec<&str> = sources.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            ["data/Item.abt", "data/items/a.ab", "data/items/b.ab"]
        );
    }

    #[test]
    fn a_written_path_can_name_one_of_its_ancestors() {
        assert_eq!(trim_components("pack/data/items", 1), "pack/data");
        assert_eq!(trim_components("pack/data/items/", 1), "pack/data");
        assert_eq!(trim_components("pack", 3), "pack");
    }
}

#[cfg(test)]
mod filesystem_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh directory under the system temporary directory, removed when
    /// the test ends.
    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Sandbox {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("abstract_project_{name}_{suffix}"));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("sandbox");
            Sandbox { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            let mut path = self.root.clone();
            for part in relative.split('/') {
                path.push(part);
            }
            path
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.path(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(&path, text).expect("write");
            path
        }

        fn make_dir(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            fs::create_dir_all(&path).expect("dir");
            path
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn paths(layout: &ProjectLayout) -> Vec<String> {
        layout
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect()
    }

    fn ids(result: Result<ProjectLayout, Diagnostics>) -> Vec<ErrorId> {
        match result {
            Ok(_) => panic!("expected diagnostics"),
            Err(diagnostics) => diagnostics.iter().map(|item| item.id).collect(),
        }
    }

    fn sample(sandbox: &Sandbox) {
        sandbox.write("pack/data/templates/Product.abt", "schema Product {\n}\n");
        sandbox.write("pack/data/items/atlas.ab", "Product :: @id.atlas\n");
        sandbox.write("pack/assets/textures/atlas.png", "");
    }

    #[test]
    fn a_root_a_data_directory_and_a_file_resolve_the_same_source_set() {
        let sandbox = Sandbox::new("roots");
        sample(&sandbox);
        let expected = ["data/items/atlas.ab", "data/templates/Product.abt"];

        let from_root = resolve(&[sandbox.path("pack")]).expect("project root");
        assert_eq!(paths(&from_root), expected);
        assert_eq!(
            from_root.discovery_root,
            from_root.project_root.join("data")
        );
        assert_eq!(from_root.assets_dir, from_root.project_root.join("assets"));
        assert!(!from_root.is_single_file());

        let from_data = resolve(&[sandbox.path("pack/data")]).expect("data directory");
        assert_eq!(paths(&from_data), expected);
        assert_eq!(from_data.project_root, from_root.project_root);

        let from_file = resolve(&[sandbox.path("pack/data/items/atlas.ab")]).expect("one file");
        assert_eq!(paths(&from_file), expected);
        assert_eq!(from_file.project_root, from_root.project_root);
        assert_eq!(
            from_file.selected.as_deref(),
            Some(["data/items/atlas.ab".to_string()].as_slice())
        );
    }

    #[test]
    fn a_directory_inside_the_data_directory_is_e806() {
        let sandbox = Sandbox::new("inside");
        sample(&sandbox);
        let result = resolve(&[sandbox.path("pack/data/items")]);
        let Err(diagnostics) = result else {
            panic!("expected E806");
        };
        let first = diagnostics.first().expect("one diagnostic");
        assert_eq!(first.id, ErrorId::E806);
        assert!(
            first.message.contains("is inside the data directory"),
            "{}",
            first.message
        );
        assert_eq!(
            first.notes.first().map(|note| note.text.as_str()),
            Some("name the project root or the 'data' directory.")
        );
    }

    #[test]
    fn a_named_template_file_and_a_foreign_extension_are_e806() {
        let sandbox = Sandbox::new("kinds");
        sample(&sandbox);
        sandbox.write("pack/data/notes.txt", "");
        assert_eq!(
            ids(resolve(&[sandbox.path("pack/data/templates/Product.abt")])),
            [ErrorId::E806]
        );
        assert_eq!(
            ids(resolve(&[sandbox.path("pack/data/notes.txt")])),
            [ErrorId::E806]
        );
        assert_eq!(
            ids(resolve(&[sandbox.path("pack/data/missing.ab")])),
            [ErrorId::E806]
        );
        assert_eq!(ids(resolve(&[])), [ErrorId::E806]);
    }

    #[test]
    fn mixing_kinds_and_projects_is_e807_and_a_second_directory_is_e804() {
        let sandbox = Sandbox::new("mixing");
        sample(&sandbox);
        sandbox.write("other/data/items/beacon.ab", "Product :: @id.beacon\n");

        assert_eq!(
            ids(resolve(&[
                sandbox.path("pack"),
                sandbox.path("pack/data/items/atlas.ab")
            ])),
            [ErrorId::E807]
        );
        assert_eq!(
            ids(resolve(&[
                sandbox.path("pack/data/items/atlas.ab"),
                sandbox.path("other/data/items/beacon.ab")
            ])),
            [ErrorId::E807]
        );
        assert_eq!(
            ids(resolve(&[sandbox.path("pack"), sandbox.path("pack/data")])),
            [ErrorId::E804]
        );
    }

    #[test]
    fn an_empty_directory_is_e103() {
        let sandbox = Sandbox::new("empty");
        sandbox.make_dir("empty");
        assert_eq!(ids(resolve(&[sandbox.path("empty")])), [ErrorId::E103]);
    }

    #[test]
    fn the_walk_skips_dot_directories_and_the_four_vendor_names() {
        let sandbox = Sandbox::new("ignored");
        sandbox.write("pack/data/keep.ab", "Product :: @id.keep\n");
        for ignored in [".git", "node_modules", "target", "build", "out", ".hidden"] {
            sandbox.write(&format!("pack/data/{ignored}/skipped.ab"), "");
        }
        let layout = resolve(&[sandbox.path("pack")]).expect("project");
        assert_eq!(paths(&layout), ["data/keep.ab"]);
    }

    #[test]
    fn a_named_file_in_a_skipped_directory_is_still_a_source() {
        let sandbox = Sandbox::new("named_hidden");
        sandbox.write("pack/data/keep.ab", "Product :: @id.keep\n");
        sandbox.write("pack/data/.hidden/extra.ab", "Product :: @id.extra\n");
        let layout = resolve(&[sandbox.path("pack/data/.hidden/extra.ab")]).expect("project");
        assert_eq!(paths(&layout), ["data/.hidden/extra.ab", "data/keep.ab"]);
        assert_eq!(
            layout.selected.as_deref(),
            Some(["data/.hidden/extra.ab".to_string()].as_slice())
        );
    }

    #[test]
    fn a_project_without_a_data_directory_uses_the_named_directory() {
        let sandbox = Sandbox::new("flat");
        sandbox.write("flat/Item.abt", "schema Item {\n}\n");
        sandbox.write("flat/thing.ab", "Item :: @id.thing\n");
        let layout = resolve(&[sandbox.path("flat")]).expect("project");
        assert_eq!(layout.project_root, layout.discovery_root);
        assert!(layout.data_dir.is_none());
        assert_eq!(paths(&layout), ["Item.abt", "thing.ab"]);

        let from_file = resolve(&[sandbox.path("flat/thing.ab")]).expect("one file");
        assert_eq!(from_file.project_root, layout.project_root);
        assert_eq!(paths(&from_file), ["Item.abt", "thing.ab"]);
    }

    #[test]
    fn the_data_marker_and_extensions_are_matched_case_insensitively() {
        let sandbox = Sandbox::new("case");
        sandbox.write("pack/DATA/items/Frost.AB", "Option :: @id.frost\n");
        sandbox.write("pack/DATA/Option.ABT", "schema Option {\n}\n");
        let layout = resolve(&[sandbox.path("pack")]).expect("project");
        assert_eq!(paths(&layout), ["DATA/Option.ABT", "DATA/items/Frost.AB"]);
        assert_eq!(layout.data_dir, Some(layout.project_root.join("DATA")));
    }

    #[test]
    fn sources_are_sorted_by_project_relative_path() {
        let sandbox = Sandbox::new("order");
        for name in ["b", "a", "c"] {
            sandbox.write(
                &format!("pack/data/zone/{name}.ab"),
                &format!("Product :: @id.{name}\n"),
            );
        }
        sandbox.write("pack/data/Product.abt", "schema Product {\n}\n");
        let layout = resolve(&[sandbox.path("pack")]).expect("project");
        assert_eq!(
            paths(&layout),
            [
                "data/Product.abt",
                "data/zone/a.ab",
                "data/zone/b.ab",
                "data/zone/c.ab"
            ]
        );
    }

    #[test]
    fn a_file_named_twice_is_collected_once() {
        let sandbox = Sandbox::new("twice");
        sample(&sandbox);
        let file = sandbox.path("pack/data/items/atlas.ab");
        let layout = resolve(&[file.clone(), file]).expect("project");
        assert_eq!(
            paths(&layout),
            ["data/items/atlas.ab", "data/templates/Product.abt"]
        );
        assert_eq!(
            layout.selected.as_deref(),
            Some(["data/items/atlas.ab".to_string()].as_slice())
        );
    }

    #[test]
    fn a_file_that_is_not_valid_utf8_is_e102() {
        let sandbox = Sandbox::new("utf8");
        sandbox.make_dir("pack/data");
        fs::write(
            sandbox.path("pack/data/bad.ab"),
            [0x54, 0x41, 0x4d, 0xd1, 0x4f],
        )
        .expect("write");
        assert_eq!(ids(resolve(&[sandbox.path("pack")])), [ErrorId::E102]);
    }
}
