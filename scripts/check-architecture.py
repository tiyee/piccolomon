#!/usr/bin/env python3
"""Check production package boundaries from Cargo metadata, without compiling GPUI."""
import json
import os
from pathlib import Path
import subprocess
import sys

allowed = {
    "ryne-app": {"ryne-ui", "ryne-session", "ryne-features", "ryne-services"},
    "ryne-parser": set(),
    "ryne-core": {"ryne-parser"},
    "ryne-syntax": {"ryne-parser", "ryne-core"},
    "ryne-projection": {"ryne-parser", "ryne-core", "ryne-syntax"},
    "ryne-extensions": {"ryne-parser", "ryne-core", "ryne-syntax", "ryne-projection"},
    "ryne-features": {"ryne-parser", "ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
    "ryne-services": {"ryne-parser", "ryne-extensions", "ryne-syntax", "ryne-projection"},
    "ryne-session": {"ryne-parser", "ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
    "ryne-ui": {"ryne-parser", "ryne-session", "ryne-core", "ryne-syntax", "ryne-projection", "ryne-extensions"},
}

def check_packages(metadata):
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
        gpui_dependencies = {
            identity
            for dependency in package["dependencies"]
            if dependency["kind"] != "dev"
            for identity in (dependency["name"], dependency.get("rename"))
            if identity and (identity == "gpui" or identity.startswith(("gpui-", "gpui_")))
        }
        if name != "ryne-ui" and gpui_dependencies:
            errors.append(f"GPUI outside UI: {name}")
        algorithms = {"formulary", "math-core", "merman", "tree-sitter", "tree-sitter-highlight", "svg2pdf", "pdf-writer", "krilla", "krilla-svg", "usvg", "ureq"}
        if name != "ryne-services" and normal.intersection(algorithms):
            errors.append(f"Concrete service outside services: {name}")
    if packages.keys() != allowed.keys():
        errors.append("Missing workspace packages")
    return errors


def main():
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output([
        os.environ.get("CARGO", "cargo"), "metadata", "--locked", "--no-deps", "--format-version", "1"
    ], cwd=root))
    errors = check_packages(metadata)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Architecture passed: {len(metadata['packages'])} private ryne- packages; production dependency boundaries verified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
