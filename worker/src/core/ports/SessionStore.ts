import { mkdir, rename, writeFile } from 'fs/promises';
import { homedir } from "os";
import { join, dirname } from "path";
import { logger } from '../../utils/logger.js';
import { readFile } from 'fs/promises';
import type { AgentSession } from '@earendil-works/pi-coding-agent';
import { serializeAgentSessionKey, type AgentSessionKey } from '../models/AgentSession.js';

export const RUNHELM_DIR = '.runhelm';
export const SESSION_FILE_EXTENSION = '.jsonl';
const TEMP_SESSIONS_DIR = 'temp_session';
const NATIVE_SESSIONS_DIR = 'native_session';

export function encodeSessionKey(sessionKey: string): string {
    return Buffer.from(sessionKey, 'utf8').toString('base64url');
}

// Path for temporary session store to load into Pi Session Manager
export function tempSessionDir(): string {
    return join(homedir(), RUNHELM_DIR, TEMP_SESSIONS_DIR);
}

export function nativeSessionDir(): string {
    return join(homedir(), RUNHELM_DIR, NATIVE_SESSIONS_DIR);
}

export async function writeTempSessionFile(session_key: AgentSessionKey, contents: string): Promise<string> {
    const serlializedSK = serializeAgentSessionKey(session_key);
    const tempDir = tempSessionDir();
    const tempAtomicPath = join(tempDir, `${encodeSessionKey(serlializedSK)}.${process.pid}.${Date.now()}.tmp`);
    const finalPath = join(tempDir, `${encodeSessionKey(serlializedSK)}.${SESSION_FILE_EXTENSION}`);

    await mkdir(tempDir, { recursive: true });

    // Writing to temp file first for atomic operation
    await writeFile(tempAtomicPath, contents, 'utf8');
    await rename(tempAtomicPath, finalPath);
    return finalPath;
}

export async function persistSessionBestEffort(
  sessionKey: AgentSessionKey,
  session: AgentSession,
  sessionStore: SessionStore,
): Promise<void> {
  if (!sessionKey) return;

  const sessionFile = session.sessionFile;
  if (!sessionFile) {
    logger.warn({ sessionKey }, '[AgentExecutor] Pi session file is unavailable; session was not persisted');
    return;
  }

  try {
    const content = await readFile(sessionFile, 'utf8');
    await sessionStore.write(sessionKey, { content });
  } catch (error) {
    logger.warn({ sessionKey, sessionFile, error }, '[AgentExecutor] Could not persist agent session');
  }
}

export type SessionData = {
  content: string;
};

export class SessionStoreError extends Error {
    constructor(
        public readonly sessionKey: string,
        message: string,
        options?: { cause?: unknown }
    ) {
        super(message, options);
        this.name = 'SessionStoreError';
    }
}

export interface SessionStore {
    load(session_key: AgentSessionKey): Promise<SessionData | null>;
    write(session_key: AgentSessionKey, session_data: SessionData): Promise<void>;
}
