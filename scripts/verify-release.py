#!/usr/bin/env python3
"""Verify one downloaded native Abstract release artifact without a shell."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path


def fail(message: str) -> None:
    raise SystemExit(f"release verification failed: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def checksum_for(checksums: Path, asset_name: str) -> str:
    expected = None
    try:
        lines = checksums.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        fail(f"cannot read checksum manifest: {error}")
    for number, raw_line in enumerate(lines, start=1):
        if not raw_line:
            continue
        try:
            digest, stored_name = raw_line.split(maxsplit=1)
        except ValueError:
            fail(f"malformed checksum entry on line {number}")
        name = stored_name.lstrip(" *")
        if name.startswith("./"):
            name = name[2:]
        if name != asset_name:
            continue
        if len(digest) != 64 or any(character not in "0123456789abcdefABCDEF" for character in digest):
            fail(f"invalid SHA-256 digest for {asset_name}")
        if expected is not None:
            fail(f"duplicate checksum entries for {asset_name}")
        expected = digest.lower()
    if expected is None:
        fail(f"SHA256SUMS.txt has no entry for {asset_name}")
    return expected


def run(binary: Path, *arguments: str, cwd: Path | None = None, capture: bool = False) -> str:
    try:
        completed = subprocess.run(
            [str(binary), *arguments],
            cwd=cwd,
            check=True,
            capture_output=capture,
            text=True,
            encoding="utf-8",
            timeout=30,
        )
    except FileNotFoundError as error:
        fail(f"cannot execute {binary}: {error}")
    except subprocess.CalledProcessError as error:
        stderr = error.stderr.strip() if error.stderr else ""
        fail(f"{' '.join(error.cmd)} exited {error.returncode}{': ' + stderr if stderr else ''}")
    except subprocess.TimeoutExpired:
        fail(f"{' '.join([str(binary), *arguments])} exceeded the 30 second limit")
    return completed.stdout if capture else ""


def normalized_document(path: Path) -> object:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        document["abstract"].pop("compiler")
    except (OSError, json.JSONDecodeError, KeyError, AttributeError) as error:
        fail(f"cannot normalize {path}: {error}")
    return document


def verify_bundle(binary: Path, temporary_parent: Path) -> None:
    key = "hex:" + "31" * 32
    with tempfile.TemporaryDirectory(prefix="abstract-release-", dir=temporary_parent) as temporary:
        root = Path(temporary)
        data = root / "data"
        data.mkdir()
        (data / "schema.abt").write_text(
            "schema ReleaseCheck {\nlabel: text\n}\n", encoding="utf-8"
        )
        (data / "record.ab").write_text(
            "ReleaseCheck :: @id.release\nlabel: verified\n", encoding="utf-8"
        )
        expected = root / "expected.json"
        bundle = root / "roundtrip.abx"
        unbundled = root / "unbundled.json"
        tampered = root / "tampered.abx"

        run(binary, "compile", str(data), "JSON", "--out", str(expected))
        run(binary, "bundle", str(data), "--key", key, "--out", str(bundle))
        run(binary, "unbundle", str(bundle), "--key", key, "--out", str(unbundled))
        if normalized_document(expected) != normalized_document(unbundled):
            fail("bundle round trip changed the compiled document outside abstract.compiler")

        bytes_ = bytearray(bundle.read_bytes())
        if not bytes_:
            fail("bundle command wrote an empty container")
        bytes_[-1] ^= 0x40
        tampered.write_bytes(bytes_)
        tampered_output = root / "tampered.json"
        try:
            rejected = subprocess.run(
                [str(binary), "unbundle", str(tampered), "--key", key, "--out", str(tampered_output)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=30,
            )
        except subprocess.TimeoutExpired:
            fail("tampered sealed bundle check exceeded the 30 second limit")
        if rejected.returncode == 0 or tampered_output.exists():
            fail("tampered sealed bundle was accepted or produced output")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--asset", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--automotive", required=True, type=Path)
    args = parser.parse_args()
    args.asset = args.asset.resolve()

    if not args.tag.startswith("v") or len(args.tag) == 1:
        fail(f"tag must have the form v<version>: {args.tag}")
    expected_version = args.tag[1:]
    for path in (args.asset, args.checksums, args.manifest):
        if not path.is_file():
            fail(f"required downloaded file is missing: {path}")
    if not args.automotive.is_dir():
        fail(f"automotive project is missing: {args.automotive}")

    expected_digest = checksum_for(args.checksums, args.asset.name)
    actual_digest = sha256(args.asset)
    if actual_digest != expected_digest:
        fail(f"SHA-256 mismatch for {args.asset.name}: expected {expected_digest}, got {actual_digest}")

    expected_manifest_digest = checksum_for(args.checksums, args.manifest.name)
    actual_manifest_digest = sha256(args.manifest)
    if actual_manifest_digest != expected_manifest_digest:
        fail("SHA-256 mismatch for release-manifest.json")

    try:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read release manifest: {error}")
    if not isinstance(manifest, dict):
        fail("release manifest must be a JSON object")
    if manifest.get("compiler") != expected_version:
        fail("release tag and compatibility manifest compiler version disagree")

    version = run(args.asset, "--version", capture=True).strip()
    if version != f"abstract {expected_version}":
        fail(f"release tag and binary version disagree: {version!r}")

    run(args.asset, "lint", str(args.automotive))
    with tempfile.TemporaryDirectory(prefix="abstract-automotive-", dir=args.asset.parent) as temporary:
        actual = Path(temporary) / "automotive.json"
        run(args.asset, "compile", str(args.automotive), "JSON", "--out", str(actual))
        expected = args.automotive / "expected.json"
        if normalized_document(expected) != normalized_document(actual):
            fail("automotive JSON differs outside abstract.compiler")

    verify_bundle(args.asset, args.asset.parent)
    print(f"release verification passed: {args.asset.name} ({expected_version})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
