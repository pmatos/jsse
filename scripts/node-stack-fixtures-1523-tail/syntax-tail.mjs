import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import vm from "node:vm";
import { pathToFileURL } from "node:url";
const lines = fs.readFileSync(process.argv[2], "utf8").split("\n").filter(Boolean);
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "drv-"));
fs.writeFileSync(path.join(dir, "lib.mjs"), "export const a = 1; export default 2;\n");
let n = 0;
for (const line of lines) {
  const tab = line.indexOf("\t");
  const goal = line.slice(0, tab);
  const src = JSON.parse(line.slice(tab + 1));
  try {
    if (goal === "module") {
      const file = path.join(dir, `m${n++}.mjs`);
      fs.writeFileSync(file, src + "\n");
      await import(pathToFileURL(file).href);
      console.log(goal, JSON.stringify(src), "loads");
    } else {
      new vm.Script(src);
      console.log(goal, JSON.stringify(src), "parses");
    }
  } catch (e) {
    console.log(goal, JSON.stringify(src), e.name + ": " + e.message.split(dir).join(""));
  }
}
fs.rmSync(dir, { recursive: true });
