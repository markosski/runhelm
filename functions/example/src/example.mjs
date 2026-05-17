export default async function run(ctx) {
  return {
    response: "Hello, world!",
    input: ctx.inputs?.[0] ?? null,
  };
}
