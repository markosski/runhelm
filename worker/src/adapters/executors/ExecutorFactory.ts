import type { TaskExecutor } from '../../core/ports/TaskExecutor.js';
import type { TaskKind } from '../../core/models/TaskDef.js';
import { AgentExecutor } from './AgentExecutor.js';
import { ApiCallExecutor } from './ApiCallExecutor.js';
import { FunctionExecutor } from './FunctionExecutor.js';
import { logger } from '../../utils/logger.js';

export class ExecutorFactory {
    getExecutor(kind: TaskKind): TaskExecutor {
        if ('Agent' in kind) {
            logger.info("selected AgentExecutor");
            return new AgentExecutor();
        } else if ('ApiCall' in kind) {
            logger.info("selected ApiCallExecutor");
            return new ApiCallExecutor();
        } else if ('Function' in kind) {
            logger.info("selected FunctionExecutor");
            return new FunctionExecutor();
        }

        throw new Error(`Unsupported task kind: ${JSON.stringify(kind)}`);
    }
}
