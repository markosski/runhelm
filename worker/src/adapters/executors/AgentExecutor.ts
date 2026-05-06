import type { TaskExecutor, JsonValue, TaskExecutionResult } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { getModel, streamSimple, Type } from '@mariozechner/pi-ai';
import { Agent } from '@mariozechner/pi-agent-core';
import { Ajv } from 'ajv';
import { logger } from '../../utils/logger.js';
import { createBraveSearchTool } from './agent_tools/braveSearchTool.js';
import { createFetchUrlTool } from './agent_tools/fetchUrlTool.js';
import { createHttpRequestTool } from './agent_tools/httpRequestTool.js';
import { createCurrentTimeTool } from './agent_tools/currentTimeTool.js';
import { createAskUserTool, InputNeededError } from './agent_tools/askUserTool.js';


type AvailableTool = {
    name: string;
    description: string;
    tool: any;
};

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

        if (payload.input_provided) {
            contextPrompt += `\n\nUSER RESPONSE TO PREVIOUS INQUIRY:\n${payload.input_provided}\n`;
        }

        const finalPrompt = prompt + contextPrompt;

        const availableTools: AvailableTool[] = [
            {
                name: "fetch_url",
                description: "fetch URL data and read website/page content; use this as the primary URL fetching tool",
                tool: createFetchUrlTool()
            },
            {
                name: "http_request",
                description: "make arbitrary HTTP requests; use this as the secondary URL/API fetching tool when fetch_url is not suitable or fails",
                tool: createHttpRequestTool()
            },
            {
                name: "get_current_time",
                description: "get the current time when time/date context is needed",
                tool: createCurrentTimeTool()
            }
        ];

        const systemBraveApiKey = "system_brave_api_key";
        const braveApiKey = await credentialsPort.getCredential(systemBraveApiKey);
        if (braveApiKey) {
            availableTools.push({
                name: "web_search",
                description: "find information on the internet with web search",
                tool: createBraveSearchTool(braveApiKey)
            });
        } else {
            logger.error(`Error retrieving system credentials for ${systemBraveApiKey}`);
        }

        let inputNeededQuestion: string | undefined = undefined;

        if (ask) {
            availableTools.push({
                name: "ask_user",
                description: "ask the user for additional information or clarification when needed",
                tool: createAskUserTool((question) => {
                    inputNeededQuestion = question;
                    agent.abort();
                })
            });
        }

        const approvedToolNames = Array.isArray(agentDef.tools) ? agentDef.tools : ["_all_"];
        const approvedTools = approvedToolNames.includes("_all_")
            ? availableTools
            : availableTools.filter((availableTool) => approvedToolNames.includes(availableTool.name));
        const unavailableApprovedToolNames = approvedToolNames
            .filter((toolName: string) => toolName !== "_all_")
            .filter((toolName: string) => !approvedTools.some((approvedTool) => approvedTool.name === toolName));

        if (unavailableApprovedToolNames.length > 0) {
            logger.warn({ unavailableApprovedToolNames }, "[AgentExecutor] Ignoring approved tools that are not available");
        }

        const toolAvailabilityPrompt = approvedTools.length > 0
            ? `You have access to the following approved tools:\n${approvedTools.map((approvedTool) => `- ${approvedTool.name} - ${approvedTool.description}`).join('\n')}`
            : "You do not have access to any tools for this task.";

        const agentOpts: any = {
            streamFn: streamSimple,
            getApiKey: () => apiKey,
        };
        const agent = new Agent(agentOpts);

        agent.state.model = model;
        agent.state.systemPrompt = `
            ${ask ? "CRITICAL: If you cannot complete the task because you need more information or clarification from the user, you MUST call the 'ask_user' tool. DO NOT return a JSON object with a response that asks a question. Calling 'ask_user' is the ONLY way to request more information." : ""}

            You are an autonomous AI agent.
            ${toolAvailabilityPrompt}

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
                        return { status: 'ok', output: parsed };
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
        return { status: 'ok', output: { response: resultText } };
    }
}
