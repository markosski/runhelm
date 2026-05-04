export type CredentialName = string;

export interface CredentialsPort {
    getCredential(name: CredentialName): Promise<string | undefined>;
}
