import { access, readdir, readFile } from 'node:fs/promises';
import { constants } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import {
    AuthStorage,
    DefaultResourceLoader,
    ExtensionRunner,
    ModelRegistry,
    SessionManager,
    type Skill,
    SettingsManager,
    wrapRegisteredTools,
} from '@earendil-works/pi-coding-agent';
import type { AgentTool } from '@earendil-works/pi-agent-core';
import { logger } from '../../../utils/logger.js';

type PackageJson = {
    pi?: unknown;
};

export type PiResourceToolProviderOptions = {
    cwd?: string;
    agentDir?: string;
    nodeModulesDir?: string;
    extensionPaths?: string[];
};

export type PiResourceLoadResult = {
    tools: AgentTool<any>[];
    skills: Skill[];
};

export class PiResourceToolProvider {
    constructor(private readonly options: PiResourceToolProviderOptions = {}) {}

    async loadTools(): Promise<AgentTool<any>[]> {
        return (await this.loadResources()).tools;
    }

    async loadResources(): Promise<PiResourceLoadResult> {
        const cwd = this.options.cwd ?? process.cwd();
        const agentDir = this.options.agentDir ?? process.env.RUNHELM_PI_AGENT_DIR ?? join(process.env.HOME ?? cwd, '.pi', 'agent');
        const nodeModulesDir = this.options.nodeModulesDir ?? join(cwd, 'node_modules');
        const extensionPaths = [
            ...await discoverPiPackageRoots(nodeModulesDir),
            ...normalizeExtensionPaths(cwd, this.options.extensionPaths ?? parseExtensionPathsEnv()),
        ];

        const settingsManager = SettingsManager.inMemory();
        const resourceLoader = new DefaultResourceLoader({
            cwd,
            agentDir,
            settingsManager,
            additionalExtensionPaths: extensionPaths,
            noPromptTemplates: true,
            noThemes: true,
            noContextFiles: true,
        });

        await resourceLoader.reload();
        const extensionsResult = resourceLoader.getExtensions();
        for (const error of extensionsResult.errors) {
            logger.warn({ extensionPath: error.path, error: error.error }, '[PiResourceToolProvider] Pi extension load issue');
        }
        const skillsResult = resourceLoader.getSkills();
        for (const diagnostic of skillsResult.diagnostics) {
            logger.warn(diagnostic, '[PiResourceToolProvider] Pi skill load issue');
        }

        const runner = new ExtensionRunner(
            extensionsResult.extensions,
            extensionsResult.runtime,
            cwd,
            SessionManager.inMemory(cwd),
            ModelRegistry.inMemory(AuthStorage.inMemory())
        );
        runner.bindCore(
            {
                sendMessage: () => undefined,
                sendUserMessage: () => undefined,
                appendEntry: () => undefined,
                setSessionName: () => undefined,
                getSessionName: () => undefined,
                setLabel: () => undefined,
                getActiveTools: () => [],
                getAllTools: () => [],
                setActiveTools: () => undefined,
                refreshTools: () => undefined,
                getCommands: () => [],
                setModel: async () => false,
                getThinkingLevel: () => 'off',
                setThinkingLevel: () => undefined,
            },
            {
                getModel: () => undefined,
                isIdle: () => true,
                getSignal: () => undefined,
                abort: () => undefined,
                hasPendingMessages: () => false,
                shutdown: () => undefined,
                getContextUsage: () => undefined,
                compact: () => undefined,
                getSystemPrompt: () => '',
            }
        );
        runner.onError((error) => {
            logger.warn(error, '[PiResourceToolProvider] Pi extension runtime issue');
        });

        return {
            tools: wrapRegisteredTools(runner.getAllRegisteredTools(), runner),
            skills: skillsResult.skills,
        };
    }
}

async function discoverPiPackageRoots(nodeModulesDir: string): Promise<string[]> {
    if (!await exists(nodeModulesDir)) {
        return [];
    }

    const packageRoots: string[] = [];
    for (const entry of await readdir(nodeModulesDir, { withFileTypes: true })) {
        if (!entry.isDirectory() || entry.name.startsWith('.')) {
            continue;
        }

        const entryPath = join(nodeModulesDir, entry.name);
        if (entry.name.startsWith('@')) {
            for (const scopedEntry of await readdir(entryPath, { withFileTypes: true })) {
                if (scopedEntry.isDirectory()) {
                    packageRoots.push(join(entryPath, scopedEntry.name));
                }
            }
        } else {
            packageRoots.push(entryPath);
        }
    }

    const piPackageRoots: string[] = [];
    for (const packageRoot of packageRoots) {
        if (await hasPiManifest(packageRoot)) {
            piPackageRoots.push(packageRoot);
        }
    }
    return piPackageRoots;
}

async function hasPiManifest(packageRoot: string): Promise<boolean> {
    try {
        const packageJson = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8')) as PackageJson;
        return packageJson.pi !== undefined;
    } catch {
        return false;
    }
}

function normalizeExtensionPaths(cwd: string, extensionPaths: string[]): string[] {
    return extensionPaths.map((extensionPath) => isAbsolute(extensionPath) ? extensionPath : resolve(cwd, extensionPath));
}

function parseExtensionPathsEnv(): string[] {
    const raw = process.env.RUNHELM_AGENT_EXTENSION_PATHS;
    if (!raw) {
        return [];
    }
    return raw.split(',').map((entry) => entry.trim()).filter(Boolean);
}

async function exists(path: string): Promise<boolean> {
    try {
        await access(path, constants.F_OK);
        return true;
    } catch {
        return false;
    }
}
