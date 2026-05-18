import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { buildFunctionArtifacts } from "../../build-function-artifacts.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

const functions = [
  {
    id: "example.example",
    source: "src/example.ts",
    dependencies: [],
  }
];

await buildFunctionArtifacts({ root, functions });
