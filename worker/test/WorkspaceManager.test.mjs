import assert from 'node:assert/strict';
import { mkdtemp, readFile, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
    materializeTaskWorkspace,
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
