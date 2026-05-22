import type { TaskExecutor, JsonValue, TaskExecutionResult } from '../../core/ports/TaskExecutor.js';
import type { AgentVerifierConfig, TaskExecutionPayload, VerifierExecutionContext, VerifierExecutionResult } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getModel, streamSimple } from '@earendil-works/pi-ai';
import { Agent } from '@earendil-works/pi-agent-core';
import { createCodingTools, formatSkillsForPrompt } from '@earendil-works/pi-coding-agent';
import { Ajv } from 'ajv';
import { logger } from '../../utils/logger.js';
import { createBraveSearchTool } from './agent_tools/braveSearchTool.js';
import { createFetchUrlTool } from './agent_tools/fetchUrlTool.js';
import { createHttpRequestTool } from './agent_tools/httpRequestTool.js';
import { createCurrentTimeTool } from './agent_tools/currentTimeTool.js';
import { createAskUserTool, InputNeededError } from './agent_tools/askUserTool.js';
import { ToolRegistry } from './agent_tools/ToolRegistry.js';
import { PiResourceToolProvider } from './agent_tools/PiResourceToolProvider.js';
import { selectApprovedTools } from './agent_tools/toolSelection.js';
import { selectApprovedSkills } from './agent_tools/skillSelection.js';

function extractAssistantText(agent: Agent): string {
    let resultText = '';
    const lastMsg = agent.state.messages[agent.state.messages.length - 1];
    if (lastMsg && lastMsg.role === 'assistant') {
        for (const content of lastMsg.content) {
            if (content.type === 'text') {
                resultText += content.text;
            }
        }
    }
    return resultText;
}

function extractJsonString(resultText: string): string {
    let jsonString = resultText.trim();
    
    // Remove markdown code blocks if present
    const markdownMatch = jsonString.match(/```(?:json)?\s*([\s\S]*?)\s*```/);
    if (markdownMatch?.[1]) {
        jsonString = markdownMatch[1].trim();
    }

    const firstObjectBrace = jsonString.indexOf('{');
    const firstArrayBracket = jsonString.indexOf('[');
    const hasObject = firstObjectBrace !== -1;
    const hasArray = firstArrayBracket !== -1;

    if (!hasObject && !hasArray) {
        return ""; // No JSON structures found
    }

    const startsWithArray = hasArray && (!hasObject || firstArrayBracket < firstObjectBrace);
    const firstJsonChar = startsWithArray ? firstArrayBracket : firstObjectBrace;
    const lastJsonChar = startsWithArray ? jsonString.lastIndexOf(']') : jsonString.lastIndexOf('}');

    if (firstJsonChar !== -1 && lastJsonChar !== -1 && lastJsonChar > firstJsonChar) {
        return jsonString.substring(firstJsonChar, lastJsonChar + 1).trim();
    }
    return "";
}

/**
 * Attempts to repair common JSON errors like unescaped quotes or missing braces.
 */
function repairJson(str: string): string {
    let repaired = str.trim();
    if (!repaired) return "";

    // If it doesn't start with { or [, it's definitely not JSON
    if (!repaired.startsWith('{') && !repaired.startsWith('[')) return "";

    try {
        JSON.parse(repaired);
        return repaired; // Already valid
    } catch (e) {
        // Try a very basic "loose" repair for unescaped quotes in string values:
        // This regex looks for quotes that are NOT preceded by : or , or { or [
        // and NOT followed by : or , or } or ]
        // Note: This is a heuristic and not perfect.
        try {
            const partiallyRepaired = repaired.replace(/([^\s:{[,])"([^\s:}\],])/g, '$1\\"$2');
            JSON.parse(partiallyRepaired);
            return partiallyRepaired;
        } catch (e2) {
            return repaired; // Give up and return original for standard error handling
        }
    }
}

function parseRetryTimes(value: unknown): number {
    if (typeof value !== 'number' || !Number.isFinite(value)) {
        return 0;
    }
    return Math.max(0, Math.floor(value));
}

type ChildResult = {
    exitCode: number | null;
    signal: NodeJS.Signals | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
};

const VERIFIER_TIMEOUT_MS = 60_000;

export class AgentExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<TaskExecutionResult> {
        const agentDef = (payload.task.kind as any).Agent;
        const ask = (agentDef.ask ?? (payload.task as any).ask) === true;
        const modelIdFull = agentDef.model_id as string;
        const providerUrl = agentDef.provider_url as string;
        const prompt = agentDef.prompt as string;
        logger.info(`[AgentExecutor] Running agent model: ${modelIdFull} with provider: ${providerUrl}`);

        if (!modelIdFull.includes('/')) {
            throw new Error(`Invalid model_id format: '${modelIdFull}'. Expected format 'provider/model' (e.g., 'google/gemini-2.5-flash').`);
        }

        const parts = modelIdFull.split('/');
        const providerName = parts[0];
        const modelName = parts.slice(1).join('/');

        let model = getModel(providerName as any, modelName as any);
        if (!model) {
            throw new Error(`Model not found for ${providerName}/${modelName}`);
        }

        // Override the baseUrl with the one provided in the task definition
        if (providerUrl) {
            model = { ...model, baseUrl: providerUrl };
        }

        let apiKey: string | undefined = undefined;
        if (payload.task.required_credentials && payload.task.required_credentials.length > 0) {
            const credName = payload.task.required_credentials[0];
            logger.info(`Fetching secret for ${credName}`);

            if (credName) {
                const fetched = await credentialsPort.getCredential(credName);
                if (fetched) {
                    apiKey = fetched;
                } else {
                    logger.warn(`[AgentExecutor] Required credential ${credName} not found`);
                }
            }
        }

        let contextPrompt = "";

        if (payload.inputs.length > 0) {
            contextPrompt += `\n\nUpstream task data:\n`;
            for (const input of payload.inputs) {
                contextPrompt += `${JSON.stringify(input, null, 2)}\n`;
            }
        }

        if (payload.execution_metadata?.loop_context) {
            logger.info('[AgentExecutor] Verifier feedback provided')
            contextPrompt += `\n\nADDITIONAL CONTEXT FROM TASK VERIFIER:\n${JSON.stringify(payload.execution_metadata.loop_context, null, 2)}\n`;
        }

        if (payload.input_provided) {
            logger.info('[AgentExecutor] User response provided')
            contextPrompt += `\n\nUSER RESPONSE TO PREVIOUS INQUIRY:\n${payload.input_provided}\n`;
        }

        const finalPrompt = prompt + contextPrompt;
        const agentOpts: any = {
            streamFn: streamSimple,
            getApiKey: () => apiKey,
        };
        const agent = new Agent(agentOpts);
        agent.subscribe((event) => {
            if (event.type === 'tool_execution_start') {
                logger.info({ toolName: event.toolName, args: event.args }, '[AgentExecutor] Agent tool started');
            } else if (event.type === 'tool_execution_end') {
                logger.info(
                    { toolName: event.toolName, isError: event.isError },
                    '[AgentExecutor] Agent tool finished'
                );
            }
        });

        const toolRegistry = new ToolRegistry();
        toolRegistry.registerTools([
            createFetchUrlTool(),
            createHttpRequestTool(),
            createCurrentTimeTool(),
        ]);
        toolRegistry.registerTools(createCodingTools(process.cwd()));
        const piResources = await new PiResourceToolProvider().loadResources();
        toolRegistry.registerTools(piResources.tools);

        const systemBraveApiKey = "system_brave_api_key";
        const braveApiKey = await credentialsPort.getCredential(systemBraveApiKey);
        if (braveApiKey) {
            toolRegistry.registerTool(createBraveSearchTool(braveApiKey));
        } else {
            logger.error(`Error retrieving system credentials for ${systemBraveApiKey}`);
        }

        let inputNeededQuestion: string | undefined = undefined;

        if (ask) {
            toolRegistry.registerTool(createAskUserTool((question) => {
                inputNeededQuestion = question;
                agent.abort();
            }));
        }

        const availableTools = toolRegistry.getTools();
        const { approvedTools, unavailableApprovedToolNames } = selectApprovedTools(availableTools, agentDef.tools);
        const { approvedSkills, unavailableApprovedSkillNames } = selectApprovedSkills(piResources.skills, agentDef.skills);

        if (unavailableApprovedToolNames.length > 0) {
            logger.warn({ unavailableApprovedToolNames }, "[AgentExecutor] Ignoring approved tools that are not available");
        }

        if (unavailableApprovedSkillNames.length > 0) {
            throw new Error(`Requested agent skills are not available: ${unavailableApprovedSkillNames.join(', ')}`);
        }

        const toolAvailabilityPrompt = approvedTools.length > 0
            ? `You have access to the following approved tools:\n${approvedTools.map((approvedTool) => `- ${approvedTool.name} - ${approvedTool.description}`).join('\n')}`
            : "You do not have access to any tools for this task.";
        const canLoadSkills = approvedTools.some((approvedTool) => approvedTool.name === 'read');
        if (approvedSkills.length > 0 && !canLoadSkills) {
            throw new Error('Agent skills require the read tool so the agent can load SKILL.md content');
        }

        const skillsPrompt = approvedSkills.length > 0 && canLoadSkills
            ? `\n\n${formatSkillsForPrompt(approvedSkills)}`
            : '';

        agent.state.model = model;
        agent.state.systemPrompt = `
            ${ask ? "CRITICAL: If you cannot complete the task because you need more information or clarification from the user, you MUST call the 'ask_user' tool. DO NOT return a JSON object with a response that asks a question. Calling 'ask_user' is the ONLY way to request more information." : ""}

            You are an autonomous AI agent.
            ${toolAvailabilityPrompt}
            ${skillsPrompt}

            Use the approved tools available to you to gather the needed information.
            DO NOT ask for permission before using your approved tools. Execute them right away and use the results to answer the user.
            `;

        if (payload.task.output_schema) {
            const retryTimes = parseRetryTimes(agentDef.schema_failure_retry_times);
            agent.state.systemPrompt += `
            \n\n
            IMPORTANT: Your FINAL response must be valid JSON that adheres to the following schema:
            \n
            ${JSON.stringify(payload.task.output_schema, null, 2)}

            CRITICAL: When you have gathered all information and are ready to provide the final answer, you MUST output ONLY the raw JSON object. 
            - Do NOT include any conversational text or explanations.
            - Do NOT include any preamble like "Here is the result".
            - Do NOT wrap the JSON in markdown code blocks (e.g., no \`\`\`json).
            - The entire response must be parseable by JSON.parse().
            If output_schema validation fails, you will be asked to correct the JSON. Retry up to ${retryTimes} time${retryTimes === 1 ? '' : 's'} and only return corrected raw JSON.
            ${ask ? "REMINDER: If you are missing information to fulfill the request, use the 'ask_user' tool instead of returning JSON." : ""}`;
        }

        agent.state.systemPrompt += `
            \n\n
            NOTE: You are allowed and encouraged to use approved tools to gather information FIRST before producing the final response. 
        `;

        logger.info(`Final prompt: \n ${agent.state.systemPrompt}`);

        agent.state.tools = approvedTools.map((approvedTool) => approvedTool.tool);

        try {
            await agent.prompt(finalPrompt);
        } catch (e) {
            logger.error({ error: e, message: (e as any).message, stack: (e as any).stack }, "[AgentExecutor] Error during agent execution");
            if (e instanceof InputNeededError || inputNeededQuestion) {
                return { status: 'input_needed', description: inputNeededQuestion || (e as any).question || "Input needed" };
            }
            throw e;
        }

        if (inputNeededQuestion) {
            return { status: 'input_needed', description: inputNeededQuestion };
        }

        if (agent.state.errorMessage) {
            throw new Error(`Agent failed: ${agent.state.errorMessage}`);
        }

        if (payload.task.output_schema) {
            const retryTimes = parseRetryTimes(agentDef.schema_failure_retry_times);
            const ajv = new Ajv();
            const validate = ajv.compile(payload.task.output_schema);

            for (let attempt = 0; attempt <= retryTimes; attempt++) {
                const resultText = extractAssistantText(agent);
                let parsed: JsonValue = null;
                let parseErrorMessage: string | undefined;

                const rawExtracted = extractJsonString(resultText);
                const repaired = repairJson(rawExtracted);

                try {
                    if (repaired) {
                        parsed = JSON.parse(repaired);
                    } else if (payload.task.output_schema?.properties?.response) {
                        // FALLBACK: If no JSON found but we expect a 'response' string, wrap the raw text
                        logger.info("[AgentExecutor] No JSON found, applying auto-wrap fallback");
                        parsed = { response: resultText.trim() };
                    } else {
                        throw new Error("No JSON object found in response");
                    }
                } catch (err) {
                    parseErrorMessage = err instanceof Error ? err.message : String(err);
                }

                if (parseErrorMessage === undefined) {
                    if (validate(parsed)) {
                        return await finalizeAgentOutput(payload, parsed);
                    }

                    const validationMessage = ajv.errorsText(validate.errors);
                    if (attempt >= retryTimes) {
                        logger.error({ validationMessage, rawResponse: resultText }, "[AgentExecutor] JSON response failed output_schema validation");
                        throw new Error(`Agent JSON response failed output_schema validation: ${validationMessage}`);
                    }

                    await agent.prompt(`
                        Your previous response failed output_schema validation:
                        ${validationMessage}

                        Retry ${attempt + 1} of ${retryTimes}. Return ONLY a corrected raw JSON object that satisfies the schema.
                    `);
                } else {
                    if (attempt >= retryTimes) {
                        logger.error({ parseErrorMessage, rawResponse: resultText }, "[AgentExecutor] Failed to parse expected JSON");
                        throw new Error(`Failed to parse JSON response from agent. Expected schema: ${JSON.stringify(payload.task.output_schema)}`);
                    }

                    await agent.prompt(`
                        Your previous response was not valid parseable JSON:
                        ${parseErrorMessage}

                        Retry ${attempt + 1} of ${retryTimes}. Return ONLY a corrected raw JSON object that satisfies the schema.
                    `);
                }

                if (agent.state.errorMessage) {
                    throw new Error(`Agent failed: ${agent.state.errorMessage}`);
                }
            }
        }

        const resultText = extractAssistantText(agent);
        return await finalizeAgentOutput(payload, { response: resultText });
    }
}

async function finalizeAgentOutput(
    payload: TaskExecutionPayload,
    output: JsonValue
): Promise<TaskExecutionResult> {
    if (!payload.task.verifier) {
        return { status: 'ok', output };
    }

    const verifier = await executeVerifier(payload.task.verifier, buildVerifierContext(payload, output));
    return { status: 'ok', output, verifier };
}

function buildVerifierContext(payload: TaskExecutionPayload, output: JsonValue): VerifierExecutionContext {
    const metadataContext = payload.execution_metadata?.verifier_context;
    return {
        output,
        generation: metadataContext?.generation ?? payload.execution_metadata?.loop_context?.generation ?? 1,
        max_iterations: metadataContext?.max_iterations ?? payload.task.verifier?.max_iterations ?? 1,
        feedback_history: metadataContext?.feedback_history ?? payload.execution_metadata?.loop_context?.feedback_history ?? [],
        upstream_context: metadataContext?.upstream_context ?? {},
    };
}

async function executeVerifier(
    verifier: AgentVerifierConfig,
    context: VerifierExecutionContext
): Promise<VerifierExecutionResult> {
    if ((verifier.dependencies ?? []).length > 0) {
        throw new Error('Agent verifier dependencies are not supported');
    }
    if (!verifier.code || verifier.code.trim().length === 0) {
        throw new Error('Agent verifier code must be non-empty');
    }

    const workDir = await mkdtemp(join(tmpdir(), 'runhelm-verifier-'));
    try {
        await writeFile(join(workDir, 'package.json'), JSON.stringify({ type: 'module', private: true }, null, 2), 'utf8');
        await writeFile(join(workDir, 'verifier.mjs'), verifier.code, 'utf8');
        await writeFile(join(workDir, 'runner.mjs'), verifierRunnerSource(), 'utf8');

        const result = await runChild(
            process.execPath,
            ['runner.mjs'],
            workDir,
            JSON.stringify(context),
            VERIFIER_TIMEOUT_MS
        );

        if (result.timedOut) {
            throw new Error(`Verifier execution timed out after ${VERIFIER_TIMEOUT_MS}ms`);
        }
        if (result.exitCode !== 0) {
            throw new Error(`Verifier execution failed: ${formatChildFailure(result)}`);
        }

        const envelope = parseVerifierEnvelope(result.stdout);
        if (!envelope) {
            throw new Error(`Verifier did not produce a valid result: ${formatChildFailure(result)}`);
        }
        if (envelope.status === 'error') {
            throw new Error(envelope.message);
        }

        return validateVerifierDecision(envelope.output);
    } finally {
        await rm(workDir, { recursive: true, force: true });
    }
}

function validateVerifierDecision(value: unknown): VerifierExecutionResult {
    if (!value || typeof value !== 'object') {
        throw new Error('Verifier result must be an object');
    }
    const result = value as Record<string, unknown>;
    if (result.decision !== 'continue' && result.decision !== 'complete') {
        throw new Error('Verifier decision must be "continue" or "complete"');
    }
    if (result.decision === 'continue') {
        if (typeof result.feedback !== 'string' || result.feedback.trim().length === 0) {
            throw new Error('Verifier continue decision requires non-empty feedback');
        }
        return {
            decision: 'continue',
            feedback: result.feedback,
            output: value,
        };
    }
    const complete: VerifierExecutionResult = {
        decision: 'complete',
        output: value,
    };
    if (typeof result.feedback === 'string') {
        complete.feedback = result.feedback;
    }
    return complete;
}

type VerifierEnvelope =
    | { status: 'ok'; output: unknown }
    | { status: 'error'; message: string };

function parseVerifierEnvelope(stdout: string): VerifierEnvelope | undefined {
    const resultLine = stdout
        .split('\n')
        .find((line) => line.startsWith('__RUNHELM_VERIFIER_RESULT__'));
    if (!resultLine) {
        return undefined;
    }
    const parsed = JSON.parse(resultLine.slice('__RUNHELM_VERIFIER_RESULT__'.length)) as VerifierEnvelope;
    if (parsed.status !== 'ok' && parsed.status !== 'error') {
        return undefined;
    }
    return parsed;
}

function runChild(
    command: string,
    args: string[],
    cwd: string,
    stdin: string,
    timeoutMs: number
): Promise<ChildResult> {
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, {
            cwd,
            stdio: ['pipe', 'pipe', 'pipe'],
            env: process.env,
        });

        let stdout = '';
        let stderr = '';
        let timedOut = false;
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill('SIGTERM');
        }, timeoutMs);

        child.stdout.setEncoding('utf8');
        child.stderr.setEncoding('utf8');
        child.stdout.on('data', (chunk) => {
            stdout += chunk;
        });
        child.stderr.on('data', (chunk) => {
            stderr += chunk;
        });
        child.on('error', (error) => {
            clearTimeout(timer);
            reject(error);
        });
        child.on('close', (exitCode, signal) => {
            clearTimeout(timer);
            resolve({ exitCode, signal, stdout, stderr, timedOut });
        });
        child.stdin.end(stdin);
    });
}

function formatChildFailure(result: ChildResult): string {
    const details = [
        result.exitCode === null ? undefined : `exit code ${result.exitCode}`,
        result.signal ? `signal ${result.signal}` : undefined,
        result.stderr.trim() ? result.stderr.trim() : undefined,
        result.stdout.trim() ? result.stdout.trim() : undefined,
    ].filter(Boolean);
    return details.join(', ') || 'unknown error';
}

function verifierRunnerSource(): string {
    return `
import verifier from './verifier.mjs';

const RESULT_PREFIX = '__RUNHELM_VERIFIER_RESULT__';

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

try {
  if (typeof verifier !== 'function') {
    throw new Error('Agent verifier must export a default function');
  }

  const contextJson = await readStdin();
  const context = contextJson.length > 0 ? JSON.parse(contextJson) : {};
  const output = await verifier(context);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'ok', output: output ?? null }) + '\\n');
} catch (error) {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  process.stdout.write(RESULT_PREFIX + JSON.stringify({ status: 'error', message }) + '\\n');
}
`.trimStart();
}
