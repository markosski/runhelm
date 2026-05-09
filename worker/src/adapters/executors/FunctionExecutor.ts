import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { JsonValue, TaskExecutor, TaskExecutionResult } from '../../core/ports/TaskExecutor.js';
import type { FunctionDependency, TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { logger } from '../../utils/logger.js';

const DEFAULT_FUNCTION_TIMEOUT_MS = 300_000;
const PACKAGE_NAME_PATTERN = /^(?:@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]*$/i;
const PACKAGE_VERSION_PATTERN = /^[a-zA-Z0-9._~^*<>=| -]+$/;

type ChildResult = {
    exitCode: number | null;
    signal: NodeJS.Signals | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
};

type FunctionEnvelope =
    | { status: 'ok'; output: unknown }
    | { status: 'error'; message: string };

export class FunctionExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<TaskExecutionResult> {
        if (!('Function' in payload.task.kind)) {
            return { status: 'error', message: 'FunctionExecutor received a non-Function task' };
        }

        const functionDef = payload.task.kind.Function;
        const dependencies = functionDef.dependencies;
        const timeoutMs = functionTimeoutMs();
        const workDir = await mkdtemp(join(tmpdir(), 'runhelm-function-'));

        logger.info(
            { taskId: payload.task.id, dependencyCount: dependencies.length, workDir },
            '[FunctionExecutor] Executing JavaScript ESM function'
        );

        try {
            validateDependencies(dependencies);
            await writeRuntimeFiles(workDir, functionDef.code);

            if (dependencies.length > 0) {
                const installResult = await installDependencies(workDir, dependencies, timeoutMs);
                if (installResult.timedOut) {
                    return { status: 'error', message: `Function dependency install timed out after ${timeoutMs}ms` };
                }
                if (installResult.exitCode !== 0) {
                    return {
                        status: 'error',
                        message: `Function dependency install failed: ${formatChildFailure(installResult)}`,
                    };
                }
            }

            const credentials = await readRequiredCredentials(payload, credentialsPort);
            const context = {
                inputs: payload.inputs,
                credentials,
            };

            const runResult = await runChild(
                process.execPath,
                ['runner.mjs'],
                workDir,
                JSON.stringify(context),
                timeoutMs
            );

            if (runResult.timedOut) {
                return { status: 'error', message: `Function execution timed out after ${timeoutMs}ms` };
            }

            const envelope = parseFunctionEnvelope(runResult.stdout);
            if (!envelope) {
                return {
                    status: 'error',
                    message: `Function did not produce a valid result: ${formatChildFailure(runResult)}`,
                };
            }

            if (envelope.status === 'error') {
                return { status: 'error', message: envelope.message };
            }

            return { status: 'ok', output: envelope.output as JsonValue };
        } catch (error) {
            return { status: 'error', message: error instanceof Error ? error.message : String(error) };
        } finally {
            await rm(workDir, { recursive: true, force: true });
        }
    }
}

async function writeRuntimeFiles(workDir: string, code: string): Promise<void> {
    await writeFile(
        join(workDir, 'package.json'),
        JSON.stringify({ type: 'module', private: true }, null, 2),
        'utf8'
    );
    await writeFile(join(workDir, 'task.mjs'), code, 'utf8');
    await writeFile(join(workDir, 'runner.mjs'), runnerSource(), 'utf8');
}

async function installDependencies(
    workDir: string,
    dependencies: FunctionDependency[],
    timeoutMs: number
): Promise<ChildResult> {
    const packages = dependencies.map((dependency) => `${dependency.name}@${dependency.version}`);
    return runChild(
        'npm',
        ['install', '--omit=dev', '--package-lock=false', '--ignore-scripts', ...packages],
        workDir,
        undefined,
        timeoutMs
    );
}

async function readRequiredCredentials(
    payload: TaskExecutionPayload,
    credentialsPort: CredentialsPort
): Promise<Record<string, string>> {
    const credentials: Record<string, string> = {};

    for (const name of payload.task.required_credentials) {
        const value = await credentialsPort.getCredential(name);
        if (value === undefined) {
            throw new Error(`Missing required credential: ${name}`);
        }
        credentials[name] = value;
    }

    return credentials;
}

function validateDependencies(dependencies: FunctionDependency[]): void {
    for (const dependency of dependencies) {
        if (!PACKAGE_NAME_PATTERN.test(dependency.name)) {
            throw new Error(`Invalid npm dependency name: ${dependency.name}`);
        }
        if (!PACKAGE_VERSION_PATTERN.test(dependency.version)) {
            throw new Error(`Invalid npm dependency version for ${dependency.name}: ${dependency.version}`);
        }
    }
}

function functionTimeoutMs(): number {
    const configured = process.env.RUNHELM_FUNCTION_TIMEOUT_MS;
    if (!configured) {
        return DEFAULT_FUNCTION_TIMEOUT_MS;
    }

    const parsed = Number.parseInt(configured, 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_FUNCTION_TIMEOUT_MS;
}

function runChild(
    command: string,
    args: string[],
    cwd: string,
    stdin: string | undefined,
    timeoutMs: number
): Promise<ChildResult> {
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, {
            cwd,
            stdio: ['pipe', 'pipe', 'pipe'],
            env: process.env,
        });

        let stdout = '';
        let stderr = '';
        let timedOut = false;
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill('SIGTERM');
        }, timeoutMs);

        child.stdout.setEncoding('utf8');
        child.stderr.setEncoding('utf8');
        child.stdout.on('data', (chunk) => {
            stdout += chunk;
        });
        child.stderr.on('data', (chunk) => {
            stderr += chunk;
        });
        child.on('error', (error) => {
            clearTimeout(timer);
            reject(error);
        });
        child.on('close', (exitCode, signal) => {
            clearTimeout(timer);
            resolve({ exitCode, signal, stdout, stderr, timedOut });
        });

        if (stdin !== undefined) {
            child.stdin.end(stdin);
        } else {
            child.stdin.end();
        }
    });
}

function parseFunctionEnvelope(stdout: string): FunctionEnvelope | undefined {
    const resultLine = stdout
        .split('\n')
        .find((line) => line.startsWith('__RUNHELM_RESULT__'));

    if (!resultLine) {
        return undefined;
    }

    const parsed = JSON.parse(resultLine.slice('__RUNHELM_RESULT__'.length)) as FunctionEnvelope;
    if (parsed.status !== 'ok' && parsed.status !== 'error') {
        return undefined;
    }

    return parsed;
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

function runnerSource(): string {
    return `
import task from './task.mjs';

const RESULT_PREFIX = '__RUNHELM_RESULT__';

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

try {
  if (typeof task !== 'function') {
    throw new Error('Function task must export a default function');
  }

  const contextJson = await readStdin();
  const context = contextJson.length > 0 ? JSON.parse(contextJson) : {};
  const output = await task(context);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'ok', output: output ?? null }) + '\\n');
} catch (error) {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'error', message }) + '\\n');
}
`.trimStart();
}
