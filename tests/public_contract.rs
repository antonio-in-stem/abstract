//! Public export checks run through the actual, single compiler pipeline.
use abstract_lang::public_contract::PublicCompilation;
use abstract_lang::{
    compile_public_sources, compile_sources, CompileOptions, ErrorId, SourceFile, Value,
};

fn sources(schema: &str, instance: &str) -> Vec<SourceFile> {
    vec![
        SourceFile::new("templates/Settings.abt", schema),
        SourceFile::new("settings.ab", instance),
    ]
}
fn compile(schema: &str, instance: &str) -> Result<PublicCompilation, abstract_lang::Diagnostics> {
    compile_public_sources(sources(schema, instance), CompileOptions::default())
}
fn entries(product: &PublicCompilation) -> &[Value] {
    let Value::List(v) = product.fragment().get("entries").unwrap() else {
        panic!("entries array")
    };
    v
}
fn variants(entry: &Value) -> &[Value] {
    let Value::List(v) = entry.get("variants").unwrap() else {
        panic!("variants array")
    };
    v
}
fn text(s: &str) -> Value {
    Value::Text(s.into())
}
fn error(schema: &str, instance: &str) -> abstract_lang::Diagnostics {
    // Dependency rejection must not masquerade as an ordinary compile failure.
    compile_sources(sources(schema, instance), CompileOptions::default())
        .expect("ordinary compile valid");
    let e = compile(schema, instance).expect_err("profile must reject");
    assert_eq!(e.first().unwrap().id, ErrorId::E702);
    e
}

#[test]
fn five_scalar_types_are_exact_and_do_not_change_the_document() {
    let schema = "schema Settings {\n caption: text(1..80) @public = \"café\"\n quota: int @public = 9223372036854775807\n scale: float @public = -0.0\n enabled: bool @public = true\n tone: enum(Quiet, bright) @public = quiet\n}\n";
    let p = compile(schema, "Settings :: @id.main\n").unwrap();
    let old = compile_sources(
        sources(&schema.replace(" @public", ""), "Settings :: @id.main\n"),
        CompileOptions::default(),
    )
    .unwrap();
    assert_eq!(p.document, old);
    assert_eq!(entries(&p).len(), 5);
    let find = |name: &str| {
        entries(&p)
            .iter()
            .find(|e| e.get("path") == Some(&Value::List(vec![text(name)])))
            .unwrap()
    };
    assert_eq!(
        variants(find("quota"))[0].get("default"),
        Some(&text("9223372036854775807"))
    );
    assert_eq!(
        variants(find("scale"))[0].get("default"),
        Some(&text("8000000000000000"))
    );
    assert_eq!(
        variants(find("enabled"))[0].get("default"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        variants(find("caption"))[0].get("default"),
        Some(&text("café"))
    );
    assert_eq!(
        find("tone").get("domain").unwrap().get("members"),
        Some(&Value::List(vec![text("quiet"), text("bright")]))
    );
    let rendered = p.render().unwrap();
    assert!(rendered.contains("\"bindingStatus\":\"unbound\""));
    let sha = abstract_lang::crypto::sha256(p.document.to_json_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert!(rendered.contains(&format!("\"documentSha256\":\"{sha}\"")));
    assert!(rendered.ends_with("}\n"));
    assert!(!rendered.contains("templates/Settings.abt"));
}

#[test]
fn signed_zero_and_i64_minimum_survive_all_version_materializations() {
    let p = compile("versions 1..3\nschema Settings {\n f: float @public\n n: int @public = -9223372036854775808\n}\n",
        "Settings :: @id.main\n f: -0 @removed(2)\n f: -0.0 @since(2) @removed(3)\n f: 0.0 @since(3)\n").unwrap();
    let v = variants(&entries(&p)[0]);
    assert_eq!(v.len(), 3);
    assert_eq!(v[0].get("default"), Some(&text("0000000000000000")));
    assert_eq!(v[1].get("default"), Some(&text("8000000000000000")));
    assert_eq!(v[2].get("default"), Some(&text("0000000000000000")));
    assert_eq!(
        variants(&entries(&p)[1])[0].get("default"),
        Some(&text("-9223372036854775808"))
    );
}

#[test]
fn optional_ancestor_absence_is_explicit_and_not_writable() {
    let p = compile("versions 1..3\nschema Settings {\n group @optional {\n caption: text @public = \"baked\"\n }\n}\n",
        "Settings :: @id.main\n group.caption: shown @since(2)\n").unwrap();
    let v = variants(&entries(&p)[0]);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].get("presence"), Some(&text("absent")));
    assert_eq!(v[0].get("writable"), Some(&Value::Bool(false)));
    assert_eq!(v[0].get("default"), None);
    assert_eq!(v[1].get("default"), Some(&text("shown")));
    assert_eq!(
        v[1].get("versions").unwrap().get("max"),
        Some(&Value::Int(3))
    );
}

#[test]
fn nested_occurrences_and_multiple_instances_have_structured_stable_ids() {
    let schema = "schema Inner {\n n: int @public = 4\n}\nschema Settings {\n left: $(Inner)\n right: $(Inner)\n}\n";
    let instance =
        "Settings :: @id.z\n left.n: 1\n right.n: 2\nSettings :: @id.a\n left.n: 3\n right.n: 4\n";
    let p = compile(schema, instance).unwrap();
    assert_eq!(entries(&p).len(), 4);
    assert_eq!(entries(&p)[0].get("rootInstanceId"), Some(&text("a")));
    assert_eq!(
        entries(&p)[0].get("path"),
        Some(&Value::List(vec![text("left"), text("n")]))
    );
    assert_eq!(entries(&p)[0].get("declaringSchema"), Some(&text("Inner")));
    assert_eq!(
        entries(&p)[0].get("declaringPath"),
        Some(&Value::List(vec![text("n")]))
    );
    let mut input = sources(schema, instance);
    input.push(SourceFile::new(
        "templates/Unrelated.abt",
        "schema Unrelated {\n x: text\n}\n",
    ));
    input.reverse();
    let other = compile_public_sources(input, CompileOptions::default()).unwrap();
    assert_eq!(p.fragment(), other.fragment());
}

#[test]
fn declaration_and_instance_windows_are_intersected() {
    let p = compile(
        "versions 1..4\nschema Settings {\n n: int @public @since(2) @removed(4) = 3\n}\n",
        "Settings :: @id.main @since(3)\n",
    )
    .unwrap();
    let range = entries(&p)[0].get("declarationVersions").unwrap();
    assert_eq!(range.get("min"), Some(&Value::Int(3)));
    assert_eq!(range.get("max"), Some(&Value::Int(3)));
}

#[test]
fn unrelated_static_logic_is_admitted() {
    let p = compile("schema Settings {\n public_value: int @public = 4\n private_value: int = 5\n derived: int\n}\nlogic Settings {\n require .private_value > 0 else throw \"positive\"\n derive .derived = .private_value\n}\n", "Settings :: @id.main\n").unwrap();
    assert_eq!(entries(&p).len(), 1);
}

#[test]
fn untaken_branch_read_is_rejected_with_both_source_locations() {
    let e = error("schema Settings {\n n: int @public = 4\n choose: bool = false\n result: int = 0\n}\nlogic Settings {\n if .choose {\n derive .result = .n\n }\n}\n", "Settings :: @id.main\n");
    assert!(!e.first().unwrap().notes.is_empty());
}

#[test]
fn untaken_conditional_write_and_derive_missing_are_rejected() {
    error("schema Settings {\n n: int @public = 4\n choose: bool = false\n}\nlogic Settings {\n if .choose {\n derive .n = 5\n }\n}\n", "Settings :: @id.main\n");
    error(
        "schema Settings {\n n: int @public @optional\n}\nlogic Settings {\n derive? .n = 5\n}\n",
        "Settings :: @id.main\n",
    );
}

#[test]
fn require_presence_and_length_observations_are_rejected() {
    error("schema Settings {\n n: text @public = x\n}\nlogic Settings {\n require length(.n) > 0 else throw \"nonempty\"\n}\n", "Settings :: @id.main\n");
    error("schema Settings {\n n: text @public @optional\n}\nlogic Settings {\n require .n exists else throw \"present\"\n}\n", "Settings :: @id.main\n n: x\n");
}

#[test]
fn interpolation_defaults_authored_values_and_clones_reject() {
    error(
        "schema Settings {\n n: text @public\n label: text = \"$n\"\n}\n",
        "Settings :: @id.main\n n: x\n",
    );
    error(
        "schema Settings {\n n: text @public\n label: text\n}\n",
        "Settings :: @id.main\n n: x\n label: $n\n",
    );
    error(
        "schema Settings {\n n: text @public\n}\n",
        "Settings :: @id.source\n n: x\nSettings :: @id.copy\n &source\n",
    );
}

#[test]
fn unsupported_nominations_are_not_silently_omitted() {
    error(
        "schema Settings {\n good: int @public = 1\n bad[]: int @public = [1, 2]\n}\n",
        "Settings :: @id.main\n",
    );
    error(
        "schema Settings {\n group @public {\n n: int = 1\n }\n}\n",
        "Settings :: @id.main\n group.n: 1\n",
    );
    error(
        "schema Settings {\n list[] {\n n: int @public\n }\n}\n",
        "Settings :: @id.main\n list(n): (1), (2)\n",
    );
}

#[test]
fn empty_fragment_is_explicit_and_document_replacement_is_detected() {
    let mut p = compile(
        "schema Settings {\n n: int = 2\n}\n",
        "Settings :: @id.main\n",
    )
    .unwrap();
    assert!(entries(&p).is_empty());
    assert_eq!(p.fragment().get("complete"), Some(&Value::Bool(true)));
    p.document = compile_sources(Vec::new(), CompileOptions::default()).unwrap();
    assert_eq!(p.render().unwrap_err().first().unwrap().id, ErrorId::E703);
}

#[test]
fn too_much_public_text_fails_without_a_partial_product() {
    let s = "a".repeat(abstract_lang::public_contract::MAX_FRAGMENT_BYTES + 1);
    let schema = format!("schema Settings {{\n caption: text @public = \"{s}\"\n}}\n");
    let e = compile(&schema, "Settings :: @id.main\n").unwrap_err();
    assert_eq!(e.first().unwrap().id, ErrorId::E703);
}

#[test]
fn optional_schema_chain_is_bounded_before_occurrence_expansion() {
    let mut schema = String::new();
    for n in 0..70 {
        schema.push_str(&format!(
            "schema S{n} {{\n child: $(S{}) @optional\n}}\n",
            n + 1
        ));
    }
    schema.push_str("schema S70 {\n n: int @public = 1\n}\n");
    let instance = "S0 :: @id.main\n";
    compile_sources(sources(&schema, instance), CompileOptions::default())
        .expect("ordinary optional chain is valid");
    let e = compile(&schema, instance).unwrap_err();
    assert_eq!(e.first().unwrap().id, ErrorId::E703);
}

#[test]
fn dynamic_path_inside_an_untaken_literal_loop_is_rejected() {
    error(
        "schema Settings {\n group {\n n: int @public = 4\n }\n choose: bool = false\n result: int = 0\n}\nlogic Settings {\n if .choose {\n for $slot in [n] {\n derive .result = .group.$slot\n }\n }\n}\n",
        "Settings :: @id.main\n group.n: 4\n",
    );
}

#[test]
fn nested_logic_reads_its_local_field_and_keeps_a_public_root_sibling_independent() {
    let p = compile(
        "schema Inner {\n n: int = 7\n copied: int\n}\nschema Settings {\n n: int @public = 4\n child: $(Inner)\n}\nlogic Inner {\n derive .copied = .n\n}\n",
        "Settings :: @id.main\n child.n: 7\n",
    ).unwrap();
    assert_eq!(entries(&p).len(), 1);
    assert_eq!(
        entries(&p)[0].get("path"),
        Some(&Value::List(vec![text("n")]))
    );
    assert_eq!(
        variants(&entries(&p)[0])[0].get("default"),
        Some(&text("4"))
    );
    assert_eq!(
        p.document.data()[0].get("child").unwrap().get("copied"),
        Some(&Value::Int(7))
    );
}

#[test]
fn nested_logic_reading_its_public_local_field_is_rejected() {
    error(
        "schema Inner {\n n: int @public = 7\n copied: int\n}\nschema Settings {\n child: $(Inner)\n}\nlogic Inner {\n derive .copied = .n\n}\n",
        "Settings :: @id.main\n child.n: 7\n",
    );
}
