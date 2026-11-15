const test = require("node:test");
const assert = require("node:assert/strict");
const { sessionItems } = require("./extension");

const sessions = [
  { agent: "claude", id: "abc", profile: "/home/me/.claude-work", title: "Fix login", cwd: "/home/me/api", resume: "cl work --resume abc" },
  { agent: "codex", id: "abc", profile: "/home/me/.codex", title: "", cwd: null, resume: "codex resume abc" },
];

test("sessionItems keeps agent data and makes unique IDs", () => {
  const items = sessionItems({ sessions });
  assert.equal(items.length, 2);
  assert.equal(items[0].label, "Fix login");
  assert.equal(items[0].uid, "claude:/home/me/.claude-work:abc");
  assert.equal(items[0].session.resume, "cl work --resume abc");
  assert.equal(items[1].label, "Untitled session");
  assert.notEqual(items[0].uid, items[1].uid);
});

test("sessionItems rejects an unknown answer instead of showing an empty list", () => {
  assert.throws(() => sessionItems({}), /no session list/);
});
