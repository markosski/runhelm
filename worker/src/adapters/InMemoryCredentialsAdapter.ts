import type { CredentialName, CredentialsPort } from '../core/ports/CredentialsPort.js';

export type CredentialEntries =
    | Readonly<Record<CredentialName, string>>
    | ReadonlyMap<CredentialName, string>;

export class InMemoryCredentialsAdapter implements CredentialsPort {
    private readonly credentials: ReadonlyMap<CredentialName, string>;

    constructor(credentials: CredentialEntries) {
        this.credentials =
            credentials instanceof Map
                ? new Map(credentials)
                : new Map(Object.entries(credentials));
    }

    async getCredential(name: CredentialName): Promise<string | undefined> {
        return this.credentials.get(name);
    }
}
