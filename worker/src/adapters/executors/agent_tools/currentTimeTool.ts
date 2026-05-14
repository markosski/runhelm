import { Type } from '@earendil-works/pi-ai';
import { logger } from '../../../utils/logger.js';

export function createCurrentTimeTool() {
    return {
        name: "get_current_time",
        description: "Get the current UTC time and date. Use this to orient yourself in time.",
        label: "Current Time",
        parameters: Type.Object({}),
        execute: async (toolCallId: string, args: any, signal?: AbortSignal) => {
            logger.info(`[CurrentTimeTool] Executing get_current_time`);
            const now = new Date();
            const timeData = {
                iso: now.toISOString(),
                utc: now.toUTCString(),
                timestamp: now.getTime(),
                timezoneOffset: now.getTimezoneOffset()
            };
            return {
                content: [{ type: "text", text: `Current time is ${timeData.iso} (UTC).` }],
                details: timeData
            };
        }
    } as any;
}
