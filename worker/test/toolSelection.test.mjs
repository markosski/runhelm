import assert from 'node:assert/strict';
import test from 'node:test';
import { selectApprovedTools } from '../dist/adapters/executors/agent_tools/toolSelection.js';

const availableTools = [
    tool('fetch_url'),
    tool('extension_tool'),
];

test('selects no tools for an empty allowlist', () => {
    const result = selectApprovedTools(availableTools, []);

    assert.deepEqual(result.approvedTools.map((tool) => tool.name), []);
    assert.deepEqual(result.unavailableApprovedToolNames, []);
});

test('selects all available tools for _all_', () => {
    const result = selectApprovedTools(availableTools, ['_all_']);

    assert.deepEqual(result.approvedTools.map((tool) => tool.name), ['fetch_url', 'extension_tool']);
    assert.deepEqual(result.unavailableApprovedToolNames, []);
});

test('selects explicit built-in and extension tools by name', () => {
    const result = selectApprovedTools(availableTools, ['extension_tool']);

    assert.deepEqual(result.approvedTools.map((tool) => tool.name), ['extension_tool']);
    assert.deepEqual(result.unavailableApprovedToolNames, []);
});

test('reports requested tool names that are not available', () => {
    const result = selectApprovedTools(availableTools, ['extension_tool', 'missing_tool']);

    assert.deepEqual(result.approvedTools.map((tool) => tool.name), ['extension_tool']);
    assert.deepEqual(result.unavailableApprovedToolNames, ['missing_tool']);
});

function tool(name) {
    return {
        name,
        description: `${name} description`,
        tool: {
            name,
            description: `${name} description`,
            execute: async () => ({ content: [], details: {} }),
        },
    };
}
