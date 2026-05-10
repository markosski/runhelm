// This is the skeleton worker for executing agentic tasks
// It uses pi-mono as a dependency for execution

import { Ajv } from 'ajv';
import { ExecutorFactory } from './adapters/executors/ExecutorFactory.js';
import { FileCredentialsAdapter, defaultCredentialsFilePath } from './adapters/FileCredentialsAdapter.js';
import type { TaskExecutionPayload } from './core/models/TaskDef.js';
import type { CredentialsPort } from './core/ports/CredentialsPort.js';
import type { TaskExecutionResult } from './core/ports/TaskExecutor.js';

import * as net from 'net';
import * as os from 'os';
import { logger } from './utils/logger.js';

const DEFAULT_SOCKET_PATH = '/tmp/runhelm.sock';
const SUPPORTED_CAPABILITIES = ['Agent', 'ApiCall', 'Function'] as const;

type WorkerRegistrationMessage = {
    type: 'register';
    worker_id: string;
    capabilities: typeof SUPPORTED_CAPABILITIES[number][];
};

type RegistrationAckMessage = {
    type: 'registration_ack';
    worker_id: string;
};

type TaskDispatchMessage = TaskExecutionPayload & {
    type: 'task_dispatch';
    task_id: string;
};

type OrchestratorMessage = RegistrationAckMessage | TaskDispatchMessage;

type WorkerExecutionResult =
    | { kind: 'success'; output: unknown }
    | { kind: 'input_needed'; description: string }
    | { kind: 'failure'; reason: string };

type TaskResultMessage = {
    type: 'task_result';
    task_id: string;
    result: WorkerExecutionResult;
};

function createWorkerId(): string {
    return process.env.WORKER_ID || `${os.hostname()}-${process.pid}`;
}

function serializeNdjson(message: unknown): string {
    return `${JSON.stringify(message)}\n`;
}

function mapExecutionResult(result: TaskExecutionResult): WorkerExecutionResult {
    switch (result.status) {
        case 'ok':
            return { kind: 'success', output: result.output };
        case 'input_needed':
            return { kind: 'input_needed', description: result.description };
        case 'error':
            return { kind: 'failure', reason: nonEmptyMessage(result.message, 'Task failed without an error message') };
    }
}

function nonEmptyMessage(message: string, fallback: string): string {
    return message.trim().length > 0 ? message : fallback;
}

async function processTask(
    payload: TaskExecutionPayload,
    executorFactory: ExecutorFactory,
    credentialsAdapter: CredentialsPort,
    ajv: Ajv
): Promise<WorkerExecutionResult> {
    try {
        logger.info(`Received task: ${payload.task?.id || 'unknown'}`);

        // Get the appropriate executor based on task kind
        const executor = executorFactory.getExecutor(payload.task.kind);
        const result = await executor.execute(payload, credentialsAdapter);

        if (result.status === 'ok') {
            // Validate the result against the output_schema if provided
            const outputSchema = payload.task?.output_schema;
            if (outputSchema) {
                const validate = ajv.compile(outputSchema);
                const isValid = validate(result.output);
                if (!isValid) {
                    const errorMsg = `Output schema validation failed: ${ajv.errorsText(validate.errors)}`;
                    return { kind: 'failure', reason: errorMsg };
                }
            }
        }
        return mapExecutionResult(result);
    } catch (error) {
        return { kind: 'failure', reason: describeUnknownError(error) };
    }
}

function describeUnknownError(error: unknown): string {
    if (error instanceof Error) {
        return nonEmptyMessage(error.message, error.name || 'Unknown error');
    }

    if (typeof error === 'string') {
        return nonEmptyMessage(error, 'Task processing threw an empty string');
    }

    try {
        const serialized = JSON.stringify(error);
        if (serialized && serialized !== '{}') {
            return serialized;
        }
    } catch {
        // Fall through to the generic object description.
    }

    return `Task processing threw ${Object.prototype.toString.call(error)}`;
}

async function main() {
    logger.info("Worker starting up...");

    const executorFactory = new ExecutorFactory();
    const credentialsFilePath = defaultCredentialsFilePath();
    const credentialsAdapter = await FileCredentialsAdapter.fromFile(credentialsFilePath);

    const ajv = new Ajv();
    const socketPath = process.env.RUNHELM_SOCKET_PATH || DEFAULT_SOCKET_PATH;
    const workerId = createWorkerId();
    const socket = net.createConnection(socketPath);

    let buffer = '';
    let processing = Promise.resolve();

    const send = (message: unknown) => {
        socket.write(serializeNdjson(message));
    };

    const handleMessage = async (message: OrchestratorMessage) => {
        switch (message.type) {
            case 'registration_ack':
                logger.info({ workerId: message.worker_id }, "Worker registration acknowledged");
                return;
            case 'task_dispatch': {
                logger.info({ taskId: message.task_id }, "Received task dispatch");
                const result = await processTask(message, executorFactory, credentialsAdapter, ajv);
                const response: TaskResultMessage = {
                    type: 'task_result',
                    task_id: message.task_id,
                    result,
                };
                send(response);
                logger.info({ taskId: message.task_id, resultKind: result.kind }, "Sent task result");
                return;
            }
        }
    };

    socket.on('connect', () => {
        const registration: WorkerRegistrationMessage = {
            type: 'register',
            worker_id: workerId,
            capabilities: [...SUPPORTED_CAPABILITIES],
        };
        logger.info({ socketPath, workerId }, "Connected to orchestrator IPC socket");
        send(registration);
    });

    socket.on('data', (data) => {
        buffer += data.toString('utf8');

        let newlineIndex = buffer.indexOf('\n');
        while (newlineIndex !== -1) {
            const line = buffer.slice(0, newlineIndex).trim();
            buffer = buffer.slice(newlineIndex + 1);

            if (line.length > 0) {
                processing = processing
                    .then(async () => {
                        try {
                            await handleMessage(JSON.parse(line) as OrchestratorMessage);
                        } catch (err) {
                            logger.error({ err, line }, "Failed to process orchestrator IPC message");
                        }
                    });
            }

            newlineIndex = buffer.indexOf('\n');
        }
    });

    socket.on('error', (err) => {
        logger.error({ err, socketPath }, "Orchestrator socket error");
    });

    socket.on('close', () => {
        logger.info("Orchestrator socket closed");
        process.exit(0);
    });

    const cleanup = () => {
        socket.end();
    };

    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
}

main().catch((err) => {
    logger.error({ err }, "Worker failed to start");
    process.exit(1);
});
