use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use abstract_lang::{compile_paths, compile_project, compile_sources, CompileOptions, SourceFile};

#[test]
fn compiles_example_project_to_human_readable_data() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example");

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
