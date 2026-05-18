export type TaskKind =
    | { Agent: { model_id: string; provider_url: string; prompt: string; tools: string[]; skills: string[]; ask?: boolean; schema_failure_retry_times?: number } }
    | { ApiCall: { url: string; method: string } }
    | { Function: InlineFunctionTask | ReferencedFunctionTask };

export interface InlineFunctionTask {
    code: string;
    dependencies: FunctionDependency[];
}

export interface ReferencedFunctionTask {
    ref: string;
}

export interface FunctionDependency {
    name: string;
    version: string;
}

export interface TaskDef {
    id: string;
    kind: TaskKind;
    timeout_secs?: number;
    input_schemas: any[];
    output_schema?: any;
    expected_side_effects: any[];
    required_credentials: string[];
}

export interface TaskExecutionPayload {
    task: TaskDef;
    inputs: any[];
    input_provided?: string;
}
