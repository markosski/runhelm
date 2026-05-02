import type { TaskExecutor } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { getModel, streamSimple, Type } from '@mariozechner/pi-ai';
import { Agent } from '@mariozechner/pi-agent-core';
import { logger } from '../../utils/logger.js';
import { createBraveSearchTool } from './agent_tools/braveSearchTool.js';
import { createFetchUrlTool } from './agent_tools/fetchUrlTool.js';
import { createHttpRequestTool } from './agent_tools/httpRequestTool.js';
import { createCurrentTimeTool } from './agent_tools/currentTimeTool.js';

export class AgentExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<any> {
        const agentDef = (payload.task.kind as any).Agent;
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

        const finalPrompt = prompt;

        const agent = new Agent({
            streamFn: streamSimple,
            getApiKey: () => apiKey,
        });

        agent.state.model = model as any;
        agent.state.systemPrompt = "You are an autonomous AI agent with access to web search, HTTP request, and time tools. If the user asks about a topic, website, or domain you don't know about, you MUST use the brave_search or fetch_url tools to look it up immediately. You can use get_current_time to orient yourself in time. DO NOT ask for permission before using your tools. Execute them right away and use the results to answer the user.";
        if (payload.task.output_schema) {
            agent.state.systemPrompt += `\n\nIMPORTANT: Your FINAL response must be valid JSON that adheres to the following schema:\n${JSON.stringify(payload.task.output_schema, null, 2)}\n\nNOTE: You are allowed and encouraged to use tools to gather information FIRST before producing the final JSON. Only the final answer to the user needs to be JSON without markdown.`;
        }

        const systemBraveApiKey = "system_brave_api_key";
        const braveApiKey = await credentialsPort.getCredential("system_brave_api_key");

        const tools: any[] = [
            createFetchUrlTool(),
            createHttpRequestTool(),
            createCurrentTimeTool()
        ];

        if (braveApiKey) {
            tools.push(createBraveSearchTool(braveApiKey));
        } else {
            logger.error(`Error retrieving system credentials for ${systemBraveApiKey}`);
        }
        agent.state.tools = tools;

        await agent.prompt(finalPrompt);

        if (agent.state.errorMessage) {
            throw new Error(`Agent failed: ${agent.state.errorMessage}`);
        }

        let resultText = '';
        const lastMsg = agent.state.messages[agent.state.messages.length - 1];
        if (lastMsg && lastMsg.role === 'assistant') {
            for (const content of lastMsg.content) {
                if (content.type === 'text') {
                    resultText += content.text;
                }
            }
        }

        if (payload.task.output_schema) {
            try {
                const jsonMatch = resultText.match(/```json\n([\s\S]*?)\n```/) || resultText.match(/```\n([\s\S]*?)\n```/);
                const jsonString = (jsonMatch && jsonMatch[1] !== undefined) ? jsonMatch[1] : resultText;
                return JSON.parse(jsonString.trim());
            } catch (err) {
                logger.error({ err, rawResponse: resultText }, "[AgentExecutor] Failed to parse expected JSON");
                throw new Error("Failed to parse JSON response from agent");
            }
        }

        return { response: resultText };
    }
}
