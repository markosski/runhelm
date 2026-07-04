import assert from 'node:assert/strict';
import test from 'node:test';
import { FunctionExecutor } from '../dist/adapters/executors/FunctionExecutor.js';

test('passes selected workspace path to inline function context', async () => {
    const selectedWorkspacePath = '/tmp/runhelm/workflow-1/taskid-build-report';
    const executor = new FunctionExecutor();

    const result = await executor.execute(
        {
            workflow_inst_id: 'workflow-1',
            task: {
                id: 'build-report',
                kind: {
                    Function: {
                        code: `
export default async function run(context) {
  return {
    workspacePath: context.workspacePath,
    inputs: context.inputs
  };
}
`.trim(),
                        dependencies: [],
                    },
                },
                required_credentials: [],
            },
            workspace_path: selectedWorkspacePath,
            inputs: [{ report: 'quarterly' }],
        },
        {
            async getCredential() {
                return undefined;
            },
        }
    );

    assert.equal(result.status, 'ok', result.status === 'error' ? result.message : undefined);
    assert.deepEqual(result.output, {
        workspacePath: selectedWorkspacePath,
        inputs: [{ report: 'quarterly' }],
    });
});

test('maps required_credentials to child process environment', async () => {
    const executor = new FunctionExecutor();

    const result = await executor.execute(
        {
            workflow_inst_id: 'workflow-1',
            task: {
                id: 'read-gh-token',
                kind: {
                    Function: {
                        code: `
export default async function run() {
  return { ghToken: process.env.GH_TOKEN };
}
`.trim(),
                        dependencies: [],
                    },
                },
                required_credentials: ['gh_token'],
            },
            workspace_path: '/tmp/runhelm/workflow-1/taskid-read-gh-token',
            inputs: [],
        },
        {
            async getCredential(name) {
                return name === 'gh_token' ? 'ghp_test_token' : undefined;
            },
        }
    );

    assert.equal(result.status, 'ok', result.status === 'error' ? result.message : undefined);
    assert.deepEqual(result.output, {
        ghToken: 'ghp_test_token',
    });
});
