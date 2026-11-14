"""Find final Cargo artifacts from either intermediate test-binary layout."""
from pathlib import Path


# Sample input: <target>/<profile>/deps/uish_driver-1a2b (Cargo 1.99) or <target>/<profile>/build/flea-gui/1a2b/out/uish_driver-1a2b (Cargo 1.100), both give <target>/<profile>.
def profile_directory(executable):
    for directory in Path(executable).parents:
        if directory.name in ("deps", "examples"):
            return directory.parent
        if directory.name == "out" and len(directory.parents) >= 4 and directory.parents[2].name == "build":
            return directory.parents[3]
    raise ValueError(f"cannot locate Cargo's final artifact directory from {executable}")
