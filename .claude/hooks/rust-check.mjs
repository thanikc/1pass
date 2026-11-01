// PostToolUse: format and type-check Rust after edits under src-tauri/.
import { spawnSync } from "node:child_process";

let input = "";
for await (const chunk of process.stdin) input += chunk;
const path = (JSON.parse(input).tool_input?.file_path ?? "").replaceAll("\\", "/");
if (!path.endsWith(".rs") || !path.includes("/src-tauri/")) process.exit(0);

const manifest = path.slice(0, path.indexOf("/src-tauri/")) + "/src-tauri/Cargo.toml";
const run = (args) => spawnSync("cargo", args, { encoding: "utf8" });

if (run(["--version"]).error) process.exit(0); // cargo not installed; skip quietly

run(["fmt", "--manifest-path", manifest]);
const check = run(["check", "--manifest-path", manifest, "--message-format=short", "--quiet"]);
if (check.status !== 0) {
  console.error(`cargo check failed:\n${check.stderr || check.stdout}`);
  process.exit(2);
}
