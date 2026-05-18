import { readFile } from "node:fs/promises";

export async function loadFunctionArtifact(artifactPath) {
  const artifactUrl = new URL(artifactPath, import.meta.url);
  const artifact = JSON.parse(await readFile(artifactUrl, "utf8"));
  const codeUrl = `data:text/javascript;base64,${Buffer.from(artifact.code, "utf8").toString("base64")}`;
  return (await import(codeUrl)).default;
}
