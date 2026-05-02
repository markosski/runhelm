import type { TaskExecutionPayload } from '../models/TaskDef.js';
import type { CredentialsPort } from './CredentialsPort.js';

export interface TaskExecutor {
    execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<any>;
}
