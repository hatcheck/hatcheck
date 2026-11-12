"use strict";
// On macOS and Linux, point bin/<name> straight at vendor/<platform>-<arch>/hatcheck so running
// a command starts the native binary without Node in between. On Windows, or when npm runs with
// --ignore-scripts, the Node wrappers in bin/ stay and run the same binary through run.js.
const fs = require("fs");
const path = require("path");

if (process.platform === "win32") {
  process.exit(0);
}

const target = `${process.platform}-${process.arch}`;
const rel = path.join("..", "vendor", target, "hatcheck");
const binDir = path.join(__dirname, "bin");
const binary = path.join(binDir, rel);

if (!fs.existsSync(binary)) {
  process.exit(0);
}

try {
  fs.chmodSync(binary, 0o755);
  for (const name of fs.readdirSync(binDir)) {
    const dest = path.join(binDir, name);
    fs.unlinkSync(dest);
    fs.symlinkSync(rel, dest);
  }
} catch {
  // Keep the Node wrappers if the filesystem refuses symlinks.
}
