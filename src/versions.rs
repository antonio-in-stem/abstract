//! Version ranges, existence windows, per-version materialisation and overlay
//! reduction (SPEC §4.12, §5.13, §5.14, §7.4, §7.5).
//!
//! A project declares one closed integer range (`versions 1..3`, absent means
//! `1..1`). Fields, body statements and whole instances carry `@since(n)` and
//! `@removed(n)` annotations; the resulting existence set is always one
//! contiguous interval `[since, removed)` intersected with the project range.
//!
//! # The phases this module closes
//!
//! Given the parsed sources, the whole of SPEC §7.1 after P1 is four calls:
//!
//! ```text
//! P2  schema::build_tables(&templates)?          -> TemplateTables
//!     resolve::collect_instances(&instances)     -> Vec<&InstanceDecl>
//!     resolve::check_instances(&declarations, &tables)?
//!     resolve::InstanceTable::new(declarations)  -> InstanceTable
//! P3  schema::validate_schemas(&tables)?         (checks every logic block too)
//! P4  versions::materialise(&tables, &instances, emitted, assets_dir, checks)?
//! P5  versions::reduce_overlays(tables.versions, &documents)
//! ```
//!
//! `materialise` returns one `D(v)` per version of `tables.versions`,
//! ascending, so `documents[documents.len() - 1]` is the base of SPEC §7.5 and
//! is what `CompiledDocument::new` takes as `data`. `emitted` is `None` for a
//! whole-project compile and the selected ids in single-file mode (SPEC §2.5);
//! every discovered instance is compiled and validated either way.

use std::fmt;
use std::path::Path;

use crate::diagnostics::Diagnostics;
use crate::output::Value;
use crate::resolve::{check_clone_paths, InstanceTable, Resolver};
use crate::schema::TemplateTables;
use crate::validate::{compile_instance, AssetChecks};

/// The greatest number of versions a project's declared range may cover.
///
/// SPEC §4.12 bounds neither number of a `versions` declaration, and SPEC §7.4
/// compiles every version in the range, so with no bound the work a four-line
/// project demands is unbounded and the allocation behind it fails — which
/// SPEC §3.7 forbids: "An implementation MUST NOT abort, panic or crash on any
/// input." The bound is on the count, not on the numbers: `versions 100..163`
/// declares 64 versions and is as legal as `versions 1..64`. A wider range is
/// E602, reported with a message that names this bound.
pub const MAX_PROJECT_VERSIONS: u32 = 4096;

/// A closed range of integer versions, `min <= max` (SPEC §4.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VersionRange {
    pub min: u32,
    pub max: u32,
}

impl VersionRange {
    /// The range a project has when it declares none.
    pub const DEFAULT: VersionRange = VersionRange { min: 1, max: 1 };

    /// Builds a range, or `None` when it is empty or starts below 1 (E602).
    pub fn new(min: u32, max: u32) -> Option<Self> {
        if min >= 1 && min <= max {
            Some(Self { min, max })
        } else {
            None
        }
    }

    pub fn contains(self, version: u32) -> bool {
        version >= self.min && version <= self.max
    }

    pub fn contains_range(self, other: VersionRange) -> bool {
        other.min >= self.min && other.max <= self.max
    }

    pub fn intersect(self, other: VersionRange) -> Option<VersionRange> {
        VersionRange::new(self.min.max(other.min), self.max.min(other.max))
    }

    /// The number of versions in the range; never zero.
    pub fn count(self) -> u32 {
        self.max - self.min + 1
    }

    /// Every version in the range, ascending (SPEC §7.4).
    pub fn iter(self) -> std::ops::RangeInclusive<u32> {
        self.min..=self.max
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.min == self.max {
            write!(f, "{}", self.min)
        } else {
            write!(f, "{}..{}", self.min, self.max)
        }
    }
}

/// The `@since` / `@removed` annotations as written, before they are resolved
/// against the project range. Either may be absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub since: Option<u32>,
    pub removed: Option<u32>,
}

impl Window {
    pub const UNANNOTATED: Window = Window {
        since: None,
        removed: None,
    };

    pub fn is_annotated(self) -> bool {
        self.since.is_some() || self.removed.is_some()
    }

    /// The existence set: `since <= v < removed`, intersected with `project`.
    /// `since` defaults to the project minimum and `removed` to `max + 1`
    /// (SPEC §4.12). `None` means the annotations select no version.
    pub fn resolve(self, project: VersionRange) -> Option<VersionRange> {
        let since = self.since.unwrap_or(project.min);
        let removed = self.removed.unwrap_or(project.max.saturating_add(1));
        if removed == 0 {
            return None;
        }
        VersionRange::new(since, removed - 1)?.intersect(project)
    }
}

/// One overlay of the compiled document (SPEC §7.5, §8.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    /// The range this overlay covers.
    pub versions: VersionRange,
    /// Complete instance objects that replace or add to the base, ordered by
    /// `(template, id)`.
    pub data: Vec<Value>,
    /// Ids the base carries that do not exist over this range, ascending.
    pub removed: Vec<String>,
}

impl Overlay {
    /// An overlay with neither replacements nor removals is never emitted.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty() && self.removed.is_empty()
    }
}

/// Renders a set of versions as ascending maximal ranges — `1..2, 4` — for the
/// `{versions}` substitution of SPEC §9.8.
pub fn render_version_set(versions: &[u32]) -> String {
    let mut sorted: Vec<u32> = versions.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut out = String::new();
    let mut index = 0;
    while index < sorted.len() {
        let start = sorted[index];
        let mut end = start;
        while index + 1 < sorted.len() && sorted[index + 1] == end + 1 {
            index += 1;
            end = sorted[index];
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        if start == end {
            out.push_str(&start.to_string());
        } else {
            out.push_str(&format!("{start}..{end}"));
        }
        index += 1;
    }
    out
}

// ------------------------------------------------------- P4: materialisation

/// Compiles every instance for every version in which it exists (SPEC §7.3,
/// §7.4), returning `D(v)` for each `v` of the project range, ascending.
///
/// Every discovered instance is compiled, because a clone source that is not
/// itself emitted must still be validated (SPEC §2.5); `emitted` names the
/// ids that reach the document, and `None` emits them all.
///
/// An instance that fails in one version is not compiled again for the later
/// ones, so one defect is reported once, at the lowest version in which it
/// shows (SPEC §7.4).
pub fn materialise(
    tables: &TemplateTables,
    instances: &InstanceTable<'_>,
    emitted: Option<&[String]>,
    assets_dir: &Path,
    asset_checks: AssetChecks,
) -> Result<Vec<Vec<Value>>, Diagnostics> {
    let project = tables.versions;
    let mut resolver = Resolver::new(tables, instances);
    let mut errors = Diagnostics::new();

    for decl in instances.iter() {
        errors.extend(check_clone_paths(&mut resolver, decl, project));
    }

    let mut documents: Vec<Vec<Value>> = vec![Vec::new(); project.count() as usize];
    let mut failed: Vec<String> = Vec::new();
    for version in project.iter() {
        let slot = (version - project.min) as usize;
        for decl in instances.iter() {
            let inside = decl
                .window
                .resolve(project)
                .map(|window| window.contains(version))
                .unwrap_or(false);
            if !inside || failed.contains(&decl.id) {
                continue;
            }
            match compile_instance(&mut resolver, decl, version, assets_dir, asset_checks) {
                Ok(value) => {
                    let selected = emitted
                        .map(|ids| ids.iter().any(|id| id == &decl.id))
                        .unwrap_or(true);
                    if selected {
                        if let Some(document) = documents.get_mut(slot) {
                            document.push(value);
                        }
                    }
                }
                Err(reported) => {
                    errors.extend(reported);
                    failed.push(decl.id.clone());
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(documents)
    } else {
        Err(errors)
    }
}

// ----------------------------------------------------- P5: overlay reduction

/// Reduces the per-version documents to the base plus overlays (SPEC §7.5).
///
/// `documents` holds one entry per version of `project`, ascending; the entry
/// for `project.max` is the base. Each instance is reduced on its own into
/// maximal runs of structurally equal objects, and the runs that differ from
/// the base become entries; entries that share a range share one overlay.
pub fn reduce_overlays(project: VersionRange, documents: &[Vec<Value>]) -> Vec<Overlay> {
    if project.min >= project.max {
        return Vec::new();
    }
    let base = documents
        .get((project.max - project.min) as usize)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut keys: Vec<(String, String)> = Vec::new();
    for document in documents {
        for object in document {
            if let Some(key) = object_key(object) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
    }
    keys.sort();

    let mut entries: Vec<(VersionRange, Entry)> = Vec::new();
    for (template, id) in &keys {
        let carried = find_object(base, id);
        let mut version = project.min;
        while version < project.max {
            let value = find_object(document_at(documents, project, version), id);
            let mut end = version;
            while end + 1 < project.max
                && same_object(
                    find_object(document_at(documents, project, end + 1), id),
                    value,
                )
            {
                end += 1;
            }
            if !same_object(value, carried) {
                if let Some(range) = VersionRange::new(version, end) {
                    let entry = match value {
                        Some(object) => Entry::Replace(object.clone()),
                        None => Entry::Remove(id.clone()),
                    };
                    entries.push((range, entry));
                }
            }
            version = end + 1;
        }
        let _ = template;
    }

    let mut ranges: Vec<VersionRange> = Vec::new();
    for (range, _) in &entries {
        if !ranges.contains(range) {
            ranges.push(*range);
        }
    }
    ranges.sort_by(|left, right| left.min.cmp(&right.min).then(left.max.cmp(&right.max)));

    let mut overlays = Vec::with_capacity(ranges.len());
    for range in ranges {
        let mut data = Vec::new();
        let mut removed = Vec::new();
        // `entries` was built in `(template, id)` order, so `data` is already
        // ordered as SPEC §2.7 requires.
        for (entry_range, entry) in &entries {
            if *entry_range != range {
                continue;
            }
            match entry {
                Entry::Replace(object) => data.push(object.clone()),
                Entry::Remove(id) => removed.push(id.clone()),
            }
        }
        removed.sort();
        overlays.push(Overlay {
            versions: range,
            data,
            removed,
        });
    }
    overlays
}

/// One instance's contribution to one version range (SPEC §7.5 step 1).
enum Entry {
    Replace(Value),
    Remove(String),
}

fn document_at(documents: &[Vec<Value>], project: VersionRange, version: u32) -> &[Value] {
    documents
        .get((version - project.min) as usize)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn find_object<'a>(document: &'a [Value], id: &str) -> Option<&'a Value> {
    document
        .iter()
        .find(|object| object.get("id") == Some(&Value::Text(id.to_string())))
}

fn object_key(object: &Value) -> Option<(String, String)> {
    let Some(Value::Text(template)) = object.get("template") else {
        return None;
    };
    let Some(Value::Text(id)) = object.get("id") else {
        return None;
    };
    Some((template.clone(), id.clone()))
}

/// Structural equality of SPEC §7.5: same keys in the same order, same kinds,
/// same lengths, and numbers that render identically under SPEC §8.7, so that
/// `0.0` and `-0.0` are different. `⊥` equals only `⊥`.
fn same_object(left: Option<&Value>, right: Option<&Value>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => same_value(left, right),
        _ => false,
    }
}

fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => left.to_bits() == right.to_bits(),
        (Value::List(left), Value::List(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| same_value(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| left.0 == right.0 && same_value(&left.1, &right.1))
        }
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_resolves_to_a_contiguous_interval() {
        let project = VersionRange::new(1, 3).unwrap();
        assert_eq!(
            Window::UNANNOTATED.resolve(project),
            Some(VersionRange::new(1, 3).unwrap())
        );
        assert_eq!(
            Window {
                since: Some(2),
                removed: None
            }
            .resolve(project),
            Some(VersionRange::new(2, 3).unwrap())
        );
        assert_eq!(
            Window {
                since: None,
                removed: Some(3)
            }
            .resolve(project),
            Some(VersionRange::new(1, 2).unwrap())
        );
        assert_eq!(
            Window {
                since: Some(3),
                removed: Some(2)
            }
            .resolve(project),
            None
        );
    }

    #[test]
    fn ranges_intersect_and_render() {
        let a = VersionRange::new(1, 3).unwrap();
        let b = VersionRange::new(2, 5).unwrap();
        assert_eq!(a.intersect(b), VersionRange::new(2, 3));
        assert_eq!(a.to_string(), "1..3");
        assert_eq!(VersionRange::new(4, 4).unwrap().to_string(), "4");
        assert!(a.contains(2));
        assert!(!a.contains(4));
        assert_eq!(a.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn version_sets_render_as_maximal_ranges() {
        assert_eq!(render_version_set(&[1, 2, 4]), "1..2, 4");
        assert_eq!(render_version_set(&[3]), "3");
        assert_eq!(render_version_set(&[]), "");
        assert_eq!(render_version_set(&[2, 1, 2]), "1..2");
    }
}

#[cfg(test)]
mod overlay_tests {
    use super::*;
    use crate::ast::{parse, SourceUnit};
    use crate::resolve::{check_instances, collect_instances};
    use crate::source::SourceFile;

    /// The `Item` schema of SPEC §4.12, used by both worked examples.
    const ITEM: &str = "versions 1..3\n\nschema Item {\n    name: text(1..40)\n    glow: bool @since(2) = false\n    legacy_tint: int(0..255) @removed(3) @optional\n}\n";

    fn document(template: &str, instance: &str) -> (VersionRange, Vec<Vec<Value>>) {
        let source = SourceFile::new("data/schema.abt", template);
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file]).expect("tables build");
        crate::schema::validate_schemas(&tables).expect("the schemas are valid");

        let source = SourceFile::new("data/items.ab", instance);
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
            panic!("an instance file");
        };
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("the instances are consistent");
        let instances = InstanceTable::new(declarations);
        let documents = materialise(
            &tables,
            &instances,
            None,
            Path::new("assets"),
            AssetChecks::Skipped,
        )
        .expect("the project compiles");
        (tables.versions, documents)
    }

    fn keys(value: &Value) -> Vec<String> {
        match value {
            Value::Object(entries) => entries.iter().map(|(key, _)| key.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// The same pipeline as `document`, for a project spread over several
    /// instance files, so that the ids of SPEC 5.3 can come from file stems.
    fn project(template: &str, files: &[(&str, &str)]) -> (VersionRange, Vec<Vec<Value>>) {
        let source = SourceFile::new("data/templates/Pack.abt", template);
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file]).expect("tables build");
        crate::schema::validate_schemas(&tables).expect("the schemas are valid");

        let sources: Vec<SourceFile> = files
            .iter()
            .map(|(path, text)| SourceFile::new(*path, *text))
            .collect();
        let parsed: Vec<_> = sources
            .iter()
            .map(|source| match parse(source).expect("the instances parse") {
                SourceUnit::Instance(file) => file,
                SourceUnit::Template(_) => panic!("an instance file"),
            })
            .collect();
        let declarations = collect_instances(&parsed);
        check_instances(&declarations, &tables).expect("the instances are consistent");
        let instances = InstanceTable::new(declarations);
        let documents = materialise(
            &tables,
            &instances,
            None,
            Path::new("assets"),
            AssetChecks::Skipped,
        )
        .expect("the project compiles");
        (tables.versions, documents)
    }

    /// SPEC Appendix C without its logic block, which chapter 6 evaluates.
    /// `owner.contact` is therefore absent: it is written by `derive?`.
    const PACK: &str = concat!(
        "versions 1..2\n",
        "\n",
        "schema Pack {\n",
        "    id: text(3..24)\n",
        "    title: text(1..40)\n",
        "    tier: enum(free, plus) = free\n",
        "    slot_count: int(1..9) @optional\n",
        "}\n",
        "\n",
        "schema Sticker {\n",
        "    pack: ref(Pack)\n",
        "    title: text(1..60)\n",
        "    icon: image(png 128x128)\n",
        "    rarity: enum(common, rare, epic) = common\n",
        "    glow: bool @since(2) = false\n",
        "    tint: int(0..255) @optional @removed(2)\n",
        "    owner {\n",
        "        team: text(1..40)\n",
        "        contact: text(1..60) @optional\n",
        "    }\n",
        "    tags[]: enum(core_ui, core_game, promo) @optional\n",
        "    copy[] {\n",
        "        key: enum(en_us, es_es, es_mx) @tag\n",
        "        value: text(1..80)\n",
        "    }\n",
        "}\n",
    );

    #[test]
    fn the_order_of_the_instance_files_never_reaches_the_document() {
        // SPEC 2.7 and 7.6: instances are ordered by (template, id) alone, so
        // the order in which the files were discovered cannot be observed.
        let template = concat!(
            "schema Pack {\n",
            "    title: text(1..40)\n",
            "}\n",
            "\n",
            "schema Sticker {\n",
            "    pack: ref(Pack)\n",
            "    title: text(1..40)\n",
            "}\n",
        );
        let winter = ("data/winter.ab", "Pack :: @id.winter\n    title: Winter\n");
        let ember = (
            "data/ember.ab",
            "Sticker :: @id.ember\n    pack: winter\n    title: Ember\n",
        );
        let frost = (
            "data/frost.ab",
            "Sticker :: @id.frost\n    pack: winter\n    title: Frost\n",
        );
        let (_, forwards) = project(template, &[winter, ember, frost]);
        let (_, backwards) = project(template, &[frost, ember, winter]);
        assert_eq!(forwards, backwards);
        assert_eq!(
            forwards[0].iter().map(id_of).collect::<Vec<_>>(),
            ["winter", "ember", "frost"]
        );
    }

    #[test]
    fn the_worked_example_of_appendix_c_compiles_and_reduces_to_one_overlay() {
        let (range, documents) = project(
            PACK,
            &[
                (
                    "data/packs/winter_2026.ab",
                    "Pack :: @tier.plus\n    title: Winter 2026\n    slot_count: 3\n",
                ),
                (
                    "data/packs/spring_2026.ab",
                    "Pack :: @since(2)\n    title: Spring 2026\n",
                ),
                (
                    "data/stickers/frost.ab",
                    concat!(
                        "Sticker :: @id.frost, @rarity.rare\n",
                        "    pack: winter_2026\n",
                        "    title: Frost\n",
                        "    icon: ./textures/$id.png\n",
                        "    tint: 200\n",
                        "    owner.team: Studio A\n",
                        "    tags: core_*\n",
                        "    copy(key, value): (en_us, Frost), (es_*, Escarcha)\n",
                    ),
                ),
                (
                    "data/stickers/ember.ab",
                    concat!(
                        "Sticker :: @id.ember, @rarity.epic\n",
                        "&frost.*\n",
                        "    title: Ember\n",
                        "    tags: [core_ui, promo]\n",
                        "    copy(key, value): (en_us, Ember), (es_*, Brasa)\n",
                    ),
                ),
            ],
        );
        assert_eq!(range, VersionRange::new(1, 2).unwrap());

        // D(2) is the base: every instance, ordered by (template, id).
        let base = &documents[1];
        assert_eq!(
            base.iter().map(id_of).collect::<Vec<_>>(),
            ["spring_2026", "winter_2026", "ember", "frost"]
        );
        assert_eq!(keys(&base[0]), ["template", "id", "title", "tier"]);
        assert_eq!(
            base[0].get("tier"),
            Some(&Value::Text("free".to_string())),
            "an absent defaulted field is filled"
        );
        assert_eq!(
            keys(&base[1]),
            ["template", "id", "title", "tier", "slot_count"]
        );

        let ember = &base[2];
        assert_eq!(
            keys(ember),
            [
                "template", "id", "pack", "title", "icon", "rarity", "glow", "owner", "tags",
                "copy"
            ]
        );
        assert_eq!(
            ember.get("icon"),
            Some(&Value::Text("./textures/ember.png".to_string())),
            "a cloned value interpolates against the cloning instance"
        );
        assert_eq!(ember.get("rarity"), Some(&Value::Text("epic".to_string())));
        assert_eq!(ember.get("glow"), Some(&Value::Bool(false)));
        assert_eq!(
            ember.get("tags"),
            Some(&Value::List(vec![
                Value::Text("core_ui".to_string()),
                Value::Text("promo".to_string()),
            ])),
            "an own assignment replaces a cloned list wholesale"
        );
        assert_eq!(
            keys(ember.get("owner").expect("owner is an object")),
            ["team"]
        );

        let frost = &base[3];
        assert_eq!(
            frost.get("icon"),
            Some(&Value::Text("./textures/frost.png".to_string()))
        );
        assert_eq!(
            frost.get("tags"),
            Some(&Value::List(vec![
                Value::Text("core_ui".to_string()),
                Value::Text("core_game".to_string()),
            ])),
            "core_* expands in the enum's declared order"
        );
        let copy = frost.get("copy").expect("copy is a list");
        let Value::List(rows) = copy else {
            panic!("copy is a list");
        };
        assert_eq!(rows.len(), 3, "es_* clones the whole tuple row");
        assert_eq!(rows[1].get("key"), Some(&Value::Text("es_es".to_string())));
        assert_eq!(
            rows[1].get("value"),
            Some(&Value::Text("Escarcha".to_string()))
        );
        assert_eq!(rows[2].get("key"), Some(&Value::Text("es_mx".to_string())));

        // D(1): spring_2026 has not been declared yet, and tint replaces glow.
        let first = &documents[0];
        assert_eq!(
            first.iter().map(id_of).collect::<Vec<_>>(),
            ["winter_2026", "ember", "frost"]
        );
        assert_eq!(
            keys(&first[1]),
            [
                "template", "id", "pack", "title", "icon", "rarity", "tint", "owner", "tags",
                "copy"
            ]
        );
        assert_eq!(
            first[1].get("tint"),
            Some(&Value::Int(200)),
            "the clone carries tint in the version in which the field exists"
        );

        // One overlay: both stickers differ from the base over 1..1, and
        // spring_2026 is removed over the same range (SPEC 7.5 step 2).
        let overlays = reduce_overlays(range, &documents);
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].versions, VersionRange::new(1, 1).unwrap());
        assert_eq!(
            overlays[0].data.iter().map(id_of).collect::<Vec<_>>(),
            ["ember", "frost"]
        );
        assert_eq!(overlays[0].removed, ["spring_2026"]);
    }

    fn id_of(value: &Value) -> String {
        match value.get("id") {
            Some(Value::Text(id)) => id.clone(),
            _ => String::new(),
        }
    }

    #[test]
    fn the_worked_example_of_section_7_5_reduces_to_two_overlays() {
        let (project, documents) = document(
            ITEM,
            "Item :: @id.torch\n    name: Torch\n    legacy_tint: 200\n",
        );
        assert_eq!(documents.len(), 3);
        assert_eq!(
            keys(&documents[0][0]),
            ["template", "id", "name", "legacy_tint"]
        );
        assert_eq!(
            keys(&documents[1][0]),
            ["template", "id", "name", "glow", "legacy_tint"]
        );
        assert_eq!(keys(&documents[2][0]), ["template", "id", "name", "glow"]);

        let overlays = reduce_overlays(project, &documents);
        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[0].versions, VersionRange::new(1, 1).unwrap());
        assert_eq!(overlays[0].data.len(), 1);
        assert!(overlays[0].removed.is_empty());
        assert_eq!(
            keys(&overlays[0].data[0]),
            ["template", "id", "name", "legacy_tint"]
        );
        assert_eq!(overlays[1].versions, VersionRange::new(2, 2).unwrap());
        assert_eq!(
            keys(&overlays[1].data[0]),
            ["template", "id", "name", "glow", "legacy_tint"]
        );
    }

    #[test]
    fn instance_windows_add_and_remove_whole_objects() {
        let (project, documents) = document(
            ITEM,
            "Item :: @id.lantern @since(2)\n    name: Lantern\n\nItem :: @id.candle @removed(3)\n    name: Candle\n",
        );
        // D(1) = candle only, D(2) = both, D(3) = lantern only (SPEC §7.4).
        assert_eq!(
            documents[0].iter().map(id_of).collect::<Vec<_>>(),
            ["candle"]
        );
        assert_eq!(
            documents[1].iter().map(id_of).collect::<Vec<_>>(),
            ["candle", "lantern"]
        );
        assert_eq!(
            documents[2].iter().map(id_of).collect::<Vec<_>>(),
            ["lantern"]
        );

        let overlays = reduce_overlays(project, &documents);
        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[0].versions, VersionRange::new(1, 1).unwrap());
        assert_eq!(
            overlays[0].data.iter().map(id_of).collect::<Vec<_>>(),
            ["candle"]
        );
        assert_eq!(overlays[0].removed, ["lantern"]);
        assert_eq!(overlays[1].versions, VersionRange::new(2, 2).unwrap());
        assert_eq!(
            overlays[1].data.iter().map(id_of).collect::<Vec<_>>(),
            ["candle"]
        );
        assert!(overlays[1].removed.is_empty());
    }

    #[test]
    fn a_run_that_equals_the_base_is_discarded() {
        // `steady` never changes, so it contributes no entry at all.
        let (project, documents) = document(ITEM, "Item :: @id.steady\n    name: Steady\n");
        let overlays = reduce_overlays(project, &documents);
        assert_eq!(
            overlays.len(),
            1,
            "only the version that lacks 'glow' differs"
        );
        assert_eq!(overlays[0].versions, VersionRange::new(1, 1).unwrap());
    }

    #[test]
    fn a_single_version_project_emits_no_overlay() {
        let (project, documents) = document(
            "schema Item {\n    name: text(1..40)\n}\n",
            "Item :: @id.one\n    name: One\n",
        );
        assert_eq!(project, VersionRange::DEFAULT);
        assert!(reduce_overlays(project, &documents).is_empty());
    }

    #[test]
    fn two_statements_with_disjoint_ranges_give_one_field_two_values() {
        let (project, documents) = document(
            "versions 1..2\n\nschema Item {\n    name: text(1..40)\n}\n",
            "Item :: @id.one\n    name: Old  @removed(2)\n    name: New  @since(2)\n",
        );
        assert_eq!(
            documents[0][0].get("name"),
            Some(&Value::Text("Old".to_string()))
        );
        assert_eq!(
            documents[1][0].get("name"),
            Some(&Value::Text("New".to_string()))
        );
        let overlays = reduce_overlays(project, &documents);
        assert_eq!(overlays.len(), 1);
        assert_eq!(
            overlays[0].data[0].get("name"),
            Some(&Value::Text("Old".to_string()))
        );
    }

    #[test]
    fn structural_equality_separates_two_numbers_that_render_differently() {
        assert!(same_value(&Value::Float(1.5), &Value::Float(1.5)));
        assert!(!same_value(&Value::Float(0.0), &Value::Float(-0.0)));
        assert!(!same_value(&Value::Int(1), &Value::Float(1.0)));
        assert!(!same_value(
            &Value::Object(vec![
                ("a".to_string(), Value::Int(1)),
                ("b".to_string(), Value::Int(2)),
            ]),
            &Value::Object(vec![
                ("b".to_string(), Value::Int(2)),
                ("a".to_string(), Value::Int(1)),
            ]),
        ));
        assert!(same_object(None, None));
        assert!(!same_object(None, Some(&Value::Int(1))));
    }

    #[test]
    fn every_version_is_compiled_and_a_required_field_must_hold_in_each() {
        // `glow` exists from version 2 and has a default, so version 1 is
        // fine; a required field supplied only by an annotated statement is
        // not (SPEC §4.12).
        let source = SourceFile::new(
            "data/schema.abt",
            "versions 1..2\n\nschema Item {\n    name: text(1..40)\n}\n",
        );
        let SourceUnit::Template(file) = parse(&source).expect("the template parses") else {
            panic!("a template file");
        };
        let tables = crate::schema::build_tables(&[file]).expect("tables build");
        let source = SourceFile::new(
            "data/items.ab",
            "Item :: @id.one\n    name: One  @since(2)\n",
        );
        let SourceUnit::Instance(file) = parse(&source).expect("the instances parse") else {
            panic!("an instance file");
        };
        let declarations = collect_instances(std::slice::from_ref(&file));
        check_instances(&declarations, &tables).expect("consistent");
        let instances = InstanceTable::new(declarations);
        let errors = materialise(
            &tables,
            &instances,
            None,
            Path::new("assets"),
            AssetChecks::Skipped,
        )
        .expect_err("version 1 has no name");
        assert_eq!(
            errors.first().map(|item| item.id),
            Some(crate::diagnostics::ErrorId::E411)
        );
        assert_eq!(errors.len(), 1, "one defect is reported once");
    }
}
