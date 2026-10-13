#!/usr/bin/env python3
"""Check production package boundaries from Cargo metadata, without compiling GPUI."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output([
    os.environ.get("CARGO", "cargo"), "metadata", "--locked", "--no-deps", "--format-version", "1"
], cwd=root))
allowed = {
    "ryne-app": {"ryne-ui", "ryne-session", "ryne-features", "ryne-services"},
    "ryne-core": set(),
    "ryne-syntax": {"ryne-core"},
    "ryne-projection": {"ryne-core", "ryne-syntax"},
    "ryne-extensions": {"ryne-core", "ryne-syntax", "ryne-projection"},
    "ryne-features": {"ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
    "ryne-services": {"ryne-extensions", "ryne-syntax", "ryne-projection"},
    "ryne-session": {"ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
    "ryne-ui": {"ryne-session", "ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
}
packages = {p["name"]: p for p in metadata["packages"]}
errors = []
for name, package in packages.items():
    if name not in allowed or package["publish"] != []:
        errors.append(f"Unexpected or publishable package: {name}")
        continue
    normal = {d["name"] for d in package["dependencies"] if d["kind"] != "dev"}
    reverse = normal.intersection(packages).difference(allowed[name])
    if reverse:
        errors.append(f"Reverse dependency: {name} -> {sorted(reverse)}")
    if name != "ryne-ui" and "gpui" in normal:
        errors.append(f"GPUI outside UI: {name}")
    algorithms = {"formulary", "math-core", "merman", "tree-sitter", "tree-sitter-highlight", "svg2pdf", "pdf-writer", "krilla", "krilla-svg", "usvg", "ureq"}
    if name != "ryne-services" and normal.intersection(algorithms):
        errors.append(f"Concrete service outside services: {name}")
if packages.keys() != allowed.keys():
    errors.append("Missing workspace packages")
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"Architecture passed: {len(packages)} private ryne- packages; production dependency boundaries verified.")
