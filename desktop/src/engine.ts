// The hatcheck engine, as the pages see it: each call runs the hatcheck CLI through the app's
// `engine` command and reads its --json output. Outside the app, in a plain browser, sample
// data stands in, so the pages can be worked on with `npm run dev`.
import { invoke } from "@tauri-apps/api/core";
import { sample } from "./sample";

export const inApp = "__TAURI_INTERNALS__" in window;

export type Limit = {
  name: string;
  short: string;
  kind: string | null;
  used_percent: number | null;
  left_percent: number | null;
  resets_at: string | null;
  window_seconds: number | null;
  severity: string | null;
  per_model: boolean;
};

export type Agent = "claude" | "codex" | "agy" | "kimi" | "zcode" | "dsh" | "mcode" | "qwen" | "copilot" | "opencode";

export type Account = {
  agent: Agent;
  profiles: string[];
  plan: string;
  source: "live" | "cached" | "none";
  saved_at: string | null;
  why: string | null;
  limits: Limit[];
  extra: string | null;
  fable: null | { left: boolean; resets_at?: string | null; back_at?: string | null };
};

export type InsightAccount = { agent: string; profile: string; label: string };
export type InsightSeries = {
  account: InsightAccount;
  readings: { time: string; used_percent: number; resets_at: string | null }[];
  trend: null | { pace_percent_per_hour: number; over_seconds: number; readings: number; runs_out_at: string | null; before_reset: boolean | null };
};
export type InsightForecast = {
  kind: "runs_out" | "unused"; account: InsightAccount; left_percent?: number; resets_at?: string | null;
  runs_out_at?: string | null; pace_percent_per_hour?: number;
};
export type InsightSpend = { session_count: number; cost_usd: number; profile?: string; project?: string; model?: string };
export type Insights = {
  days: number;
  series: InsightSeries[];
  forecasts: InsightForecast[];
  weekly: { session_count: number; costed_session_count: number; cost_usd: number };
  profiles: InsightSpend[]; projects: InsightSpend[]; models: InsightSpend[];
};

export type Session = {
  agent: Agent;
  id: string;
  profile: string;
  account: number | null;
  title: string;
  cwd: string | null;
  last_active: string;
  entrypoint: string;
  branch: string | null;
  model: string | null;
  open: null | { pid: number | null; status: string | null; in?: string };
  continued_in: string | null;
  bound_to: string | null;
  resume: string;
  cost_usd: number | null;
};

export type SessionMetadata = { alias: string | null; favorite: boolean; tags: string[]; archived: boolean };
export type SavedFilter = {
  query: string; agent: string; profile: string; hours: number; model: string; cost: string;
  tag: string; favorites: boolean; archived: boolean;
};
export type Library = { sessions: Record<string, SessionMetadata>; filters: Record<string, SavedFilter> };

export type Workspace = { name: string; folder: string; exists: boolean; bindings: Record<string, string> };

export type Instance = { name: string; display: string; app: string; data_dir: string | null; running: boolean };

export type Conversation = {
  agent: string;
  id: string;
  title: string | null;
  cwd: string | null;
  model: string | null;
  turns_total: number;
  tool_calls: number;
  left_out: string;
  turns: { role: string; text: string; time: string | null }[];
};

export type ExportReport = {
  dry_run: boolean;
  exported: { source: { agent: string; id: string; profile: string }; path: string; format: string; bytes: number }[];
  warning: string;
};

export type MoveReport = {
  dry_run: boolean;
  action: "copy" | "move" | "update" | "fork" | "convert";
  agent?: string;
  session: string;
  source: string;
  target: string;
  target_agent?: string;
  new_session: string | null;
  title?: string | null;
  deleted_original?: boolean;
  repointed?: boolean;
  backup?: { path: string; dry_run: boolean; files: number; bytes: number; skipped: number };
  why?: string | null;
  items?: { what: string; from: string; to: string; state: string; detail: string }[];
  prompts?: number;
  tool_calls?: number;
  left_out?: string;
  turns_dropped?: number;
  brief?: boolean;
  paste?: string | null;
  cwd: string | null;
  resume: string;
};

// Settings, per computer: kept in the app's local storage.
const KEY = "hatcheck.settings";
type Settings = { live: boolean; minutes: number; notify: boolean; low: number; unused: number; terminal: string };
// unused: a limit about to reset with this much or more of it left is worth a notification; 0 for never.
const DEFAULTS: Settings = { live: false, minutes: 5, notify: true, low: 10, unused: 50, terminal: "" };

export function settings(): Settings {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) || "{}") };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveSettings(change: Partial<Settings>): Settings {
  const next = { ...settings(), ...change };
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // private mode or storage off: the setting lasts until the window closes
  }
  return next;
}

export async function engine(args: string[]): Promise<string> {
  if (!inApp) return sample(args);
  return invoke<string>("engine", { args, terminal: settings().terminal || null });
}

async function json<T>(args: string[]): Promise<T> {
  return JSON.parse(await engine(args)) as T;
}

export const usage = () =>
  json<{ accounts: Account[]; sorted_by: string | null }>(["usage", "--json", ...(settings().live ? [] : ["--cached"])]);

export const insights = (days = 7) =>
  json<Insights>(["insights", "--days", String(days), "--unused", String(settings().unused), "--json"]);

export const recent = (hours: number, everything = false) =>
  json<{ sessions: Session[]; hidden: Record<string, number> }>(
    ["--json", "--cached", "--no-usage", "--all-folders", String(hours), ...(everything ? ["--all"] : [])]);

export const conversation = (s: Session, last = 300) =>
  json<Conversation>(["show", s.id, "--from", s.profile, "--json", "--last", String(last)]);

export const resume = (s: Session) => engine(["open", s.id, "--from", s.profile]);

// A move, or with fork, hatcheck fork: a new session with its own ID, which always keeps the original.
// With brief, --brief: a conversion into another agent hands over a brief in place of the conversation.
export function move(s: Session, to: string, how: { dryRun: boolean; keep?: boolean; fork?: boolean; brief?: boolean; backup?: boolean }) {
  const args = [how.fork ? "fork" : "move", s.id, to, "--from", s.profile, "--json"];
  if (how.dryRun) args.push("--dry-run");
  if (how.keep && !how.fork) args.push("--keep");
  if (how.backup && !how.fork && !how.keep) args.push("--backup");
  if (how.brief) args.push("--brief");
  return json<MoveReport>(args);
}

export function exportSession(s: Session, format: string, directory: string, replace: boolean) {
  const args = ["export", s.id, "--from", s.profile, "--format", format, "--output", directory + "/", "--json"];
  if (replace) args.push("--replace");
  return json<ExportReport>(args);
}

export const signIn = (profile: string) => engine(["open", "--login", profile]);
export const newSession = (profile: string, cwd: string) => engine(["open", "--new", profile, "--cwd", cwd]);

export const workspaces = () => json<{ workspaces: Workspace[] }>(["workspaces", "--json"]);
export const pinWorkspace = (folder: string, name: string) =>
  engine(["workspaces", "add", folder, ...(name ? ["--name", name] : [])]);
export const unpinWorkspace = (folder: string) => engine(["workspaces", "remove", folder]);

export const library = () => json<Library>(["library", "--json"]);
export function updateSession(s: Session, change: {
  alias?: string | null; favorite?: boolean; archived?: boolean; addTags?: string[]; removeTags?: string[];
}) {
  const args = ["library", "--json", "set", s.id, "--from", s.profile];
  if (change.alias !== undefined) args.push(...(change.alias === null ? ["--clear-alias"] : ["--alias", change.alias]));
  if (change.favorite !== undefined) args.push(change.favorite ? "--favorite" : "--no-favorite");
  if (change.archived !== undefined) args.push(change.archived ? "--archive" : "--unarchive");
  for (const tag of change.removeTags || []) args.push("--untag", tag);
  for (const tag of change.addTags || []) args.push("--tag", tag);
  return json<SessionMetadata & { key: string }>(args);
}
export function saveFilter(name: string, filter: SavedFilter) {
  const args = ["library", "save-filter", name, "--query", filter.query, "--agent", filter.agent,
    "--profile", filter.profile, "--hours", String(filter.hours), "--model", filter.model,
    "--cost", filter.cost, "--tag", filter.tag];
  if (filter.favorites) args.push("--favorites");
  if (filter.archived) args.push("--archived");
  return engine(args);
}
export const removeFilter = (name: string) => engine(["library", "remove-filter", name]);

export const instances = () => json<{ instances: Instance[] }>(["desktop", "--json"]);
export const openInstance = (name: string) => engine(["desktop", "open", name]);
export const newInstance = (name: string, display: string, hue: number) =>
  engine(["desktop", "new", name, display, "--hue", String(hue)]);

export const version = async () => (await engine(["--version"])).trim();

// Window and app actions, done by the app's own commands.
export const openMain = (tab?: string) => (inApp ? invoke("open_main", { tab: tab ?? null }) : Promise.resolve());
export const hidePopover = () => (inApp ? invoke("hide_popover") : Promise.resolve());
export const quit = () => (inApp ? invoke("quit") : Promise.resolve());
