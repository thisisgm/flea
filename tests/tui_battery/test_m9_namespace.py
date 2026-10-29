"""Execute the actual auxiliary namespace with its private resolver and argument tail."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from .evidence import Commands, MARKER
from .fixtures import namespace


class NamespaceExecutionTests(unittest.TestCase):
    def test_no_chafa_policy_and_auxiliary_arguments_reach_the_real_owned_child(self):
        root = Path(tempfile.mkdtemp(prefix="m9-namespace.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (root / MARKER).write_text("owned actual auxiliary child namespace\n")
        fx = root / "fx"
        for name in ("tmp", "home", "config", "cache", "state", "runtime", "data", "tools"):
            (fx / name).mkdir(parents=True, exist_ok=True)
        chafa = fx / "tools/chafa"
        chafa.write_text("#!/bin/sh\nexit 99\n")
        chafa.chmod(0o700)
        script = "import json,os,shutil,sys; print(json.dumps([os.environ['PATH'],shutil.which('chafa'),sys.argv[1:]]))"
        commands = Commands()
        tail = ["one", "two words"]
        argv = namespace(Path("/usr/bin/python3"), fx, "/fx", {}, tool_policy="no-chafa",
                         arguments=["-B", "-c", script, *tail])
        result = commands.run(argv, timeout=10)
        self.assertEqual(json.loads(result.stdout), ["/fx/tools-no-chafa", None, tail])
        self.assertEqual(len(commands.records), 1)
        self.assertTrue(chafa.is_file())


if __name__ == "__main__":
    unittest.main()
