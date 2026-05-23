import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
    AgentVerifierConfig,
    TaskExecutionPayload,
    VerifierExecutionContext,
    VerifierExecutionResult,
} from '../../core/models/TaskDef.js';
import type { JsonValue } from '../../core/ports/TaskExecutor.js';

type ChildResult = {
    exitCode: number | null;
    signal: NodeJS.Signals | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
};

type VerifierEnvelope =
    | { status: 'ok'; output: unknown }
    | { status: 'error'; message: string };

const VERIFIER_TIMEOUT_MS = 60_000;
const RESULT_PREFIX = '__RUNHELM_VERIFIER_RESULT__';

export async function executeTaskVerifier(
    payload: TaskExecutionPayload,
    output: JsonValue
): Promise<VerifierExecutionResult | undefined> {
    if (!payload.task.verifier) {
        return undefined;
    }

    return executeVerifier(payload.task.verifier, buildVerifierContext(payload, output));
}

export function buildVerifierContext(payload: TaskExecutionPayload, output: JsonValue): VerifierExecutionContext {
    const metadataContext = payload.execution_metadata?.verifier_context;
    return {
        output,
        generation: metadataContext?.generation ?? payload.execution_metadata?.loop_context?.generation ?? 1,
        max_iterations: metadataContext?.max_iterations ?? payload.task.verifier?.max_iterations ?? 1,
        feedback_history: metadataContext?.feedback_history ?? [],
        upstream_context: metadataContext?.upstream_context ?? {},
    };
}

async function executeVerifier(
    verifier: AgentVerifierConfig,
    context: VerifierExecutionContext
): Promise<VerifierExecutionResult> {
    if ((verifier.dependencies ?? []).length > 0) {
        throw new Error('Verifier dependencies are not supported');
    }
    if (!verifier.code || verifier.code.trim().length === 0) {
        throw new Error('Verifier code must be non-empty');
    }

    const workDir = await mkdtemp(join(tmpdir(), 'runhelm-verifier-'));
    try {
        await writeFile(join(workDir, 'package.json'), JSON.stringify({ type: 'module', private: true }, null, 2), 'utf8');
        await writeFile(join(workDir, 'verifier.mjs'), verifier.code, 'utf8');
        await writeFile(join(workDir, 'runner.mjs'), verifierRunnerSource(), 'utf8');

        const result = await runChild(
            process.execPath,
            ['runner.mjs'],
            workDir,
            JSON.stringify(context),
            VERIFIER_TIMEOUT_MS
        );

        if (result.timedOut) {
            throw new Error(`Verifier execution timed out after ${VERIFIER_TIMEOUT_MS}ms`);
        }
        if (result.exitCode !== 0) {
            throw new Error(`Verifier execution failed: ${formatChildFailure(result)}`);
        }

        const envelope = parseVerifierEnvelope(result.stdout);
        if (!envelope) {
            throw new Error(`Verifier did not produce a valid result: ${formatChildFailure(result)}; workDir=${workDir}; stdout=${JSON.stringify(result.stdout)} stderr=${JSON.stringify(result.stderr)}`);
        }
        if (envelope.status === 'error') {
            throw new Error(envelope.message);
        }

        return validateVerifierDecision(envelope.output);
    } finally {
        if (process.env.RUNHELM_KEEP_VERIFIER_WORKDIR !== '1') {
            await rm(workDir, { recursive: true, force: true });
        }
    }
}

export function validateVerifierDecision(value: unknown): VerifierExecutionResult {
    if (!value || typeof value !== 'object') {
        throw new Error('Verifier result must be an object');
    }

    const result = value as Record<string, unknown>;
    if (result.decision !== 'continue' && result.decision !== 'complete') {
        throw new Error('Verifier decision must be "continue" or "complete"');
    }

    if (result.decision === 'continue') {
        if (typeof result.feedback !== 'string' || result.feedback.trim().length === 0) {
            throw new Error('Verifier continue decision requires non-empty feedback');
        }
        return {
            decision: 'continue',
            feedback: result.feedback,
            output: value,
        };
    }

    const complete: VerifierExecutionResult = {
        decision: 'complete',
        output: value,
    };
    if (typeof result.feedback === 'string') {
        complete.feedback = result.feedback;
    }
    return complete;
}

function parseVerifierEnvelope(stdout: string): VerifierEnvelope | undefined {
    const resultLine = stdout
        .split('\n')
        .find((line) => line.startsWith(RESULT_PREFIX));
    if (!resultLine) {
        return undefined;
    }

    const parsed = JSON.parse(resultLine.slice(RESULT_PREFIX.length)) as VerifierEnvelope;
    if (parsed.status !== 'ok' && parsed.status !== 'error') {
        return undefined;
    }
    return parsed;
}

function runChild(
    command: string,
    args: string[],
    cwd: string,
    stdin: string,
    timeoutMs: number
): Promise<ChildResult> {
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, {
            cwd,
            stdio: ['pipe', 'pipe', 'pipe'],
            env: verifierChildEnv(),
        });

        let stdout = '';
        let stderr = '';
        let timedOut = false;
        const stdoutComplete = collectStream(child.stdout);
        const stderrComplete = collectStream(child.stderr);
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill('SIGTERM');
        }, timeoutMs);

        child.on('error', (error) => {
            clearTimeout(timer);
            reject(error);
        });
        child.on('close', async (exitCode, signal) => {
            clearTimeout(timer);
            try {
                [stdout, stderr] = await Promise.all([stdoutComplete, stderrComplete]);
                resolve({ exitCode, signal, stdout, stderr, timedOut });
            } catch (error) {
                reject(error);
            }
        });
        child.stdin.end(stdin);
    });
}

function collectStream(stream: NodeJS.ReadableStream): Promise<string> {
    stream.setEncoding('utf8');
    return new Promise((resolve, reject) => {
        let output = '';
        stream.on('data', (chunk) => {
            output += chunk;
        });
        stream.on('error', reject);
        stream.on('end', () => {
            resolve(output);
        });
    });
}

function verifierChildEnv(): NodeJS.ProcessEnv {
    const env: NodeJS.ProcessEnv = {};
    for (const key of ['PATH', 'HOME', 'TMPDIR', 'TEMP', 'TMP']) {
        if (process.env[key] !== undefined) {
            env[key] = process.env[key];
        }
    }
    return env;
}

function formatChildFailure(result: ChildResult): string {
    const details = [
        result.exitCode === null ? undefined : `exit code ${result.exitCode}`,
        result.signal ? `signal ${result.signal}` : undefined,
        result.stderr.trim() ? result.stderr.trim() : undefined,
        result.stdout.trim() ? result.stdout.trim() : undefined,
    ].filter(Boolean);
    return details.join(', ') || 'unknown error';
}

function verifierRunnerSource(): string {
    return `
import verifier from './verifier.mjs';

const RESULT_PREFIX = '__RUNHELM_VERIFIER_RESULT__';

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

try {
  if (typeof verifier !== 'function') {
    throw new Error('Verifier must export a default function');
  }

  const contextJson = await readStdin();
  const context = contextJson.length > 0 ? JSON.parse(contextJson) : {};
  const output = await verifier(context);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'ok', output: output ?? null }) + '\\n');
} catch (error) {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'error', message }) + '\\n');
}
`.trimStart();
}
