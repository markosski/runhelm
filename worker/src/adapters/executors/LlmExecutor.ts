
import type { TaskExecutor } from '../../core/ports/TaskExecutor.js';
import type { TaskExecutionPayload } from '../../core/models/TaskDef.js';
import type { CredentialsPort } from '../../core/ports/CredentialsPort.js';
import { getModel, completeSimple, type Context, type KnownProvider } from '@mariozechner/pi-ai';
import { logger } from '../../utils/logger.js';

export class LlmExecutor implements TaskExecutor {
    async execute(payload: TaskExecutionPayload, credentialsPort: CredentialsPort): Promise<any> {
        const agentDef = (payload.task.kind as any).Agent;
        const modelIdFull = agentDef.model_id as string;
        const providerUrl = agentDef.provider_url as string;
        const prompt = agentDef.prompt as string;
        logger.info(`[LlmExecutor] Running agent model: ${modelIdFull} with provider: ${providerUrl}`);

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

        let finalPrompt = prompt;
        if (payload.task.output_schema) {
            finalPrompt += `\n\nEnsure your response is valid JSON that adheres to the following schema:\n${JSON.stringify(payload.task.output_schema, null, 2)}\nReturn ONLY valid JSON, do not use markdown blocks like \`\`\`json.`;
        }

        const context: Context = {
            messages: [
                {
                    role: 'user',
                    content: finalPrompt,
                    timestamp: Date.now()
                }
            ]
        };

        const options: any = {};
        if (apiKey) {
            options.apiKey = apiKey;
        }

        const response = await completeSimple(model as any, context, options);

        let resultText = '';
        for (const content of response.content) {
            if (content.type === 'text') {
                resultText += content.text;
            }
        }

        if (payload.task.output_schema) {
            try {
                const jsonMatch = resultText.match(/```json\n([\s\S]*?)\n```/) || resultText.match(/```\n([\s\S]*?)\n```/);
                const jsonString = (jsonMatch && jsonMatch[1] !== undefined) ? jsonMatch[1] : resultText;
                return JSON.parse(jsonString.trim());
            } catch (err) {
                logger.error({ err, rawResponse: resultText }, "[LlmExecutor] Failed to parse expected JSON");
                throw new Error("Failed to parse JSON response from agent");
            }
        }

        return { response: resultText };
    }
}
