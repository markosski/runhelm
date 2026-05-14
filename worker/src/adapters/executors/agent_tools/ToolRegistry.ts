export type AvailableTool = {
    name: string;
    description: string;
    tool: any;
};

export class ToolRegistry {
    private readonly tools = new Map<string, AvailableTool>();

    registerTool(tool: any): void {
        if (!tool || typeof tool !== 'object') {
            throw new Error('Tool must be an object');
        }
        if (typeof tool.name !== 'string' || tool.name.trim().length === 0) {
            throw new Error('Tool must have a non-empty string name');
        }
        if (typeof tool.description !== 'string' || tool.description.trim().length === 0) {
            throw new Error(`Tool ${tool.name} must have a non-empty string description`);
        }
        if (typeof tool.execute !== 'function') {
            throw new Error(`Tool ${tool.name} must have an execute function`);
        }

        this.tools.set(tool.name, {
            name: tool.name,
            description: tool.description,
            tool,
        });
    }

    registerTools(tools: any[]): void {
        for (const tool of tools) {
            this.registerTool(tool);
        }
    }

    getTools(): AvailableTool[] {
        return [...this.tools.values()];
    }
}
