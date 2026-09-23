#!/usr/bin/env python3
"""Writes the license notices for everything compiled into the sayit binary.

MIT and Apache-2.0 require these notices whenever a built binary is shared
(for example the DMG). The list comes from `cargo metadata`: every crate
the binary depends on for this platform, plus ONNX Runtime, which `ort-sys`
links statically. Speech models aren't bundled, so they aren't listed.

Usage: scripts/third-party-notices.py OUTPUT_FILE
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The ONNX Runtime release that ort-sys downloads, and where its notices are
# kept in this repository. Refresh both together when ort-sys changes:
# https://github.com/microsoft/onnxruntime/tree/v<version>
ORT_SYS = "2.0.0-rc.12"
ORT_VERSION = "1.24.2"
ORT_NOTICES = ROOT / "assets" / "licenses" / f"onnxruntime-{ORT_VERSION}"

LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)", re.IGNORECASE)

MIT_TEXT = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""

RULE = "=" * 78


def host_target():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True)
    return next(l.split()[1] for l in out.stdout.splitlines() if l.startswith("host:"))


def bundled_packages(metadata):
    """Crates reachable through normal (non-dev, non-build) dependencies."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        node = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        for dep in nodes[node]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)
    return sorted((packages[i] for i in seen), key=lambda p: (p["name"], p["version"]))


def offers_mit(license_expr):
    return re.search(r"\bMIT\b", license_expr or "") is not None and " AND " not in license_expr


def license_texts(package):
    """The crate's own license files; for crates offering MIT, just that one.
    Crates offering MIT without shipping a file get the standard MIT text."""
    folder = Path(package["manifest_path"]).parent
    files = sorted(f for f in folder.iterdir() if f.is_file() and LICENSE_FILE.match(f.name))
    if package.get("license_file"):
        extra = folder / package["license_file"]
        if extra.is_file() and extra not in files:
            files.append(extra)
    if offers_mit(package["license"]):
        mit = [f for f in files if "MIT" in f.name.upper()]
        if mit:
            files = mit
        elif not files:
            holders = ", ".join(package.get("authors") or []) or f"the {package['name']} authors"
            return [f"Copyright (c) {holders}\n\n{MIT_TEXT}"]
    if not files:
        sys.exit(f"no license text found for {package['name']} {package['version']}")
    return [f.read_text(errors="replace").strip() + "\n" for f in files]


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", host_target()],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    packages = bundled_packages(json.loads(out.stdout))

    ort_sys = [p["version"] for p in packages if p["name"] == "ort-sys"]
    if ort_sys and ort_sys != [ORT_SYS]:
        sys.exit(f"ort-sys is {ort_sys[0]}, but the ONNX Runtime notices are for "
                 f"ort-sys {ORT_SYS}: update ORT_SYS, ORT_VERSION and {ORT_NOTICES}")

    parts = [
        "sayit: third-party software notices\n\n"
        "sayit includes the software listed below. Each is used under the license\n"
        "shown with it; where a component offers a choice of licenses, sayit uses\n"
        "the MIT License. The source code of every Rust component is available on\n"
        "crates.io and in the repository linked with it.\n"
    ]
    for package in packages:
        header = f"{package['name']} {package['version']}\nLicense: {package['license']}"
        if package.get("repository"):
            header += f"\nSource: {package['repository']}"
        parts.append(f"{RULE}\n{header}\n{RULE}\n\n" + "\n".join(license_texts(package)))

    parts.append(
        f"{RULE}\nONNX Runtime {ORT_VERSION}, linked statically\nLicense: MIT\n"
        f"Source: https://github.com/microsoft/onnxruntime/tree/v{ORT_VERSION}\n{RULE}\n\n"
        + (ORT_NOTICES / "LICENSE").read_text().strip() + "\n\n"
        + (ORT_NOTICES / "ThirdPartyNotices.txt").read_text().strip() + "\n"
    )
    Path(sys.argv[1]).write_text("\n".join(parts))
    print(f"Wrote {sys.argv[1]} ({len(packages)} crates + ONNX Runtime {ORT_VERSION})")


if __name__ == "__main__":
    main()
