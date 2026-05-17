import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const distDir = join(root, "dist");

const functions = [
  {
    id: "example.example",
    source: "src/example.mjs",
    dependencies: [],
  }
];

await mkdir(distDir, { recursive: true });

for (const def of functions) {
  const code = await readFile(join(root, def.source), "utf8");
  const functionDef = {
    id: def.id,
    dependencies: def.dependencies,
    code: code.trimEnd(),
  };
  const dependencyYaml = def.dependencies.length === 0
    ? ["dependencies: []"]
    : ["dependencies:", ...dependencyLines(def.dependencies)];
  const yaml = [`id: ${def.id}`, ...dependencyYaml, "code: |", indent(code.trimEnd(), 2), ""].join("\n");
  await writeFile(join(distDir, `${def.id}.yaml`), yaml, "utf8");
  await writeFile(join(distDir, `${def.id}.json`), `${JSON.stringify(functionDef, null, 2)}\n`, "utf8");
}

function dependencyLines(dependencies) {
  return dependencies.flatMap((dependency) => [
    `  - name: ${dependency.name}`,
    `    version: ${dependency.version}`,
  ]);
}

function indent(value, spaces) {
  const prefix = " ".repeat(spaces);
  return value.split("\n").map((line) => `${prefix}${line}`).join("\n");
}
