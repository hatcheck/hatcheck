// Build the engine the app ships: the hatcheck binary from the Rust crate (crates/hatcheck),
// at src-tauri/binaries/hatcheck-<target triple>, where Tauri looks for its sidecars.
//
//     node scripts/build-engine.mjs [--target TRIPLE]
//
// Without --target it's built for this machine, as `npm run tauri build` builds the app.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const desktop = dirname(dirname(fileURLToPath(import.meta.url)));
const repo = dirname(desktop);
const args = process.argv.slice(2);
const at = args.indexOf("--target");
const target = at >= 0 ? args[at + 1] : undefined;
if (at >= 0 && !target) {
  console.error("usage: node scripts/build-engine.mjs [--target TRIPLE]");
  process.exit(2);
}
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  .slice("host:".length)
  .trim();
const triple = target ?? host;
const exe = triple.includes("windows") ? ".exe" : "";

const cargo = ["build", "--release", "--locked", "-p", "hatcheck", "--bin", "hatcheck"];
if (target) cargo.push("--target", target);
execFileSync("cargo", cargo, { cwd: repo, stdio: "inherit" });

const built = join(repo, "target", ...(target ? [target] : []), "release", "hatcheck" + exe);
const folder = join(desktop, "src-tauri", "binaries");
mkdirSync(folder, { recursive: true });
const sidecar = join(folder, `hatcheck-${triple}${exe}`);
copyFileSync(built, sidecar);
console.log("Built", relative(desktop, sidecar));
