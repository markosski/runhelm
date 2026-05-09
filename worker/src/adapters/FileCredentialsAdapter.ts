import { readFile } from 'fs/promises';
import { homedir } from 'os';
import { join } from 'path';
import type { CredentialName, CredentialsPort } from '../core/ports/CredentialsPort.js';

const RUNHELM_DIR = '.runhelm';
const CREDENTIALS_FILE = 'file_credentials.json';

export function defaultCredentialsFilePath(): string {
    return join(homedir(), RUNHELM_DIR, CREDENTIALS_FILE);
}

export class FileCredentialsAdapter implements CredentialsPort {
    private readonly credentials: ReadonlyMap<CredentialName, string>;

    private constructor(credentials: ReadonlyMap<CredentialName, string>) {
        this.credentials = credentials;
    }

    static async fromFile(filePath = defaultCredentialsFilePath()): Promise<FileCredentialsAdapter> {
        let contents: string;

        try {
            contents = await readFile(filePath, 'utf8');
        } catch (error) {
            if (isNodeError(error) && error.code === 'ENOENT') {
                throw new Error(`Credential file not found: ${filePath}`);
            }
            throw new Error(`Unable to read credential file: ${filePath}`);
        }

        let parsed: unknown;

        try {
            parsed = JSON.parse(contents);
        } catch {
            throw new Error(`Credential file contains invalid JSON: ${filePath}`);
        }

        return new FileCredentialsAdapter(parseCredentials(parsed, filePath));
    }

    async getCredential(name: CredentialName): Promise<string | undefined> {
        return this.credentials.get(name);
    }
}

function parseCredentials(value: unknown, filePath: string): ReadonlyMap<CredentialName, string> {
    if (!isPlainCredentialObject(value)) {
        throw new Error(`Credential file must contain a JSON object: ${filePath}`);
    }

    const credentials = new Map<CredentialName, string>();

    for (const [name, credential] of Object.entries(value)) {
        if (typeof credential !== 'string') {
            throw new Error(`Credential file has non-string value for credential "${name}": ${filePath}`);
        }
        credentials.set(name, credential);
    }

    return credentials;
}

function isPlainCredentialObject(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
    return error instanceof Error && 'code' in error;
}
