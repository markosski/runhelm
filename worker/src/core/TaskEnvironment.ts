import type { TaskExecutionPayload } from './models/TaskDef.js';
import type { CredentialsPort } from './ports/CredentialsPort.js';

export async function resolveCredentialEnvironment(
    payload: TaskExecutionPayload,
    credentialsPort: CredentialsPort
): Promise<Record<string, string>> {
    const env: Record<string, string> = {};

    for (const credentialName of payload.task.required_credentials) {
        const value = await credentialsPort.getCredential(credentialName);
        if (value === undefined) {
            throw new Error(`Missing required credential: ${credentialName}`);
        }
        env[credentialName.toUpperCase()] = value;
    }

    return env;
}

export async function withTaskEnvironment<T>(
    env: Record<string, string>,
    run: () => Promise<T>
): Promise<T> {
    const previous = new Map<string, string | undefined>();

    for (const [name, value] of Object.entries(env)) {
        previous.set(name, process.env[name]);
        process.env[name] = value;
    }

    try {
        return await run();
    } finally {
        for (const [name, value] of previous.entries()) {
            if (value === undefined) {
                delete process.env[name];
            } else {
                process.env[name] = value;
            }
        }
    }
}
