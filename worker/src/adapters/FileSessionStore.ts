import { mkdir, readFile, rename, writeFile } from 'fs/promises';
import { homedir } from 'os';
import { join } from 'path';
import type { SessionData, SessionStore } from '../core/ports/SessionStore.js';

const RUNHELM_DIR = '.runhelm';
const SESSIONS_DIR = 'agent_sessions';
const SESSION_FILE_EXTENSION = '.jsonl';

export function defaultSessionStoreDir(): string {
    return join(homedir(), RUNHELM_DIR, SESSIONS_DIR);
}

export class FileSessionStore implements SessionStore {
    constructor(private readonly rootDir = defaultSessionStoreDir()) {}

    async load(sessionKey: string): Promise<SessionData | null> {
        const filePath = this.sessionFilePath(sessionKey);

        try {
            return { lines: await readFile(filePath, 'utf8') };
        } catch (error) {
            if (isNodeError(error) && error.code === 'ENOENT') {
                return null;
            }
            throw new Error(`Unable to read session file: ${filePath}`);
        }
    }

    async write(sessionKey: string, sessionData: SessionData): Promise<void> {
        const filePath = this.sessionFilePath(sessionKey);
        const tempPath = `${filePath}.${process.pid}.${Date.now()}.tmp`;

        try {
            await mkdir(this.rootDir, { recursive: true });
            await writeFile(tempPath, sessionData.lines, 'utf8');
            await rename(tempPath, filePath);
        } catch {
            throw new Error(`Unable to write session file: ${filePath}`);
        }
    }

    private sessionFilePath(sessionKey: string): string {
        return join(this.rootDir, `${encodeSessionKey(sessionKey)}${SESSION_FILE_EXTENSION}`);
    }
}

function encodeSessionKey(sessionKey: string): string {
    return Buffer.from(sessionKey, 'utf8').toString('base64url');
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
    return error instanceof Error && 'code' in error;
}
