import { Type } from '@earendil-works/pi-ai';
import { logger } from '../../../utils/logger.js';

export class InputNeededError extends Error {
    constructor(public question: string) {
        super("Input needed");
    }
}

export function createAskUserTool(onAsk: (question: string) => void) {
    return {
        name: "ask_user",
        description: "ask the user for additional information or clarification when needed",
        parameters: Type.Object({
            question: Type.String({
                description: "The question or description of what is needed from the user"
            })
        }),
        execute: async (toolCallId: string, { question }: { question: string }) => {
            logger.info(`[AskUserTool] ask_user called with question: ${question}`);
            onAsk(question);
            throw new InputNeededError(question);
        }
    };
}
