#!/usr/bin/env python3
"""Verify the distributable ZIP on macOS and write its public checksum."""

import hashlib
import json
import plistlib
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


def verify(archive: Path) -> None:
    root = Path(__file__).resolve().parent.parent
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    with tempfile.TemporaryDirectory(prefix="sysly-release-") as directory:
        extracted = Path(directory)
        subprocess.run(["ditto", "-x", "-k", str(archive), directory], check=True)
        app = extracted / "Sysly.app"
        with (app / "Contents/Info.plist").open("rb") as source:
            info = plistlib.load(source)
        expected = {
            "CFBundleIdentifier": "dev.sysly.monitor",
            "CFBundleShortVersionString": version,
            "LSMinimumSystemVersion": "14.0",
            "CFBundleExecutable": "Sysly",
        }
        for key, value in expected.items():
            if info.get(key) != value:
                raise ValueError(f"{key}: expected {value!r}, got {info.get(key)!r}")
        binary = app / "Contents/MacOS/Sysly"
        architectures = subprocess.check_output(["lipo", "-archs", str(binary)], text=True).split()
        if architectures != ["arm64"]:
            raise ValueError(f"Expected arm64, got {architectures}")
        if not (app / "Contents/Resources/Sysly.icns").is_file():
            raise ValueError("Missing app icon")
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
        result = subprocess.run(
            [str(binary), "--sample", "2", "1"],
            check=True, capture_output=True, text=True, timeout=30,
        )
        samples = [json.loads(line) for line in result.stdout.splitlines()]
        if len(samples) != 2 or any(not isinstance(sample, dict) or not sample for sample in samples):
            raise ValueError("Expected two nonempty JSON samples")
        total_memory = int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True))
        core_count = int(subprocess.check_output(["sysctl", "-n", "hw.logicalcpu"], text=True))
        for sample in samples:
            if sample["memory"]["total"] != total_memory or len(sample["cores"]) != core_count:
                raise ValueError("Sample memory or core count does not match this Mac")
        if samples[1]["sequence"] <= samples[0]["sequence"]:
            raise ValueError("Sampling sequence did not advance")
    with archive.open("rb") as source:
        checksum = hashlib.file_digest(source, "sha256").hexdigest()
    checksum_path = archive.with_name(archive.name + ".sha256")
    checksum_path.write_text(f"{checksum}  {archive.name}\n")
    print(f"Verified Sysly {version}: arm64, macOS 14+, bundle, signature, sampling CLI")
    print(f"SHA-256: {checksum}")
    print(f"Checksum written to {checksum_path}")
    print("Ad-hoc signature verification does not establish Developer ID signing or notarization.")


if __name__ == "__main__":
    archive = Path(sys.argv[1] if len(sys.argv) > 1 else "dist/Sysly.zip").resolve()
    verify(archive)
