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
  model: agent === "codex" ? "gpt-5.5" : "claude-fable-5-1", open: null, continued_in: null, bound_to: null, resume,
  cost_usd: agent === "codex" ? 3.2 : 0.5, ...extra,
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

type Metadata = { alias: string | null; favorite: boolean; tags: string[]; archived: boolean };
type Filter = { query: string; agent: string; profile: string; hours: number; model: string; cost: string;
  tag: string; favorites: boolean; archived: boolean };
const libraryRows: { sessions: Record<string, Metadata>; filters: Record<string, Filter> } = {
  sessions: {
    [`claude:${H}/.claude-work:3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10`]:
      { alias: "Login flake", favorite: true, tags: ["backend", "urgent"], archived: false },
  },
  filters: {},
};

const workspaceRows: { name: string; folder: string; exists: boolean; bindings: Record<string, string> }[] = [
  { name: "API", folder: `${H}/code/api`, exists: true, bindings: { claude: `${H}/.claude-work`, codex: `${H}/.codex` } },
  { name: "Blog", folder: `${H}/code/blog`, exists: true, bindings: { claude: `${H}/.claude` } },
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
  if (first === "insights") {
    const account = { agent: "claude", profile: `${H}/.claude-work`, label: "week" };
    return JSON.stringify({ version: 1, days: 7,
      series: [{ account, readings: [20, 35, 50, 70].map((used, i) => ({ time: hours(-6 + i * 2), used_percent: used, resets_at: hours(48) })),
        trend: { pace_percent_per_hour: 25, over_seconds: 7200, readings: 4, runs_out_at: hours(1.2), before_reset: true } }],
      forecasts: [{ kind: "runs_out", account, runs_out_at: hours(1.2), resets_at: hours(48), pace_percent_per_hour: 25 },
        { kind: "unused", account: { ...account, label: "5h" }, left_percent: 60, resets_at: hours(1) }],
      weekly: { session_count: 3, costed_session_count: 2, cost_usd: 4 },
      profiles: [{ profile: `${H}/.claude-work`, session_count: 2, cost_usd: 1.5 }, { profile: `${H}/.claude`, session_count: 1, cost_usd: 2.5 }],
      projects: [{ project: `${H}/code/api`, session_count: 2, cost_usd: 1.5 }, { project: `${H}/code/blog`, session_count: 1, cost_usd: 2.5 }],
      models: [{ model: "claude-sonnet-5", session_count: 2, cost_usd: 1.5 }, { model: "claude-opus-5", session_count: 1, cost_usd: 2.5 }] });
  }
  if (first === "--json") return JSON.stringify({ version: 1, sessions, accounts: [], hidden: { Desktop: 2 } });
  if (first === "show") return JSON.stringify({ version: 1, agent: "claude", id: args[1], title: "Fix the flaky login test",
    cwd: `${H}/code/api`, model: "claude-fable-5-1", turns_total: 4, tool_calls: 1,
    left_out: "Claude's thinking, images, plans, /rewind snapshots and subagent transcripts", turns });
  if (first === "export") {
    const id = args[1], from = args[args.indexOf("--from") + 1], format = args[args.indexOf("--format") + 1];
    const source = sessions.find((s) => s.id === id && s.profile === from)!;
    const output = args[args.indexOf("--output") + 1];
    const extension = format === "json" ? ".json" : format === "html" ? ".html" : ".md";
    const path = `${output}${source.agent}-${source.id.slice(0, 8)}${extension}`;
    return JSON.stringify({ version: 1, dry_run: args.includes("--dry-run"), warning: "Review before sharing",
      exported: [{ source: { agent: source.agent, id: source.id, profile: source.profile }, path, format, bytes: 12000 }] });
  }
  if (first === "move" || first === "fork") {
    // To the Codex home it's a conversion: a new Codex session, with --brief a brief in the conversation's place.
    const fork = first === "fork", keep = args.includes("--keep"), convert = args[2] === "codex", brief = args.includes("--brief");
    const dry = args.includes("--dry-run"), from = args[args.indexOf("--from") + 1];
    const source = sessions.find((s) => s.id === args[1] && s.profile === from)!;
    const id = convert ? "019a9d2c-3e4f-7a5b-8c6d-7e8f9a0b1c2d" : fork ? "5b1e0c4a-7d2f-4e6b-9a3c-8f1d2e3c4b5a" : args[1];
    const target = convert ? `${H}/.codex` : args[2].startsWith("/") ? args[2] : `${H}/.claude-${args[2]}`;
    const common = { version: 1, dry_run: dry, agent: source.agent, session: source.id, source: source.profile, cwd: source.cwd,
      ...(args.includes("--backup") ? { backup: { path: `${H}/.config/hatcheck/backups/sample`, dry_run: dry, files: 2, bytes: 54000 } } : {}) };
    if (!dry) {
      const moved = { ...source, id, profile: target };
      if (!keep && !fork && !convert) sessions.splice(sessions.indexOf(source), 1);
      sessions.push(moved);
    }
    if (convert) return JSON.stringify({ ...common, action: "convert", target, target_agent: "codex", new_session: id,
      prompts: 2, tool_calls: 1, left_out: "Claude's thinking, images, plans, /rewind snapshots and subagent transcripts",
      brief, turns_dropped: 0, resume: `codex resume ${id}` });
    return JSON.stringify({ ...common, action: fork ? "fork" : keep ? "copy" : "move", target, new_session: fork ? id : null,
      deleted_original: !fork && !keep,
      items: [{ what: "transcript", from: "", to: "", state: "new", detail: "53.0 KB" }, { what: "/rewind file snapshots", from: "", to: "", state: "new", detail: "4 files" }],
      resume: `cl ${args[2]} --resume ${id}` });
  }
  if (first === "library") {
    const value = (name: string, fallback = "") => {
      const at = args.indexOf(name);
      return at < 0 ? fallback : args[at + 1];
    };
    const action = args.slice(1).find((a) => ["set", "clear", "save-filter", "remove-filter"].includes(a));
    if (action === "set" || action === "clear") {
      const id = args[args.indexOf(action) + 1], profile = value("--from");
      const s = sessions.find((x) => x.id === id && x.profile === profile)!;
      const key = `${s.agent}:${s.profile}:${s.id}`;
      if (action === "clear") {
        delete libraryRows.sessions[key];
        return JSON.stringify({ cleared: key });
      }
      const old = libraryRows.sessions[key] || { alias: null, favorite: false, tags: [], archived: false };
      const meta = { ...old, tags: [...old.tags] };
      if (args.includes("--alias")) meta.alias = value("--alias");
      if (args.includes("--clear-alias")) meta.alias = null;
      if (args.includes("--favorite")) meta.favorite = true;
      if (args.includes("--no-favorite")) meta.favorite = false;
      if (args.includes("--archive")) meta.archived = true;
      if (args.includes("--unarchive")) meta.archived = false;
      meta.tags = meta.tags.filter((tag) => !args.some((a, i) => a === "--untag" && args[i + 1] === tag));
      for (const [i, a] of args.entries()) if (a === "--tag" && !meta.tags.includes(args[i + 1])) meta.tags.push(args[i + 1]);
      meta.tags.sort();
      if (!meta.alias && !meta.favorite && !meta.archived && !meta.tags.length) delete libraryRows.sessions[key];
      else libraryRows.sessions[key] = meta;
      return JSON.stringify({ key, agent: s.agent, profile: s.profile, id: s.id, ...meta });
    }
    if (action === "save-filter") {
      const name = args[args.indexOf(action) + 1];
      libraryRows.filters[name] = {
        query: value("--query"), agent: value("--agent"), profile: value("--profile"),
        hours: Number(value("--hours", "168")), model: value("--model"), cost: value("--cost"), tag: value("--tag"),
        favorites: args.includes("--favorites"), archived: args.includes("--archived"),
      };
      return `Saved ${name}\n`;
    }
    if (action === "remove-filter") {
      delete libraryRows.filters[args[args.indexOf(action) + 1]];
      return "Removed\n";
    }
    return JSON.stringify({ version: 1, ...libraryRows });
  }
  if (first === "workspaces") {
    if (args[1] === "add") {
      const folder = args[2], at = args.indexOf("--name");
      const name = at >= 0 ? args[at + 1] : folder.split("/").pop() || folder;
      const index = workspaceRows.findIndex((w) => w.folder === folder);
      if (index >= 0) workspaceRows.splice(index, 1);
      workspaceRows.push({ name, folder, exists: true, bindings: {} });
      return JSON.stringify(workspaceRows.at(-1));
    }
    if (args[1] === "remove") {
      const index = workspaceRows.findIndex((w) => w.folder === args[2]);
      if (index >= 0) workspaceRows.splice(index, 1);
      return "Unpinned\n";
    }
    return JSON.stringify({ version: 1, workspaces: workspaceRows });
  }
  if (first === "open" && args.includes("--new")) return "Opening a terminal for the new session\n";
  if (first === "desktop" && args[1] === "--json") return JSON.stringify({ version: 1, instances: [
    { name: "work", display: "Work", app: "/Applications/Claude Work.app", data_dir: `${H}/.claude-instances/work`, running: true },
    { name: "personal", display: "Personal", app: "/Applications/Claude Personal.app", data_dir: `${H}/.claude-instances/personal`, running: false }] });
  return "";
}
