#!/usr/bin/env python3
"""Remove a folder's binding, or the warning hook: hatcheck unbind [FOLDER] [--hook].
The work is in bind.py, so keep the two in one folder."""
import importlib.util
import os
import sys


def main(argv=None, prog=None):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "bind.py")
    spec = importlib.util.spec_from_file_location("hatcheck_bind", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.unbind_main(argv, prog)


if __name__ == "__main__":
    sys.exit(main())
