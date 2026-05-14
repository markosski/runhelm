import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { PiResourceToolProvider } from '../dist/adapters/executors/agent_tools/PiResourceToolProvider.js';

test('loads a TypeScript Pi extension tool through the Pi resource loader', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-pi-ts-extension-'));
    const extensionPath = join(dir, 'extension.ts');

    try {
        await writeFile(extensionPath, `
            import { Type } from '@earendil-works/pi-ai';

            export default function(pi) {
                pi.registerTool({
                    name: 'hello_ts_tool',
                    label: 'Hello TS Tool',
                    description: 'Say hello from a TypeScript extension',
                    parameters: Type.Object({
                        name: Type.String()
                    }),
                    async execute(toolCallId, params, signal, onUpdate, ctx) {
                        return {
                            content: [{ type: 'text', text: 'hello ' + params.name }],
                            details: { cwd: ctx.cwd, hasUI: ctx.hasUI }
                        };
                    }
                });
            }
        `, 'utf8');

        const tools = await new PiResourceToolProvider({
            cwd: dir,
            agentDir: join(dir, '.pi-agent'),
            extensionPaths: [extensionPath],
        }).loadTools();

        assert.deepEqual(tools.map((tool) => tool.name), ['hello_ts_tool']);

        const result = await tools[0].execute('call-1', { name: 'RunHelm' });
        assert.equal(result.content[0].text, 'hello RunHelm');
        assert.equal(result.details.cwd, dir);
        assert.equal(result.details.hasUI, false);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('auto-discovers preinstalled Pi packages from node_modules', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-pi-package-'));
    const packageRoot = join(dir, 'node_modules', '@acme', 'pi-tools');
    const extensionsDir = join(packageRoot, 'extensions');

    try {
        await mkdir(extensionsDir, { recursive: true });
        await writeFile(join(packageRoot, 'package.json'), JSON.stringify({
            name: '@acme/pi-tools',
            pi: {
                extensions: ['./extensions/package-tool.ts'],
            },
        }), 'utf8');
        await writeFile(join(extensionsDir, 'package-tool.ts'), `
            import { Type } from '@earendil-works/pi-ai';

            export default function(pi) {
                pi.registerTool({
                    name: 'package_ts_tool',
                    label: 'Package TS Tool',
                    description: 'Loaded from a package manifest',
                    parameters: Type.Object({}),
                    async execute() {
                        return { content: [{ type: 'text', text: 'ok' }], details: {} };
                    }
                });
            }
        `, 'utf8');

        const tools = await new PiResourceToolProvider({
            cwd: dir,
            agentDir: join(dir, '.pi-agent'),
        }).loadTools();

        assert.deepEqual(tools.map((tool) => tool.name), ['package_ts_tool']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('loads skills from preinstalled Pi packages', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-pi-package-skills-'));
    const packageRoot = join(dir, 'node_modules', '@acme', 'pi-skills');
    const skillDir = join(packageRoot, 'skills', 'ticket-triage');

    try {
        await mkdir(skillDir, { recursive: true });
        await writeFile(join(packageRoot, 'package.json'), JSON.stringify({
            name: '@acme/pi-skills',
            pi: {
                skills: ['./skills'],
            },
        }), 'utf8');
        await writeFile(join(skillDir, 'SKILL.md'), `---
name: ticket-triage
description: Triage support tickets by severity and owner.
---

# Ticket Triage

Assign priority and route the ticket.
`, 'utf8');

        const resources = await new PiResourceToolProvider({
            cwd: dir,
            agentDir: join(dir, '.pi-agent'),
        }).loadResources();

        assert.deepEqual(resources.skills.map((skill) => skill.name), ['ticket-triage']);
        assert.equal(resources.skills[0].description, 'Triage support tickets by severity and owner.');
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('skips broken Pi extensions while loading valid tools', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-pi-broken-extension-'));
    const validPath = join(dir, 'valid.ts');
    const brokenPath = join(dir, 'broken.ts');

    try {
        await writeFile(validPath, `
            import { Type } from '@earendil-works/pi-ai';
            export default function(pi) {
                pi.registerTool({
                    name: 'valid_tool',
                    label: 'Valid Tool',
                    description: 'Still loads',
                    parameters: Type.Object({}),
                    async execute() {
                        return { content: [{ type: 'text', text: 'valid' }], details: {} };
                    }
                });
            }
        `, 'utf8');
        await writeFile(brokenPath, `
            throw new Error('broken extension');
            export default function() {}
        `, 'utf8');

        const tools = await new PiResourceToolProvider({
            cwd: dir,
            agentDir: join(dir, '.pi-agent'),
            extensionPaths: [brokenPath, validPath],
        }).loadTools();

        assert.deepEqual(tools.map((tool) => tool.name), ['valid_tool']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
