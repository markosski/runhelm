import assert from 'node:assert/strict';
import test from 'node:test';
import {
    agentSessionKey,
    serializeAgentSessionKey,
} from '../dist/core/models/AgentSession.js';

test('derives the same logical session key for a human-input continuation-shaped payload', () => {
    const initialPayload = taskPayload({
        workflow_inst_id: 'workflow-1',
        generation_index: 1,
    });
    const humanInputPayload = taskPayload({
        workflow_inst_id: 'workflow-1',
        generation_index: 2,
        input_provided: 'Use the customer name from the support ticket.',
    });

    assert.equal(
        serializeAgentSessionKey(agentSessionKey(initialPayload)),
        'workflow-1/draft-response'
    );
    assert.equal(
        serializeAgentSessionKey(agentSessionKey(humanInputPayload)),
        'workflow-1/draft-response'
    );
});

test('derives the same logical session key for a verifier-feedback continuation-shaped payload', () => {
    const initialPayload = taskPayload({
        workflow_inst_id: 'workflow-1',
        generation_index: 1,
    });
    const verifierFeedbackPayload = taskPayload({
        workflow_inst_id: 'workflow-1',
        generation_index: 2,
        loop_context: {
            generation: 2,
            max_iterations: 3,
            feedback_history: [
                {
                    generation: 1,
                    feedback: 'Add the missing source citation.',
                },
            ],
            previous_output: {
                draft: 'Initial answer without citation.',
            },
        },
    });

    assert.equal(
        serializeAgentSessionKey(agentSessionKey(initialPayload)),
        'workflow-1/draft-response'
    );
    assert.equal(
        serializeAgentSessionKey(agentSessionKey(verifierFeedbackPayload)),
        'workflow-1/draft-response'
    );
});

function taskPayload(overrides) {
    return {
        workflow_inst_id: overrides.workflow_inst_id,
        task: {
            id: 'draft-response',
            kind: {
                Agent: {
                    model_id: 'test/model',
                    provider_url: '',
                    prompt: 'Draft a customer response.',
                    tools: [],
                    skills: [],
                    reuse_session: true,
                },
            },
            required_credentials: [],
        },
        inputs: [],
        execution_metadata: {
            generation_index: overrides.generation_index,
            ...(overrides.loop_context !== undefined
                ? { loop_context: overrides.loop_context }
                : {}),
        },
        ...(overrides.input_provided !== undefined
            ? { input_provided: overrides.input_provided }
            : {}),
    };
}
