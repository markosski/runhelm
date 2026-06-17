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

    assert.equal(result.status, 'ok');
    assert.deepEqual(result.output, {
        workspacePath: selectedWorkspacePath,
        inputs: [{ report: 'quarterly' }],
    });
});
