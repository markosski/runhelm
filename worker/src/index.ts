import { Ajv } from 'ajv';
import { ExecutorFactory } from './adapters/executors/ExecutorFactory.js';
import { FileCredentialsAdapter, defaultCredentialsFilePath } from './adapters/FileCredentialsAdapter.js';
import { FileSessionStore } from './adapters/FileSessionStore.js';
import type { TaskExecutionPayload } from './core/models/TaskDef.js';
import type { CredentialsPort } from './core/ports/CredentialsPort.js';
import type { SessionStore } from './core/ports/SessionStore.js';
import type { TaskExecutionResult } from './core/ports/TaskExecutor.js';

import * as os from 'os';
import { logger } from './utils/logger.js';

const DEFAULT_ORCHESTRATOR_HTTP_URL = 'http://127.0.0.1:3001';
const DEFAULT_POLL_DELAY_MS = 1_000;
const DEFAULT_ORCHESTRATOR_RETRY_DELAY_MS = 1_000;
const DEFAULT_RESULT_ACK_RETRY_DELAY_MS = 1_000;
const DEFAULT_RESULT_ACK_MAX_ATTEMPTS = 3;

type WorkerRegistrationMessage = {
    type: 'register';
    worker_id: string;
    host_id: string;
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

type WorkerResponse = RegistrationAckMessage | NoTaskMessage | TaskDispatchMessage;

type ResultAckMessage = {
    status: 'accepted';
};

type WorkerExecutionResult =
    | { kind: 'success'; output: unknown }
    | { kind: 'input_needed'; description: string }
    | { kind: 'failure'; reason: string };

type ResultAckRetryPolicy = {
    maxAttempts: number;
    retryDelayMs: number;
};

class HttpError extends Error {
    constructor(
        public readonly status: number,
        public readonly url: string,
        message: string
    ) {
        super(`HTTP ${status} from ${url}: ${message}`);
        this.name = 'HttpError';
    }
}

function createWorkerId(): string {
    return process.env.WORKER_ID || `${os.hostname()}-${process.pid}`;
}

function requiredWorkerHostId(): string {
    const hostId = process.env.RUNHELM_WORKER_HOST_ID?.trim();
    if (!hostId) {
        throw new Error('RUNHELM_WORKER_HOST_ID is required and must identify the worker host durable state domain');
    }

    return hostId;
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
    sessionStore: SessionStore,
    ajv: Ajv
): Promise<WorkerExecutionResult> {
    try {
        logger.info(`Received task: ${payload.task?.id || 'unknown'}`);

        // Get the appropriate executor based on task kind
        const executor = executorFactory.getExecutor(payload.task.kind);
        const result = await executor.execute(payload, credentialsAdapter, sessionStore);

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

function workerRegistration(workerId: string, workerHostId: string): WorkerRegistrationMessage {
    return {
        type: 'register',
        worker_id: workerId,
        host_id: workerHostId,
    };
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
    const response = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
    });

    if (!response.ok) {
        throw new HttpError(response.status, url, await response.text());
    }

    return await response.json() as T;
}

function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

function describeError(error: unknown): string {
    if (error instanceof Error) {
        const cause = (error as Error & { cause?: unknown }).cause;
        if (cause instanceof Error) {
            return `${error.message}: ${cause.message}`;
        }

        return error.message;
    }

    return String(error);
}

async function registerWorkerUntilAck(baseUrl: string, workerId: string, workerHostId: string): Promise<void> {
    const url = `${baseUrl}/workers/register`;
    let attempt = 0;

    while (true) {
        try {
            const ack = await postJson<RegistrationAckMessage>(url, workerRegistration(workerId, workerHostId));
            if (ack.type === 'registration_ack' && ack.worker_id === workerId) {
                logger.info({ workerId, workerHostId }, "Worker registered with orchestrator");
                return;
            }

            logger.warn({ ack, workerId, workerHostId }, "Unexpected worker registration ack");
        } catch (err) {
            attempt += 1;
            const retryContext = {
                error: describeError(err),
                attempt,
                workerId,
                workerHostId,
                retryDelayMs: DEFAULT_ORCHESTRATOR_RETRY_DELAY_MS,
            };

            if (attempt % 30 === 0) {
                logger.warn(retryContext, "Still waiting for orchestrator worker API");
            } else if (attempt <= 3 || attempt % 5 === 0) {
                logger.info(retryContext, "Waiting for orchestrator worker API");
            }
        }

        await sleep(DEFAULT_ORCHESTRATOR_RETRY_DELAY_MS);
    }
}

async function postTaskResultUntilAck(
    baseUrl: string,
    taskId: string,
    result: WorkerExecutionResult,
    retryPolicy: ResultAckRetryPolicy = {
        maxAttempts: DEFAULT_RESULT_ACK_MAX_ATTEMPTS,
        retryDelayMs: DEFAULT_RESULT_ACK_RETRY_DELAY_MS,
    }
): Promise<void> {
    const url = `${baseUrl}/workers/tasks/${encodeURIComponent(taskId)}/result`;
    let lastError: unknown;

    for (let attempt = 1; attempt <= retryPolicy.maxAttempts; attempt++) {
        try {
            const ack = await postJson<ResultAckMessage>(url, result);
            if (ack.status === 'accepted') {
                return;
            }

            lastError = new Error(`Unexpected result ack: ${JSON.stringify(ack)}`);
            logger.warn({ taskId, ack, attempt, maxAttempts: retryPolicy.maxAttempts }, "Task result post did not receive accepted ack");
        } catch (err) {
            lastError = err;
            logger.warn({ taskId, err, attempt, maxAttempts: retryPolicy.maxAttempts }, "Task result post failed");
        }

        if (attempt < retryPolicy.maxAttempts) {
            await sleep(retryPolicy.retryDelayMs);
        }
    }

    throw new Error(`Task result for ${taskId} was not acknowledged after ${retryPolicy.maxAttempts} attempts`, {
        cause: lastError,
    });
}

async function runWorker(
    workerId: string,
    workerHostId: string,
    executorFactory: ExecutorFactory,
    credentialsAdapter: CredentialsPort,
    sessionStore: SessionStore,
    ajv: Ajv
) {
    const baseUrl = (process.env.RUNHELM_ORCHESTRATOR_HTTP_URL || DEFAULT_ORCHESTRATOR_HTTP_URL)
        .replace(/\/$/, '');
    logger.info({ baseUrl, workerId, workerHostId }, "Connecting to orchestrator HTTP API");

    await registerWorkerUntilAck(baseUrl, workerId, workerHostId);

    while(true) {
        let message: WorkerResponse;
        try {
            message = await postJson<WorkerResponse>(`${baseUrl}/workers/tasks/claim`, {
                worker_id: workerId,
            });
        } catch (err) {
            if (err instanceof HttpError && err.status === 404) {
                logger.warn({ workerId }, "Worker is not registered with orchestrator; re-registering");
                await registerWorkerUntilAck(baseUrl, workerId, workerHostId);
            } else {
                logger.warn({ error: describeError(err), workerId, retryDelayMs: DEFAULT_ORCHESTRATOR_RETRY_DELAY_MS }, "Worker task claim failed; retrying");
                await sleep(DEFAULT_ORCHESTRATOR_RETRY_DELAY_MS);
            }
            continue;
        }

        if (message.type === 'no_task') {
            await new Promise((resolve) => setTimeout(resolve, DEFAULT_POLL_DELAY_MS));
            continue;
        }

        if (message.type === 'registration_ack') {
            continue;
        }

        logger.info({ taskId: message.task_id }, "Claimed task dispatch");
        // TODO: consider adding a timeout for task execution and implement a heartbeat mechanism to let the orchestrator know the worker is still alive and working on the task, especially for long-running tasks
        const result = await processTask(message, executorFactory, credentialsAdapter, sessionStore, ajv);
        await postTaskResultUntilAck(baseUrl, message.task_id, result);
        logger.info({ taskId: message.task_id, resultKind: result.kind }, "Task result acknowledged");
    }
}

async function main() {
    logger.info("Worker starting up...");

    const workerId = createWorkerId();
    const workerHostId = requiredWorkerHostId();
    const executorFactory = new ExecutorFactory();
    const credentialsFilePath = defaultCredentialsFilePath();
    const credentialsAdapter = await FileCredentialsAdapter.fromFile(credentialsFilePath);
    const sessionStore = new FileSessionStore();

    const ajv = new Ajv();

    await runWorker(workerId, workerHostId, executorFactory, credentialsAdapter, sessionStore, ajv);
}

main().catch((err) => {
    logger.error({ err }, "Worker failed to start");
    process.exit(1);
});
