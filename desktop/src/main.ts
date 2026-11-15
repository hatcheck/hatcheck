// The main window: browse every session with a read-only preview, resume or open, copy or move one
// (with a dry run first), see every plan, add profiles, and Claude Desktop instances.
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import {
  conversation, exportSession, inApp, insights, instances, library, move, newInstance, newSession, openInstance, pinWorkspace, recent,
  removeFilter, resume, saveFilter, saveSettings, settings, signIn, unpinWorkspace, updateSession, usage, version,
  workspaces, type Account, type InsightSeries, type InsightSpend, type MoveReport, type SavedFilter, type Session, type SessionMetadata, type Workspace,
} from "./engine";
import { accountHead, agentName, ago, errorText, esc, learnHome, limitRow, pasteCommand, profileName, tilde, toast } from "./format";
import { setLocalePreference, startLocalization } from "./i18n";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
let accounts: Account[] = [];
let sessions: Session[] = [];
let selected: Session | null = null;
let visibleSessions: Session[] = [];

// Tabs.
function showTab(name: string) {
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
    const active = tab.dataset.tab === name;
    tab.classList.toggle("active", active);
    if (active) tab.setAttribute("aria-current", "page");
    else tab.removeAttribute("aria-current");
  }
  for (const body of document.querySelectorAll<HTMLElement>(".tab-body")) body.hidden = body.id !== "tab-" + name;
  if (name === "plans" || name === "profiles") void loadAccounts();
  if (name === "plans") void loadInsights();
  if (name === "workspaces") void loadWorkspaces();
  if (name === "desktop") void loadInstances();
}

// Sessions and Hatcheck's own library metadata.
let sessionLibrary: Record<string, SessionMetadata> = {};
let savedFilters: Record<string, SavedFilter> = {};
const sessionKey = (s: Session) => `${s.agent}:${s.profile}:${s.id}`;
const metadata = (s: Session): SessionMetadata => sessionLibrary[sessionKey(s)] ||
  { alias: null, favorite: false, tags: [], archived: false };
const displayTitle = (s: Session) => metadata(s).alias || s.title;

async function loadLibrary() {
  const data = await library();
  sessionLibrary = data.sessions;
  savedFilters = data.filters;
  renderLibraryFilters();
}

function renderLibraryFilters() {
  const tags = [...new Set(Object.values(sessionLibrary).flatMap((m) => m.tags))].sort();
  const tag = $<HTMLSelectElement>("filter-tag");
  const keptTag = tag.value;
  tag.innerHTML = `<option value="">Every tag</option>` + tags.map((t) => `<option value="${esc(t)}">${esc(t)}</option>`).join("");
  tag.value = tags.includes(keptTag) ? keptTag : "";
  const saved = $<HTMLSelectElement>("saved-filter");
  const kept = saved.value;
  saved.innerHTML = `<option value="">Saved views…</option>` + Object.keys(savedFilters).sort().map((name) =>
    `<option value="${esc(name)}">${esc(name)}</option>`).join("");
  saved.value = Object.hasOwn(savedFilters, kept) ? kept : "";
}

async function loadSessions() {
  const list = $("session-list");
  try {
    const [data] = await Promise.all([
      recent(Number($<HTMLSelectElement>("filter-hours").value), $<HTMLInputElement>("filter-all").checked),
      loadLibrary(),
    ]);
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
  const model = $<HTMLInputElement>("filter-model").value.toLowerCase();
  const cost = $<HTMLSelectElement>("filter-cost").value;
  const tag = $<HTMLSelectElement>("filter-tag").value;
  const favorites = $<HTMLInputElement>("filter-favorites").checked;
  const archived = $<HTMLInputElement>("filter-archived").checked;
  const costMatches = (value: number | null) => !cost || value !== null &&
    (cost === "under-1" ? value < 1 : cost === "1-10" ? value >= 1 && value < 10 : value >= 10);
  const shown = sessions.filter((s) => {
    const meta = metadata(s);
    const searchable = `${s.title} ${meta.alias || ""} ${meta.tags.join(" ")} ${s.cwd || ""}`.toLowerCase();
    return (!agent || s.agent === agent) && (!profile || s.profile === profile) &&
      (!model || (s.model || "").toLowerCase().includes(model)) && costMatches(s.cost_usd) &&
      (!tag || meta.tags.includes(tag)) && (!favorites || meta.favorite) && (archived || !meta.archived) &&
      words.every((w) => searchable.includes(w));
  });
  visibleSessions = shown;
  $<HTMLButtonElement>("export-shown").textContent = `Export ${shown.length} shown…`;
  $<HTMLButtonElement>("export-shown").disabled = !shown.length;
  $("session-list").innerHTML = shown.length
    ? shown.map((s) => {
      const meta = metadata(s);
      return `<button class="row-session ${selected && selected.id === s.id && selected.profile === s.profile ? "selected" : ""}"
          data-session="${esc(s.id)}" data-profile="${esc(s.profile)}">
        <span class="title">${esc(displayTitle(s))}</span>
        <span class="meta"><span class="agent ${s.agent}">${agentName(s.agent)}</span>
          ${esc(profileName(s.profile))} · ${esc(tilde(s.cwd))} · ${ago(s.last_active)}${s.open ? ` · <span class="open-dot">● open</span>` : ""}${
            s.bound_to ? ` · <span class="warn">⚠ bound to ${esc(profileName(s.bound_to))}</span>` : ""}</span>
        ${meta.favorite || meta.archived || meta.tags.length ? `<span class="meta">${meta.favorite ? `<span class="tag live">Favorite</span> ` : ""}${
          meta.archived ? `<span class="tag">Archived</span> ` : ""}${meta.tags.map((t) => `<span class="tag">#${esc(t)}</span>`).join(" ")}</span>` : ""}
      </button>`;
    }).join("")
    : `<p class="empty">No sessions match.</p>`;
}

async function select(s: Session) {
  selected = s;
  renderSessions();
  const detail = $("session-detail");
  const dsh = s.agent === "dsh";
  const meta = metadata(s);
  detail.innerHTML = `<div class="detail-head">
      <h2>${esc(displayTitle(s))}</h2>
      ${meta.alias ? `<div class="meta">Agent title: ${esc(s.title)}</div>` : ""}
      <div class="meta">${agentName(s.agent, true)} · ${esc(tilde(s.profile))} · ${esc(tilde(s.cwd))}${
        s.branch ? " · " + esc(s.branch) : ""}${s.model ? " · " + esc(s.model) : ""} · ${ago(s.last_active)} ago</div>
      <div class="meta mono">${esc(s.id)}</div>
      ${meta.tags.length ? `<div class="meta">${meta.tags.map((t) => `<span class="tag">#${esc(t)}</span>`).join(" ")}</div>` : ""}
      <div class="actions">
        <button id="act-resume" ${s.open ? `disabled title="It's open in ${dsh ? "dsh" : "a terminal"}"` : ""}>${dsh ? "Open DeepSeek Harness" : "Resume in a terminal"}</button>
        <button id="act-copy" class="ghost">Copy command</button>
        <button id="act-export" class="ghost">Export…</button>
        <button id="act-move" class="ghost">Move to another profile…</button>
        <button id="act-favorite" class="ghost">${meta.favorite ? "Unfavorite" : "Favorite"}</button>
        <button id="act-metadata" class="ghost">Edit labels…</button>
        <button id="act-archive" class="ghost">${meta.archived ? "Unarchive" : "Archive"}</button>
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
let dialogPreviousFocus: HTMLElement | null = null;

function focusable(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>("button, input, select, textarea, a[href], [tabindex]")]
    .filter((el) => !(el as HTMLButtonElement).disabled && el.tabIndex >= 0 && el.getClientRects().length > 0);
}

function dialog(title: string, body: string, actions: { label: string; kind?: string; run: () => void | Promise<void> }[]) {
  if ($("dialog").hidden) dialogPreviousFocus = document.activeElement as HTMLElement | null;
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
  const first = focusable($("dialog"))[0] || $("dialog").querySelector<HTMLElement>(".dialog-box");
  first?.focus();
}
const closeDialog = () => {
  const wasOpen = !$("dialog").hidden;
  $("dialog").hidden = true;
  if (wasOpen && dialogPreviousFocus?.isConnected) dialogPreviousFocus.focus();
  dialogPreviousFocus = null;
};

async function changeMetadata(s: Session, change: Parameters<typeof updateSession>[1]) {
  const updated = await updateSession(s, change);
  const next = { alias: updated.alias, favorite: updated.favorite, tags: updated.tags, archived: updated.archived };
  if (!next.alias && !next.favorite && !next.archived && !next.tags.length) delete sessionLibrary[updated.key];
  else sessionLibrary[updated.key] = next;
  renderLibraryFilters();
  renderSessions();
  if (selected && sessionKey(selected) === sessionKey(s)) await select(s);
}

function editMetadata(s: Session) {
  const meta = metadata(s);
  dialog("Session labels",
    `<label>Personal alias <input id="metadata-alias" value="${esc(meta.alias || "")}" placeholder="Original agent title" /></label>
     <label>Tags <input id="metadata-tags" value="${esc(meta.tags.join(", "))}" placeholder="backend, urgent" /></label>
     <p class="note">These labels live only in Hatcheck's library.json. The agent's title and transcript are unchanged.</p>`,
    [
      { label: "Cancel", kind: "ghost", run: closeDialog },
      { label: "Save labels", run: async () => {
        const alias = $<HTMLInputElement>("metadata-alias").value.trim();
        const tags = [...new Set($<HTMLInputElement>("metadata-tags").value.split(",").map((t) => t.trim()).filter(Boolean))];
        await changeMetadata(s, {
          alias: alias || null,
          addTags: tags.filter((t) => !meta.tags.includes(t)),
          removeTags: meta.tags.filter((t) => !tags.includes(t)),
        });
        closeDialog();
        toast("Session labels saved");
      } },
    ]);
}

function currentFilter(): SavedFilter {
  return {
    query: $<HTMLInputElement>("search").value,
    agent: $<HTMLSelectElement>("filter-agent").value,
    profile: $<HTMLSelectElement>("filter-profile").value,
    hours: Number($<HTMLSelectElement>("filter-hours").value),
    model: $<HTMLInputElement>("filter-model").value,
    cost: $<HTMLSelectElement>("filter-cost").value,
    tag: $<HTMLSelectElement>("filter-tag").value,
    favorites: $<HTMLInputElement>("filter-favorites").checked,
    archived: $<HTMLInputElement>("filter-archived").checked,
  };
}

async function applyFilter(name: string) {
  if (!name) return;
  const filter = savedFilters[name];
  if (!filter) return;
  const reload = $<HTMLSelectElement>("filter-hours").value !== String(filter.hours);
  $<HTMLInputElement>("search").value = filter.query;
  $<HTMLSelectElement>("filter-agent").value = filter.agent;
  $<HTMLSelectElement>("filter-profile").value = filter.profile;
  $<HTMLSelectElement>("filter-hours").value = String(filter.hours);
  $<HTMLInputElement>("filter-model").value = filter.model;
  $<HTMLSelectElement>("filter-cost").value = filter.cost;
  $<HTMLSelectElement>("filter-tag").value = filter.tag;
  $<HTMLInputElement>("filter-favorites").checked = filter.favorites;
  $<HTMLInputElement>("filter-archived").checked = filter.archived;
  if (reload) await loadSessions();
  else renderSessions();
}

function saveCurrentFilter() {
  dialog("Save this session view",
    `<label>Name <input id="filter-name" required placeholder="Such as Backend favorites" /></label>
     <p class="note">Saves the search, agent, profile, date range, model, cost, tag, favorite and archive filters.</p>`,
    [
      { label: "Cancel", kind: "ghost", run: closeDialog },
      { label: "Save view", run: async () => {
        const name = $<HTMLInputElement>("filter-name").value.trim();
        if (!name) return toast("Give the view a name.", "error");
        await saveFilter(name, currentFilter());
        await loadLibrary();
        $<HTMLSelectElement>("saved-filter").value = name;
        closeDialog();
        toast(`Saved view "${name}"`);
      } },
    ]);
}

function removeCurrentFilter() {
  const name = $<HTMLSelectElement>("saved-filter").value;
  if (!name) return toast("Choose a saved view first.", "error");
  dialog(`Remove "${name}"?`, `<p>Only this saved view is removed. Sessions and their library labels stay as they are.</p>`, [
    { label: "Cancel", kind: "ghost", run: closeDialog },
    { label: "Remove view", run: async () => {
      await removeFilter(name);
      await loadLibrary();
      closeDialog();
      toast(`Removed view "${name}"`);
    } },
  ]);
}

function startExport(items: Session[]) {
  if (!items.length) return toast("No sessions match this export.", "error");
  dialog(`Export ${items.length} session${items.length === 1 ? "" : "s"}`,
    `<label>Format <select id="export-format"><option value="markdown">Markdown (.md)</option><option value="html">Self-contained HTML (.html)</option><option value="json">JSON (.json)</option></select></label>
     <label>Destination folder <input id="export-output" value="~/hatcheck-exports" required /></label>
     <label class="check"><input id="export-replace" type="checkbox" /> Replace existing files</label>
     <p class="warn">Sessions may contain secrets or private source code. Review every export before sharing it.</p>
     <p class="note">Each document includes its original agent, profile and session ID, its destination, and the same warning.</p>`,
    [
      { label: "Cancel", kind: "ghost", run: closeDialog },
      { label: "Export", run: async () => {
        const format = $<HTMLSelectElement>("export-format").value;
        const output = $<HTMLInputElement>("export-output").value.trim();
        if (!output) return toast("Choose a destination folder.", "error");
        const replace = $<HTMLInputElement>("export-replace").checked;
        const results = await Promise.allSettled(items.map((s) => exportSession(s, format, output, replace)));
        const paths: string[] = [], errors: string[] = [];
        for (const result of results) {
          if (result.status === "fulfilled") paths.push(...result.value.exported.map((e) => e.path));
          else errors.push(errorText(result.reason));
        }
        const shown = paths.slice(0, 20);
        dialog(paths.length ? "Export complete" : "Export failed",
          `${paths.length ? `<p>Exported ${paths.length} document${paths.length === 1 ? "" : "s"}:</p>
            <ul>${shown.map((p) => `<li class="mono">${esc(tilde(p))}</li>`).join("")}</ul>
            ${paths.length > shown.length ? `<p class="note">And ${paths.length - shown.length} more.</p>` : ""}` : ""}
           ${errors.length ? `<div class="error">${errors.map(esc).join("<br>")}</div>` : ""}
           <p class="warn">Review the documents for secrets and private source code before sharing them.</p>`,
          [{ label: "Close", kind: "ghost", run: closeDialog }]);
      } },
    ]);
}

type Destination = { value: string; label: string; agent: Account["agent"] };

function destinations(s: Session): Destination[] {
  const out: Destination[] = [];
  for (const a of accounts) {
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
  const targetAgent = r.target_agent ?? r.agent;
  const dsh = targetAgent === "dsh";
  const importer = r.action === "convert" && (r.target_agent === "copilot" || r.target_agent === "opencode");
  const sourceAgent = agentName((r.agent || "claude") as Account["agent"], true);
  return `<p>${verb} ${esc(what)}.</p>
    <div class="card handoff-summary">
      <div><strong>From</strong> ${esc(sourceAgent)} · <span class="mono">${esc(tilde(r.source))}</span><br><span class="note">Session ${esc(r.session)}</span></div>
      <div><strong>To</strong> ${targetAgent ? esc(agentName(targetAgent as Account["agent"], true)) : "the same agent"} · <span class="mono">${esc(tilde(r.target))}</span><br>
        <span class="note">${r.new_session ? `New session ${esc(r.new_session)}` : `Same session ${esc(r.session)}`}</span></div>
    </div>` +
    (r.action === "convert"
      ? `<p class="note"><strong>Preserved as text:</strong> ${r.prompts} of your messages and ${r.tool_calls} tool calls. <strong>Left out:</strong> ${esc(r.left_out)}.${
        r.brief ? ` Handed over as a brief: the first prompt, the files and commands its tool calls named, and the last turns${
          r.turns_dropped ? `, without the oldest ${r.turns_dropped}` : ""}.`
        : r.turns_dropped ? ` The oldest ${r.turns_dropped} turns are left out to fit.` : ""}</p>`
      : `<p class="note"><strong>Preserved:</strong> the whole ${esc(sourceAgent)} session and every item below.${
        r.repointed ? " Recorded project paths change to the destination; signed thinking isn't rewritten." : ""}</p>`) +
    (r.why ? `<p class="note">${esc(r.why)}</p>` : "") +
    (items ? `<table class="items"><tr><th>What</th><th>Size</th><th>There</th></tr>${items}</table>` : "") +
    (r.backup ? `<p class="note">${r.dry_run ? "Would save" : "Saved"} a local backup of ${r.backup.files} files (${(r.backup.bytes / 1024).toFixed(1)} KB) in <span class="mono">${esc(tilde(r.backup.path))}</span>. Shared indexes and credentials aren't copied.${r.backup.skipped ? ` ${r.backup.skipped} symlinks or special files were skipped.` : ""}</p>` : "") +
    (r.deleted_original ? `<p class="note">${r.dry_run ? "Would delete" : "Deleted"} the original from ${esc(tilde(r.source))}.</p>` : "") +
    (dsh && r.paste
      ? `<p class="note">Open DSH with <code>${esc(r.resume)}</code>, then paste into its web UI: <code>${esc(r.paste)}</code></p>`
      : dsh
        ? `<p class="note">Open DSH with <code>${esc(r.resume)}</code>, then choose the session in its web UI; the selected session isn't resumed directly.</p>`
        : importer
          ? `<p class="note">Import it with <code>${esc(r.resume)}</code> from the session's folder: ${r.target_agent === "copilot"
              ? "copilot prints the ID of the session it makes, for <code>copilot --resume</code>."
              : "then resume it with <code>opencode --session ses_&lt;id&gt;</code>, the ID the file is named by."}</p>`
          : `<p class="note">Resume it with <code>${esc(r.resume)}</code></p>`);
}

async function startMove(s: Session) {
  if (!accounts.length) await loadAccounts();
  const options = destinations(s);
  if (!options.length) return toast("There's no other profile or home to move it to.", "error");
  dialog(`Move "${displayTitle(s)}"`,
    `<label>To <select id="move-to">${options.map((o) => `<option value="${esc(o.value)}" data-agent="${esc(o.agent)}">${esc(o.label)}</option>`).join("")}</select></label>
     <label class="check"><input id="move-keep" type="checkbox" /> Keep the original, which copies it instead</label>
     <label class="check"><input id="move-backup" type="checkbox" /> Back up session-owned source files before deleting the original (same-agent moves)</label>
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
  const backup = $<HTMLInputElement>("move-backup");
  let kept = keep.checked;
  fork.addEventListener("change", () => {
    if (fork.checked) kept = keep.checked;
    keep.checked = fork.checked || kept;
    keep.disabled = fork.checked;
    fit();
  });
  // A brief is only for a conversion into another agent. Backup is only for a move that deletes
  // the original; a copy, fork or conversion already keeps it.
  const to = $<HTMLSelectElement>("move-to"), brief = $<HTMLInputElement>("move-brief");
  const fit = () => {
    brief.disabled = to.selectedOptions[0]?.dataset.agent === s.agent;
    if (brief.disabled) brief.checked = false;
    backup.disabled = to.selectedOptions[0]?.dataset.agent !== s.agent || keep.checked || fork.checked;
    if (backup.disabled) backup.checked = false;
  };
  to.addEventListener("change", fit);
  keep.addEventListener("change", fit);
  fit();
}

async function runMove(s: Session, dryRun: boolean) {
  const to = $<HTMLSelectElement>("move-to").value;
  const how = { dryRun, keep: $<HTMLInputElement>("move-keep").checked, fork: $<HTMLInputElement>("move-fork").checked,
    brief: $<HTMLInputElement>("move-brief").checked, backup: $<HTMLInputElement>("move-backup").checked };
  try {
    const report = await move(s, to, how);
    $("move-report").innerHTML = reportText(report);
    if (!dryRun) {
      toast("Done: " + report.resume);
      await loadSessions();
      const moved = sessions.find((x) => x.id === (report.new_session || report.session) && x.profile === report.target);
      const undo = moved && report.deleted_original && !how.keep && !how.fork &&
        (report.action === "move" || report.action === "update") &&
        (report.target_agent ?? report.agent) === moved.agent && moved.agent === s.agent ? moved : null;
      dialog("Moved", reportText(report), [
        { label: "Close", kind: "ghost", run: closeDialog },
        ...(undo ? [{ label: "Undo move", kind: "ghost", run: async () => {
          const undone = await move(undo, report.source, { dryRun: false });
          await loadSessions();
          toast("Move undone; any backup is kept");
          dialog("Move undone", reportText(undone), [{ label: "Close", kind: "ghost", run: closeDialog }]);
        } }] : []),
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

// Workspaces: pinned projects joined with the sessions and accounts the engine already listed.
let workspaceRows: Workspace[] = [];

async function loadWorkspaces() {
  const list = $("workspace-list");
  try {
    const [data] = await Promise.all([
      workspaces(),
      sessions.length ? Promise.resolve() : loadSessions(),
      accounts.length ? Promise.resolve() : loadAccounts(),
    ]);
    workspaceRows = data.workspaces;
    list.classList.remove("loading");
    renderWorkspaces();
  } catch (e) {
    list.innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
  }
}

function renderWorkspaces() {
  $("workspace-list").innerHTML = workspaceRows.length ? workspaceRows.map((w) => {
    const recent = sessions.filter((s) => s.cwd === w.folder).slice(0, 5);
    const bindings = Object.entries(w.bindings);
    const usageAccounts = [...new Set(bindings.map(([, profile]) =>
      accounts.find((a) => a.profiles.includes(profile))).filter((a): a is Account => Boolean(a)))];
    return `<article class="card workspace-card">
      <h2>${esc(w.name)}</h2>
      <div class="meta folder">${esc(tilde(w.folder))}${w.exists ? "" : ` · <span class="warn">folder isn't there</span>`}</div>
      <div class="bindings">${bindings.length ? bindings.map(([agent, profile]) =>
        `<span class="tag">${agentName(agent as Account["agent"], true)} · ${esc(tilde(profile))}</span>`).join("<br>") :
        `<span class="note">No folder bindings.</span>`}</div>
      ${usageAccounts.map((a) => `<div class="note">${esc(a.plan)} · ${a.limits.filter((l) => l.left_percent !== null)
        .map((l) => `${esc(l.short)} ${Math.round(l.left_percent!)}% left`).join(", ") || "no limit data"}</div>`).join("")}
      <div class="workspace-sessions">${recent.length ? recent.map((s) =>
        `<button class="workspace-session" data-session="${esc(s.id)}" data-profile="${esc(s.profile)}">
          <span class="title">${esc(displayTitle(s))}</span><span class="meta">${agentName(s.agent)} · ${esc(profileName(s.profile))} · ${ago(s.last_active)}</span>
        </button>`).join("") : `<p class="note">No sessions in the selected time range.</p>`}</div>
      <div class="workspace-actions">
        <button data-workspace-new="${esc(w.folder)}" ${w.exists ? "" : "disabled"}>New session…</button>
        <button class="ghost" data-workspace-remove="${esc(w.folder)}">Unpin</button>
      </div>
    </article>`;
  }).join("") : `<p class="empty">No pinned workspaces. Add a project folder above.</p>`;
}

async function startWorkspaceSession(workspace: Workspace) {
  if (!accounts.length) await loadAccounts();
  const options = accounts.flatMap((a) => a.profiles.map((profile) => ({
    value: profileName(profile),
    label: `${agentName(a.agent, true)} · ${tilde(profile)}${workspace.bindings[a.agent] === profile ? " · folder binding" : ""}`,
    bound: workspace.bindings[a.agent] === profile,
  }))).sort((a, b) => Number(b.bound) - Number(a.bound) || a.label.localeCompare(b.label));
  if (!options.length) return toast("Add and sign in to a profile first.", "error");
  dialog(`New session in ${workspace.name}`,
    `<label>Agent and profile <select id="new-session-profile">${options.map((o) =>
      `<option value="${esc(o.value)}">${esc(o.label)}</option>`).join("")}</select></label>
     <p class="note">Starts in ${esc(tilde(workspace.folder))}. Hatcheck refuses a profile that conflicts with the folder's binding; it never switches accounts by itself.</p>
     <p class="note">For DeepSeek Harness, DSH's web UI opens; start the new conversation there.</p>`,
    [
      { label: "Cancel", kind: "ghost", run: closeDialog },
      { label: "Start new session", run: async () => {
        const profile = $<HTMLSelectElement>("new-session-profile").value;
        await newSession(profile, workspace.folder);
        toast("Opening a terminal for the new session");
        closeDialog();
      } },
    ]);
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

function sparkline(series: InsightSeries): string {
  const width = 260, height = 56;
  const points = series.readings.map((r, i) => {
    const x = series.readings.length === 1 ? width / 2 : i * width / (series.readings.length - 1);
    const y = height - Math.max(0, Math.min(100, r.used_percent)) * height / 100;
    return { x, y };
  });
  const name = `${profileName(series.account.profile)} · ${series.account.label}`;
  const line = points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
  return `<div class="card insight-chart"><div><strong>${esc(name)}</strong><span class="note"> ${series.readings.length} readings</span></div>
    <svg viewBox="0 0 ${width} ${height}" role="img" aria-label="Usage over time for ${esc(name)}">
      <line x1="0" y1="${height}" x2="${width}" y2="${height}" class="axis" />
      ${points.length > 1 ? `<polyline points="${line}" class="usage-line" />` : ""}
      ${points.map((p) => `<circle cx="${p.x}" cy="${p.y}" r="2.5" class="usage-point" />`).join("")}
    </svg></div>`;
}

function spendList(title: string, key: "profile" | "project" | "model", rows: InsightSpend[]): string {
  const top = rows.slice(0, 5);
  const max = Math.max(...top.map((r) => r.cost_usd), 0);
  return `<div class="card spend-list"><h2>${esc(title)}</h2>${top.map((row) => {
    const label = key === "model" ? row.model! : key === "project" ? row.project! : row.profile!;
    const width = max > 0 ? row.cost_usd / max * 100 : 0;
    return `<div class="spend-row"><span class="spend-name" title="${esc(label)}">${esc(key === "model" ? label : tilde(label))}</span>
      <span class="spend-bar"><i style="width:${width}%"></i></span><span>$${row.cost_usd.toFixed(2)}</span><span class="note">${row.session_count}</span></div>`;
  }).join("")}</div>`;
}

async function loadInsights() {
  const target = $("insights");
  try {
    const data = await insights();
    target.innerHTML = `<h2>Local insights · ${data.days} days</h2>
      <div class="insight-summary card"><strong>${data.weekly.session_count} sessions</strong> · ${data.weekly.costed_session_count} with recorded costs · $${data.weekly.cost_usd.toFixed(2)}</div>
      ${data.forecasts.length ? `<div class="forecast-list">${data.forecasts.map((f) => `<div class="card forecast ${f.kind}">
        <strong>${f.kind === "runs_out" ? "Likely to run out" : "Likely to go unused"}</strong> · ${esc(profileName(f.account.profile))} · ${esc(f.account.label)}
        <div class="note">${f.kind === "runs_out" ? `Runs out ${f.runs_out_at ? new Date(f.runs_out_at).toLocaleString() : "time unknown"}` :
          `${Math.round(f.left_percent || 0)}% left · resets ${f.resets_at ? new Date(f.resets_at).toLocaleString() : "time unknown"}`}</div></div>`).join("")}</div>` : ""}
      ${data.series.length ? `<div class="insight-grid">${data.series.map(sparkline).join("")}</div>` :
        `<p class="note">No plan readings in this period. Hatcheck records them as it reads plans; no reading isn't zero usage.</p>`}
      <div class="spend-grid">${spendList("Profiles", "profile", data.profiles)}${spendList("Projects", "project", data.projects)}${spendList("Models", "model", data.models)}</div>`;
  } catch (e) {
    target.innerHTML = `<p class="error">${esc(errorText(e))}</p>`;
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

// The command palette: every action stays explicit; nothing resumes or switches by itself.
type PaletteAction = { label: string; hint: string; run: () => void | Promise<void> };
let paletteActions: PaletteAction[] = [];
let paletteIndex = 0;
let palettePreviousFocus: HTMLElement | null = null;

function paletteCommands(): PaletteAction[] {
  const actions: PaletteAction[] = [
    { label: "Show usage and local insights", hint: "Plans", run: () => showTab("plans") },
    { label: "Browse sessions", hint: "Sessions", run: () => showTab("sessions") },
    { label: "Browse workspaces", hint: "Workspaces", run: () => showTab("workspaces") },
    { label: "Browse profiles", hint: "Profiles", run: () => showTab("profiles") },
    { label: "Settings", hint: "Usage, notifications and terminal", run: () => showTab("settings") },
    { label: "Refresh sessions and plans", hint: "Reload local data", run: async () => {
      await Promise.all([loadSessions(), loadAccounts(), loadInsights()]);
    } },
  ];
  for (const s of sessions) {
    const title = displayTitle(s);
    const hint = `${agentName(s.agent, true)} · ${profileName(s.profile)} · ${tilde(s.cwd)}`;
    actions.push({ label: `Resume “${title}”`, hint, run: async () => {
      showTab("sessions");
      await select(s);
      await resume(s);
    } });
    actions.push({ label: `Move “${title}” to another profile`, hint, run: () => startMove(s) });
    actions.push({ label: `Export “${title}”`, hint, run: () => startExport([s]) });
  }
  const profiles = new Set(accounts.flatMap((a) => a.profiles));
  for (const profile of profiles) {
    const account = accounts.find((a) => a.profiles.includes(profile));
    if (!account) continue;
    actions.push({ label: `Open or sign in to ${profileName(profile)}`, hint: `${agentName(account.agent, true)} · ${account.plan}`,
      run: async () => { await signIn(profileName(profile)); } });
  }
  for (const workspace of workspaceRows) {
    if (workspace.exists) actions.push({ label: `New session in ${workspace.name}`, hint: tilde(workspace.folder),
      run: () => startWorkspaceSession(workspace) });
  }
  return actions;
}

function filteredPaletteActions(): PaletteAction[] {
  const words = $<HTMLInputElement>("palette-input").value.toLowerCase().split(/\s+/).filter(Boolean);
  return paletteCommands().filter((action) => {
    const text = `${action.label} ${action.hint}`.toLowerCase();
    return words.every((word) => text.includes(word));
  }).slice(0, 30);
}

function setPaletteIndex(index: number) {
  paletteIndex = Math.max(0, Math.min(index, paletteActions.length - 1));
  for (const option of document.querySelectorAll<HTMLElement>(".palette-option")) {
    const active = Number(option.dataset.paletteIndex) === paletteIndex;
    option.classList.toggle("active", active);
    option.setAttribute("aria-selected", String(active));
    if (active) {
      $<HTMLInputElement>("palette-input").setAttribute("aria-activedescendant", option.id);
      option.scrollIntoView({ block: "nearest" });
    }
  }
}

function renderPalette() {
  paletteActions = filteredPaletteActions();
  paletteIndex = Math.min(paletteIndex, Math.max(0, paletteActions.length - 1));
  $("palette-results").innerHTML = paletteActions.length ? paletteActions.map((action, index) =>
    `<button id="palette-option-${index}" class="palette-option ${index === paletteIndex ? "active" : ""}" role="option"
      aria-selected="${index === paletteIndex}" data-palette-index="${index}"><span>${esc(action.label)}</span><span class="hint">${esc(action.hint)}</span></button>`).join("")
    : `<p class="empty">No matching sessions or commands.</p>`;
  setPaletteIndex(paletteIndex);
}

function openPalette() {
  if (!$("dialog").hidden) closeDialog();
  palettePreviousFocus = document.activeElement as HTMLElement | null;
  $("palette").hidden = false;
  $<HTMLInputElement>("palette-input").value = "";
  paletteIndex = 0;
  renderPalette();
  $<HTMLInputElement>("palette-input").focus();
}

function closePalette() {
  const wasOpen = !$("palette").hidden;
  $("palette").hidden = true;
  if (wasOpen && palettePreviousFocus?.isConnected) palettePreviousFocus.focus();
  palettePreviousFocus = null;
}

async function runPaletteAction(index: number) {
  const action = paletteActions[index];
  if (!action) return;
  closePalette();
  await action.run();
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

function labelControls() {
  const labels: Record<string, string> = {
    search: "Search titles, aliases, tags and folders", "filter-agent": "Filter by agent", "filter-profile": "Filter by profile",
    "filter-hours": "Filter by time range", "filter-all": "Include Desktop, SDK and other hosts", "filter-model": "Filter by model",
    "filter-cost": "Filter by cost", "filter-tag": "Filter by tag", "filter-favorites": "Show favorites only",
    "filter-archived": "Include archived sessions", "saved-filter": "Apply a saved view", "workspace-folder": "Project folder to pin",
    "workspace-name": "Workspace display name", "add-agent": "Agent for the new profile", "add-name": "New profile name",
    "inst-name": "Desktop instance name", "inst-display": "Desktop instance display name", "inst-hue": "Desktop icon hue",
    "set-live": "Ask for live usage", "set-minutes": "Refresh interval in minutes", "set-notify": "Enable usage notifications",
    "set-low": "Low quota notification threshold", "set-unused": "Unused quota notification threshold", "set-terminal": "Terminal for resume",
    "session-list": "Sessions", "session-detail": "Conversation preview", plans: "Plan limits", profiles: "Profiles and homes",
    "workspace-list": "Pinned workspaces", "instance-list": "Claude Desktop instances",
  };
  for (const [id, label] of Object.entries(labels)) {
    $(id).setAttribute("aria-label", label);
  }
  for (const button of document.querySelectorAll<HTMLButtonElement>("button[title]")) {
    if (!button.hasAttribute("aria-label")) button.setAttribute("aria-label", button.title);
  }
}

function handleKeyboard(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    if ($("palette").hidden) openPalette();
    else closePalette();
    return;
  }
  if (!$("palette").hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      closePalette();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setPaletteIndex(paletteIndex + (event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "Enter" && event.target === $("palette-input")) {
      event.preventDefault();
      void runPaletteAction(paletteIndex);
    }
    return;
  }
  if ($("dialog").hidden) return;
  if (event.key === "Escape") {
    event.preventDefault();
    closeDialog();
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusable($("dialog"));
  if (!items.length) return;
  const first = items[0], last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

function wire() {
  document.addEventListener("click", async (event) => {
    const target = event.target as HTMLElement;
    if (target.id === "dialog") closeDialog();
    if (target.id === "palette") closePalette();
    const el = target.closest("button") as HTMLButtonElement | null;
    if (!el) return;
    try {
      if (el.id === "command-palette") openPalette();
      else if (el.dataset.paletteIndex) await runPaletteAction(Number(el.dataset.paletteIndex));
      else if (el.dataset.tab) showTab(el.dataset.tab);
      else if (el.dataset.session) {
        const s = sessions.find((x) => x.id === el.dataset.session && x.profile === el.dataset.profile);
        if (s) {
          showTab("sessions");
          await select(s);
        }
      } else if (el.dataset.workspaceNew) {
        const workspace = workspaceRows.find((w) => w.folder === el.dataset.workspaceNew);
        if (workspace) await startWorkspaceSession(workspace);
      } else if (el.dataset.workspaceRemove) {
        await unpinWorkspace(el.dataset.workspaceRemove);
        toast("Workspace unpinned; its folder wasn't changed");
        await loadWorkspaces();
      } else if (el.id === "act-resume" && selected) {
        await resume(selected);
        toast(selected.agent === "dsh" ? "Opening DSH; choose the session in its web UI." : "Opening a terminal for " + selected.title);
      } else if (el.id === "act-copy" && selected) {
        const text = pasteCommand(selected);
        if (inApp) await writeText(text);
        else await navigator.clipboard?.writeText(text);
        toast("Copied: " + text);
      } else if (el.id === "act-export" && selected) startExport([selected]);
      else if (el.id === "export-shown") startExport(visibleSessions);
      else if (el.id === "act-move" && selected) await startMove(selected);
      else if (el.id === "act-favorite" && selected) {
        await changeMetadata(selected, { favorite: !metadata(selected).favorite });
      } else if (el.id === "act-metadata" && selected) editMetadata(selected);
      else if (el.id === "act-archive" && selected) {
        await changeMetadata(selected, { archived: !metadata(selected).archived });
        toast(metadata(selected).archived ? "Session archived; its transcript is unchanged" : "Session unarchived");
      } else if (el.id === "save-filter") saveCurrentFilter();
      else if (el.id === "remove-filter") removeCurrentFilter();
      else if (el.dataset.signin) {
        const dsh = el.dataset.signin === "dsh" || el.dataset.signin.startsWith("dsh:");
        await signIn(el.dataset.signin);
        toast(dsh ? "Opening DSH to sign in to " + el.dataset.signin : "Opening a terminal to sign in to " + el.dataset.signin);
      } else if (el.dataset.instance) await openInstance(el.dataset.instance);
      else if (el.id === "reload") await Promise.all([loadSessions(), loadAccounts(), loadInsights()]);
      else if (el.id === "check-updates") await checkUpdates();
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  document.addEventListener("keydown", handleKeyboard);
  $("palette-input").addEventListener("input", () => {
    paletteIndex = 0;
    renderPalette();
  });
  for (const id of ["search", "filter-agent", "filter-profile", "filter-model", "filter-cost", "filter-tag",
    "filter-favorites", "filter-archived"]) $(id).addEventListener("input", renderSessions);
  for (const id of ["filter-hours", "filter-all"]) $(id).addEventListener("change", () => void loadSessions());
  $("saved-filter").addEventListener("change", (event) => void applyFilter((event.target as HTMLSelectElement).value));
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
  $("add-workspace").addEventListener("submit", async (event) => {
    event.preventDefault();
    const folder = $<HTMLInputElement>("workspace-folder").value.trim();
    const name = $<HTMLInputElement>("workspace-name").value.trim();
    try {
      await pinWorkspace(folder, name);
      $<HTMLInputElement>("workspace-folder").value = "";
      $<HTMLInputElement>("workspace-name").value = "";
      toast("Workspace pinned");
      await loadWorkspaces();
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
  $("set-language").addEventListener("change", (event) => {
    const value = (event.target as HTMLSelectElement).value as "system" | "en" | "zh-CN";
    setLocalePreference(value);
  });
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
  labelControls();
  startLocalization();
  const mac = navigator.userAgent.includes("Mac");
  const windows = navigator.userAgent.includes("Windows");
  $("desktop-tab").hidden = !mac && !windows;
  if (windows) {
    $("instance-note").innerHTML =
      "Makes <code>Claude NAME</code> in the Start menu, a shortcut with an icon of that colour, and " +
      "<code>~\\.claude-instances\\NAME</code> for its data.";
  }
  try {
    $("about-version").textContent = (await version()).replace(/^\S+ /, "");
  } catch {
    // the engine says why elsewhere
  }
  await Promise.all([loadSessions(), loadAccounts(), loadInsights()]);
  if (inApp) {
    await listen<string | null>("shown", (event) => {
      if (event.payload) showTab(event.payload);
      void loadSessions();
    });
  }
}

start();
