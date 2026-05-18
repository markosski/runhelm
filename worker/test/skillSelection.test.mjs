import assert from 'node:assert/strict';
import test from 'node:test';
import { selectApprovedSkills } from '../dist/adapters/executors/agent_tools/skillSelection.js';

const availableSkills = [
    skill('ticket-triage'),
    skill('release-notes'),
];

test('selects no skills for an empty allowlist', () => {
    const result = selectApprovedSkills(availableSkills, []);

    assert.deepEqual(result.approvedSkills.map((skill) => skill.name), []);
    assert.deepEqual(result.unavailableApprovedSkillNames, []);
});

test('selects explicit skills by name', () => {
    const result = selectApprovedSkills(availableSkills, ['release-notes']);

    assert.deepEqual(result.approvedSkills.map((skill) => skill.name), ['release-notes']);
    assert.deepEqual(result.unavailableApprovedSkillNames, []);
});

test('reports requested skill names that are not available', () => {
    const result = selectApprovedSkills(availableSkills, ['release-notes', 'missing-skill']);

    assert.deepEqual(result.approvedSkills.map((skill) => skill.name), ['release-notes']);
    assert.deepEqual(result.unavailableApprovedSkillNames, ['missing-skill']);
});

test('rejects _all_ for skills', () => {
    assert.throws(
        () => selectApprovedSkills(availableSkills, ['_all_']),
        /does not support _all_/
    );
});

test('rejects missing skills field', () => {
    assert.throws(
        () => selectApprovedSkills(availableSkills, undefined),
        /must be an array/
    );
});

function skill(name) {
    return {
        name,
        description: `${name} description`,
    };
}
