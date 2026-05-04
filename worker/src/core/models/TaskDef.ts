export type TaskKind =
    | { Agent: { model_id: string; provider_url: string; prompt: string; tools?: string[]; ask?: boolean; schema_failure_retry_times?: number } }
    | { ApiCall: { endpoint: string; method: string } }
    | { Function: { function_name: string; code?: string } };

export interface TaskDef {
    id: string;
    kind: TaskKind;
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
