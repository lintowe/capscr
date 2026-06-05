#!/usr/bin/env node
// Regenerate platform icons + installer artwork from icon-master.png if it
// exists, otherwise fall back to icons/icon.png. Runs before dev/build.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

const mode = process.argv[2];
if (!mode || (mode !== "--dev" && mode !== "--build")) {
  console.error("usage: build-hook.mjs --dev|--build");
  process.exit(1);
}

const master = resolve(root, "icons", "icon-master.png");
const iconPng = resolve(root, "icons", "icon.png");
const ico = resolve(root, "icons", "icon.ico");


// source = master if present (preferred — kept high-res for sharp downscale),
// otherwise fall back to icons/icon.png (which `cargo tauri icon` overwrites
// at 512px, so it loses fidelity over time).
const source = existsSync(master) ? master : iconPng;

if (!existsSync(source)) {
  console.error(`icon source not found: ${source}`);
  process.exit(1);
}

const sourceMtime = statSync(source).mtimeMs;
const needs = (target) => !existsSync(target) || sourceMtime > statSync(target).mtimeMs;

if (needs(ico)) {
  console.log(`[capscr] regenerating platform icons from ${source}`);
  mkdirSync(resolve(root, "icons"), { recursive: true });
  // prefer the npm-global `tauri` binary (what tauri-action installs in CI);
  // fall back to `cargo tauri` which is what local `cargo install tauri-cli`
  // provides
  const candidates = [
    ["tauri", ["icon", source, "-o", resolve(root, "icons")]],
    ["cargo", ["tauri", "icon", source, "-o", resolve(root, "icons")]],
  ];
  let lastSuccess = false;
  let lastStatus = 1;
  for (const [bin, args] of candidates) {
    const r = spawnSync(bin, args, {
      cwd: root,
      stdio: "inherit",
      shell: true,
    });
    lastStatus = r.status ?? 1;
    if (r.status === 0) {
      lastSuccess = true;
      break;
    }
    // any non-zero exit (incl. cmd's 9009 / "not recognized" / 101 for no-such-cargo-subcommand)
    // → fall through to the next candidate; we only stop on success.
    if (r.error?.code === "ENOENT") continue;
  }
  if (!lastSuccess) {
    console.error(`[capscr] icon generation failed (exit ${lastStatus})`);
    process.exit(lastStatus);
  }
} else {
  console.log("[capscr] icons up-to-date, skipping regen");
}



const frontendCmd = mode === "--dev" ? "dev" : "build";
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const r = spawnSync(npm, ["run", frontendCmd], {
  cwd: resolve(root, "frontend"),
  stdio: "inherit",
  shell: true,
});
process.exit(r.status ?? 1);
