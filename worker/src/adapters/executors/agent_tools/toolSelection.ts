import type { AvailableTool } from './ToolRegistry.js';

export type ToolSelectionResult = {
    approvedTools: AvailableTool[];
    unavailableApprovedToolNames: string[];
};

export function selectApprovedTools(availableTools: AvailableTool[], approvedToolNames: unknown): ToolSelectionResult {
    const toolNames = Array.isArray(approvedToolNames) ? approvedToolNames : ["_all_"];
    const approvedTools = toolNames.includes("_all_")
        ? availableTools
        : availableTools.filter((availableTool) => toolNames.includes(availableTool.name));
    const unavailableApprovedToolNames = toolNames
        .filter((toolName: unknown): toolName is string => typeof toolName === 'string')
        .filter((toolName: string) => toolName !== "_all_")
        .filter((toolName: string) => !approvedTools.some((approvedTool) => approvedTool.name === toolName));

    return { approvedTools, unavailableApprovedToolNames };
}
