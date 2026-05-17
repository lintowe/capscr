#!/usr/bin/env node
// Regenerate platform icons from icon.png if needed, then run the requested
// frontend command. Lives in scripts/ so the .ico stays out of the repo —
// tauri.conf.json calls this from before{Dev,Build}Command.

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

const source = resolve(root, "icon.png");
const ico = resolve(root, "icons", "icon.ico");

function needsIconRegen() {
  if (!existsSync(ico)) return true;
  const sm = statSync(source).mtimeMs;
  const im = statSync(ico).mtimeMs;
  return sm > im;
}

if (!existsSync(source)) {
  console.error(`icon source not found: ${source}`);
  process.exit(1);
}

if (needsIconRegen()) {
  console.log("[capscr] regenerating icons from icon.png");
  mkdirSync(resolve(root, "icons"), { recursive: true });
  const r = spawnSync("cargo", ["tauri", "icon", source, "-o", resolve(root, "icons")], {
    cwd: root,
    stdio: "inherit",
    shell: true,
  });
  if (r.status !== 0) {
    console.error(`[capscr] icon generation failed (exit ${r.status})`);
    process.exit(r.status ?? 1);
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
