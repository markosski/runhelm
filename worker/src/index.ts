// This is the skeleton worker for executing agentic tasks
// It uses pi-mono as a dependency for execution

import { Ajv } from 'ajv';
import { ExecutorFactory } from './adapters/executors/ExecutorFactory.js';
import { InMemoryCredentialsAdapter } from './adapters/InMemoryCredentialsAdapter.js';
import type { TaskExecutionPayload } from './core/models/TaskDef.js';

import * as net from 'net';
import * as fs from 'fs';
import { logger } from './utils/logger.js';

async function processTask(
    line: string,
    executorFactory: ExecutorFactory,
    credentialsAdapter: InMemoryCredentialsAdapter,
    ajv: Ajv
): Promise<string> {
    try {
        const taskData = JSON.parse(line);
        const payload = taskData as TaskExecutionPayload;
        logger.info(`Received task: ${payload.task?.id || 'unknown'}`);

        // Get the appropriate executor based on task kind
        const executor = executorFactory.getExecutor(payload.task.kind);
        const result = await executor.execute(payload, credentialsAdapter);

        if (result.status === 'ok') {
            // Validate the result against the output_schema if provided
            const outputSchema = taskData.task?.output_schema;
            if (outputSchema) {
                const validate = ajv.compile(outputSchema);
                const isValid = validate(result.output);
                if (!isValid) {
                    const errorMsg = `Output schema validation failed: ${ajv.errorsText(validate.errors)}`;
                    return JSON.stringify({ status: "error", message: errorMsg, code: null });
                }
            }
        }
        return JSON.stringify(result);
    } catch (error) {
        return JSON.stringify({ status: "error", message: String(error), code: null });
    }
}

async function main() {
    logger.info("Worker starting up...");

    const executorFactory = new ExecutorFactory();
    const credentialsAdapter = new InMemoryCredentialsAdapter({
        "llm_api_key": process.env.LLM_API_KEY || "AIzaSyDAdxRMQO6y-UwcrcvuYv-FMXm8fvh5X8I",
        "system_brave_api_key": process.env.BRAVE_API_KEY || "BSAop3Ar-a6z2LOpEQBeCBI4gBs599S"
    });

    const ajv = new Ajv();
    const socketPath = process.env.WORKER_SOCKET || '/tmp/worker.sock';

    // --- SOCKET MODE (Resident) ---
    if (fs.existsSync(socketPath)) {
        fs.rmSync(socketPath, { recursive: true, force: true });
    }

    const server = net.createServer((socket) => {
        logger.info("Client connected via socket");
        let buffer = '';
        socket.on('data', async (data) => {
            try {
                const chunk = data.toString();
                logger.info(`Received chunk: ${data.length} bytes`);
                buffer += chunk;
                
                // Attempt to find and parse complete JSON objects in the buffer
                // We look for a balanced string that starts with { and ends with }
                let startIndex = buffer.indexOf('{');
                if (startIndex === -1) {
                    // No object start found, clear junk if any
                    if (buffer.length > 1000) buffer = ''; 
                    return;
                }

                // Try to see if the current buffer (from the first { onwards) is valid JSON
                const potentialJson = buffer.substring(startIndex);
                try {
                    // Basic check to avoid parsing massive incomplete strings
                    if (potentialJson.includes('}')) {
                        const parsed = JSON.parse(potentialJson);
                        // If we are here, potentialJson is a complete valid JSON object
                        logger.info(`Valid task detected (${potentialJson.length} characters)`);
                        
                        const response = await processTask(JSON.stringify(parsed), executorFactory, credentialsAdapter, ajv);
                        logger.info(`Sending response (${response.length} characters)`);
                        socket.write(response + '\n');
                        
                        buffer = ''; // Clear buffer after successful processing
                    }
                } catch (e) {
                    // Not valid JSON yet, wait for more chunks
                    // We only log if it looks like it SHOULD have been finished
                    if (potentialJson.length > 5000) {
                         logger.debug("Buffer growing large without valid JSON...");
                    }
                }
            } catch (err) {
                logger.error({ err }, "Error in data listener");
                socket.end(JSON.stringify({ status: "error", message: String(err), code: "INTERNAL_ERROR" }) + '\n');
            }
        });

        socket.on('error', (err) => {
            logger.error({ err }, "Socket error");
        });
    });

    await new Promise<void>((resolve, reject) => {
        server.on('error', (err) => {
            logger.error({ err }, "Server failed to start");
            reject(err);
        });

        server.listen(socketPath, () => {
            logger.info(`🚀 Worker listening on socket: ${socketPath}`);
            try {
                fs.chmodSync(socketPath, 0o666);
            } catch (err) {
                logger.warn({ err }, "Could not set socket permissions");
            }
        });
    });

    // Handle cleanup on exit
    const cleanup = () => {
        if (fs.existsSync(socketPath)) fs.unlinkSync(socketPath);
        process.exit();
    };
    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
}

main().catch((err) => {
    logger.error({ err }, "Worker failed to start");
    process.exit(1);
});
