const { execFile } = require("child_process");

function binary() {
  return process.env.HATCHECK_BIN ||
    require("vscode").workspace.getConfiguration("hatcheck").get("path", "hatcheck");
}

function run(args) {
  return new Promise((resolve, reject) => {
    execFile(binary(), args, { timeout: 30_000, maxBuffer: 32 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) reject(new Error(stderr.trim() || error.message));
      else resolve(stdout);
    });
  });
}

async function runJson(args) {
  return JSON.parse(await run(args));
}

function sessionItems(report) {
  if (!report || !Array.isArray(report.sessions)) throw new Error("Hatcheck returned no session list");
  return report.sessions.map((session) => ({
    label: session.title || "Untitled session",
    description: `${session.agent} · ${session.profile}`,
    detail: `${session.cwd || "?"} — ${session.resume}`,
    uid: `${session.agent}:${session.profile}:${session.id}`,
    session,
  }));
}

async function showSessions() {
  const vscode = require("vscode");
  const report = await runJson(["--json", "--cached", "--no-usage", "--all-folders", "168"]);
  const picked = await vscode.window.showQuickPick(sessionItems(report), {
    placeHolder: "Choose a session to resume, or copy its command",
    matchOnDescription: true,
    matchOnDetail: true,
  });
  if (!picked) return;
  const action = await vscode.window.showQuickPick(["Resume in terminal", "Copy resume command", "Copy raw session JSON"], {
    placeHolder: picked.label,
  });
  const session = picked.session;
  if (action === "Resume in terminal") {
    const terminal = vscode.window.createTerminal({
      name: `Hatcheck: ${picked.label}`,
      ...(session.cwd ? { cwd: vscode.Uri.file(session.cwd) } : {}),
    });
    terminal.show();
    terminal.sendText(session.resume, true);
  } else if (action === "Copy resume command") {
    await vscode.env.clipboard.writeText(session.resume);
  } else if (action === "Copy raw session JSON") {
    await vscode.env.clipboard.writeText(JSON.stringify(session, null, 2));
  }
}

async function showDocument(content, language) {
  const vscode = require("vscode");
  const document = await vscode.workspace.openTextDocument({ content, language });
  await vscode.window.showTextDocument(document, { preview: false });
}

async function showUsage() {
  await showDocument(await run(["usage", "--cached"]), "text");
}

async function showInsights() {
  const report = await runJson(["insights", "--json"]);
  await showDocument(JSON.stringify(report, null, 2), "json");
}

function activate(context) {
  const vscode = require("vscode");
  const guard = (fn) => async () => {
    try {
      await fn();
    } catch (error) {
      await vscode.window.showErrorMessage(`Hatcheck: ${error.message}`);
    }
  };
  context.subscriptions.push(
    vscode.commands.registerCommand("hatcheck.sessions", guard(showSessions)),
    vscode.commands.registerCommand("hatcheck.usage", guard(showUsage)),
    vscode.commands.registerCommand("hatcheck.insights", guard(showInsights)),
  );
}

function deactivate() {}

module.exports = { activate, deactivate, sessionItems };
