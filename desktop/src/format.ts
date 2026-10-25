// Formatting shared by the popover and the main window: times, bars, paths, commands.
import type { Account, Agent, Limit, Session } from "./engine";

let homeDir = "";

/** The home folder, for printing paths as ~/...; guessed from the paths the engine gives. */
export function learnHome(paths: string[]) {
  for (const p of paths) {
    const m = p.match(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)[/\\]\.(claude|codex|copilot|dsh|gemini|kimi-code|minimax|qwen|zcode)/);
    if (m) {
      homeDir = m[1];
      return;
    }
  }
}

export const isWindows = navigator.userAgent.includes("Windows");

export function tilde(path: string | null | undefined): string {
  if (!path) return "?";
  return homeDir && path.startsWith(homeDir) ? "~" + path.slice(homeDir.length) : path;
}

/** A profile's short name, or its path when it isn't a conventional home. */
export function profileName(path: string): string {
  const base = path.split(/[/\\]/).pop() || path;
  if (base === ".claude") return "default";
  if (base === "antigravity-cli") return "agy";
  if (base === ".codex") return "codex";
  if (base === ".kimi-code") return "kimi";
  if (base === ".zcode") return "zcode";
  if (base === ".dsh") return "dsh";
  if (base === ".minimax") return "mcode";
  if (base === ".minimax-code") return tilde(path);
  if (base === ".qwen") return "qwen";
  if (base === ".copilot") return "copilot";
  if (base === "opencode") return "opencode"; // ~/.local/share/opencode: its one data folder
  if (base.startsWith(".claude-")) return base.slice(".claude-".length);
  if (base.startsWith(".codex-")) return "codex:" + base.slice(".codex-".length);
  if (base.startsWith(".kimi-code-")) return "kimi:" + base.slice(".kimi-code-".length);
  if (base.startsWith(".dsh-")) return "dsh:" + base.slice(".dsh-".length);
  if (base.startsWith(".minimax-")) return "mcode:" + base.slice(".minimax-".length);
  if (base.startsWith(".qwen-")) return "qwen:" + base.slice(".qwen-".length);
  if (base.startsWith(".copilot-")) return "copilot:" + base.slice(".copilot-".length);
  return tilde(path);
}

/** The agent's short name; the long form spells out CLI, Code and Harness names. */
export function agentName(agent: Agent, long = false): string {
  if (agent === "codex") return "Codex";
  if (agent === "agy") return long ? "Antigravity CLI" : "Antigravity";
  if (agent === "kimi") return long ? "Kimi Code CLI" : "Kimi";
  if (agent === "zcode") return "ZCode";
  if (agent === "dsh") return long ? "DeepSeek Harness" : "DeepSeek";
  if (agent === "mcode") return long ? "MiniMax Code" : "MiniMax";
  if (agent === "qwen") return long ? "Qwen Code" : "Qwen";
  if (agent === "copilot") return long ? "GitHub Copilot CLI" : "Copilot";
  if (agent === "opencode") return "OpenCode";
  return long ? "Claude Code" : "Claude";
}

export function accountName(a: Account): string {
  return a.profiles.map(tilde).join(", ");
}

const MINUTE = 60_000;

/** How long ago, as 4m, 3h, 2d. */
export function ago(iso: string | null): string {
  if (!iso) return "";
  const ms = Date.now() - Date.parse(iso);
  if (ms < 60 * MINUTE) return Math.max(0, Math.floor(ms / MINUTE)) + "m";
  if (ms < 24 * 60 * MINUTE) return Math.floor(ms / (60 * MINUTE)) + "h";
  return Math.floor(ms / (24 * 60 * MINUTE)) + "d";
}

/** How long until, as 2h10m or 3d4h. */
export function until(iso: string | null): string {
  if (!iso) return "";
  const minutes = Math.max(0, Math.round((Date.parse(iso) - Date.now()) / MINUTE));
  if (minutes < 60) return minutes + "m";
  if (minutes < 24 * 60) return Math.floor(minutes / 60) + "h" + String(minutes % 60).padStart(2, "0") + "m";
  return Math.floor(minutes / (24 * 60)) + "d" + Math.floor((minutes % (24 * 60)) / 60) + "h";
}

/** A reset time as the clock shows it: 3:10pm today, Tue 12pm this week, else Oct 22. */
export function clock(iso: string | null): string {
  if (!iso) return "";
  const t = new Date(iso);
  const hours = (t.getTime() - Date.now()) / (60 * MINUTE);
  const time = t.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (hours < 20) return time;
  if (hours < 6 * 24) return t.toLocaleDateString([], { weekday: "short" }) + " " + time;
  return t.toLocaleDateString([], { month: "short", day: "numeric" });
}

/** How much of a limit's window is still to run, 0 to 1, for the time bar. */
export function windowLeft(limit: Limit): number | null {
  if (!limit.resets_at || !limit.window_seconds) return null;
  return Math.min(1, Math.max(0, (Date.parse(limit.resets_at) - Date.now()) / (limit.window_seconds * 1000)));
}

/** The colour class for what's left of a limit. */
export function level(left: number | null): string {
  if (left === null) return "unknown";
  return left <= 10 ? "low" : left <= 35 ? "mid" : "ok";
}

/** The colour class for how near a reset is: soon is red, as the CLI's time bars are. */
export function urgency(fraction: number | null): string {
  if (fraction === null) return "";
  return fraction < 0.15 ? "soon" : fraction < 0.4 ? "nearing" : "far";
}

export function esc(text: string | null | undefined): string {
  return (text ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

function shellQuote(s: string): string {
  return /^[A-Za-z0-9_./~:@%+=-]+$/.test(s) ? s : "'" + s.replace(/'/g, "'\\''") + "'";
}

/** The command to paste in a terminal: into the session's folder, then resume or open it. */
export function pasteCommand(s: Session): string {
  if (!s.cwd) return s.resume;
  if (isWindows) return `Set-Location -LiteralPath '${s.cwd.replace(/'/g, "''")}'; ${s.resume}`;
  const cwd = tilde(s.cwd);
  return `cd ${cwd.startsWith("~/") ? "~/" + shellQuote(cwd.slice(2)) : shellQuote(cwd)} && ${s.resume}`;
}

/** One limit as a row: name, % left with its bar, and the time left until it resets. */
export function limitRow(limit: Limit, compact = false): string {
  const left = limit.left_percent;
  const pct = left === null ? "?" : Math.round(left) + "%";
  const fraction = windowLeft(limit);
  const reset = limit.resets_at
    ? left === null
      ? `reset ${clock(limit.resets_at)}, after this snapshot`
      : compact
        ? until(limit.resets_at)
        : `resets ${clock(limit.resets_at)} · in ${until(limit.resets_at)}`
    : "";
  return `<div class="limit ${limit.per_model ? "per-model" : ""}">
      <span class="limit-name" title="${esc(limit.name)}">${esc(compact ? limit.short : limit.name)}</span>
      <span class="pct ${level(left)}">${pct}</span>
      <span class="bar ${level(left)}"><i style="width:${left ?? 0}%"></i></span>
      <span class="timebar ${urgency(fraction)}" title="${fraction === null ? "" : Math.round(fraction * 100) + "% of the window to go"}">${
        fraction === null ? "" : `<i style="width:${fraction * 100}%"></i>`}</span>
      <span class="reset">${esc(reset)}</span>
    </div>`;
}

/** An account's heading: its profiles, plan, and where the numbers come from. */
export function accountHead(a: Account): string {
  const source =
    a.source === "live"
      ? `<span class="tag live">live</span>`
      : a.source === "cached"
        ? `<span class="tag cached" title="${esc(a.why || "")}">saved ${ago(a.saved_at)} ago</span>`
        : `<span class="tag none" title="${esc(a.why || "")}">no data</span>`;
  const fable = a.fable
    ? a.fable.left
      ? `<span class="tag fable" title="Fable left; it resets ${esc(clock(a.fable.resets_at ?? null))}">Fable ✓</span>`
      : `<span class="tag fable-out" title="Out of Fable until ${esc(clock(a.fable.back_at ?? null))}">Fable ✗</span>`
    : "";
  return `<div class="account-head">
      <span class="agent ${a.agent}">${agentName(a.agent)}</span>
      <span class="account-name">${esc(accountName(a))}</span>
      <span class="plan">${esc(a.plan)}</span>${fable}${source}
    </div>`;
}

export function toast(message: string, kind: "info" | "error" = "info") {
  let box = document.getElementById("toasts");
  if (!box) {
    box = document.createElement("div");
    box.id = "toasts";
    document.body.appendChild(box);
  }
  const el = document.createElement("div");
  el.className = "toast " + kind;
  el.textContent = message;
  box.appendChild(el);
  setTimeout(() => el.remove(), kind === "error" ? 9000 : 3500);
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
