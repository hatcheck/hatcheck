"use strict";
const { spawnSync } = require("child_process");
const path = require("path");

module.exports = function run(entry) {
  const name = path.basename(entry).replace(/\.(js|cmd|exe)$/i, "");
  const exe = process.platform === "win32" ? "hatcheck.exe" : "hatcheck";
  const target = `${process.platform}-${process.arch}`;
  const bin = path.join(__dirname, "vendor", target, exe);
  const args =
    name === "hatcheck"
      ? process.argv.slice(2)
      : ["--as", name, ...process.argv.slice(2)];
  const result = spawnSync(bin, args, { stdio: "inherit" });
  if (result.error) {
    console.error(
      `hatcheck: no prebuilt binary for ${target} (${result.error.message})`
    );
    process.exit(1);
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
  }
  process.exit(result.status === null ? 1 : result.status);
};
