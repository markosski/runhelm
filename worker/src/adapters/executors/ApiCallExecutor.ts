import type { TaskExecutor, TaskExecutionResult } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { logger } from '../../utils/logger.js';

export class ApiCallExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<TaskExecutionResult> {
        if (!('ApiCall' in payload.task.kind)) {
            return { status: 'error', message: 'ApiCallExecutor received a non-ApiCall task' };
        }

        const apiCallDef = payload.task.kind.ApiCall;
        logger.info(`[ApiCallExecutor] Calling API: ${apiCallDef.method} ${apiCallDef.url}`);
        // Simulate API call
        return { 
            status: 'ok', 
            output: { response: `API call to ${apiCallDef.url} succeeded` } 
        };
    }
}
