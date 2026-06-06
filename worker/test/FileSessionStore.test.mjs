import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { FileSessionStore, defaultSessionStoreDir } from '../dist/adapters/FileSessionStore.js';
import { SessionStoreError } from '../dist/core/ports/SessionStore.js';

const sessionKey = {
    workflowInstId: 'workflow',
    taskId: 'task',
};

test('returns null when a session file does not exist', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);

        assert.equal(await store.load(sessionKey), null);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('round-trips JSONL session content exactly', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));
    const session = '{"type":"user","message":"hello"}\n{"type":"assistant","message":"world"}\n';

    try {
        const store = new FileSessionStore(dir);

        await store.write(sessionKey, { content: session });

        assert.deepEqual(await store.load(sessionKey), { content: session });
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('encodes logical keys into one session file', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);
        const slashLikeKey = {
            workflowInstId: 'workflow-instance',
            taskId: 'task-id',
        };

        await store.write(slashLikeKey, { content: '{"type":"entry"}\n' });

        assert.deepEqual(await store.load(slashLikeKey), { content: '{"type":"entry"}\n' });
        assert.equal((await readdir(dir)).length, 1);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('overwrites existing session content', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));

    try {
        const store = new FileSessionStore(dir);

        await store.write(sessionKey, { content: '{"type":"old"}\n' });
        await store.write(sessionKey, { content: '{"type":"new"}\n' });

        assert.deepEqual(await store.load(sessionKey), { content: '{"type":"new"}\n' });
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('throws a typed session store error when a session cannot be read', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-sessions-'));
    const sessionPath = join(dir, `${Buffer.from('workflow$task', 'utf8').toString('base64url')}.jsonl`);

    try {
        await mkdir(sessionPath);
        const store = new FileSessionStore(dir);

        await assert.rejects(
            store.load(sessionKey),
            (error) => {
                assert.equal(error instanceof SessionStoreError, true);
                assert.equal(error.sessionKey, 'workflow$task');
                assert.match(error.message, /Unable to read session file/);
                assert.ok(error.cause);
                return true;
            }
        );
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('throws a typed session store error when a session cannot be written', async () => {
    const rootPath = join(tmpdir(), `runhelm-session-store-file-${process.pid}-${Date.now()}`);

    try {
        await writeFile(rootPath, 'not a directory');
        const store = new FileSessionStore(rootPath);

        await assert.rejects(
            store.write(sessionKey, { content: '{"type":"entry"}\n' }),
            (error) => {
                assert.equal(error instanceof SessionStoreError, true);
                assert.equal(error.sessionKey, 'workflow$task');
                assert.match(error.message, /Unable to write session file/);
                assert.ok(error.cause);
                return true;
            }
        );
    } finally {
        await rm(rootPath, { force: true });
    }
});

test('default store path uses worker-local cache instead of credential directory', () => {
    assert.match(defaultSessionStoreDir(), /\/\.cache\/runhelm\/file_session_store$/);
});
