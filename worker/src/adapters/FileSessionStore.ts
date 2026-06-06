import { readFile, writeFile, mkdir } from 'fs/promises';
import { homedir } from 'os';
import { join } from 'path';
import { SESSION_FILE_EXTENSION, SessionStoreError, encodeSessionKey,
    type SessionData, type SessionStore } from '../core/ports/SessionStore.js';
import { serializeAgentSessionKey, type AgentSessionKey } from '../core/models/AgentSession.js';

const CACHE_DIR = '.cache';
const RUNHELM_CACHE_DIR = 'runhelm';
const SESSIONS_DIR = 'file_session_store';

// Path for session store specific to this session store implementation
export function defaultSessionStoreDir(): string {
    return join(homedir(), CACHE_DIR, RUNHELM_CACHE_DIR, SESSIONS_DIR);
}

export class FileSessionStore implements SessionStore {
    constructor(private readonly rootDir = defaultSessionStoreDir()) {}

    async load(sessionKey: AgentSessionKey): Promise<SessionData | null> {
        const serializedSK = serializeAgentSessionKey(sessionKey);
        const filePath = this.sessionFilePath(serializedSK);

        try {
            return { content: await readFile(filePath, 'utf8') };
        } catch (error) {
            if (isNodeError(error) && error.code === 'ENOENT') {
                return null;
            }
            throw new SessionStoreError(
                serializedSK,
                `Unable to read session file: ${filePath}`,
                { cause: error }
            );
        }
    }

    async write(sessionKey: AgentSessionKey, sessionData: SessionData): Promise<void> {
        const serializedSK = serializeAgentSessionKey(sessionKey);
        const filePath = this.sessionFilePath(serializedSK);

        try {
            await mkdir(this.rootDir, { recursive: true });
            await writeFile(filePath, sessionData.content, 'utf8');
        } catch (error) {
            throw new SessionStoreError(
                serializeAgentSessionKey(sessionKey),
                `Unable to write session file`,
                { cause: error }
            );
        }
    }

    private sessionFilePath(sessionKey: string): string {
        return join(this.rootDir, `${encodeSessionKey(sessionKey)}${SESSION_FILE_EXTENSION}`);
    }
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
    return error instanceof Error && 'code' in error;
}
