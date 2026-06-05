import type { TaskExecutionPayload } from "./TaskDef.js";

type AgentSessionKey = {
  workflowInstId: string;
  taskId: string;
};

function agentSessionKey(payload: TaskExecutionPayload): AgentSessionKey {
  return {
    workflowInstId: payload.workflow_inst_id,
    taskId: payload.task.id,
  };
}

function serializeAgentSessionKey(key: AgentSessionKey): string {
  return `${key.workflowInstId}/${key.taskId}`;
}