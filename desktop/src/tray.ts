// The menu-bar popover: what's left of each plan, recent sessions to resume, and the Claude
// Desktop instances. It also refreshes in the background and sends the notifications.
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  hidePopover, inApp, instances, openInstance, openMain, quit, recent, resume, settings, usage, version,
  type Account, type Limit, type Session,
} from "./engine";
import { accountHead, agentName, ago, clock, errorText, esc, learnHome, limitRow, pasteCommand, profileName, tilde, toast } from "./format";

const $ = (id: string) => document.getElementById(id)!;
let sessionsById = new Map<string, Session>();

async function loadPlans(): Promise<Account[]> {
  try {
    const data = await usage();
    learnHome(data.accounts.flatMap((a) => a.profiles));
    $("plans-note").textContent = data.sorted_by === "fable" ? "sorted by Fable" : "";
    $("plans").classList.remove("loading");
    $("plans").innerHTML = data.accounts.length
      ? data.accounts.map((a) => `<div class="account">${accountHead(a)}${a.limits.map((l) => limitRow(l, true)).join("")}${
          a.source === "none" ? `<div class="why">${esc(a.why)}</div>` : ""}</div>`).join("")
      : `<p class="empty">No Claude Code profiles or Codex homes yet.</p>`;
    return data.accounts;
  } catch (e) {
    $("plans").innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
    return [];
  }
}

async function loadSessions() {
  try {
    const data = await recent(24);
    learnHome(data.sessions.map((s) => s.profile));
    sessionsById = new Map(data.sessions.map((s) => [s.id + s.profile, s]));
    const folders = new Map<string, Session[]>();
    for (const s of [...data.sessions].sort((a, b) => b.last_active.localeCompare(a.last_active)).slice(0, 14)) {
      const key = s.cwd || "?";
      folders.set(key, [...(folders.get(key) || []), s]);
    }
    $("sessions").classList.remove("loading");
    $("sessions").innerHTML = folders.size
      ? [...folders].map(([folder, list]) => `<div class="folder"><div class="folder-name">${esc(tilde(folder))}</div>${
          list.map((s) => `<div class="session">
            <div class="session-main">
              <span class="title" title="${esc(s.title)}">${esc(s.title)}</span>
              <span class="meta"><span class="agent ${s.agent}">${agentName(s.agent)}</span>
                ${esc(profileName(s.profile))} · ${ago(s.last_active)}${s.open ? ` · <span class="open-dot">● open</span>` : ""}${
                  s.bound_to ? ` · <span class="warn" title="This folder is bound to ${esc(tilde(s.bound_to))}">⚠ bound</span>` : ""}</span>
            </div>
            <div class="actions">
              <button data-resume="${esc(s.id + s.profile)}" ${s.open ? "disabled title=\"It's open in a terminal\"" : ""}>Resume</button>
              <button class="ghost" data-copy="${esc(s.id + s.profile)}" title="Copy the command">Copy</button>
            </div>
          </div>`).join("")}</div>`).join("")
      : `<p class="empty">No sessions in the last 24 hours.</p>`;
  } catch (e) {
    $("sessions").innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

async function loadInstances() {
  try {
    const list = (await instances()).instances;
    $("desktop-section").hidden = list.length === 0;
    $("instances").innerHTML = list.map((i) => `<button class="instance" data-instance="${esc(i.name)}" title="${esc(i.app)}">${
      i.running ? `<span class="open-dot">●</span> ` : ""}${esc(i.display)}</button>`).join("");
  } catch {
    $("desktop-section").hidden = true; // not macOS, or no instances
  }
}

// Notifications: once when a limit runs low, once when a low one resets, and once when a limit
// is about to reset with much of it unused (the last fifth of its window, a day at most).
const SEEN = "hatcheck.limits";
function resetsSoonUnused(l: Limit, unused: number): boolean {
  if (unused <= 0 || l.left_percent === null || !l.resets_at || !l.window_seconds) return false;
  const toGo = (Date.parse(l.resets_at) - Date.now()) / 1000;
  const horizon = Math.min(l.window_seconds / 5, 24 * 3600);
  return toGo > 0 && toGo <= horizon && l.left_percent >= unused;
}

async function notifyChanges(accounts: Account[]) {
  const config = settings();
  if (!config.notify || !inApp) return;
  let before: Record<string, { left: number | null; resets: string | null; warned?: boolean }> = {};
  try {
    before = JSON.parse(localStorage.getItem(SEEN) || "{}");
  } catch {
    before = {};
  }
  const now: typeof before = {};
  const messages: string[] = [];
  for (const a of accounts) {
    if (a.source === "none") continue;
    const who = a.profiles.map(profileName).join(", ");
    for (const l of a.limits) {
      const key = a.profiles[0] + "|" + l.name;
      const was = before[key];
      // The unused warning goes once per reset: it stays "warned" while the reset is the same.
      let warned = Boolean(was && was.resets === l.resets_at && was.warned);
      if (!warned && resetsSoonUnused(l, config.unused)) {
        messages.push(`${who}: ${l.name} resets ${clock(l.resets_at)} with ${Math.round(l.left_percent!)}% unused`);
        warned = true;
      }
      now[key] = { left: l.left_percent, resets: l.resets_at, warned };
      if (!was || l.left_percent === null) continue;
      const wasLow = was.left !== null && was.left <= config.low;
      if (!wasLow && l.left_percent <= config.low) {
        messages.push(`${who}: ${l.name} is down to ${Math.round(l.left_percent)}%, back ${clock(l.resets_at)}`);
      } else if (wasLow && l.left_percent > config.low && was.resets !== l.resets_at) {
        messages.push(`${who}: ${l.name} has reset, ${Math.round(l.left_percent)}% left`);
      }
    }
  }
  try {
    localStorage.setItem(SEEN, JSON.stringify(now));
  } catch {
    // without storage there's nothing to compare with next time
  }
  if (!messages.length) return;
  let allowed = await isPermissionGranted();
  if (!allowed) allowed = (await requestPermission()) === "granted";
  if (allowed) for (const body of messages.slice(0, 4)) sendNotification({ title: "Hatcheck", body });
}

let busy = false;
async function refresh() {
  if (busy) return;
  busy = true;
  try {
    const [accounts] = await Promise.all([loadPlans(), loadSessions(), loadInstances()]);
    await notifyChanges(accounts);
  } finally {
    busy = false;
  }
}

document.addEventListener("click", async (event) => {
  const target = event.target as HTMLElement;
  const button = target.closest("button") as HTMLButtonElement | null;
  if (!button) return;
  try {
    if (button.dataset.resume) {
      const s = sessionsById.get(button.dataset.resume)!;
      button.disabled = true;
      await resume(s);
      toast("Opening a terminal for " + s.title);
      await hidePopover();
    } else if (button.dataset.copy) {
      const s = sessionsById.get(button.dataset.copy)!;
      if (inApp) await writeText(pasteCommand(s));
      else await navigator.clipboard?.writeText(pasteCommand(s));
      toast("Copied: " + pasteCommand(s));
    } else if (button.dataset.instance) {
      await openInstance(button.dataset.instance);
      await hidePopover();
    } else if (button.id === "refresh") {
      await refresh();
    } else if (button.id === "open-main") {
      await openMain("sessions");
    } else if (button.id === "settings") {
      await openMain("settings");
    } else if (button.id === "quit") {
      await quit();
    }
  } catch (e) {
    toast(errorText(e), "error");
  } finally {
    if (button.dataset.resume) button.disabled = false;
  }
});

async function start() {
  try {
    $("version").textContent = `${await version()} · Unofficial`;
  } catch {
    $("version").textContent = "Unofficial";
  }
  await refresh();
  if (inApp) {
    await listen("shown", () => refresh());
    await listen("refresh", () => refresh());
  }
  // In the background too, for the notifications; every few minutes, as set.
  const tick = () => setTimeout(async () => {
    await refresh();
    tick();
  }, settings().minutes * 60_000);
  tick();
}

start();
