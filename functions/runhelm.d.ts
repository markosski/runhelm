export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export interface RunHelmFunctionContext<
  TInputs extends readonly unknown[] = readonly unknown[],
  TCredentials extends Record<string, string> = Record<string, string>
> {
  inputs: TInputs;
  credentials: TCredentials;
}

export type RunHelmFunction<
  TInputs extends readonly unknown[] = readonly unknown[],
  TCredentials extends Record<string, string> = Record<string, string>,
  TOutput = unknown
> = (ctx: RunHelmFunctionContext<TInputs, TCredentials>) => TOutput | Promise<TOutput>;
