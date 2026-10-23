#!/usr/bin/env python3
"""Prepare, package and publish a manually dispatched Ryne release."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "win-x86_64": "x86_64-pc-windows-msvc",
    "mac-x86_64": "x86_64-apple-darwin",
    "mac-arm_64": "aarch64-apple-darwin",
}
NUMBER = r"(?:0|[1-9][0-9]*)"
PRERELEASE = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
VERSION = re.compile(
    rf"v?({NUMBER}\.{NUMBER}\.{NUMBER}(?:-{PRERELEASE}(?:\.{PRERELEASE})*)?)"
)


def normalize_version(value):
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError("Use a version such as 0.2.0, v0.2.0 or 0.2.0-rc.1")
    return match[1]


def run(*args, **kwargs):
    return subprocess.run(args, check=True, cwd=ROOT, **kwargs)


def prepare(version, platform):
    manifest = ROOT / "Cargo.toml"
    source = manifest.read_text(encoding="utf-8")
    workspace = tomllib.loads(source)["workspace"]
    names = set()
    for member in workspace["members"]:
        for directory in ROOT.glob(member):
            package = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))["package"]
            if package.get("version") == {"workspace": True}:
                names.add(package["name"])
    source, count = re.subn(
        r'(?m)(\[workspace\.package\]\s*\nversion\s*=\s*)"[^"\n]+"',
        lambda match: f'{match[1]}"{version}"',
        source,
        count=1,
    )
    if count != 1:
        raise ValueError("Cannot locate workspace.package.version")

    lockfile = ROOT / "Cargo.lock"
    blocks = lockfile.read_text(encoding="utf-8").split("[[package]]")
    updated = set()
    for index in range(1, len(blocks)):
        package = tomllib.loads("[[package]]" + blocks[index])["package"][0]
        if package["name"] in names and "source" not in package:
            blocks[index] = re.sub(
                r'^version = "[^"\n]+"',
                f'version = "{version}"',
                blocks[index],
                count=1,
                flags=re.M,
            )
            updated.add(package["name"])
    if updated != names:
        raise ValueError(f"Missing workspace packages in Cargo.lock: {names - updated}")

    manifest.write_text(source, encoding="utf-8")
    lockfile.write_text("[[package]]".join(blocks), encoding="utf-8")
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"version={version}\ntarget={TARGETS[platform]}\n")
    print(f"Prepared Ryne {version} for {platform} ({TARGETS[platform]})")


def api(endpoint):
    result = subprocess.run(
        ["gh", "api", endpoint], cwd=ROOT, text=True, capture_output=True
    )
    if result.returncode:
        if "(HTTP 404)" in result.stderr:
            return None
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def verify_tag(version):
    repo = os.environ["GH_REPO"]
    ref = api(f"repos/{repo}/git/ref/tags/v{version}")
    if ref is None:
        return
    obj = ref["object"]
    while obj["type"] == "tag":
        obj = api(f"repos/{repo}/git/tags/{obj['sha']}")["object"]
    if obj["type"] != "commit" or obj["sha"] != os.environ["GITHUB_SHA"]:
        raise ValueError(
            f"Tag v{version} already points to a different commit; use that ref or a new version"
        )


def package(version, platform):
    output = ROOT / "target/release-artifacts"
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    stem = f"Ryne-{version}-{platform}"
    dist = ROOT / "dist"

    if platform == "win-x86_64":
        source = dist / f"{stem}.zip"
        if not source.is_file():
            raise ValueError(f"Windows bundle script did not produce {source.relative_to(ROOT)}")
        artifact = output / source.name
        shutil.copy2(source, artifact)
    else:
        app = ROOT / "target/Ryne.app"
        executable = app / "Contents/MacOS/Ryne"
        if not executable.is_file():
            raise ValueError("macOS bundle script did not produce target/Ryne.app")
        expected_arch = "x86_64" if platform == "mac-x86_64" else "arm64"
        archs = run("lipo", "-archs", str(executable), capture_output=True, text=True).stdout.split()
        if archs != [expected_arch]:
            raise ValueError(f"Expected a {expected_arch} app bundle, found architectures: {archs}")
        artifact = output / f"{stem}.zip"
        run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(artifact))

    with artifact.open("rb") as data:
        digest = hashlib.file_digest(data, "sha256").hexdigest()
    (output / f"{stem}.sha256").write_text(
        f"{digest}  {artifact.name}\n", encoding="utf-8", newline="\n"
    )
    print(f"Packaged {stem} in {output}")


def publish(version):
    verify_tag(version)
    tag = f"v{version}"
    repo = os.environ["GH_REPO"]
    artifacts = sorted((ROOT / "target/release-artifacts").glob("*"))
    if not artifacts:
        raise ValueError("No release artifacts found")
    if api(f"repos/{repo}/releases/tags/{tag}") is None:
        args = [
            "gh", "release", "create", tag, "--target", os.environ["GITHUB_SHA"],
            "--title", f"Ryne {version}", "--generate-notes",
        ]
        if "-" in version:
            args.append("--prerelease")
        run(*args, *(str(path) for path in artifacts))
    else:
        run("gh", "release", "upload", tag, *(str(path) for path in artifacts), "--clobber")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "verify-tag", "package", "publish"])
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", choices=TARGETS)
    args = parser.parse_args()
    version = normalize_version(args.version)
    if args.command in {"prepare", "package"} and args.platform is None:
        parser.error("--platform is required for prepare/package")
    if args.command == "prepare":
        prepare(version, args.platform)
    elif args.command == "verify-tag":
        verify_tag(version)
    elif args.command == "package":
        package(version, args.platform)
    else:
        publish(version)


if __name__ == "__main__":
    main()
