import { mkdir, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import type { TaskDispatchPayload, TaskExecutionPayload } from './models/TaskDef.js';

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
