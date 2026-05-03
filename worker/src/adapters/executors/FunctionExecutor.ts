import type { TaskExecutor, JsonValue } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { logger } from '../../utils/logger.js';

export class FunctionExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<JsonValue> {
        const functionDef = (payload.task.kind as any).Function;
        logger.info(`[FunctionExecutor] Executing function: ${functionDef.function_name}`);
        // Simulate execution
        return { response: `Function ${functionDef.function_name} executed successfully` };
    }
}
