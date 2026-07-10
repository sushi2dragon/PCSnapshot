import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const out = resolve(root, "dist", "chromium");
const manifest = JSON.parse(await readFile(resolve(root, "manifest.chromium.json"), "utf8"));

if (!manifest.key || !manifest.permissions?.includes("nativeMessaging")) {
  throw new Error("Chromium manifest must have a stable key and nativeMessaging permission");
}

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
await cp(resolve(root, "src"), resolve(out, "src"), { recursive: true });
await writeFile(resolve(out, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(out);
