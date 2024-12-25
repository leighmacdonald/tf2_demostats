#!/bin/env python
import re
import sys

cargo_path = "Cargo.toml"
version_re = re.compile(r'^version = "(.+?)"', re.MULTILINE)


def version(v):
    filled = []
    for point in v.split("."):
        filled.append(point.zfill(8))
    return tuple(filled)


def get_current_version():
    with open(cargo_path) as f:
        try:
            return version_re.findall(f.read())[0]
        except IndexError:
            return None


def set_next_version(next_version: str):
    content = open(cargo_path, 'r').read()
    with open(cargo_path, 'w') as f:
        try:
            replaced =  version_re.sub(f"version = \"{next_version}\"", content)
            f.write(replaced)
        except IndexError:
            return None


def update(next_version: str):
    current_version = get_current_version()
    if version(next_version) <= version(current_version):
        raise ValueError(f"Invalid version, must be greater than previous: {current_version}")

    set_next_version(next_version)
    print(f"Set version successfully: {current_version} -> {next_version}")

if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("Missing version parameter")
        update(sys.argv[1])
    except ValueError as err:
        print(err)
        sys.exit(1)
