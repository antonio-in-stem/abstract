"""Independent CLI arithmetic checks using Python integer results as the oracle.

Run after cargo build --release. Temporary projects stay inside target/.
This is development verification, not a compiler dependency.
"""
import argparse
import json
from pathlib import Path
import random
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser()
    parser.add_argument("--compiler", type=Path, default=root / "target/release/abstract.exe")
    args = parser.parse_args()
    compiler = args.compiler.resolve()
    rng = random.Random(120140)
    expressions = []
    for _ in range(30):
        a, b, c = (rng.randint(-100000, 100000) for _ in range(3))
        expressions.append((f"({a} + {b}) * {c}", (a + b) * c))
    expressions += [
        ("9223372036854775807 - 1", 9223372036854775806),
        ("div(-7, 3)", -2), ("-7 % 3", -1),
        ("round(-2.5)", -3), ("floor(-2.2)", -3), ("ceil(-2.2)", -2),
        ("pow(2, 60)", 2**60), ("clamp(12, 0, 10)", 10),
        ("min(4, -7, 2)", -7), ("max(4, -7, 2)", 4),
    ]
    failures = [
        ("9223372036854775807 + 1", "int", "E525"),
        ("1 / 0", "float", "E525"), ("1 % 0", "int", "E525"),
        ("abs(-9223372036854775808)", "int", "E525"),
        ("sqrt(-1)", "float", "E525"), ("clamp(2, 4, 1)", "int", "E525"),
        ("9007199254740993 / 1", "float", "E525"),
        ("pow(2, 63)", "int", "E525"), ("round(1e100)", "int", "E525"),
        ("unknown(1)", "int", "E524"), ("abs(1, 2)", "int", "E524"),
    ]
    (root / "target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="calc-oracle-", dir=root / "target") as temp:
        project = Path(temp)
        data = project / "data"
        data.mkdir()
        (data / "sample.ab").write_text("Sample :: @id.sample\n", encoding="utf8")

        def compile_source(source):
            (data / "model.abt").write_text(source, encoding="utf8")
            return subprocess.run([str(compiler), "compile", str(project), "JSON"],
                                  capture_output=True, text=True, encoding="utf8", timeout=15)

        fields = "\n".join(f"v{i}: int" for i in range(len(expressions)))
        rules = "\n".join(f"derive .v{i} = calc({expr})" for i, (expr, _) in enumerate(expressions))
        result = compile_source(f"schema Sample {{\n{fields}\n}}\nlogic Sample {{\n{rules}\n}}\n")
        assert result.returncode == 0, result.stderr
        actual = json.loads(result.stdout)["data"][0]
        for i, (expr, expected) in enumerate(expressions):
            assert actual[f"v{i}"] == expected, (expr, expected, actual[f"v{i}"])

        for expression, field_type, error in failures:
            result = compile_source(f"schema Sample {{\nvalue: {field_type}\n}}\nlogic Sample {{\n"
                                    f"derive .value = calc({expression})\n}}\n")
            assert result.returncode != 0 and error in result.stderr, (expression, result.stdout, result.stderr)
    print(json.dumps({"seed": 120140, "oracle_cases": len(expressions),
                      "expected_failures": len(failures), "passed": True}))


if __name__ == "__main__":
    main()
