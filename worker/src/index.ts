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
const DEFAULT_ORCHESTRATOR_HTTP_URL = 'http://127.0.0.1:3000';
const DEFAULT_POLL_DELAY_MS = 1_000;
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

type NoTaskMessage = {
    type: 'no_task';
};

type TaskDispatchMessage = TaskExecutionPayload & {
    type: 'task_dispatch';
    task_id: string;
};

type OrchestratorMessage = RegistrationAckMessage | NoTaskMessage | TaskDispatchMessage;

type TaskRequestMessage = {
    type: 'task_request';
};

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
            return { kind: 'failure', reason: result.message };
    }
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
        return { kind: 'failure', reason: String(error) };
    }
}

function workerRegistration(workerId: string): WorkerRegistrationMessage {
    return {
        type: 'register',
        worker_id: workerId,
        capabilities: [...SUPPORTED_CAPABILITIES],
    };
}

function taskRequest(): TaskRequestMessage {
    return { type: 'task_request' };
}

async function runIpcWorker(
    workerId: string,
    executorFactory: ExecutorFactory,
    credentialsAdapter: CredentialsPort,
    ajv: Ajv
) {
    const socketPath = process.env.RUNHELM_SOCKET_PATH || DEFAULT_SOCKET_PATH;
    const socket = net.createConnection(socketPath);

    let buffer = '';
    let processing = Promise.resolve();

    const send = (message: unknown) => {
        socket.write(serializeNdjson(message));
    };

    const requestTask = () => {
        send(taskRequest());
    };

    const handleMessage = async (message: OrchestratorMessage) => {
        switch (message.type) {
            case 'registration_ack':
                logger.info({ workerId: message.worker_id }, "Worker registration acknowledged");
                requestTask();
                return;
            case 'no_task':
                requestTask();
                return;
            case 'task_dispatch': {
                logger.info({ taskId: message.task_id }, "Claimed task dispatch");
                const result = await processTask(message, executorFactory, credentialsAdapter, ajv);
                const response: TaskResultMessage = {
                    type: 'task_result',
                    task_id: message.task_id,
                    result,
                };
                send(response);
                logger.info({ taskId: message.task_id, resultKind: result.kind }, "Sent task result");
                requestTask();
                return;
            }
        }
    };

    socket.on('connect', () => {
        logger.info({ socketPath, workerId }, "Connected to orchestrator IPC socket");
        send(workerRegistration(workerId));
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

async function postJson<T>(url: string, body: unknown): Promise<T> {
    const response = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
    });

    if (!response.ok) {
        throw new Error(`HTTP ${response.status} from ${url}: ${await response.text()}`);
    }

    return await response.json() as T;
}

async function runHttpWorker(
    workerId: string,
    executorFactory: ExecutorFactory,
    credentialsAdapter: CredentialsPort,
    ajv: Ajv
) {
    const baseUrl = (process.env.RUNHELM_ORCHESTRATOR_HTTP_URL || DEFAULT_ORCHESTRATOR_HTTP_URL).replace(/\/$/, '');
    logger.info({ baseUrl, workerId }, "Connecting to orchestrator HTTP API");

    await postJson<RegistrationAckMessage>(`${baseUrl}/workers/register`, workerRegistration(workerId));

    for (;;) {
        const message = await postJson<OrchestratorMessage>(`${baseUrl}/workers/tasks/claim`, {
            worker_id: workerId,
        });

        if (message.type === 'no_task') {
            await new Promise((resolve) => setTimeout(resolve, DEFAULT_POLL_DELAY_MS));
            continue;
        }

        if (message.type === 'registration_ack') {
            continue;
        }

        logger.info({ taskId: message.task_id }, "Claimed task dispatch");
        const result = await processTask(message, executorFactory, credentialsAdapter, ajv);
        await postJson(`${baseUrl}/workers/tasks/${encodeURIComponent(message.task_id)}/result`, result);
        logger.info({ taskId: message.task_id, resultKind: result.kind }, "Sent task result");
    }
}

async function main() {
    logger.info("Worker starting up...");

    const executorFactory = new ExecutorFactory();
    const credentialsFilePath = defaultCredentialsFilePath();
    const credentialsAdapter = await FileCredentialsAdapter.fromFile(credentialsFilePath);

    const ajv = new Ajv();
    const workerId = createWorkerId();

    if (process.env.RUNHELM_WORKER_TRANSPORT === 'http') {
        await runHttpWorker(workerId, executorFactory, credentialsAdapter, ajv);
    } else {
        await runIpcWorker(workerId, executorFactory, credentialsAdapter, ajv);
    }
}

main().catch((err) => {
    logger.error({ err }, "Worker failed to start");
    process.exit(1);
});
