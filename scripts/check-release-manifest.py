"""Check the release's component versions against their source contracts."""

import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def capture(pattern, path):
    match = re.search(pattern, read(path), re.MULTILINE)
    if not match:
        raise ValueError(f"Cannot find version contract in {path}: {pattern}")
    return match.group(1)


def main():
    manifest = json.loads(read("release-manifest.json"))
    extension = json.loads(read("vscode/package.json"))
    pom = ET.fromstring(read("java/pom.xml"))
    ns = {"m": "http://maven.apache.org/POM/4.0.0"}
    language = manifest.get("language")
    actual = {
        "compiler": capture(r'^version = "([^"]+)"', "Cargo.toml"),
        "rustMinimum": capture(r'^rust-version = "([^"]+)"', "Cargo.toml"),
        "documentFormat": int(capture(r"DOCUMENT_FORMAT: i64 = (\d+)", "src/lib.rs")),
        "analysisProtocol": int(capture(r'\("version", number\((\d+)\)\)', "src/analysis.rs")),
        "bundleFormat": capture(r'MAGIC: \[u8; 4\] = \*b"([^"]+)"', "src/bundle.rs"),
        "extension": extension["version"],
        "vscodeMinimum": extension["engines"]["vscode"].removeprefix("^"),
        "javaRuntime": pom.findtext("m:version", namespaces=ns),
        "javaMinimum": int(pom.findtext("m:properties/m:maven.compiler.target", namespaces=ns).removeprefix("1.")),
    }
    errors = [f"{key}: manifest={manifest.get(key)!r}, source={value!r}"
              for key, value in actual.items() if manifest.get(key) != value]
    if not isinstance(language, str) or not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)", language):
        errors.append(f"language: expected a major.minor version, found {language!r}")
    if errors:
        print("Release manifest is out of date:\n" + "\n".join(errors), file=sys.stderr)
        return 1
    print(
        f"Release manifest matches all {len(actual)} source contracts "
        "and its language version is valid."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
