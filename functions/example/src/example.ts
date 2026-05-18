import type { RunHelmFunction } from "../../runhelm";

const run: RunHelmFunction = async (ctx) => {
  return {
    response: "Hello, world!",
    input: ctx.inputs?.[0] ?? null,
  };
};

export default run;
