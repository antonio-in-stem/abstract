//! Public nomination is schema metadata, independent of export-profile admission.

use abstract_lang::ast::{parse, SourceUnit, TemplateFile, TemplateItem};
use abstract_lang::schema::{Modifiers, SchemaDecl};
use abstract_lang::{
    compile_sources, CompileOptions, CompiledDocument, Diagnostics, ErrorId, Format, Position,
    SourceFile,
};

fn template(text: &str) -> TemplateFile {
    match parse(&SourceFile::new("templates/settings.abt", text)).expect("template parses") {
        SourceUnit::Template(file) => file,
        SourceUnit::Instance(_) => panic!("a template was expected"),
    }
}

fn schema(text: &str) -> SchemaDecl {
    template(text)
        .items
        .into_iter()
        .find_map(|item| match item {
            TemplateItem::Schema(schema) => Some(schema),
            _ => None,
        })
        .expect("a schema was declared")
}

fn errors(text: &str) -> Diagnostics {
    parse(&SourceFile::new("templates/settings.abt", text)).expect_err("template is rejected")
}

fn compile(template: &str, instances: &str) -> Result<CompiledDocument, Diagnostics> {
    compile_sources(
        vec![
            SourceFile::new("templates/settings.abt", template),
            SourceFile::new("items/settings.ab", instances),
        ],
        CompileOptions::default(),
    )
}

fn marked_pair(template: &str, instances: &str) -> (CompiledDocument, CompiledDocument) {
    assert!(template.contains(" __PUBLIC__"));
    let plain = compile(&template.replace(" __PUBLIC__", ""), instances)
        .expect("the original compilation succeeds");
    let nominated = compile(&template.replace(" __PUBLIC__", " @public"), instances)
        .expect("nomination does not change ordinary compilation");
    for format in [Format::Json, Format::Yaml, Format::Raw] {
        assert_eq!(
            nominated.render(format).unwrap(),
            plain.render(format).unwrap(),
            "nomination changed {format} output"
        );
    }
    (plain, nominated)
}

#[test]
fn an_unmarked_field_has_no_nomination_or_position() {
    let defaults = Modifiers::default();
    assert!(!defaults.public_);
    assert!(defaults.public_at.is_none());
    let schema = schema("schema Settings {\n    value: text\n}\n");
    assert!(!schema.fields[0].is_public());
    assert!(schema.fields[0].modifiers.public_at.is_none());
}

#[test]
fn nomination_retains_its_at_position_with_bom_crlf_and_a_tab() {
    let schema = schema("\u{feff}schema Settings {\r\n\tMax-Count: int @public\r\n}\r\n");
    let field = &schema.fields[0];
    assert!(field.is_public());
    assert_eq!(field.name, "max_count");
    assert_eq!(field.spelled, "Max-Count");
    assert_eq!(field.modifiers.public_at, Some(Position::new(2, 17)));
    assert_eq!(field.at.file, "templates/settings.abt");
}

#[test]
fn parser_nomination_does_not_restrict_any_existing_type_or_list() {
    let types = [
        "text(1..80)",
        "int(-4..8)",
        "float(0.5..2.0)",
        "bool",
        "enum(quiet, bright)",
        "file(png)",
        "image(png 16x16)",
        "ref(Other)",
        "$(Other)",
    ];
    for ty in types {
        for list in ["", "[]", "[1..2]"] {
            let parsed = schema(&format!(
                "schema Settings {{\n    value{list}: {ty} @public\n}}\n"
            ));
            assert!(parsed.fields[0].is_public(), "{ty}{list}");
            assert_eq!(parsed.fields[0].is_list(), !list.is_empty());
        }
    }
}

#[test]
fn group_nomination_is_local_and_can_coexist_with_a_tag_nomination() {
    let schema = schema(
        "schema Settings {\n    group @public {\n        private_value: text\n        key: enum(a, b) @tag @public\n    }\n    rows[] @public {\n        private_value: bool\n    }\n}\n",
    );
    assert!(schema.fields[0].is_public());
    let group = schema.fields[0].group_fields().unwrap();
    assert!(!group[0].is_public());
    assert!(group[0].modifiers.public_at.is_none());
    assert!(group[1].is_public());
    assert!(group[1].is_tag());
    assert!(schema.fields[1].is_public());
    assert!(!schema.fields[1].group_fields().unwrap()[0].is_public());
}

#[test]
fn repeated_nomination_is_e303_at_the_second_modifier() {
    let errors = errors("schema Settings {\n    a: text @public @public\n}\n");
    assert_eq!(errors.len(), 1);
    let error = errors.iter().next().unwrap();
    assert_eq!(error.id, ErrorId::E303);
    assert_eq!(error.position, Some(Position::new(2, 21)));
    assert_eq!(error.message, "Modifier '@public' is repeated.");
    let group = errors_for_group_repeat();
    assert_eq!(group.iter().next().unwrap().id, ErrorId::E303);
}

fn errors_for_group_repeat() -> Diagnostics {
    errors("schema Settings {\n    group @public @public {\n        value: text\n    }\n}\n")
}

#[test]
fn nomination_does_not_take_arguments() {
    for arguments in ["()", "(1)", "(a, b)", " (1)"] {
        let errors = errors(&format!(
            "schema Settings {{\n    value: text @public{arguments}\n}}\n"
        ));
        assert_eq!(errors.iter().next().unwrap().id, ErrorId::E210);
    }
}

#[test]
fn misplaced_nomination_uses_existing_modifier_position_errors() {
    for (line, expected) in [
        ("value @public: text", ErrorId::E210),
        ("value: bool = false @public", ErrorId::E303),
        ("value: text @ public", ErrorId::E210),
    ] {
        let errors = errors(&format!("schema Settings {{\n    {line}\n}}\n"));
        assert_eq!(errors.iter().next().unwrap().id, expected, "{line}");
    }
}

#[test]
fn envelope_reservations_are_not_bypassed_by_nomination() {
    for line in ["id: text @public", "template: text @public"] {
        let errors = errors(&format!("schema Settings {{\n    {line}\n}}\n"));
        assert_eq!(errors.iter().next().unwrap().id, ErrorId::E314);
    }
    let group = schema(
        "schema Settings {\n    group {\n        id: text @public\n        template: text @public\n    }\n}\n",
    );
    assert!(group.fields[0]
        .group_fields()
        .unwrap()
        .iter()
        .all(|field| field.is_public()));
}

#[test]
fn public_spelling_in_fields_enums_literals_and_comments_is_not_a_modifier() {
    let source = "schema Settings {\n    public: text = \"literal @public\"\n    mode: enum(public, private) = public\n    // @public does not nominate a field\n}\n";
    assert!(schema(source).fields.iter().all(|field| !field.is_public()));
    let output = compile(source, "Settings :: @id.main\n")
        .unwrap()
        .render(Format::Json)
        .unwrap();
    assert!(output.contains("\"public\": \"literal @public\""));
    assert!(output.contains("\"mode\": \"public\""));
}

#[test]
fn scalar_defaults_and_exact_numeric_output_do_not_change() {
    let (_, nominated) = marked_pair(
        "schema Settings {\n    caption: text(1..80) __PUBLIC__ = \"Garden 🌿 é\"\n    minimum: int __PUBLIC__ = -9223372036854775808\n    maximum: int __PUBLIC__ = 9223372036854775807\n    negative_zero: float __PUBLIC__ = -0.0\n    scale: float(0.5..2.0) __PUBLIC__ = 1.25\n    enabled: bool __PUBLIC__ = true\n    tone: enum(quiet, bright) __PUBLIC__ = bright\n    private_value: text = \"retained\"\n}\n",
        "Settings :: @id.main\n",
    );
    let json = nominated.render(Format::Json).unwrap();
    assert!(json.contains("\"minimum\": -9223372036854775808"));
    assert!(json.contains("\"maximum\": 9223372036854775807"));
    assert!(json.contains("\"negative_zero\": -0.0"));
    assert!(json.contains("\"private_value\": \"retained\""));
}

#[test]
fn presence_versions_and_even_affected_logic_keep_ordinary_semantics() {
    let (_, nominated) = marked_pair(
        "versions 1..3\nschema Settings {\n    base: int __PUBLIC__ = 7\n    copied: int __PUBLIC__ @optional\n    late: bool @since(2) __PUBLIC__ = true\n    absent: text @optional __PUBLIC__\n}\nlogic Settings {\n    derive .copied = .base\n    require .copied == 7 else throw \"copy must agree\"\n}\n",
        "Settings :: @id.main\n",
    );
    let json = nominated.render(Format::Json).unwrap();
    assert!(json.contains("\"copied\": 7"));
    assert!(json.contains("\"late\": true"));
    assert!(!json.contains("\"absent\""));
    assert!(!nominated.overlays().is_empty());
}

#[test]
fn nomination_does_not_relax_requiredness_or_validation() {
    for source in [
        "schema Settings {\n    value: int(1..10) __PUBLIC__ = 99\n}\n",
        "schema Settings {\n    value: text __PUBLIC__\n}\n",
        "schema Settings {\n    value: int __PUBLIC__ @optional = 1\n}\n",
    ] {
        let plain = compile(&source.replace(" __PUBLIC__", ""), "Settings :: @id.main\n")
            .expect_err("the original is invalid");
        let marked = compile(
            &source.replace(" __PUBLIC__", " @public"),
            "Settings :: @id.main\n",
        )
        .expect_err("nomination must not accept invalid data");
        assert_eq!(
            marked.iter().map(|error| error.id).collect::<Vec<_>>(),
            plain.iter().map(|error| error.id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn disjoint_windows_do_not_permit_redeclaring_a_nominated_field() {
    let errors = errors(
        "versions 1..2\nschema Settings {\n    value: text @public @removed(2)\n    value: text @since(2)\n}\n",
    );
    assert_eq!(errors.iter().next().unwrap().id, ErrorId::E302);
}
