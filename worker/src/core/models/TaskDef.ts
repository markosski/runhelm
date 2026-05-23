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
    verifier?: AgentVerifierConfig;
    timeout_secs?: number;
    input_schemas: any[];
    output_schema?: any;
    expected_side_effects: any[];
    required_credentials: string[];
}

export interface AgentVerifierConfig {
    max_iterations: number;
    on_exhausted_continue: boolean;
    rerun_from_task_id?: string;
    code: string;
    dependencies?: FunctionDependency[];
}

export interface LoopExecutionContext {
    generation: number;
    max_iterations: number;
    latest_feedback?: string;
    previous_output?: any;
}

export interface VerifierExecutionContext {
    output: any;
    generation: number;
    max_iterations: number;
    feedback_history: string[];
    upstream_context: Record<string, any>;
}

export interface ExecutionMetadata {
    loop_context?: LoopExecutionContext;
    verifier_context?: VerifierExecutionContext;
}

export interface VerifierExecutionResult {
    decision: 'continue' | 'complete';
    feedback?: string;
    output: any;
}

export interface TaskExecutionPayload {
    task: TaskDef;
    inputs: any[];
    execution_metadata?: ExecutionMetadata;
    input_provided?: string;
}
