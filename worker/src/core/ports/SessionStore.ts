
export type SessionData = {
  lines: string;
};

export interface SessionStore {
    load(session_key: string): Promise<SessionData | null>;
    write(session_key: string, session_data: SessionData): Promise<void>;
}
