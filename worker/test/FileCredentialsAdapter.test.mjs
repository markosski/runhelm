import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { FileCredentialsAdapter } from '../dist/adapters/FileCredentialsAdapter.js';

test('loads string credentials from a JSON file', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-credentials-'));
    const filePath = join(dir, 'file_credentials.json');

    try {
        await writeFile(filePath, JSON.stringify({
            llm_api_key: 'llm-key',
            system_brave_api_key: 'brave-key',
        }));

        const adapter = await FileCredentialsAdapter.fromFile(filePath);

        assert.equal(await adapter.getCredential('llm_api_key'), 'llm-key');
        assert.equal(await adapter.getCredential('system_brave_api_key'), 'brave-key');
        assert.equal(await adapter.getCredential('missing'), undefined);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('rejects a missing credential file', async () => {
    await assert.rejects(
        FileCredentialsAdapter.fromFile(join(tmpdir(), 'missing-file_credentials.json')),
        /Credential file not found/
    );
});

test('rejects invalid JSON', async () => {
    const { dir, filePath } = await writeCredentialFile('{');

    try {
        await assert.rejects(
            FileCredentialsAdapter.fromFile(filePath),
            /Credential file contains invalid JSON/
        );
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('rejects non-object JSON', async () => {
    const { dir, filePath } = await writeCredentialFile('[]');

    try {
        await assert.rejects(
            FileCredentialsAdapter.fromFile(filePath),
            /Credential file must contain a JSON object/
        );
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('rejects non-string credential values without exposing the value', async () => {
    const secretValue = 'secret-object-value';
    const { dir, filePath } = await writeCredentialFile(JSON.stringify({
        llm_api_key: { nested: secretValue },
    }));

    try {
        await assert.rejects(
            FileCredentialsAdapter.fromFile(filePath),
            (error) => {
                assert.match(error.message, /non-string value/);
                assert.match(error.message, /llm_api_key/);
                assert.equal(error.message.includes(secretValue), false);
                return true;
            }
        );

        const contents = await readFile(filePath, 'utf8');
        assert.equal(contents.includes(secretValue), true);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

async function writeCredentialFile(contents) {
    const dir = await mkdtemp(join(tmpdir(), 'runhelm-credentials-'));
    const filePath = join(dir, 'file_credentials.json');
    await writeFile(filePath, contents);
    return { dir, filePath };
}
