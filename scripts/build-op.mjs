// Builds `op` for the target Tauri is bundling and stages it as the externalBin sidecar.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const triple = process.env.TAURI_ENV_TARGET_TRIPLE;
if (!triple) {
  console.error("TAURI_ENV_TARGET_TRIPLE is not set; run this through `npm run bundle`.");
  process.exit(1);
}

const root = (p) => fileURLToPath(new URL(`../${p}`, import.meta.url));

// -p onepass-cli only: --workspace would unify onepass-core features and pull crypto into op.
const build = spawnSync("cargo", ["build", "--release", "-p", "onepass-cli", "--target", triple], {
  cwd: root(""),
  stdio: "inherit",
});
if (build.status !== 0) process.exit(build.status ?? 1);

mkdirSync(root("src-tauri/binaries"), { recursive: true });
copyFileSync(root(`target/${triple}/release/op`), root(`src-tauri/binaries/op-${triple}`));
