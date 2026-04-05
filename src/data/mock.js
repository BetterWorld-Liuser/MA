export const mockWorkspace = {
    title: '默认任务',
    tasks: [
        { id: 'task-1', name: '重构认证模块', status: 'active', updatedAt: '14:32' },
        { id: 'task-2', name: '添加支付集成', status: 'idle', updatedAt: '11:08' },
        { id: 'task-3', name: '修复登录 bug', status: 'running', updatedAt: '09:41' },
    ],
    activeTaskId: 'task-1',
    selectedModel: 'claude-sonnet-4-6',
    workingDirectory: 'D:/playground/MA',
    chat: [
        {
            role: 'user',
            author: 'User',
            time: '14:32',
            content: '帮我把 auth 模块拆成更小的单元。',
        },
        {
            role: 'assistant',
            author: 'March',
            time: '14:32',
            content: '好的，我先看一下现有结构，然后把依赖边界切开。',
            tools: [
                { label: 'open_file', summary: 'src/auth.rs' },
                { label: 'replace_lines', summary: '12-30' },
                { label: 'reply', summary: '发送了用户可见消息' },
            ],
        },
        {
            role: 'assistant',
            author: 'March',
            time: '14:33',
            content: '已完成，auth 模块现在拆成了三个文件，接口层更清晰了。',
        },
    ],
    notes: [
        { id: 'target', content: '当前目标：拆分 auth 模块' },
        { id: 'plan', content: '1. 读现有结构 2. 拆接口层 3. 补测试' },
    ],
    openFiles: [
        { path: 'src/auth.rs', tokenUsage: '2.8k', freshness: 'high', locked: false },
        { path: 'src/lib.rs', tokenUsage: '1.9k', freshness: 'high', locked: false },
        { path: 'src/models.rs', tokenUsage: '0.9k', freshness: 'medium', locked: false },
        { path: 'config/prod.toml', tokenUsage: '0.3k', freshness: 'low', locked: true },
    ],
    hints: [
        { source: 'Telegram', content: 'foo: 部署好了吗？', timeLeft: '4m32s', turnsLeft: '3轮' },
        { source: 'CI', content: 'main 构建失败 exit 1', timeLeft: '12m08s', turnsLeft: '1轮' },
    ],
    skills: [
        {
            name: 'rust',
            path: '~/.agent/skills/rust/SKILL.md',
            description: 'Rust 项目工作流',
            opened: true,
        },
        {
            name: 'api-style',
            path: './.march/skills/api-style/SKILL.md',
            description: '本项目 API 风格约定',
            opened: false,
        },
    ],
    contextUsage: {
        percent: 42,
        current: '10.2k',
        limit: '128k',
        sections: [
            { name: '文件', size: '6.1k' },
            { name: '笔记', size: '0.8k' },
            { name: '提示', size: '0.1k' },
            { name: '对话', size: '2.1k' },
            { name: '系统', size: '1.2k' },
        ],
    },
    debugRounds: [
        {
            iteration: 1,
            contextPreview: '# Open Files\nsrc/auth.rs\n\n# Recent Chat\nUser: 帮我把 auth 模块拆成更小的单元。',
            providerRequestJson: '{\n  "model": "gpt-5",\n  "messages": [],\n  "tools": []\n}',
            providerResponseJson: '{\n  "choices": [\n    {\n      "message": {\n        "tool_calls": [\n          {\n            "id": "call_1",\n            "function": {\n              "name": "open_file",\n              "arguments": "{\\"path\\":\\"src/auth.rs\\"}"\n            }\n          }\n        ]\n      }\n    }\n  ]\n}',
            providerResponseRaw: '{\n  "choices": [\n    {\n      "message": {\n        "tool_calls": [\n          {\n            "id": "call_1",\n            "function": {\n              "name": "open_file",\n              "arguments": "{\\"path\\":\\"src/auth.rs\\"}"\n            }\n          }\n        ]\n      }\n    }\n  ]\n}',
            toolCalls: [
                {
                    id: 'call_1',
                    name: 'open_file',
                    argumentsJson: '{"path":"src/auth.rs"}',
                },
            ],
            toolResults: ['opened D:/playground/MA/src/auth.rs'],
        },
    ],
};
export function toWorkspaceView(snapshot) {
    const workspace = snapshot;
    const activeTask = workspace.active_task;
    const activeTaskId = activeTask ? String(activeTask.task.id) : '';
    return {
        title: activeTask?.task.name ?? 'March',
        workspacePath: workspace.workspace_path,
        databasePath: workspace.database_path,
        tasks: workspace.tasks.map((task) => ({
            id: String(task.id),
            name: task.name,
            status: String(task.id) === activeTaskId ? 'active' : 'idle',
            updatedAt: formatRelativeTime(task.last_active),
        })),
        activeTaskId,
        workingDirectory: activeTask?.task.working_directory
            ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.working_directory
            ?? workspace.workspace_path,
        selectedModel: activeTask?.task.selected_model ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.selected_model ?? undefined,
        chat: activeTask?.history.map((turn) => ({
            role: turn.role === 'User' ? 'user' : 'assistant',
            author: turn.role === 'User' ? 'User' : 'March',
            time: formatTime(turn.timestamp),
            content: turn.content,
            tools: turn.tool_summaries.map((tool) => ({
                label: tool.name,
                summary: tool.summary,
            })),
        })) ?? [],
        notes: activeTask?.notes ?? [],
        openFiles: activeTask?.open_files.map((file) => ({
            path: normalizePath(file.path),
            tokenUsage: formatOpenFileTokenUsage(file.snapshot),
            freshness: file.locked ? 'low' : file.snapshot ? 'high' : 'medium',
            locked: file.locked,
        })) ?? [],
        hints: activeTask?.hints.map((hint, index) => ({
            source: `Hint ${index + 1}`,
            content: hint.content,
            timeLeft: formatHintTime(hint.expires_at),
            turnsLeft: hint.turns_remaining ? `${hint.turns_remaining}轮` : '∞',
        })) ?? [],
        skills: activeTask?.runtime?.skills.map((skill) => ({
            name: skill.name,
            path: normalizePath(skill.path),
            description: skill.description,
            opened: skill.opened,
        })) ?? [],
        contextUsage: formatContextUsage(activeTask?.runtime?.context_usage),
        debugRounds: activeTask?.debug_trace?.rounds.map((round) => ({
            iteration: round.iteration,
            contextPreview: round.context_preview,
            providerRequestJson: round.provider_request_json,
            providerResponseJson: round.provider_response_json,
            providerResponseRaw: round.provider_response_raw,
            toolCalls: round.tool_calls.map((toolCall) => ({
                id: toolCall.id,
                name: toolCall.name,
                argumentsJson: toolCall.arguments_json,
            })),
            toolResults: round.tool_results,
        })) ?? [],
    };
}
function formatTime(timestamp) {
    return new Date(timestamp * 1000).toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
    });
}
function formatRelativeTime(timestamp) {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const diffSeconds = Math.max(0, nowSeconds - timestamp);
    const minutes = Math.floor(diffSeconds / 60);
    if (minutes < 1) {
        return '刚刚';
    }
    if (minutes < 60) {
        return `${minutes} 分`;
    }
    const hours = Math.floor(minutes / 60);
    if (hours < 24) {
        return `${hours} 小时`;
    }
    const days = Math.floor(hours / 24);
    return `${days} 天`;
}
function normalizePath(path) {
    const normalized = path.replaceAll('\\', '/');
    if (normalized.startsWith('//?/UNC/')) {
        return `//${normalized.slice('//?/UNC/'.length)}`;
    }
    if (normalized.startsWith('//?/')) {
        return normalized.slice('//?/'.length);
    }
    return normalized;
}
function formatOpenFileTokenUsage(snapshot) {
    if (!snapshot) {
        return '0';
    }
    if ('Available' in snapshot) {
        return formatTokenCount(estimateTokenCount(snapshot.Available.content));
    }
    return formatTokenCount(8);
}
function formatHintTime(expiresAt) {
    if (!expiresAt) {
        return 'no ttl';
    }
    const seconds = Math.max(0, expiresAt - Math.floor(Date.now() / 1000));
    const minutes = Math.floor(seconds / 60);
    const remainder = seconds % 60;
    return `${minutes}m${String(remainder).padStart(2, '0')}s`;
}
function formatContextUsage(usage) {
    if (!usage) {
        return mockWorkspace.contextUsage;
    }
    return {
        percent: usage.used_percent,
        current: formatTokenCount(usage.used_tokens),
        limit: formatTokenCount(usage.budget_tokens),
        sections: usage.sections.map((section) => ({
            name: section.name,
            size: formatTokenCount(section.tokens),
        })),
    };
}
function formatTokenCount(tokens) {
    if (tokens >= 1000) {
        return `${(tokens / 1000).toFixed(1)}k`;
    }
    return `${tokens}`;
}
function estimateTokenCount(text) {
    let asciiChars = 0;
    let nonAsciiChars = 0;
    for (const char of text) {
        if (char.charCodeAt(0) <= 0x7f) {
            asciiChars += 1;
        }
        else {
            nonAsciiChars += 1;
        }
    }
    return Math.ceil(asciiChars / 4) + nonAsciiChars;
}
