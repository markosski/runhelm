import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import type { TaskDispatchPayload, TaskExecutionPayload } from './models/TaskDef.js';

export type WorkflowWorkspaceStatus =
    | 'Pending'
    | 'Running'
    | 'InputNeeded'
    | 'Paused'
    | 'Completed'
    | 'Failed';

export type WorkspaceCleanupOptions = {
    ttlSeconds: number;
    workflowStatuses: Record<string, WorkflowWorkspaceStatus | undefined>;
    nowEpochSeconds?: number;
};

export type WorkspaceCleanupResult = {
    removed: number;
    skipped: number;
};

export function configuredWorkspaceRoot(): string {
    return process.env.RUNHELM_WORKSPACE_ROOT || path.join(homedir(), '.cache', 'runhelm', 'workspaces');
}

export function validateWorkspacePathSuffix(suffix: string): void {
    if (!suffix || path.isAbsolute(suffix)) {
        throw new Error('workspace_path_suffix must be a non-empty relative path');
    }

    const normalized = path.normalize(suffix);
    const parts = normalized.split(path.sep);

    if (
        normalized === '.' ||
        normalized.startsWith(`..${path.sep}`) ||
        parts.some((part) => part === '..' || part === '')
    ) {
        throw new Error(`workspace_path_suffix must stay under the worker workspace root: ${suffix}`);
    }
}

export function resolveWorkspacePath(root: string, suffix: string): string {
    validateWorkspacePathSuffix(suffix);
    return path.resolve(root, suffix);
}

export async function materializeWorkspacePath(workspacePath: string): Promise<string> {
    await mkdir(workspacePath, { recursive: true });
    await writeFile(path.join(workspacePath, '.timestamp'), Math.floor(Date.now() / 1000).toString());
    return workspacePath;
}

export async function deleteWorkspace(root: string, suffix: string): Promise<void> {
    const workspacePath = resolveWorkspacePath(root, suffix);
    await rm(workspacePath, { recursive: true, force: true });
}

export async function cleanupExpiredWorkspaces(
    root: string,
    options: WorkspaceCleanupOptions
): Promise<WorkspaceCleanupResult> {
    if (!Number.isFinite(options.ttlSeconds) || options.ttlSeconds < 0) {
        throw new Error('workspace cleanup ttlSeconds must be a non-negative finite number');
    }

    const nowEpochSeconds = options.nowEpochSeconds ?? Math.floor(Date.now() / 1000);
    const result: WorkspaceCleanupResult = { removed: 0, skipped: 0 };
    const workflowDirs = await readDirectories(root);

    for (const workflowDir of workflowDirs) {
        const workflowStatus = options.workflowStatuses[workflowDir.name];
        const workspaceDirs = await readDirectories(path.join(root, workflowDir.name));

        for (const workspaceDir of workspaceDirs) {
            const workspacePath = path.join(root, workflowDir.name, workspaceDir.name);
            if (!isTerminalWorkflowStatus(workflowStatus)) {
                result.skipped += 1;
                continue;
            }

            const timestamp = await readWorkspaceTimestamp(workspacePath);
            if (timestamp === undefined || nowEpochSeconds - timestamp < options.ttlSeconds) {
                result.skipped += 1;
                continue;
            }

            await rm(workspacePath, { recursive: true, force: true });
            result.removed += 1;
        }
    }

    return result;
}

export async function materializeTaskWorkspace(
    payload: TaskDispatchPayload,
    workspaceRoot = configuredWorkspaceRoot()
): Promise<TaskExecutionPayload> {
    const workspacePath = resolveWorkspacePath(workspaceRoot, payload.workspace_path_suffix);

    await materializeWorkspacePath(workspacePath);

    return {
        ...payload,
        workspace_path: workspacePath,
    };
}

function isTerminalWorkflowStatus(status: WorkflowWorkspaceStatus | undefined): boolean {
    return status === 'Completed' || status === 'Failed';
}

async function readDirectories(directoryPath: string) {
    try {
        return (await readdir(directoryPath, { withFileTypes: true })).filter((entry) => entry.isDirectory());
    } catch (error) {
        if (isNodeError(error) && error.code === 'ENOENT') {
            return [];
        }

        throw error;
    }
}

async function readWorkspaceTimestamp(workspacePath: string): Promise<number | undefined> {
    try {
        const rawTimestamp = await readFile(path.join(workspacePath, '.timestamp'), 'utf8');
        const timestamp = Number(rawTimestamp.trim());
        return Number.isFinite(timestamp) ? timestamp : undefined;
    } catch (error) {
        if (isNodeError(error) && error.code === 'ENOENT') {
            return undefined;
        }

        throw error;
    }
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
    return error instanceof Error && 'code' in error;
}
