import assert from 'node:assert/strict';
import { mkdtemp, readFile, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
    cleanupExpiredWorkspaces,
    deleteWorkspace,
    materializeTaskWorkspace,
    materializeWorkspacePath,
    resolveWorkspacePath,
} from '../dist/core/WorkspaceManager.js';

test('materializes dispatched workspace suffix under worker root', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'runhelm-worker-workspace-'));

    const payload = await materializeTaskWorkspace(
        {
            workflow_inst_id: 'workflow-1',
            task: {
                id: 'draft',
                kind: { Function: { code: 'return {};', dependencies: [] } },
                required_credentials: [],
            },
            workspace_path_suffix: 'workflow-1/taskid-draft',
            inputs: [],
        },
        root
    );

    const expectedWorkspacePath = path.join(root, 'workflow-1', 'taskid-draft');
    assert.equal(payload.workspace_path, expectedWorkspacePath);
    assert.equal((await stat(expectedWorkspacePath)).isDirectory(), true);

    const timestamp = await readFile(path.join(expectedWorkspacePath, '.timestamp'), 'utf8');
    assert.match(timestamp, /^\d+$/);
});

test('rejects workspace suffix that escapes worker root', () => {
    assert.throws(
        () => resolveWorkspacePath('/tmp/runhelm-workspaces', '../outside'),
        /workspace_path_suffix must stay under the worker workspace root/
    );
});

test('rejects absolute workspace suffix', () => {
    assert.throws(
        () => resolveWorkspacePath('/tmp/runhelm-workspaces', '/tmp/outside'),
        /workspace_path_suffix must be a non-empty relative path/
    );
});

test('ttl cleanup retains expired active workflow workspaces', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'runhelm-worker-workspace-'));
    await createWorkspace(root, 'pending-workflow/taskid-draft', 100);
    await createWorkspace(root, 'running-workflow/taskid-draft', 100);
    await createWorkspace(root, 'input-workflow/taskid-draft', 100);

    const result = await cleanupExpiredWorkspaces(root, {
        ttlSeconds: 10,
        nowEpochSeconds: 200,
        workflowStatuses: {
            'pending-workflow': 'Pending',
            'running-workflow': 'Running',
            'input-workflow': 'InputNeeded',
        },
    });

    assert.deepEqual(result, { removed: 0, skipped: 3 });
    await assertDirectoryExists(path.join(root, 'pending-workflow', 'taskid-draft'));
    await assertDirectoryExists(path.join(root, 'running-workflow', 'taskid-draft'));
    await assertDirectoryExists(path.join(root, 'input-workflow', 'taskid-draft'));
});

test('ttl cleanup removes only expired terminal workflow workspaces', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'runhelm-worker-workspace-'));
    await createWorkspace(root, 'completed-workflow/taskid-old', 100);
    await createWorkspace(root, 'failed-workflow/taskid-old', 100);
    await createWorkspace(root, 'completed-workflow/taskid-fresh', 195);

    const result = await cleanupExpiredWorkspaces(root, {
        ttlSeconds: 10,
        nowEpochSeconds: 200,
        workflowStatuses: {
            'completed-workflow': 'Completed',
            'failed-workflow': 'Failed',
        },
    });

    assert.deepEqual(result, { removed: 2, skipped: 1 });
    await assertDirectoryMissing(path.join(root, 'completed-workflow', 'taskid-old'));
    await assertDirectoryMissing(path.join(root, 'failed-workflow', 'taskid-old'));
    await assertDirectoryExists(path.join(root, 'completed-workflow', 'taskid-fresh'));
});

test('ttl cleanup retains paused and unknown workflow workspaces', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'runhelm-worker-workspace-'));
    await createWorkspace(root, 'paused-workflow/taskid-draft', 100);
    await createWorkspace(root, 'unknown-workflow/taskid-draft', 100);

    const result = await cleanupExpiredWorkspaces(root, {
        ttlSeconds: 10,
        nowEpochSeconds: 200,
        workflowStatuses: {
            'paused-workflow': 'Paused',
        },
    });

    assert.deepEqual(result, { removed: 0, skipped: 2 });
    await assertDirectoryExists(path.join(root, 'paused-workflow', 'taskid-draft'));
    await assertDirectoryExists(path.join(root, 'unknown-workflow', 'taskid-draft'));
});

test('explicit workspace deletion removes a validated workspace regardless of workflow status', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'runhelm-worker-workspace-'));
    const workspacePath = await createWorkspace(root, 'running-workflow/taskid-draft', 100);

    await deleteWorkspace(root, 'running-workflow/taskid-draft');

    await assertDirectoryMissing(workspacePath);
});

async function createWorkspace(root, suffix, timestamp) {
    const workspacePath = await materializeWorkspacePath(resolveWorkspacePath(root, suffix));
    await writeFile(path.join(workspacePath, '.timestamp'), String(timestamp));
    return workspacePath;
}

async function assertDirectoryExists(directoryPath) {
    assert.equal((await stat(directoryPath)).isDirectory(), true);
}

async function assertDirectoryMissing(directoryPath) {
    await assert.rejects(() => stat(directoryPath), /ENOENT/);
}
