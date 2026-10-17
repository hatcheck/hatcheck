#!/usr/bin/env python3
"""Remove a folder's binding, or the warning hook: hatcheck unbind [FOLDER] [--hook].
The work is in bind.py, so keep the two in one folder."""
import importlib.util
import os
import sys


def main(argv=None, prog=None):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "bind.py")
    module = sys.modules.get("hatcheck_bind")  # loaded once per process, as everywhere
    if module is None:
        spec = importlib.util.spec_from_file_location("hatcheck_bind", path)
        module = sys.modules["hatcheck_bind"] = importlib.util.module_from_spec(spec)
        try:
            spec.loader.exec_module(module)
        except BaseException:
            del sys.modules["hatcheck_bind"]
            raise
    return module.unbind_main(argv, prog)


if __name__ == "__main__":
    sys.exit(main())
