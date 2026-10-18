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

export type Account = {
  agent: "claude" | "codex";
  profiles: string[];
  plan: string;
  source: "live" | "cached" | "none";
  saved_at: string | null;
  why: string | null;
  limits: Limit[];
  extra: string | null;
  fable: null | { left: boolean; resets_at?: string | null; back_at?: string | null };
};

export type Session = {
  agent: "claude" | "codex";
  id: string;
  profile: string;
  account: number | null;
  title: string;
  cwd: string | null;
  last_active: string;
  entrypoint: string;
  branch: string | null;
  model: string | null;
  open: null | { pid: number; status: string | null };
  continued_in: string | null;
  bound_to: string | null;
  resume: string;
};

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

export type MoveReport = {
  dry_run: boolean;
  action: "copy" | "move" | "update" | "fork" | "convert" | "nothing";
  agent?: string;
  session: string;
  source: string;
  target: string;
  target_agent?: string;
  new_session: string | null;
  title?: string | null;
  reason?: string;
  why?: string | null;
  items?: { what: string; from: string; to: string; state: string; detail: string }[];
  prompts?: number;
  tool_calls?: number;
  left_out?: string;
  turns_dropped?: number;
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

export const recent = (hours: number, everything = false) =>
  json<{ sessions: Session[]; hidden: Record<string, number> }>(
    ["--json", "--cached", "--no-usage", "--all-folders", String(hours), ...(everything ? ["--all"] : [])]);

export const conversation = (s: Session, last = 300) =>
  json<Conversation>(["show", s.id, "--from", s.profile, "--json", "--last", String(last)]);

export const resume = (s: Session) => engine(["open", s.id, "--from", s.profile]);

export function move(s: Session, to: string, how: { dryRun: boolean; noKeep?: boolean; fork?: boolean }) {
  const args = ["move", s.id, to, "--from", s.profile, "--json"];
  if (how.dryRun) args.push("--dry-run");
  if (how.noKeep) args.push("--no-keep");
  if (how.fork) args.push("--fork");
  return json<MoveReport>(args);
}

export const signIn = (profile: string) => engine(["open", "--login", profile]);

export const instances = () => json<{ instances: Instance[] }>(["desktop", "--json"]);
export const openInstance = (name: string) => engine(["desktop", "open", name]);
export const newInstance = (name: string, display: string, hue: number) =>
  engine(["desktop", "new", name, display, "--hue", String(hue)]);

export const version = async () => (await engine(["--version"])).trim();

// Window and app actions, done by the app's own commands.
export const openMain = (tab?: string) => (inApp ? invoke("open_main", { tab: tab ?? null }) : Promise.resolve());
export const hidePopover = () => (inApp ? invoke("hide_popover") : Promise.resolve());
export const quit = () => (inApp ? invoke("quit") : Promise.resolve());
