"""Validate dispatch/tag release versions before allocating macOS builders."""
import os
from pathlib import Path
import re
import tomllib


def release_version(event, ref, requested, package_version):
    if event == "workflow_dispatch":
        if ref != "refs/heads/main":
            raise ValueError("Run releases from main after merging your changes.")
        version = requested.strip()
    elif event == "push" and ref.startswith("refs/tags/v"):
        version = ref.removeprefix("refs/tags/v")
    else:
        raise ValueError("Expected a manual release from main or a version tag push.")
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version):
        raise ValueError("Use a version such as 0.1.0, without the v prefix.")
    if version != package_version:
        raise ValueError(f"Requested {version}, but desktop-rune/Cargo.toml is {package_version}. Bump the package version in a PR first.")
    return version


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[2]
    package = tomllib.loads((root / "desktop-rune/Cargo.toml").read_text())["package"]
    try:
        version = release_version(os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_REF"], os.environ.get("REQUESTED_VERSION", ""), package["version"])
    except ValueError as error:
        raise SystemExit(str(error))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"version={version}\ntag=v{version}\n")
    print(f"Validated release v{version}")
