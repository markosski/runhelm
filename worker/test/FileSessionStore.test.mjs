import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { FileSessionStore } from '../dist/adapters/FileSessionStore.js';

test('returns null when a session file does not exist', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);

        assert.equal(await store.load('workflow/task'), null);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('round-trips JSONL session content exactly', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));
    const session = '{"type":"user","message":"hello"}\n{"type":"assistant","message":"world"}\n';

    try {
        const store = new FileSessionStore(dir);

        await store.write('workflow/task', { lines: session });

        assert.deepEqual(await store.load('workflow/task'), { lines: session });
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('encodes slash-containing keys into one session file', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);

        await store.write('workflow-instance/task-id', { lines: '{"type":"entry"}\n' });

        assert.deepEqual(await store.load('workflow-instance/task-id'), { lines: '{"type":"entry"}\n' });
        assert.equal((await readdir(dir)).length, 1);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('overwrites existing session content', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);

        await store.write('workflow/task', { lines: '{"type":"old"}\n' });
        await store.write('workflow/task', { lines: '{"type":"new"}\n' });

        assert.deepEqual(await store.load('workflow/task'), { lines: '{"type":"new"}\n' });
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
