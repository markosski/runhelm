import type { TaskExecutor, JsonValue } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { logger } from '../../utils/logger.js';

export class ApiCallExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<JsonValue> {
        const apiCallDef = (payload.task.kind as any).ApiCall;
        logger.info(`[ApiCallExecutor] Calling API: ${apiCallDef.method} ${apiCallDef.endpoint}`);
        // Simulate API call
        return { response: `API call to ${apiCallDef.endpoint} succeeded` };
    }
}
