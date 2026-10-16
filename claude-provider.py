#!/usr/bin/env python3
"""claude-provider is now `hatcheck provider`. This runs it, so installs from before
the rename keep working; it will go away."""
import os
import runpy
import sys

sys.argv.insert(1, "provider")
runpy.run_path(os.path.join(os.path.dirname(os.path.realpath(__file__)), "hatcheck.py"), run_name="__main__")
