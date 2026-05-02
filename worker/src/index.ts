// This is the skeleton worker for executing agentic tasks
// It uses pi-mono as a dependency for execution

import * as readline from 'readline';
import { Ajv } from 'ajv';
import { ExecutorFactory } from './adapters/executors/ExecutorFactory.js';
import { InMemoryCredentialsAdapter } from './adapters/InMemoryCredentialsAdapter.js';
import type { TaskExecutionPayload } from './core/models/TaskDef.js';

import { logger } from './utils/logger.js';

async function main() {
    logger.info("Worker starting up...");

    const executorFactory = new ExecutorFactory();
    // TODO: Initialize properly with actual credentials if needed
    const credentialsAdapter = new InMemoryCredentialsAdapter({
        "llm_api_key": "AIzaSyDAdxRMQO6y-UwcrcvuYv-FMXm8fvh5X8I",
        "system_brave_api_key": "BSAop3Ar-a6z2LOpEQBeCBI4gBs599S"
    });

    const ajv = new Ajv();

    // In a real environment, the task might come from an environment variable,
    // a file, or stdin. We will set up a basic structure to read tasks.

    logger.info("Worker is ready to receive tasks.");

    // Simple loop to simulate processing
    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout
    });

    for await (const line of rl) {
        try {
            const taskData = JSON.parse(line);
            const payload = taskData as TaskExecutionPayload;
            logger.info(`Received task: ${payload.task?.id || 'unknown'}`);

            // Get the appropriate executor based on task kind
            const executor = executorFactory.getExecutor(payload.task.kind);
            const result = await executor.execute(payload, credentialsAdapter);
            logger.info({ payload }, "Task execution finished");

            // Validate the result against the output_schema if provided
            const outputSchema = taskData.task?.output_schema;
            if (outputSchema) {
                const validate = ajv.compile(outputSchema);
                const isValid = validate(result);
                if (!isValid) {
                    throw new Error(`Output schema validation failed: ${ajv.errorsText(validate.errors)}`);
                }
            }

            // Output the result
            logger.info({ status: "ok", output: result }, "Task completed successfully");
        } catch (error) {
            logger.error({ status: "error", message: String(error), code: null }, "Task execution failed");
        }
    }

    logger.info("Worker shutting down.");
    process.exit(0);
}

main().catch((err) => {
    logger.error({ err }, "Worker failed to start");
    process.exit(1);
});
