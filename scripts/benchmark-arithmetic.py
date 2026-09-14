"""Serial end-to-end invoice compilation benchmark, including process startup."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser()
    parser.add_argument("--compiler", type=Path, default=root / "target/release/abstract.exe")
    args = parser.parse_args()
    compiler = args.compiler.resolve()
    reports = []
    with tempfile.TemporaryDirectory(prefix="calc-bench-", dir=root / "target") as temp:
        data = Path(temp) / "data"
        data.mkdir()
        (data / "model.abt").write_text('''schema Line {
quantity: int
price_cents: int
total_cents: int
}
logic Line {
derive .total_cents = calc(.quantity * .price_cents)
}
schema Invoice {
lines[]: $(Line)
total_cents: int
}
logic Invoice {
derive .total_cents = calc(sum(.lines.total_cents))
}
''', encoding="utf8")
        for count in [100, 1000, 5000]:
            values = [(1 + i % 7, 100 + i % 53) for i in range(count)]
            rows = ",\n".join(f"({q}, {p})" for q, p in values)
            (data / "invoice.ab").write_text(
                "Invoice :: @id.invoice\nlines(quantity, price_cents): [\n" + rows + "\n]\n", encoding="utf8")
            samples = []
            output_bytes = None
            for run in range(6):
                start = time.perf_counter_ns()
                result = subprocess.run([str(compiler), "compile", temp, "JSON"], capture_output=True, timeout=30)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                assert result.returncode == 0, result.stderr.decode("utf8")
                obj = json.loads(result.stdout)["data"][0]
                assert obj["total_cents"] == sum(q * p for q, p in values)
                assert len(obj["lines"]) == count
                output_bytes = len(result.stdout)
                if run: samples.append(elapsed)
            reports.append({"lines": count, "warmups": 1, "samples_ms": samples,
                            "median_ms": statistics.median(samples), "json_bytes": output_bytes})
    print(json.dumps({"measurement": "fresh-process end-to-end compile and JSON output", "results": reports}, indent=2))


if __name__ == "__main__":
    main()
