import type { TaskExecutor } from '../../core/ports/TaskExecutor.js';
import type { TaskKind } from '../../core/models/TaskDef.js';
import { AgentExecutor } from './AgentExecutor.js';
import { LlmExecutor } from './LlmExecutor.js';
import { ApiCallExecutor } from './ApiCallExecutor.js';
import { FunctionExecutor } from './FunctionExecutor.js';
import { logger } from '../../utils/logger.js';

export class ExecutorFactory {
    private agentExecutor = new AgentExecutor();
    private llmExecutor = new LlmExecutor();
    private apiCallExecutor = new ApiCallExecutor();
    private functionExecutor = new FunctionExecutor();

    getExecutor(kind: TaskKind): TaskExecutor {
        if ('Llm' in kind) {
            logger.info("selected LlmExecutor");
            return this.llmExecutor;
        } else if ('Agent' in kind) {
            logger.info("selected AgentExecutor");
            return this.agentExecutor;
        } else if ('ApiCall' in kind) {
            logger.info("selected ApiCallExecutor");
            return this.apiCallExecutor;
        } else if ('Function' in kind) {
            logger.info("selected FunctionExecutor");
            return this.functionExecutor;
        }

        throw new Error(`Unsupported task kind: ${JSON.stringify(kind)}`);
    }
}
