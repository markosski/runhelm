import type { TaskExecutionPayload } from "./TaskDef.js";

export type AgentSessionKey = {
  workflowInstId: string;
  taskId: string;
};

export function agentSessionKey(payload: TaskExecutionPayload): AgentSessionKey {
  return {
    workflowInstId: payload.workflow_inst_id,
    taskId: payload.task.id,
  };
}

export function serializeAgentSessionKey(key: AgentSessionKey): string {
  return `${key.workflowInstId}/${key.taskId}`;
}
