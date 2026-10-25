// The main window: browse every session with a read-only preview, resume or open, copy or move one
// (with a dry run first), see every plan, add profiles, and Claude Desktop instances.
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import {
  conversation, inApp, instances, move, newInstance, openInstance, recent, resume, saveSettings, settings, signIn,
  usage, version, type Account, type MoveReport, type Session,
} from "./engine";
import { accountHead, agentName, ago, errorText, esc, learnHome, limitRow, pasteCommand, profileName, tilde, toast } from "./format";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
let accounts: Account[] = [];
let sessions: Session[] = [];
let selected: Session | null = null;

// Tabs.
function showTab(name: string) {
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) tab.classList.toggle("active", tab.dataset.tab === name);
  for (const body of document.querySelectorAll<HTMLElement>(".tab-body")) body.hidden = body.id !== "tab-" + name;
  if (name === "plans" || name === "profiles") void loadAccounts();
  if (name === "desktop") void loadInstances();
}

// Sessions.
async function loadSessions() {
  const list = $("session-list");
  try {
    const data = await recent(Number($<HTMLSelectElement>("filter-hours").value), $<HTMLInputElement>("filter-all").checked);
    sessions = data.sessions.sort((a, b) => b.last_active.localeCompare(a.last_active));
    learnHome(sessions.map((s) => s.profile));
    const profiles = [...new Set(sessions.map((s) => s.profile))].sort();
    const pick = $<HTMLSelectElement>("filter-profile");
    const kept = pick.value;
    pick.innerHTML = `<option value="">Every profile</option>` +
      profiles.map((p) => `<option value="${esc(p)}">${esc(tilde(p))}</option>`).join("");
    pick.value = profiles.includes(kept) ? kept : "";
    list.classList.remove("loading");
    renderSessions();
  } catch (e) {
    list.innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

function renderSessions() {
  const words = $<HTMLInputElement>("search").value.toLowerCase().split(/\s+/).filter(Boolean);
  const agent = $<HTMLSelectElement>("filter-agent").value;
  const profile = $<HTMLSelectElement>("filter-profile").value;
  const shown = sessions.filter((s) =>
    (!agent || s.agent === agent) && (!profile || s.profile === profile) &&
    words.every((w) => (s.title + " " + (s.cwd || "")).toLowerCase().includes(w)));
  $("session-list").innerHTML = shown.length
    ? shown.map((s) => `<button class="row-session ${selected && selected.id === s.id && selected.profile === s.profile ? "selected" : ""}"
          data-session="${esc(s.id)}" data-profile="${esc(s.profile)}">
        <span class="title">${esc(s.title)}</span>
        <span class="meta"><span class="agent ${s.agent}">${agentName(s.agent)}</span>
          ${esc(profileName(s.profile))} · ${esc(tilde(s.cwd))} · ${ago(s.last_active)}${s.open ? ` · <span class="open-dot">● open</span>` : ""}${
            s.bound_to ? ` · <span class="warn">⚠ bound to ${esc(profileName(s.bound_to))}</span>` : ""}</span>
      </button>`).join("")
    : `<p class="empty">No sessions match.</p>`;
}

async function select(s: Session) {
  selected = s;
  renderSessions();
  const detail = $("session-detail");
  const dsh = s.agent === "dsh";
  detail.innerHTML = `<div class="detail-head">
      <h2>${esc(s.title)}</h2>
      <div class="meta">${agentName(s.agent, true)} · ${esc(tilde(s.profile))} · ${esc(tilde(s.cwd))}${
        s.branch ? " · " + esc(s.branch) : ""}${s.model ? " · " + esc(s.model) : ""} · ${ago(s.last_active)} ago</div>
      <div class="meta mono">${esc(s.id)}</div>
      <div class="actions">
        <button id="act-resume" ${s.open ? `disabled title="It's open in ${dsh ? "dsh" : "a terminal"}"` : ""}>${dsh ? "Open DeepSeek Harness" : "Resume in a terminal"}</button>
        <button id="act-copy" class="ghost">Copy command</button>
        <button id="act-move" class="ghost">Move to another profile…</button>
      </div>
      ${dsh ? `<p class="note">Opens DSH in this session's recorded folder. Choose the session in its web UI; the selected session isn't resumed directly.</p>` : ""}
      ${s.open ? `<p class="note">Open in ${s.open.pid ? "pid " + s.open.pid : esc(s.open.in ?? "a terminal")}${s.open.status ? " (" + esc(s.open.status) + ")" : ""}: quit it before moving it.</p>` : ""}
      ${s.bound_to ? `<p class="warn">This folder is bound to ${esc(tilde(s.bound_to))}; this session ran in ${esc(tilde(s.profile))}.</p>` : ""}
    </div>
    <div id="conversation" class="conversation loading">Loading the conversation…</div>`;
  try {
    const conv = await conversation(s);
    if (selected !== s) return;
    $("conversation").classList.remove("loading");
    const earlier = conv.turns_total - conv.turns.length;
    $("conversation").innerHTML = (earlier > 0 ? `<p class="note">${earlier} earlier turns aren't shown.</p>` : "") +
      conv.turns.map((t) => `<div class="turn ${t.role}"><div class="who">${t.role === "user" ? "You" : agentName(s.agent)}${
        t.time ? ` <span class="note">${ago(t.time)} ago</span>` : ""}</div><div class="text">${formatTurn(t.text)}</div></div>`).join("") +
      `<p class="note">Left out: ${esc(conv.left_out)}.</p>`;
  } catch (e) {
    $("conversation").innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

/** A turn's text, with tool calls and their output set apart as code. */
function formatTurn(text: string): string {
  return esc(text)
    .replace(/\[external_agent_tool_call: ([^\]]+)\]\n([\s\S]*?)\n\[\/external_agent_tool_call\]/g,
      (_, name, body) => `<div class="tool"><div class="tool-name">${name}</div><pre>${body}</pre></div>`)
    .replace(/\[external_agent_tool_result\]\n([\s\S]*?)\n\[\/external_agent_tool_result\]/g,
      (_, body) => `<pre class="tool-result">${body}</pre>`)
    .replace(/\n/g, "<br>");
}

// Moving: pick a profile, see the dry run, then do it.
function dialog(title: string, body: string, actions: { label: string; kind?: string; run: () => void | Promise<void> }[]) {
  $("dialog-title").textContent = title;
  $("dialog-body").innerHTML = body;
  const box = $("dialog-actions");
  box.innerHTML = "";
  for (const a of actions) {
    const b = document.createElement("button");
    b.textContent = a.label;
    if (a.kind) b.className = a.kind;
    b.onclick = async () => {
      b.disabled = true;
      try {
        await a.run();
      } catch (e) {
        toast(errorText(e), "error");
      } finally {
        b.disabled = false;
      }
    };
    box.appendChild(b);
  }
  $("dialog").hidden = false;
}
const closeDialog = () => {
  $("dialog").hidden = true;
};

type Destination = { value: string; label: string; agent: Account["agent"] };

function destinations(s: Session): Destination[] {
  const out: Destination[] = [];
  for (const a of accounts) {
    if (a.agent === "mcode") continue;
    const left = a.limits.filter((l) => l.left_percent !== null).map((l) => `${l.short} ${Math.round(l.left_percent!)}%`).join(", ");
    for (const p of a.profiles) {
      if (p === s.profile) continue;
      const name = profileName(p);
      out.push({ value: name.startsWith("~") ? p : name, label: `${tilde(p)} (${agentName(a.agent, true)}, ${a.plan}${left ? ", " + left : ""})`, agent: a.agent });
    }
  }
  return out;
}

function reportText(r: MoveReport): string {
  const verb = r.dry_run ? "Would" : "Did";
  const what = { copy: "copy it", move: "move it", update: "bring an older copy up to date", fork: "bring it in as a new session",
    convert: "convert it into a new session" }[r.action];
  const items = (r.items || []).map((i) => `<tr><td>${esc(i.what)}</td><td>${esc(i.detail)}</td><td>${esc(i.state)}</td></tr>`).join("");
  const dsh = (r.target_agent ?? r.agent) === "dsh";
  return `<p>${verb} ${esc(what)}, to ${esc(tilde(r.target))}.</p>` +
    (items ? `<table class="items"><tr><th>What</th><th>Size</th><th>There</th></tr>${items}</table>` : "") +
    (r.action === "convert" ? `<p class="note">${r.prompts} of your messages and ${r.tool_calls} tool calls come across as text; left out: ${esc(r.left_out)}.${
      r.brief ? ` Handed over as a brief: the first prompt, the files and commands its tool calls named, and the last turns${
        r.turns_dropped ? `, without the oldest ${r.turns_dropped}` : ""}.`
      : r.turns_dropped ? ` The oldest ${r.turns_dropped} turns are left out to fit.` : ""}</p>` : "") +
    (r.new_session ? `<p class="note">New session: <span class="mono">${esc(r.new_session)}</span></p>` : "") +
    (r.deleted_original ? `<p class="note">${r.dry_run ? "Would delete" : "Deleted"} the original from ${esc(tilde(r.source))}.</p>` : "") +
    (dsh
      ? `<p class="note">Open DSH with <code>${esc(r.resume)}</code>, then choose the session in its web UI; the selected session isn't resumed directly.</p>`
      : `<p class="note">Resume it with <code>${esc(r.resume)}</code></p>`);
}

async function startMove(s: Session) {
  if (!accounts.length) await loadAccounts();
  const options = destinations(s);
  if (!options.length) return toast("There's no other profile or home to move it to.", "error");
  dialog(`Move "${s.title}"`,
    `<label>To <select id="move-to">${options.map((o) => `<option value="${esc(o.value)}" data-agent="${esc(o.agent)}">${esc(o.label)}</option>`).join("")}</select></label>
     <label class="check"><input id="move-keep" type="checkbox" /> Keep the original, which copies it instead</label>
     <label class="check"><input id="move-fork" type="checkbox" /> Fork it: a new session with its own ID, beside any copy there</label>
     <label class="check"><input id="move-brief" type="checkbox" /> Hand over a brief in place of the whole conversation: the first prompt, the files and commands its tool calls named, and the last turns (to another agent only)</label>
     <div id="move-report" class="report"></div>`,
    [
      { label: "Cancel", kind: "ghost", run: closeDialog },
      { label: "Preview", kind: "ghost", run: () => runMove(s, true) },
      { label: "Move it", run: () => runMove(s, false) },
    ]);
  // A fork always keeps the original: its box shows that while Fork is ticked.
  const keep = $<HTMLInputElement>("move-keep"), fork = $<HTMLInputElement>("move-fork");
  let kept = keep.checked;
  fork.addEventListener("change", () => {
    if (fork.checked) kept = keep.checked;
    keep.checked = fork.checked || kept;
    keep.disabled = fork.checked;
  });
  // A brief is only for a conversion into another agent: between an agent's own profiles the
  // session goes whole, and hatcheck refuses --brief.
  const to = $<HTMLSelectElement>("move-to"), brief = $<HTMLInputElement>("move-brief");
  const fit = () => {
    brief.disabled = to.selectedOptions[0]?.dataset.agent === s.agent;
    if (brief.disabled) brief.checked = false;
  };
  to.addEventListener("change", fit);
  fit();
}

async function runMove(s: Session, dryRun: boolean) {
  const to = $<HTMLSelectElement>("move-to").value;
  const how = { dryRun, keep: $<HTMLInputElement>("move-keep").checked, fork: $<HTMLInputElement>("move-fork").checked,
    brief: $<HTMLInputElement>("move-brief").checked };
  try {
    const report = await move(s, to, how);
    $("move-report").innerHTML = reportText(report);
    if (!dryRun) {
      toast("Done: " + report.resume);
      await loadSessions();
      const moved = sessions.find((x) => x.id === (report.new_session || report.session) && x.profile === report.target);
      dialog("Moved", reportText(report), [
        { label: "Close", kind: "ghost", run: closeDialog },
        ...(moved ? [{ label: moved.agent === "dsh" ? "Open DeepSeek Harness" : "Resume it there", run: async () => {
          await resume(moved);
          if (moved.agent === "dsh") toast("Opening DSH; choose the session in its web UI.");
          closeDialog();
        } }] : []),
      ]);
    }
  } catch (e) {
    $("move-report").innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

// Plans and profiles.
async function loadAccounts() {
  try {
    accounts = (await usage()).accounts;
    learnHome(accounts.flatMap((a) => a.profiles));
    $("plans").classList.remove("loading");
    $("plans").innerHTML = accounts.map((a) => `<div class="account card">${accountHead(a)}${a.limits.map((l) => limitRow(l)).join("")}${
      a.extra ? `<div class="note">${esc(a.extra)}</div>` : ""}${a.why && a.source !== "live" ? `<div class="why">${esc(a.why)}</div>` : ""}</div>`).join("")
      || `<p class="empty">No profiles or homes yet.</p>`;
    $("profiles").classList.remove("loading");
    $("profiles").innerHTML = `<table class="items"><tr><th>Profile or home</th><th>Agent</th><th>Plan</th><th>Shares an account with</th><th></th></tr>${
      accounts.flatMap((a) => a.profiles.map((p) => `<tr><td>${esc(tilde(p))}</td><td>${agentName(a.agent, true)}</td>
        <td>${esc(a.plan)}</td><td>${esc(a.profiles.filter((x) => x !== p).map(tilde).join(", "))}</td>
        <td><button class="ghost" data-signin="${esc(profileName(p))}">Sign in…</button></td></tr>`)).join("")}</table>`;
  } catch (e) {
    $("plans").innerHTML = $("profiles").innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

async function loadInstances() {
  try {
    const list = (await instances()).instances;
    $("instance-list").classList.remove("loading");
    $("instance-list").innerHTML = list.length
      ? list.map((i) => `<div class="instance-row"><span class="name">${i.running ? `<span class="open-dot">●</span> ` : ""}${esc(i.display)}</span>
          <span class="note">${esc(i.app)}</span><button data-instance="${esc(i.name)}">${i.running ? "Bring forward" : "Open"}</button></div>`).join("")
      : `<p class="empty">No instances yet.</p>`;
  } catch (e) {
    $("instance-list").innerHTML = `<p class="note">${esc(errorText(e))}</p>`;
  }
}

function hueColor(hue: number) {
  return `hsl(${(Math.round(hue) + 20) % 360} 70% 50%)`; // Claude's orange, turned by that many degrees
}

// Settings.
function loadSettings() {
  const s = settings();
  $<HTMLInputElement>("set-live").checked = s.live;
  $<HTMLInputElement>("set-minutes").value = String(s.minutes);
  $<HTMLInputElement>("set-notify").checked = s.notify;
  $<HTMLInputElement>("set-low").value = String(s.low);
  $<HTMLInputElement>("set-unused").value = String(s.unused);
  $<HTMLSelectElement>("set-terminal").value = s.terminal;
}

function wire() {
  document.addEventListener("click", async (event) => {
    const el = (event.target as HTMLElement).closest("button") as HTMLButtonElement | null;
    if (!el) return;
    try {
      if (el.dataset.tab) showTab(el.dataset.tab);
      else if (el.dataset.session) {
        const s = sessions.find((x) => x.id === el.dataset.session && x.profile === el.dataset.profile);
        if (s) await select(s);
      } else if (el.id === "act-resume" && selected) {
        await resume(selected);
        toast(selected.agent === "dsh" ? "Opening DSH; choose the session in its web UI." : "Opening a terminal for " + selected.title);
      } else if (el.id === "act-copy" && selected) {
        const text = pasteCommand(selected);
        if (inApp) await writeText(text);
        else await navigator.clipboard?.writeText(text);
        toast("Copied: " + text);
      } else if (el.id === "act-move" && selected) await startMove(selected);
      else if (el.dataset.signin) {
        const dsh = el.dataset.signin === "dsh" || el.dataset.signin.startsWith("dsh:");
        await signIn(el.dataset.signin);
        toast(dsh ? "Opening DSH to sign in to " + el.dataset.signin : "Opening a terminal to sign in to " + el.dataset.signin);
      } else if (el.dataset.instance) await openInstance(el.dataset.instance);
      else if (el.id === "reload") await Promise.all([loadSessions(), loadAccounts()]);
      else if (el.id === "check-updates") await checkUpdates();
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  for (const id of ["search", "filter-agent", "filter-profile"]) $(id).addEventListener("input", renderSessions);
  for (const id of ["filter-hours", "filter-all"]) $(id).addEventListener("change", () => void loadSessions());
  $("add-profile").addEventListener("submit", async (event) => {
    event.preventDefault();
    const name = $<HTMLInputElement>("add-name").value.trim();
    const agent = $<HTMLSelectElement>("add-agent").value;
    const profile = agent === "claude" ? name : agent + ":" + name;
    try {
      await signIn(profile);
      toast(agent === "dsh" ? "Opening DSH to sign in to " + profile : "Opening a terminal to sign in to " + profile);
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  const hue = $<HTMLInputElement>("inst-hue");
  const paint = () => ($("hue-swatch").style.background = hueColor(Number(hue.value)));
  hue.addEventListener("input", paint);
  paint();
  $("add-instance").addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      await newInstance($<HTMLInputElement>("inst-name").value.trim(), $<HTMLInputElement>("inst-display").value.trim(), Number(hue.value));
      toast("Made it");
      await loadInstances();
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  const save = () => saveSettings({
    live: $<HTMLInputElement>("set-live").checked,
    minutes: Math.max(1, Number($<HTMLInputElement>("set-minutes").value) || 5),
    notify: $<HTMLInputElement>("set-notify").checked,
    low: Math.min(50, Math.max(1, Number($<HTMLInputElement>("set-low").value) || 10)),
    unused: Math.min(100, Math.max(0, Number($<HTMLInputElement>("set-unused").value) || 0)),
    terminal: $<HTMLSelectElement>("set-terminal").value,
  });
  for (const id of ["set-live", "set-minutes", "set-notify", "set-low", "set-unused", "set-terminal"]) $(id).addEventListener("change", save);
}

async function checkUpdates() {
  const note = $("update-note");
  if (!inApp) return (note.textContent = "Updates are checked in the app.");
  note.textContent = "Checking…";
  try {
    const update = await check();
    if (!update) return (note.textContent = "This is the newest version.");
    note.textContent = `Version ${update.version} is out; downloading…`;
    await update.downloadAndInstall();
    note.textContent = "Installed; restarting…";
    await relaunch();
  } catch (e) {
    note.textContent = "Couldn't check: " + errorText(e);
  }
}

async function start() {
  wire();
  loadSettings();
  const mac = navigator.userAgent.includes("Mac");
  $("desktop-tab").hidden = !mac;
  try {
    $("about-version").textContent = (await version()).replace(/^\S+ /, "");
  } catch {
    // the engine says why elsewhere
  }
  await Promise.all([loadSessions(), loadAccounts()]);
  if (inApp) {
    await listen<string | null>("shown", (event) => {
      if (event.payload) showTab(event.payload);
      void loadSessions();
    });
  }
}

start();
