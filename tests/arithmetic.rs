use abstract_lang::{
    compile_public_sources, compile_sources, CompileOptions, Diagnostics, ErrorId, SourceFile,
    Value,
};

fn sources(template: &str, instance: &str) -> Vec<SourceFile> {
    vec![
        SourceFile::new("templates/Numbers.abt", template),
        SourceFile::new("numbers.ab", instance),
    ]
}

fn compile(template: &str, instance: &str) -> Result<abstract_lang::CompiledDocument, Diagnostics> {
    compile_sources(sources(template, instance), CompileOptions::default())
}

fn value<'a>(document: &'a abstract_lang::CompiledDocument, name: &str) -> &'a Value {
    document.data()[0]
        .get(name)
        .unwrap_or_else(|| panic!("missing {name}"))
}

#[test]
fn checked_integer_arithmetic_and_aggregates_compile_to_typed_values() {
    let template = r#"schema Numbers {
    unit-price: int
    discount: int
    quantities[]: int
    empty_floats[]: float @optional
    subtotal: int
    quotient: int
    remainder: int
    average: float
    empty_total: float
}
logic Numbers {
    derive .subtotal = calc((.unit-price - .discount) * sum(.quantities))
    derive .quotient = calc(div(.subtotal, 7))
    derive .remainder = calc(.subtotal % 7)
    derive .average = calc(avg(.quantities))
    derive .empty_total = calc(sum(.empty_floats))
    require calc(.subtotal / 2) == 36.0 else throw "bad total"
}
"#;
    let document = compile(
        template,
        "Numbers :: @id.main\nunit-price: 10\ndiscount: 1\nquantities: 2, 3, 3\n",
    )
    .expect("arithmetic compiles");
    assert_eq!(value(&document, "subtotal"), &Value::Int(72));
    assert_eq!(value(&document, "quotient"), &Value::Int(10));
    assert_eq!(value(&document, "remainder"), &Value::Int(2));
    assert_eq!(value(&document, "average"), &Value::Float(8.0 / 3.0));
    assert_eq!(value(&document, "empty_total"), &Value::Float(0.0));
}

#[test]
fn functions_have_explicit_domains_and_deterministic_integer_pow() {
    let template = r#"schema Numbers {
    power: int
    root: float
    rounded: int
    bounded: int
}
logic Numbers {
    derive .power = calc(pow(3, 12))
    derive .root = calc(sqrt(81))
    derive .rounded = calc(round(-2.5))
    derive .bounded = calc(clamp(99, 0, 10))
}
"#;
    let document = compile(template, "Numbers :: @id.main\n").unwrap();
    assert_eq!(value(&document, "power"), &Value::Int(531_441));
    assert_eq!(value(&document, "root"), &Value::Float(9.0));
    assert_eq!(value(&document, "rounded"), &Value::Int(-3));
    assert_eq!(value(&document, "bounded"), &Value::Int(10));
}

#[test]
fn arithmetic_is_opt_in_and_preserves_ordinary_text() {
    let template = r#"schema Numbers {
    formula: text
    spaced: text
}
logic Numbers {
    derive .formula = 2 + 3
    derive .spaced = calc (2 + 3)
}
"#;
    let document = compile(template, "Numbers :: @id.main\n").unwrap();
    assert_eq!(value(&document, "formula"), &Value::Text("2 + 3".into()));
    assert_eq!(
        value(&document, "spaced"),
        &Value::Text("calc (2 + 3)".into())
    );
}

#[test]
fn static_type_and_runtime_numeric_failures_are_distinct() {
    let static_error = compile(
        "schema Numbers {\n text_value: text\n result: int\n}\nlogic Numbers {\n derive .result = calc(.text_value + 1)\n}\n",
        "Numbers :: @id.main\ntext_value: x\n",
    )
    .unwrap_err();
    assert_eq!(static_error.first().unwrap().id, ErrorId::E524);

    for (ty, expression) in [
        ("int", "calc(9223372036854775807 + 1)"),
        ("int", "calc(div(-9223372036854775808, -1))"),
        ("float", "calc(1 / 0)"),
        ("float", "calc(sqrt(-1))"),
        ("float", "calc(avg(.empty))"),
        ("float", "calc(9007199254740993 / 1)"),
    ] {
        let template = format!(
            "schema Numbers {{\n empty[]: int @optional\n result: {ty}\n}}\nlogic Numbers {{\n derive .result = {expression}\n}}\n"
        );
        let error = compile(&template, "Numbers :: @id.main\n").unwrap_err();
        assert_eq!(
            error.first().unwrap().id,
            ErrorId::E525,
            "{expression}: {error}"
        );
    }
}

#[test]
fn exactly_representable_large_integers_may_enter_float_operations() {
    let template = "schema Numbers {\n result: float\n}\nlogic Numbers {\n derive .result = calc(1152921504606846976 / 2)\n}\n";
    let document = compile(template, "Numbers :: @id.main\n").unwrap();
    assert_eq!(
        value(&document, "result"),
        &Value::Float(576_460_752_303_423_500.0)
    );
}

#[test]
fn public_export_rejects_a_calc_dependency_even_in_an_untaken_branch() {
    let template = r#"schema Numbers {
    price: int @public = 4
    choose: bool = false
    result: int = 0
}
logic Numbers {
    if .choose {
        derive .result = calc(.price * 2)
    }
}
"#;
    compile(template, "Numbers :: @id.main\n").expect("ordinary compile");
    let error = compile_public_sources(
        sources(template, "Numbers :: @id.main\n"),
        CompileOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error.first().unwrap().id, ErrorId::E702);
}

#[test]
fn calc_comparisons_refuse_lossy_integer_promotion() {
    let error = compile(
        "schema Numbers {\n ok: bool = true\n}\nlogic Numbers {\n require calc(9007199254740993) != 9007199254740992.0 else throw \"lossy\"\n}\n",
        "Numbers :: @id.main\n",
    )
    .unwrap_err();
    assert_eq!(error.first().unwrap().id, ErrorId::E525);

    compile(
        "schema Numbers {\n ok: bool = true\n}\nlogic Numbers {\n require calc(1152921504606846976) == 1152921504606846976.0 else throw \"exact\"\n}\n",
        "Numbers :: @id.main\n",
    )
    .expect("an exactly representable large integer compares exactly");
}

#[test]
fn deeply_nested_calls_fail_at_the_arithmetic_depth_boundary() {
    let expression = format!("{}1{}", "abs(".repeat(65), ")".repeat(65));
    let template = format!(
        "schema Numbers {{\n result: int\n}}\nlogic Numbers {{\n derive .result = calc({expression})\n}}\n"
    );
    let error = compile(&template, "Numbers :: @id.main\n").unwrap_err();
    assert_eq!(error.first().unwrap().id, ErrorId::E209);
}

#[test]
fn aggregate_work_inside_loops_uses_the_shared_logic_budget() {
    let template = r#"schema Numbers {
    values[]: int
    iterations[]: int
    result: int
}
logic Numbers {
    for $iteration in .iterations {
        derive .result = calc(sum(.values))
    }
}
"#;
    let thousand_values = std::iter::repeat_n("1", 1_000)
        .collect::<Vec<_>>()
        .join(", ");
    let instance =
        format!("Numbers :: @id.main\nvalues: {thousand_values}\niterations: {thousand_values}\n");

    let error = compile(template, &instance).unwrap_err();
    assert_eq!(error.first().unwrap().id, ErrorId::E525);
    assert!(error.first().unwrap().message.contains("shared limit"));
}

#[test]
fn expressions_over_the_node_limit_fail_before_evaluation() {
    // 130 leaves and 129 binary operators exceed the 256-node expression cap
    // without approaching the independent bracket-depth boundary.
    let expression = std::iter::repeat_n("1", 130)
        .collect::<Vec<_>>()
        .join(" + ");
    let template = format!(
        "schema Numbers {{\n result: int\n}}\nlogic Numbers {{\n derive .result = calc({expression})\n}}\n"
    );

    let error = compile(&template, "Numbers :: @id.main\n").unwrap_err();
    assert_eq!(error.first().unwrap().id, ErrorId::E524);
    assert!(error.first().unwrap().message.contains("256 nodes"));
}
