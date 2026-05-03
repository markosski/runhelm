import type { TaskExecutionPayload } from '../models/TaskDef.js';
import type { CredentialsPort } from './CredentialsPort.js';
import type { JsonValue } from 'type-fest';
export type { JsonValue };

export interface TaskExecutor {
    execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<JsonValue>;
}
