#!/usr/bin/env python3
"""cl, cx, claude-z and claude-k in Python, for installs from PyPI (uv tool install,
pipx), which can only put Python commands on your PATH. They do what ./cl and ./cx do,
and cl.ps1 and cx.ps1 on Windows:

    cl [-z|-k] <profile> [claude args...]    Claude Code with ~/.claude-<profile>
    claude-z [claude args...]                Claude Code on Z.ai, with ~/.claude or
    claude-k [claude args...]                CLAUDE_CONFIG_DIR (claude-k: Kimi);
                                             claude-z reconfig sets up the key
    cx <profile> [codex args...]             Codex with ~/.codex-<profile>

Without a profile name, in a folder tied to one with hatcheck bind, they use that one.
For tests: launch.py cl|cx|claude-z|claude-k [args...].
"""
import importlib.util
import os
import shutil
import signal
import subprocess
import sys

HOME = os.path.expanduser("~")
# Codex's own commands, which cx doesn't take for a home's name unless ~/.codex-<it> exists.
CODEX_COMMANDS = {"agents", "exec", "e", "review", "login", "logout", "mcp", "plugin", "app-server", "remote-control",
                  "app", "completion", "update", "doctor", "sandbox", "debug", "apply", "a", "resume", "queue", "archive",
                  "delete", "migrate-rollouts", "unarchive", "fork", "cloud", "exec-server", "features", "help"}


def load_sibling(name, file):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def bound(agent):
    """The name this folder is bound to with hatcheck bind, for claude or codex, or None."""
    sessions = load_sibling("hatcheck_sessions", "sessions.py")
    bindings = sessions.load_bindings()
    folder = sessions.bound_folder(os.getcwd(), bindings)
    name = bindings[folder].get(agent) if folder else None
    return name if isinstance(name, str) and name else None


def use_bound(env, variable, base, name):
    """Point the agent at a bound name: default for its own folder, a path, or ~/<base>-NAME."""
    if name == "default":
        env.pop(variable, None)
    elif "/" in name or os.sep in name:
        env[variable] = name
    else:
        env[variable] = os.path.join(HOME, base + "-" + name)


def run(me, program, args, env):
    """Run the agent with args, in place of this process where the system allows."""
    path = shutil.which(program, path=env.get("PATH"))
    if not path:
        sys.exit("%s: %s isn't on your PATH; install %s first" % (me, program, "Codex" if program == "codex" else "Claude Code"))
    sys.stdout.flush()
    if os.name == "nt":  # no exec on Windows: run it and leave Ctrl+C to the agent
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        sys.exit(subprocess.call([path] + args, env=env))
    os.execve(path, [program] + args, env)


def cl(argv=None, me="cl", provider=None):
    """cl [-z|-k] <profile> [claude args...], and claude-z and claude-k (provider z or k)."""
    args = list(sys.argv[1:] if argv is None else argv)
    env = dict(os.environ)
    if provider:
        if args[:1] == ["reconfig"]:
            return load_sibling("hatcheck_provider", "provider.py").setup(provider)
    else:
        if args[:1] in (["-z"], ["-k"]):
            provider = args.pop(0)[1:]
        if not args or not args[0] or args[0].startswith("-"):
            name = bound("claude")
            if not name:
                sys.stderr.write("usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)\n"
                                 "   or tie this folder to a profile with: hatcheck bind <profile>\n")
                sys.exit(2)
            use_bound(env, "CLAUDE_CONFIG_DIR", ".claude", name)
            sys.stderr.write("cl: this folder is bound to %s\n" % name)
        else:
            # Spelled as ./cl spells it: Claude Code files the login under this string.
            env["CLAUDE_CONFIG_DIR"] = os.path.join(HOME, ".claude-" + args.pop(0))
    if provider:
        settings = load_sibling("hatcheck_provider", "provider.py")
        path = settings.config_path(provider)
        values, _ = settings.read_config(path)
        spec = settings.PROVIDERS[provider]
        if values is None:
            sys.exit("%s isn't set up yet. Run: claude-%s reconfig" % (spec["name"], provider))
        if not values.get(spec["prefix"] + "API_KEY", "").strip():
            sys.exit("No %s API key in %s. Run: claude-%s reconfig" % (spec["name"], settings.shown(path), provider))
        for name, value in settings.environment(provider, values):
            if value is None:
                env.pop(name, None)
            else:
                env[name] = value
    run(me, "claude", args, env)


def claude_z(argv=None):
    return cl(argv, "claude-z", "z")


def claude_k(argv=None):
    return cl(argv, "claude-k", "k")


def cx(argv=None):
    """cx <profile> [codex args...]"""
    args = list(sys.argv[1:] if argv is None else argv)
    env = dict(os.environ)
    first = args[0] if args else ""
    named = bool(first) and not first.startswith("-") and (
        first not in CODEX_COMMANDS or os.path.isdir(os.path.join(HOME, ".codex-" + first)))
    name = None if named else bound("codex")
    if name:
        use_bound(env, "CODEX_HOME", ".codex", name)
        sys.stderr.write("cx: this folder is bound to %s\n" % name)
    else:
        if not named:
            if first and not first.startswith("-"):  # a Codex command, with no home of that name
                sys.stderr.write("cx: %s is a Codex command, not a home's name; run it with a home (cx <profile> %s ...)\n"
                                 % (first, first))
            sys.stderr.write("usage: cx <profile> [codex args...]  (uses ~/.codex-<profile>)\n"
                             "   or tie this folder to a home with: hatcheck bind <profile>\n")
            sys.exit(2)
        env["CODEX_HOME"] = os.path.join(HOME, ".codex-" + args.pop(0))
    if not shutil.which("codex", path=env.get("PATH")):
        sys.exit("cx: codex isn't on your PATH; install Codex first")
    if env.get("CODEX_HOME"):
        os.makedirs(env["CODEX_HOME"], exist_ok=True)  # Codex won't start with a CODEX_HOME that doesn't exist
    run("cx", "codex", args, env)


if __name__ == "__main__":
    commands = {"cl": cl, "cx": cx, "claude-z": claude_z, "claude-k": claude_k}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit("usage: launch.py cl|cx|claude-z|claude-k [args...]")
    sys.exit(commands[sys.argv[1]](sys.argv[2:]))
