"""Load extensionless sibling tools with the standard import protocol."""
from importlib.machinery import SourceFileLoader
from importlib.util import module_from_spec, spec_from_loader
import os
import sys


def load_source(name, path):
    """Execute source under a registered name, preserving an existing module's identity."""
    loader = SourceFileLoader(name, os.fspath(path))
    spec = spec_from_loader(name, loader)
    existing = name in sys.modules
    module = sys.modules[name] if existing else module_from_spec(spec)
    module.__spec__ = spec
    module.__loader__ = loader
    module.__package__ = spec.parent
    module.__file__ = spec.origin
    module.__cached__ = spec.cached
    # Dataclasses and imports of this module need its identity before source executes.
    sys.modules[name] = module
    try:
        loader.exec_module(module)
    except BaseException:
        if not existing:
            sys.modules.pop(name, None)
        raise
    return sys.modules[name]
