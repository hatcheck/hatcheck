#!/usr/bin/env python3
"""hatcheck, run from a clone of the repo: install.sh links this as hatcheck and hat.
The work is in hatcheck/cli.py, which a package install runs as the hatcheck command;
see hatcheck --help."""
import importlib.util
import os
import sys

if __name__ == "__main__":
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "hatcheck", "cli.py")
    spec = importlib.util.spec_from_file_location("hatcheck_cli", path)
    cli = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cli)
    sys.exit(cli.main())
