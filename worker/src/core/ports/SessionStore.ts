
export type SessionData = {
  lines: string;
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
    load(session_key: string): Promise<SessionData | null>;
    write(session_key: string, session_data: SessionData): Promise<void>;
}
