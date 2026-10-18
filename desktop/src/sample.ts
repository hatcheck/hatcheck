// Sample answers in the shape the hatcheck engine gives, for working on the pages in a
// plain browser (npm run dev), where there's no app to run the engine.
const H = "/Users/me";
const hours = (n: number) => new Date(Date.now() + n * 3_600_000).toISOString().replace(/\.\d+Z$/, "Z");

const limit = (name: string, short: string, kind: string, used: number, resetIn: number, window: number, perModel = false) => ({
  name, short, kind, used_percent: used, left_percent: 100 - used, resets_at: hours(resetIn), window_seconds: window,
  severity: used >= 90 ? "critical" : used >= 75 ? "warning" : "normal", per_model: perModel,
});

const accounts = [
  { agent: "claude", profiles: [`${H}/.claude-work`], plan: "team max 5x", source: "live", saved_at: null, why: null,
    limits: [limit("session (5h)", "5h", "session", 21, 2.2, 18000), limit("week (Fable)", "Fable", "weekly_scoped", 69, 92, 604800, true)],
    extra: null, fable: { left: true, resets_at: hours(92) } },
  { agent: "claude", profiles: [`${H}/.claude-team`], plan: "team", source: "cached", saved_at: hours(-0.8), why: "--cached",
    limits: [limit("session (5h)", "5h", "session", 0, 4.9, 18000), limit("week (Fable)", "Fable", "weekly_scoped", 0, 129, 604800, true)],
    extra: null, fable: { left: true, resets_at: hours(129) } },
  { agent: "claude", profiles: [`${H}/.claude-personal`], plan: "team", source: "cached", saved_at: hours(-1), why: "--cached",
    limits: [limit("session (5h)", "5h", "session", 59, 1.1, 18000), limit("week (Fable)", "Fable", "weekly_scoped", 100, 29, 604800, true)],
    extra: null, fable: { left: false, back_at: hours(29) } },
  { agent: "claude", profiles: [`${H}/.claude`], plan: "pro", source: "cached", saved_at: hours(-13), why: "token expired; run claude in this profile to renew it",
    limits: [limit("session (5h)", "5h", "session", 16, 3.5, 18000), limit("week (all models)", "week", "weekly_all", 36, 23, 604800)],
    extra: null, fable: null },
  { agent: "codex", profiles: [`${H}/.codex`], plan: "ChatGPT plus", source: "live", saved_at: null, why: null,
    limits: [limit("session (5h)", "5h", "session", 20, 3.1, 18000), limit("week (all models)", "week", "weekly", 36, 43, 604800)],
    extra: null, fable: null },
];

const session = (agent: string, id: string, profile: string, title: string, cwd: string, agoHours: number, resume: string, extra = {}) => ({
  agent, id, profile, account: null, title, cwd, last_active: hours(-agoHours), entrypoint: "cli", branch: "main",
  model: agent === "codex" ? "gpt-5.5" : "claude-fable-5-1", open: null, continued_in: null, bound_to: null, resume, ...extra,
});

const sessions = [
  session("claude", "3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10", `${H}/.claude-work`, "Fix the flaky login test", `${H}/code/api`, 0.07,
    "cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10", { open: { pid: 48213, status: "busy" } }),
  session("codex", "019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e", `${H}/.codex`, "Add retries to the uploader", `${H}/code/api`, 0.7,
    "codex resume 019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e"),
  session("claude", "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d", `${H}/.claude-personal`, "Add rate limiting to the login endpoint", `${H}/code/api`, 3,
    "cl personal --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d", { bound_to: `${H}/.claude-work` }),
  session("claude", "c47e2b1d-0a9f-4e8d-b7c6-5d4e3f2a1b0c", `${H}/.claude`, "Draft a post on Rust lifetimes", `${H}/code/blog`, 20,
    "claude --resume c47e2b1d-0a9f-4e8d-b7c6-5d4e3f2a1b0c"),
  session("claude", "5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e", `${H}/.claude-work`, "Plan the next sprint", `${H}/notes`, 6,
    "cl work --resume 5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e"),
];

const turns = [
  { role: "user", text: "The login test fails about one run in ten. Find out why and fix it.", time: hours(-0.5) },
  { role: "assistant", text: "I'll run it a few times to see the failure.\n\n[external_agent_tool_call: Bash]\ncommand: pytest tests/test_login.py -x --count 20\n[/external_agent_tool_call]\n[external_agent_tool_result]\n2 failed, 18 passed\n[/external_agent_tool_result]\n\nThe test waits a fixed 100 ms for the session cookie; under load it's slower. I'll wait for the cookie instead.", time: hours(-0.45) },
  { role: "user", text: "Good, do it.", time: hours(-0.3) },
  { role: "assistant", text: "Done: the test now polls for the cookie for up to 2 s. 50 runs in a row pass.", time: hours(-0.07) },
];

export function sample(args: string[]): string {
  const [first] = args;
  if (first === "--version") return "hatcheck 0.1.0\n";
  if (first === "usage") return JSON.stringify({ version: 1, sorted_by: "fable", accounts });
  if (first === "--json") return JSON.stringify({ version: 1, sessions, accounts: [], hidden: { Desktop: 2 } });
  if (first === "show") return JSON.stringify({ version: 1, agent: "claude", id: args[1], title: "Fix the flaky login test",
    cwd: `${H}/code/api`, model: "claude-fable-5-1", turns_total: 4, tool_calls: 1,
    left_out: "Claude's thinking, images, plans, /rewind snapshots and subagent transcripts", turns });
  if (first === "move" || first === "fork") {
    const fork = first === "fork", keep = args.includes("--keep"), id = fork ? "5b1e0c4a-7d2f-4e6b-9a3c-8f1d2e3c4b5a" : args[1];
    return JSON.stringify({ version: 1, dry_run: args.includes("--dry-run"), action: fork ? "fork" : keep ? "copy" : "move",
      agent: "claude", session: args[1], source: `${H}/.claude-work`, target: `${H}/.claude-${args[2]}`, new_session: fork ? id : null,
      deleted_original: !fork && !keep,
      items: [{ what: "transcript", from: "", to: "", state: "new", detail: "53.0 KB" }, { what: "/rewind file snapshots", from: "", to: "", state: "new", detail: "4 files" }],
      cwd: `${H}/code/api`, resume: `cl ${args[2]} --resume ${id}` });
  }
  if (first === "desktop" && args[1] === "--json") return JSON.stringify({ version: 1, instances: [
    { name: "work", display: "Work", app: "/Applications/Claude Work.app", data_dir: `${H}/.claude-instances/work`, running: true },
    { name: "personal", display: "Personal", app: "/Applications/Claude Personal.app", data_dir: `${H}/.claude-instances/personal`, running: false }] });
  return "";
}
