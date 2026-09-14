//! The 0.2.0 regression suite, carried into 1.0 as a checklist.
//!
//! Every test that states a rule Abstract 1.0 keeps runs against the 1.0
//! pipeline. The 21 that remain `#[ignore]`d are marked `invalid under 1.0`:
//! the specification changed the rule they assert, and the attribute cites
//! the section. Those are kept as a record of the migration (Appendix A) and
//! are replaced by conformance cases, not repaired.
//!
//! New tests belong in `tests/conformance.rs` and in the corpus it walks.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use abstract_lang::{compile_paths, compile_project, compile_sources, CompileOptions, SourceFile};

#[test]
#[ignore = "invalid under 1.0: RAW now renders the document envelope and one key per line (SPEC 8.6, A46, A50); examples/product-catalog/ originated in 0.2.0"]
fn compiles_example_project_to_human_readable_data() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/product-catalog");

    let compiled =
        compile_project(&project, CompileOptions::default()).expect("example should compile");
    let raw = compiled.to_raw_string();

    assert!(raw.starts_with("data: ["));
    assert!(raw.contains("template: \"Product\""));
    assert!(raw.contains("id: \"atlas\""));
    assert!(raw.contains("name: \"Atlas Search\""));
    assert!(raw.contains("tags: [\"core\", \"public\", \"ai_ready\"]"));
    assert!(raw.contains("capabilities: ["));
    assert!(raw.contains("availability: \"beta\""));
    assert!(raw.contains("template: \"Policy\""));
}

#[test]
fn rejects_enum_values_outside_the_schema() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..20)
            color: enum(red, blue)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/bad.ab",
        r#"
        Thing :: @id.bad, @color.green
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("invalid enum should fail");

    assert!(error.to_string().contains("color"));
    assert!(error.to_string().contains("green"));
    assert!(error.to_string().contains("red, blue"));
}

#[test]
fn expands_enum_prefix_wildcards_in_tuple_arrays() {
    let template = SourceFile::new(
        "templates/Page.abt",
        r#"
        schema Page {
            id: text(1..40)
            copy[] {
                key: enum(en_us, es_es, es_mx, es_ar) @tag
                value: text(1..40)
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "pages/home.ab",
        r#"
        Page :: @id.home
            copy(key, value): (en_us, Welcome), (es_*, Bienvenido)
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("wildcard locales should compile");
    let raw = compiled.to_raw_string();

    assert!(raw.contains("key: \"es_es\""));
    assert!(raw.contains("key: \"es_mx\""));
    assert!(raw.contains("key: \"es_ar\""));
    assert_eq!(raw.matches("Bienvenido").count(), 3);
}

#[test]
#[ignore = "invalid under 1.0: a RAW object is always multi-line (SPEC 8.6, A50)"]
fn fills_group_defaults_for_tagged_objects() {
    let template = SourceFile::new(
        "templates/Product.abt",
        r#"
        schema Product {
            id: text(1..40)
            capabilities[] {
                id: enum(search, export) @tag
                availability: enum(alpha, stable) = stable
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "products/atlas.ab",
        r#"
        Product :: @id.atlas
            capabilities: [#search, #export(availability: alpha)]
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("tagged capability entries should compile");
    let raw = compiled.to_raw_string();

    assert!(raw.contains("{id: \"search\", availability: \"stable\"}"));
    assert!(raw.contains("{id: \"export\", availability: \"alpha\"}"));
}

#[test]
fn accepts_header_tags_split_by_whitespace_and_newlines() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..20)
            rarity: enum(rare, legendary)
            origin: enum(gachapon, gift)
            flags[]: enum(alpha, beta) @optional
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/bright.ab",
        r#"
        Thing :: @id.bright, @rarity.legendary,
            @origin.gift
            flags: alpha, beta
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("header tags should allow comma and newline separated style");
    let raw = compiled.to_raw_string();

    assert!(raw.contains("id: \"bright\""));
    assert!(raw.contains("rarity: \"legendary\""));
    assert!(raw.contains("origin: \"gift\""));
    assert!(raw.contains("flags: [\"alpha\", \"beta\"]"));
}

#[test]
fn coerces_single_values_into_schema_declared_lists() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..20)
            flags[]: enum(alpha, beta) @optional
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/solo.ab",
        r#"
        Thing :: @id.solo
            flags: alpha
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("single value should be accepted for a list field");
    let raw = compiled.to_raw_string();

    assert!(raw.contains("flags: [\"alpha\"]"));
}

#[test]
#[ignore = "invalid under 1.0: `.applicable.id == helmet` projects over a list and is E519 (SPEC 6.5)"]
fn evaluates_logic_requirements_after_schema_validation() {
    let template = SourceFile::new(
        "templates/Sticker.abt",
        r#"
        schema Sticker {
            id: text(1..40)
            flags[]: enum(hat_overrides_helmet, hat_has_variations) @optional
            applicable[] {
                id: enum(helmet, sword) @tag
            }
            variant_count: int(0, 2..15) = 0
        }

        logic Sticker {
            if .flags contains "hat_has_variations" {
                require .flags contains "hat_overrides_helmet"
                    else throw "Hat variations require helmet override."

                require .applicable.id == "helmet"
                    else throw "Hat variations require helmet applicability."
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "stickers/bad.ab",
        r#"
        Sticker :: @id.bad, @variant_count.3
            flags: hat_has_variations
            applicable: #sword
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("logic should reject missing override flag");

    assert!(error
        .to_string()
        .contains("Hat variations require helmet override"));
}

#[test]
#[ignore = "invalid under 1.0: loop variables are always written '$name' (SPEC 6.9, A44)"]
fn evaluates_logic_loops_and_exists_operator() {
    let template = SourceFile::new(
        "templates/Page.abt",
        r#"
        schema Page {
            id: text(1..40)
            lang[] {
                key: enum(en_us, es_es) @tag
                value: text(1..40) @optional
            }
        }

        logic Page {
            for entry in .lang {
                require entry.value exists
                    else throw "Every language entry needs a value."
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "pages/home.ab",
        r#"
        Page :: @id.home
            lang(key, value): (en_us, Welcome), (es_es, Hola)
        "#,
    );

    compile_sources(vec![template, instance], CompileOptions::default())
        .expect("looped logic should accept entries with values");
}

#[test]
#[ignore = "invalid under 1.0: loop variables are always written '$name' (SPEC 6.9, A44)"]
fn logic_loops_bind_each_array_item() {
    let template = SourceFile::new(
        "templates/Page.abt",
        r#"
        schema Page {
            id: text(1..40)
            lang[] {
                key: enum(en_us, es_es) @tag
                value: text(1..40) @optional
            }
        }

        logic Page {
            for entry in .lang {
                require entry.value exists
                    else throw "Every language entry needs a value."
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "pages/home.ab",
        r#"
        Page :: @id.home
            lang: #en_us, #es_es(value: Hola)
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("looped logic should reject the specific missing entry");

    assert!(error
        .to_string()
        .contains("Every language entry needs a value"));
}

#[test]
fn evaluates_logic_not_equal_operator() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            status: enum(enabled, disabled)
        }

        logic Thing {
            require .status != "disabled"
                else throw "Disabled things are not allowed."
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/bad.ab",
        r#"
        Thing :: @id.bad, @status.disabled
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("not-equal logic should reject disabled values");

    assert!(error
        .to_string()
        .contains("Disabled things are not allowed"));
}

#[test]
fn serializes_compiled_project_to_json() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            count: int(0..99)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/one.ab",
        r#"
        Thing :: @id.one, @count.7
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("project should compile");
    let json = compiled.to_json_string();

    assert!(json.contains("\"data\": ["));
    assert!(json.contains("\"template\": \"Thing\""));
    assert!(json.contains("\"count\": 7"));
}

#[test]
#[ignore = "invalid under 1.0: every YAML mapping key is quoted (SPEC 8.5, A48)"]
fn serializes_compiled_project_to_yaml() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            label: text(1..40)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/one.ab",
        r#"
        Thing :: @id.one
            label: Hello
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("project should compile");
    let yaml = compiled.to_yaml_string();

    assert!(yaml.contains("data:\n  - template: \"Thing\""));
    assert!(yaml.contains("label: \"Hello\""));
}

#[test]
#[ignore = "invalid under 1.0: @tag on a root field is E311 and '#tag' on a $(Schema) field is E412 (SPEC 4.8, 5.10)"]
fn derives_sticker_sizes_from_slots() {
    let template = SourceFile::new(
        "templates/Sticker.abt",
        r#"
        schema Sticker {
            id: text(1..40)
            item_size: enum(single, double) = double
            render_size: enum(none, single, double) = double
            slots {
                1: $(LoreSpace)
                2: $(LoreSpace) @optional
                3: $(RenderSpace) @optional
                4: $(RenderSpace) @optional
            }
        }

        schema LoreSpace {
            mode: enum(generate, custom) @tag
            distribution[]: file(png)
        }

        schema RenderSpace {
            mode: enum(custom) @tag
        }

        logic Sticker {
            derive .item_size = single
            derive .render_size = none

            if .slots.2 exists {
                derive .item_size = double
            }

            if .slots.3 exists {
                derive .render_size = single
            }

            if .slots.4 exists {
                derive .render_size = double
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "stickers/clockwork.ab",
        r#"
        Sticker :: @id.clockwork
            slots.1: #generate(distribution: ./textures/sword.png)
            slots.2: #generate(distribution: ./textures/axe.png)
            slots.{3, 4}: #custom
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("sticker sizes should derive from slots");
    let json = compiled.to_json_string();

    assert!(json.contains("\"item_size\": \"double\""));
    assert!(json.contains("\"render_size\": \"double\""));
}

#[test]
fn logic_derive_is_template_defined_not_compiler_builtin() {
    let template = SourceFile::new(
        "templates/Card.abt",
        r#"
        schema Card {
            id: text(1..40)
            size: enum(small, large) = large
            parts {
                1: text(1..40)
                2: text(1..40) @optional
            }
        }

        logic Card {
            derive .size = small

            if .parts.2 exists {
                derive .size = large
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "cards/solo.ab",
        r#"
        Card :: @id.solo
            parts.1: Front
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("generic logic derivation should compile");
    let json = compiled.to_json_string();

    assert!(json.contains("\"size\": \"small\""));
}

#[test]
#[ignore = "invalid under 1.0: @tag on a root field is E311 and '#tag' on a $(Schema) field is E412 (SPEC 4.8, 5.10)"]
fn sticker_slot_four_requires_slot_three() {
    let template = SourceFile::new(
        "templates/Sticker.abt",
        r#"
        schema Sticker {
            id: text(1..40)
            item_size: enum(single, double) = double
            render_size: enum(none, single, double) = double
            slots {
                1: $(LoreSpace)
                2: $(LoreSpace) @optional
                3: $(RenderSpace) @optional
                4: $(RenderSpace) @optional
            }
        }

        schema LoreSpace {
            mode: enum(generate, custom) @tag
            distribution[]: file(png)
        }

        schema RenderSpace {
            mode: enum(custom) @tag
        }

        logic Sticker {
            if .slots.4 exists {
                require .slots.3 exists
                    else throw "Slots: slot 4 requires slot 3."
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "stickers/bad.ab",
        r#"
        Sticker :: @id.bad
            slots.1: #generate(distribution: ./textures/sword.png)
            slots.4: #custom
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("slot 4 should require slot 3");

    assert!(error.to_string().contains("slot 4 requires slot 3"));
}

#[test]
#[ignore = "invalid under 1.0: a quoted `$id` in a condition is literal text (SPEC 6.9); @tag on a root field is E311"]
fn logic_supports_literal_for_variables_length_and_dynamic_paths() {
    let template = SourceFile::new(
        "templates/Rule.abt",
        r#"
        schema Rule {
            id: text(1..40)
            applies_to: enum(helmet, sword)
            applicable[] {
                id: enum(helmet, sword) @tag
            }
            slots {
                1: $(Slot)
                2: $(Slot) @optional
            }
        }

        schema Slot {
            mode: enum(generate, custom) @tag
            distribution[]: file(png) @optional
        }

        logic Rule {
            for $id in ["helmet", "sword"] {
                if length(.applicable) == 1 && .applicable.id contains "$id" {
                    require .applies_to == "$id"
                        else throw "applies_to must be $id"
                }
            }

            for $slot in [1, 2] {
                if .slots.$slot.mode == "custom" {
                    require .slots.$slot.distribution exists
                        else throw "slot $slot needs distribution"
                }
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "rules/helmet.ab",
        r#"
        Rule :: @id.helmet, @applies_to.helmet
            applicable: #helmet
            slots.1: #custom(distribution: ./textures/helmet.png)
        "#,
    );

    compile_sources(vec![template, instance], CompileOptions::default())
        .expect("literal for loops and dynamic paths should evaluate");
}

#[test]
#[ignore = "invalid under 1.0: @tag on a root field is E311 and '#tag' on a $(Schema) field is E412 (SPEC 4.8, 5.10)"]
fn logic_interpolates_loop_variables_in_errors() {
    let template = SourceFile::new(
        "templates/Rule.abt",
        r#"
        schema Rule {
            id: text(1..40)
            slots {
                1: $(Slot)
            }
        }

        schema Slot {
            mode: enum(custom) @tag
            distribution[]: file(png) @optional
        }

        logic Rule {
            for $slot in [1] {
                if .slots.$slot.mode == "custom" {
                    require .slots.$slot.distribution exists
                        else throw "slot $slot needs distribution"
                }
            }
        }
        "#,
    );
    let instance = SourceFile::new(
        "rules/bad.ab",
        r#"
        Rule :: @id.bad
            slots.1: #custom
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("missing dynamic slot distribution should fail");

    assert!(error.to_string().contains("slot 1 needs distribution"));
}

#[test]
#[ignore = "invalid under 1.0: @tag on a root field is E311 and '#tag' on a $(Schema) field is E412 (SPEC 4.8, 5.10)"]
fn interpolates_root_variables_inside_file_paths() {
    let project = unique_temp_project("path_interpolation");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::create_dir_all(project.join("assets/textures/item/sets/red_dragon")).expect("asset dir");
    fs::write(
        project.join("assets/textures/item/sets/red_dragon/sword.png"),
        b"png",
    )
    .expect("asset file");
    fs::write(
        project.join("data/Sticker.abt"),
        r#"
        schema Sticker {
            id: text(1..40)
            slots {
                1: $(Slot)
            }
        }

        schema Slot {
            mode: enum(generate) @tag
            distribution[]: file(png)
        }

        "#,
    )
    .expect("template");
    fs::write(
        project.join("data/red_dragon.ab"),
        r#"
        Sticker :: @id.red_dragon
            slots.1: #generate(distribution: ./textures/item/sets/$id/sword.png)
        "#,
    )
    .expect("instance");

    let compiled = compile_project(&project.join("data"), CompileOptions::default())
        .expect("interpolated file path should exist");
    let json = compiled.to_json_string();

    assert!(json.contains("./textures/item/sets/red_dragon/sword.png"));
}

#[test]
#[ignore = "invalid under 1.0: a clone is written after the header (SPEC 5.7, E404, A3); @tag on a root field is E311"]
fn clone_interpolates_paths_after_overrides() {
    let project = unique_temp_project("clone_path_interpolation");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::create_dir_all(project.join("assets/textures/item/sets/red_dragon"))
        .expect("red asset dir");
    fs::create_dir_all(project.join("assets/textures/item/sets/purple_dragon"))
        .expect("purple asset dir");
    fs::write(
        project.join("assets/textures/item/sets/red_dragon/sword.png"),
        b"png",
    )
    .expect("red asset file");
    fs::write(
        project.join("assets/textures/item/sets/purple_dragon/sword.png"),
        b"png",
    )
    .expect("purple asset file");
    fs::write(
        project.join("data/Sticker.abt"),
        r#"
        schema Sticker {
            id: text(1..40)
            slots {
                1: $(Slot)
            }
        }

        schema Slot {
            mode: enum(generate) @tag
            distribution[]: file(png)
        }

        logic Sticker {
            require .slots.1.distribution exists
                else throw "missing distribution"
        }
        "#,
    )
    .expect("template");
    fs::write(
        project.join("data/red_dragon.ab"),
        r#"
        Sticker :: @id.red_dragon
            slots.1: #generate(distribution: ./textures/item/sets/$id/sword.png)
        "#,
    )
    .expect("red instance");
    fs::write(
        project.join("data/purple_dragon.ab"),
        r#"
        &red_dragon.*

        Sticker :: @id.purple_dragon
        "#,
    )
    .expect("purple instance");

    let compiled = compile_project(&project.join("data"), CompileOptions::default())
        .expect("clone should interpolate after id override");
    let json = compiled.to_json_string();

    assert!(json.contains("./textures/item/sets/red_dragon/sword.png"));
    assert!(json.contains("./textures/item/sets/purple_dragon/sword.png"));
}

#[test]
#[ignore = "invalid under 1.0: @tag on a root field is E311 (SPEC 4.8)"]
fn file_type_fails_when_file_is_missing_on_disk() {
    let project = unique_temp_project("missing_file");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::write(
        project.join("data/Sticker.abt"),
        r#"
        schema Sticker {
            id: text(1..40)
            slots {
                1: $(Slot)
            }
        }

        schema Slot {
            mode: enum(generate) @tag
            distribution[]: file(png)
        }

        "#,
    )
    .expect("template");
    fs::write(
        project.join("data/red_dragon.ab"),
        r#"
        Sticker :: @id.red_dragon
            slots.1: #generate(distribution: ./textures/item/sets/$id/sword.png)
        "#,
    )
    .expect("instance");

    let error = compile_project(&project.join("data"), CompileOptions::default())
        .expect_err("missing file should fail file type validation");

    assert!(error.to_string().contains("File not found"));
    assert!(error.to_string().contains("sword.png"));
}

#[test]
#[ignore = "invalid under 1.0: naming a .abt on the command line is E806 and the direct form is gone (SPEC 2.5, A53)"]
fn direct_file_compile_uses_siblings_as_context_without_validating_unrequested_instances() {
    let project = unique_temp_project("direct_ignores_invalid_sibling");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::create_dir_all(project.join("assets/textures/item/sets/red_dragon")).expect("asset dir");
    fs::write(
        project.join("assets/textures/item/sets/red_dragon/sword.png"),
        b"png",
    )
    .expect("asset file");
    fs::write(
        project.join("data/Sticker.abt"),
        r#"
        schema Sticker {
            id: text(1..40)
            slots {
                1: $(Slot)
            }
        }

        schema Slot {
            mode: enum(generate) @tag
            distribution[]: file(png)
        }

        "#,
    )
    .expect("template");
    fs::write(
        project.join("data/red_dragon.ab"),
        r#"
        Sticker :: @id.red_dragon
            slots.1: #generate(distribution: ./textures/item/sets/red_dragon/sword.png)
        "#,
    )
    .expect("valid instance");
    fs::write(
        project.join("data/black_dragon.ab"),
        r#"
        Sticker :: @id.black_dragon
            slots.1: #generate(distribution: ./textures/item/sets/black_dragon/missing.png)
        "#,
    )
    .expect("invalid sibling");

    let direct = compile_paths(
        &[
            project.join("data/red_dragon.ab"),
            project.join("data/Sticker.abt"),
        ],
        CompileOptions::default(),
    )
    .expect("direct compile should ignore unrequested invalid sibling");
    let json = direct.to_json_string();

    assert!(json.contains("red_dragon"));
    assert!(!json.contains("black_dragon"));
    assert!(compile_project(&project.join("data"), CompileOptions::default()).is_err());
}

fn unique_temp_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("abstract_{name}_{suffix}"));
    let _ = fs::remove_dir_all(&path);
    path
}

// ---------------------------------------------------------------------------
// Parser robustness (v0.2 audit fixes)
// ---------------------------------------------------------------------------

#[test]
fn values_with_double_colons_and_urls_are_not_misparsed() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            link: text(1..120)
            window: text(1..40)
            note: text(1..80)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/site.ab",
        r#"
        Thing :: @id.site
            link: https://example.com/path
            window: 12::30
            note: "namespaced::value stays text"
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("URLs and :: values must not be parsed as headers or comments");
    let json = compiled.to_json_string();

    assert!(json.contains("https://example.com/path"));
    assert!(json.contains("12::30"));
    assert!(json.contains("namespaced::value stays text"));
}

#[test]
fn comments_require_whitespace_before_the_slashes() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            path: text(1..80)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/one.ab",
        "Thing :: @id.one\n    path: a//b // this part is a comment\n",
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("inline // after non-space must stay in the value");
    let json = compiled.to_json_string();

    assert!(json.contains("\"a//b\""));
    assert!(!json.contains("this part is a comment"));
}

#[test]
#[ignore = "invalid under 1.0: a clone is written after the header, before any assignment (SPEC 5.7, E404, A3)"]
fn multiple_clones_merge_in_order() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            name: text(1..40) @optional
            color: enum(red, blue, green) @optional
            size: enum(small, large) @optional
        }
        "#,
    );
    let base = SourceFile::new(
        "things/base.ab",
        r#"
        Thing :: @id.base, @color.red
            name: Base
        "#,
    );
    let style = SourceFile::new(
        "things/style.ab",
        r#"
        Thing :: @id.style, @color.blue, @size.large
        "#,
    );
    let combined = SourceFile::new(
        "things/combined.ab",
        r#"
        &base.*
        &style.*

        Thing :: @id.combined
        "#,
    );

    let compiled = compile_sources(
        vec![template, base, style, combined],
        CompileOptions::default(),
    )
    .expect("multiple clones should merge instead of dropping earlier ones");
    let json = compiled.to_json_string();

    // From base (not overridden by style), from style (later clone wins).
    let combined_output = json
        .split("\"combined\"")
        .nth(1)
        .expect("combined instance");
    assert!(combined_output.contains("\"name\": \"Base\""));
    assert!(combined_output.contains("\"color\": \"blue\""));
    assert!(combined_output.contains("\"size\": \"large\""));
}

#[test]
#[ignore = "invalid under 1.0: a clone is written after the header, before any assignment (SPEC 5.7, E404, A3)"]
fn partial_clones_copy_a_single_subtree() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            name: text(1..40)
            stats {
                power: int(0..100) = 0
                agility: int(0..100) = 0
            }
        }
        "#,
    );
    let source = SourceFile::new(
        "things/hero.ab",
        r#"
        Thing :: @id.hero
            name: Hero
            stats.power: 90
            stats.agility: 70
        "#,
    );
    let borrower = SourceFile::new(
        "things/sidekick.ab",
        r#"
        &hero.stats

        Thing :: @id.sidekick
            name: Sidekick
            stats.agility: 40
        "#,
    );

    let compiled = compile_sources(vec![template, source, borrower], CompileOptions::default())
        .expect("partial clone should copy just the stats subtree");
    let json = compiled.to_json_string();
    let sidekick = json.split("\"sidekick\"").nth(1).expect("sidekick output");

    assert!(sidekick.contains("\"power\": 90"));
    assert!(sidekick.contains("\"agility\": 40"));
    assert!(sidekick.contains("\"name\": \"Sidekick\""));
}

#[test]
fn unknown_fields_are_rejected_with_a_suggestion() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            rarity: enum(rare, epic) = rare
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/typo.ab",
        r#"
        Thing :: @id.typo
            rarty: epic
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("misspelled fields must fail in strict mode");

    assert!(error.to_string().contains("Unknown field 'rarty'"));
    assert!(error.to_string().contains("Did you mean 'rarity'?"));
}

#[test]
#[ignore = "invalid under 1.0: there is no lenient mode; an undeclared field is E409 (SPEC 5.12, A52)"]
fn unknown_fields_can_be_allowed_explicitly() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/extra.ab",
        r#"
        Thing :: @id.extra
            annotation: kept
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("1.0 removed the lenient mode; this test is retired");
    assert!(compiled
        .to_json_string()
        .contains("\"annotation\": \"kept\""));
}

#[test]
fn duplicate_instance_ids_are_rejected() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        "schema Thing {\n id: text(1..40)\n}\n",
    );
    let first = SourceFile::new("things/a.ab", "Thing :: @id.same\n");
    let second = SourceFile::new("things/b.ab", "Thing :: @id.same\n");

    let error = compile_sources(vec![template, first, second], CompileOptions::default())
        .expect_err("duplicate ids must fail");
    assert!(error.to_string().contains("Duplicate instance id 'same'"));
    assert!(error.to_string().contains("things/a.ab"));
}

#[test]
#[ignore = "invalid under 1.0: E301 reads: Schema `{name}` is already declared (SPEC 10.3)"]
fn duplicate_schema_names_are_rejected() {
    let first = SourceFile::new("templates/A.abt", "schema Thing {\n id: text(1..40)\n}\n");
    let second = SourceFile::new("templates/B.abt", "schema Thing {\n id: text(1..40)\n}\n");

    let error = compile_sources(vec![first, second], CompileOptions::default())
        .expect_err("duplicate schemas must fail");
    assert!(error.to_string().contains("Duplicate schema 'Thing'"));
}

#[test]
fn multiline_arrays_do_not_need_trailing_commas() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            tags[]: enum(core, public, internal)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/multi.ab",
        "Thing :: @id.multi\n    tags: [\n        core,\n        public\n    ]\n",
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("open brackets should continue statements across lines");
    assert!(compiled
        .to_raw_string()
        .contains("tags: [\"core\", \"public\"]"));
}

#[test]
fn quoted_strings_with_braces_are_never_expanded() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            desc: text(1..120)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/braces.ab",
        r#"
        Thing :: @id.braces
            desc: "use {placeholder}. literally"
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("quoted braces must stay literal");
    assert!(compiled
        .to_json_string()
        .contains("use {placeholder}. literally"));
}

#[test]
#[ignore = "invalid under 1.0: brace patterns expand only in file/image list values (SPEC 5.8, E434)"]
fn brace_patterns_allow_spaces_inside_the_braces() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            icons[]: text(1..60)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/pattern.ab",
        r#"
        Thing :: @id.pattern
            icons: ./art/{hero, thumb}.png
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("brace pattern should expand");
    let raw = compiled.to_raw_string();
    assert!(raw.contains("./art/hero.png"));
    assert!(raw.contains("./art/thumb.png"));
}

#[test]
#[ignore = "invalid under 1.0: E416 names the row, the counts and the columns (SPEC 10.4)"]
fn tuples_respect_quotes_with_parentheses_and_arity_is_checked() {
    let template = SourceFile::new(
        "templates/Page.abt",
        r#"
        schema Page {
            id: text(1..40)
            copy[] {
                key: enum(en_us, es_es) @tag
                value: text(1..60)
            }
        }
        "#,
    );
    let good = SourceFile::new(
        "pages/good.ab",
        r#"
        Page :: @id.good
            copy(key, value): (en_us, "Hi (there)"), (es_es, "Hola (tu)")
        "#,
    );
    let compiled = compile_sources(vec![template.clone(), good], CompileOptions::default())
        .expect("parentheses inside quoted tuple values must not break parsing");
    assert!(compiled.to_json_string().contains("Hi (there)"));

    let bad = SourceFile::new(
        "pages/bad.ab",
        r#"
        Page :: @id.bad
            copy(key, value): (en_us, Hello, extra)
        "#,
    );
    let error = compile_sources(vec![template, bad], CompileOptions::default())
        .expect_err("tuple arity mismatch must fail");
    assert!(error.to_string().contains("tuple arity mismatch"));
    assert!(error.to_string().contains("pages/bad.ab:3"));
}

#[test]
fn escaped_quotes_and_backslashes_roundtrip() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            note: text(1..120)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/escapes.ab",
        "Thing :: @id.escapes\n    note: \"she said \\\"hi\\\" \\\\ done\"\n",
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("escape sequences should parse");
    let json = compiled.to_json_string();
    assert!(json.contains("she said \\\"hi\\\" \\\\ done"));
}

// ---------------------------------------------------------------------------
// Type system (v0.2 features)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "invalid under 1.0: every YAML mapping key is quoted (SPEC 8.5, A48)"]
fn bool_and_float_values_keep_native_types_in_output() {
    let template = SourceFile::new(
        "templates/Item.abt",
        r#"
        schema Item {
            id: text(1..40)
            price: float(0..100)
            scale: float = 1.0
            tradable: bool = true
            featured: bool
        }
        "#,
    );
    let instance = SourceFile::new(
        "items/coin.ab",
        r#"
        Item :: @id.coin, @featured
            price: 9.5
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("bool and float should compile");
    let json = compiled.to_json_string();
    let yaml = compiled.to_yaml_string();

    assert!(json.contains("\"price\": 9.5"));
    assert!(json.contains("\"scale\": 1.0"));
    assert!(json.contains("\"tradable\": true"));
    assert!(json.contains("\"featured\": true"));
    assert!(yaml.contains("price: 9.5"));
    assert!(yaml.contains("tradable: true"));
}

#[test]
fn float_ranges_are_validated() {
    let template = SourceFile::new(
        "templates/Item.abt",
        r#"
        schema Item {
            id: text(1..40)
            opacity: float(0..1)
        }
        "#,
    );
    let instance = SourceFile::new(
        "items/glass.ab",
        r#"
        Item :: @id.glass
            opacity: 1.5
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("out-of-range float must fail");
    assert!(error.to_string().contains("Range mismatch"));
    assert!(error.to_string().contains("1.5"));
}

#[test]
fn version_like_strings_stay_text() {
    let template = SourceFile::new(
        "templates/Item.abt",
        r#"
        schema Item {
            id: text(1..40)
            mc_version: text(1..20)
        }
        "#,
    );
    let instance = SourceFile::new(
        "items/pack.ab",
        r#"
        Item :: @id.pack
            mc_version: 1.21.5
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("multi-dot versions are text, not floats");
    assert!(compiled
        .to_json_string()
        .contains("\"mc_version\": \"1.21.5\""));
}

#[test]
fn bare_types_without_constraints_are_accepted() {
    let template = SourceFile::new(
        "templates/Item.abt",
        r#"
        schema Item {
            id: text(1..40)
            label: text
            count: int
            weight: float
            active: bool
        }
        "#,
    );
    let instance = SourceFile::new(
        "items/free.ab",
        r#"
        Item :: @id.free, @active.true
            label: anything goes here
            count: 12
            weight: 0.25
        "#,
    );

    compile_sources(vec![template, instance], CompileOptions::default())
        .expect("bare text/int/float/bool types should work");
}

#[test]
fn enum_list_wildcards_expand_against_the_vocabulary() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            flags[]: enum(hat_overrides_helmet, hat_has_variations, other) @optional
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/hats.ab",
        r#"
        Thing :: @id.hats
            flags: hat_*
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("enum list wildcards should expand");
    let raw = compiled.to_raw_string();
    assert!(raw.contains("hat_overrides_helmet"));
    assert!(raw.contains("hat_has_variations"));
    assert!(!raw.contains("\"other\""));
}

#[test]
fn image_extension_rules_apply_even_in_memory() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            icon: image(png 128x128)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/bad.ab",
        r#"
        Thing :: @id.bad
            icon: ./icons/logo.jpg
        "#,
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("jpg must be rejected when only png is allowed");
    // E420 names the extension and the allowed extensions, not the declared
    // sizes (SPEC §10.4); the size check is E423 and needs the file on disk.
    let message = error.to_string();
    assert!(message.contains("error[E420]"), "{message}");
    assert!(message.contains("allowed: png."), "{message}");
}

#[test]
fn image_probe_checks_signature_and_dimensions_on_disk() {
    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    let project = unique_temp_project("image_probe");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::create_dir_all(project.join("assets/icons")).expect("asset dir");
    fs::write(project.join("assets/icons/ok.png"), png_bytes(32, 32)).expect("ok icon");
    fs::write(project.join("assets/icons/small.png"), png_bytes(16, 16)).expect("small icon");
    fs::write(
        project.join("assets/icons/fake.png"),
        b"GIF89a\x20\x00\x20\x00xxx",
    )
    .expect("fake icon");
    fs::write(
        project.join("data/Thing.abt"),
        r#"
        schema Thing {
            id: text(1..40)
            icon: image(png 32x32)
        }
        "#,
    )
    .expect("template");

    fs::write(
        project.join("data/good.ab"),
        "Thing :: @id.good\n    icon: ./icons/ok.png\n",
    )
    .expect("good instance");
    compile_project(&project.join("data"), CompileOptions::default())
        .expect("32x32 png should validate");

    fs::write(
        project.join("data/good.ab"),
        "Thing :: @id.good\n    icon: ./icons/small.png\n",
    )
    .expect("small instance");
    let error = compile_project(&project.join("data"), CompileOptions::default())
        .expect_err("16x16 png must fail the 32x32 constraint");
    assert!(error.to_string().contains("Image size mismatch"));
    assert!(error.to_string().contains("16x16"));

    fs::write(
        project.join("data/good.ab"),
        "Thing :: @id.good\n    icon: ./icons/fake.png\n",
    )
    .expect("fake instance");
    let error = compile_project(&project.join("data"), CompileOptions::default())
        .expect_err("gif bytes in a .png file must fail");
    assert!(error.to_string().contains("Image content mismatch"));

    fs::remove_dir_all(&project).ok();
}

#[test]
fn skip_asset_checks_compiles_without_files_on_disk() {
    let project = unique_temp_project("skip_assets");
    fs::create_dir_all(project.join("data")).expect("data dir");
    fs::write(
        project.join("data/Thing.abt"),
        r#"
        schema Thing {
            id: text(1..40)
            icon: image(png)
            texture: file(png)
        }
        "#,
    )
    .expect("template");
    fs::write(
        project.join("data/one.ab"),
        "Thing :: @id.one\n    icon: ./icons/missing.png\n    texture: ./missing.png\n",
    )
    .expect("instance");

    assert!(compile_project(&project.join("data"), CompileOptions::default()).is_err());
    compile_project(
        &project.join("data"),
        CompileOptions {
            skip_asset_checks: true,
            ..CompileOptions::default()
        },
    )
    .expect("skip_asset_checks should ignore missing files");
    fs::remove_dir_all(&project).ok();
}

// ---------------------------------------------------------------------------
// Logic upgrades (v0.2 features)
// ---------------------------------------------------------------------------

#[test]
fn logic_else_and_else_if_branches_execute() {
    let template = SourceFile::new(
        "templates/Card.abt",
        r#"
        schema Card {
            id: text(1..40)
            kind: enum(big, medium, tiny)
            size: enum(small, mid, large) = small
        }

        logic Card {
            if .kind == "big" {
                derive .size = large
            } else if .kind == "medium" {
                derive .size = mid
            } else {
                derive .size = small
            }
        }
        "#,
    );

    for (kind, expected) in [("big", "large"), ("medium", "mid"), ("tiny", "small")] {
        let instance = SourceFile::new("cards/one.ab", format!("Card :: @id.one, @kind.{kind}\n"));
        let compiled = compile_sources(vec![template.clone(), instance], CompileOptions::default())
            .expect("else chains should compile");
        assert!(
            compiled
                .to_json_string()
                .contains(&format!("\"size\": \"{expected}\"")),
            "kind {kind} should produce size {expected}"
        );
    }
}

#[test]
fn logic_not_operator_negates_conditions() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            flags[]: enum(safe, banned) @optional
        }

        logic Thing {
            require not .flags contains "banned"
                else throw "Banned things are not allowed."

            if !(.flags contains "safe") {
                derive .flags = safe
            }
        }
        "#,
    );

    let bad = SourceFile::new("things/bad.ab", "Thing :: @id.bad\n    flags: banned\n");
    let error = compile_sources(vec![template.clone(), bad], CompileOptions::default())
        .expect_err("not-contains must reject");
    assert!(error.to_string().contains("Banned things are not allowed"));

    let empty = SourceFile::new("things/empty.ab", "Thing :: @id.empty\n");
    let compiled = compile_sources(vec![template, empty], CompileOptions::default())
        .expect("bang-negation should run the derive");
    assert!(compiled.to_raw_string().contains("flags: [\"safe\"]"));
}

#[test]
fn derive_if_missing_respects_authored_values() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            wave: int(1..99) @optional
        }

        logic Thing {
            derive? .wave = 3
        }
        "#,
    );

    let defaulted = SourceFile::new("things/defaulted.ab", "Thing :: @id.defaulted\n");
    let compiled = compile_sources(vec![template.clone(), defaulted], CompileOptions::default())
        .expect("derive? should fill missing values");
    assert!(compiled.to_json_string().contains("\"wave\": 3"));

    let explicit = SourceFile::new("things/explicit.ab", "Thing :: @id.explicit, @wave.7\n");
    let compiled = compile_sources(vec![template, explicit], CompileOptions::default())
        .expect("derive? should not override authored values");
    assert!(compiled.to_json_string().contains("\"wave\": 7"));
}

#[test]
fn derive_values_interpolate_variables_with_native_types() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            slot_count: int(0..9) = 0
            banner: text(1..80) @optional
        }

        logic Thing {
            for $n in [3] {
                derive .slot_count = $n
                derive .banner = "pack $id uses $n slots"
            }
        }
        "#,
    );
    let instance = SourceFile::new("things/kit.ab", "Thing :: @id.kit\n");

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("derive interpolation should work");
    let json = compiled.to_json_string();

    assert!(
        json.contains("\"slot_count\": 3"),
        "int type preserved: {json}"
    );
    assert!(json.contains("pack kit uses 3 slots"));
}

#[test]
fn header_flags_without_values_become_true() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            featured: bool = false
        }
        "#,
    );
    let instance = SourceFile::new("things/star.ab", "Thing :: @id.star, @featured\n");

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("bare @flags should mean true");
    assert!(compiled.to_json_string().contains("\"featured\": true"));
}

#[test]
fn dollar_interpolation_handles_prefixes_braces_and_escapes() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        r#"
        schema Thing {
            id: text(1..40)
            identity: text(1..40)
            path_value: text(1..120)
            braced: text(1..60)
            escaped: text(1..60)
        }
        "#,
    );
    let instance = SourceFile::new(
        "things/red.ab",
        r#"
        Thing :: @id.red
            identity: blue_ish
            path_value: ./sets/$identity/$id.png
            braced: ${id}_suffix
            escaped: "$$id is literal"
        "#,
    );

    let compiled = compile_sources(vec![template, instance], CompileOptions::default())
        .expect("interpolation should resolve");
    let json = compiled.to_json_string();

    assert!(
        json.contains("./sets/blue_ish/red.png"),
        "longest key first: {json}"
    );
    assert!(json.contains("red_suffix"));
    assert!(json.contains("$id is literal"));
}

#[test]
fn error_messages_carry_line_numbers_for_parse_problems() {
    let template = SourceFile::new(
        "templates/Thing.abt",
        "schema Thing {\n id: text(1..40)\n}\n",
    );
    let instance = SourceFile::new(
        "things/broken.ab",
        "Thing :: @id.broken\n    just some words\n",
    );

    let error = compile_sources(vec![template, instance], CompileOptions::default())
        .expect_err("statements without ':' must fail");
    assert!(
        error.to_string().contains("things/broken.ab:2"),
        "line number expected in: {error}"
    );
}
