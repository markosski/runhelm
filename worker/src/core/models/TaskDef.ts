export type TaskKind =
    | { Agent: { model_id: string; provider_url: string; prompt: string; tools?: string[]; ask?: boolean; schema_failure_retry_times?: number } }
    | { ApiCall: { url: string; method: string } }
    | { Function: { code: string; dependencies: FunctionDependency[] } };

export interface FunctionDependency {
    name: string;
    version: string;
}

export interface TaskDef {
    id: string;
    kind: TaskKind;
    input_schemas: any[];
    output_schema?: any;
    expected_side_effects: any[];
    required_credentials: string[];
}

export interface TaskExecutionPayload {
    workflow_def_id: string;
    task: TaskDef;
    inputs: any[];
    input_provided?: string;
}
