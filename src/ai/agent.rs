use crate::ai::chat::{ChatState, ChatStatus, Message, MessagePart, Role};
use crate::ai::providers::{AiProvider, ProviderMessage, ProviderContent, ContentPart, StreamChunk, ToolDef, FunctionDef};
use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

const SYSTEM_PROMPT: &str = r#"You are Termia, an AI coding assistant running inside a desktop terminal application.

You have access to tools. Work step by step — explain reasoning before making changes.

File editing:
- Use read_file first to understand the current state
- Use edit for targeted changes (preferred over rewriting entire files)
- Match existing code style and conventions

Shell commands:
- Use bash_run for single commands; bash_background for long-running ones
- Prefer non-interactive flags (--yes for npx, -y for cargo)
- Check exit codes and error output
- Never run destructive commands (rm -rf, force push, drop table) without confirmation

Searching:
- Use grep for exact symbol/string searches
- Use glob for finding files by pattern
- Read files before editing them

Keep responses clear. Use markdown for code blocks.
Context: the project root is available as <env> block in user messages."#;

fn build_tools() -> Vec<ToolDef> {
    vec![
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "read_file".into(), description: "Read file contents at path".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string","description":"Absolute path"}},"required":["path"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "write_file".into(), description: "Write/overwrite file at path".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "edit".into(), description: "Replace old_string with new_string in file".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}},"required":["path","old_string","new_string"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "bash_run".into(), description: "Run a shell command, return stdout+stderr+exit_code".into(),
            parameters: json!({"type":"object","properties":{"command":{"type":"string","description":"Shell command to run"},"description":{"type":"string","description":"What this command does (for approval)"}},"required":["command"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "grep".into(), description: "Search for pattern in files (uses ripgrep)".into(),
            parameters: json!({"type":"object","properties":{"pattern":{"type":"string","description":"Regex pattern"},"path":{"type":"string","description":"Directory or file to search (default: .)"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "glob".into(), description: "Find files matching pattern (e.g. '**/*.rs')".into(),
            parameters: json!({"type":"object","properties":{"pattern":{"type":"string","description":"Glob pattern"},"path":{"type":"string","description":"Directory to search in (default: .)"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: FunctionDef {
            name: "list_directory".into(), description: "List files and directories at path".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string","description":"Directory path"}},"required":["path"]}),
        }},
    ]
}

pub struct AgentRunner {
    provider: Arc<dyn AiProvider>,
    model: String,
    max_steps: usize,
    workspace_root: PathBuf,
}

impl AgentRunner {
    pub fn new(provider: Arc<dyn AiProvider>, model: String, workspace_root: PathBuf) -> Self {
        Self { provider, model, max_steps: 10, workspace_root }
    }

    pub async fn run(&self, mut state: ChatState, user_message: String) -> Result<ChatState> {
        // Inject env context into user message
        let enriched = self.enrich_message(user_message);

        state.messages.push(Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::User,
            parts: vec![MessagePart::Text { text: enriched }],
        });
        state.status = ChatStatus::Streaming;

        let tools = build_tools();
        let mut step = 0;

        while step < self.max_steps {
            step += 1;
            state.agent_meta.step = Some(format!("Step {}/{}", step, self.max_steps));

            let provider_messages = convert_messages(&state.messages);
            let mut rx = self.provider.stream_chat(&self.model, &provider_messages, Some(SYSTEM_PROMPT), &tools).await?;

            let mut text_buffer = String::new();
            let mut reasoning_buffer = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool: Option<ToolCall> = None;
            let mut current_args = String::new();

            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::TextDelta(t) => text_buffer.push_str(&t),
                    StreamChunk::ReasoningDelta(r) => reasoning_buffer.push_str(&r),
                    StreamChunk::ToolCallStart { id, name } => {
                        current_tool = Some(ToolCall { id, name, arguments: String::new() });
                    }
                    StreamChunk::ToolCallArgs(args) => current_args.push_str(&args),
                    StreamChunk::ToolCallEnd { id } => {
                        if let Some(mut tc) = current_tool.take() {
                            tc.arguments = std::mem::take(&mut current_args);
                            if tc.id.is_empty() { tc.id = id; }
                            tool_calls.push(tc);
                        }
                    }
                    StreamChunk::Error(e) => { state.error = Some(e); state.status = ChatStatus::Error; return Ok(state); }
                    StreamChunk::Done(usage) => {
                        state.agent_meta.tokens.input_tokens += usage.input_tokens;
                        state.agent_meta.tokens.output_tokens += usage.output_tokens;
                        state.agent_meta.tokens.cached_input_tokens += usage.cached_input_tokens;
                        state.agent_meta.last_input_tokens = usage.input_tokens;
                        state.agent_meta.last_cached_tokens = usage.cached_input_tokens;
                    }
                }
            }

            let mut parts: Vec<MessagePart> = Vec::new();
            if !reasoning_buffer.is_empty() { parts.push(MessagePart::Reasoning { text: reasoning_buffer }); }
            if !text_buffer.is_empty() && tool_calls.is_empty() { parts.push(MessagePart::Text { text: text_buffer }); }

            if tool_calls.is_empty() {
                state.messages.push(Message { id: uid(), role: Role::Assistant, parts });
                break;
            }

            // Add tool parts to assistant message
            for tc in &tool_calls {
                parts.push(match tc.name.as_str() {
                    "read_file" => MessagePart::ToolReadFile {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::ReadFileInput { path: arg(&tc.arguments, "path") }), output: None,
                    },
                    "write_file" => MessagePart::ToolWriteFile {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::WriteFileInput {
                            path: arg(&tc.arguments, "path"), content: arg(&tc.arguments, "content"),
                        }), output: None,
                    },
                    "edit" => MessagePart::ToolEdit {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::EditInput {
                            path: arg(&tc.arguments, "path"),
                            old_string: arg(&tc.arguments, "old_string"),
                            new_string: arg(&tc.arguments, "new_string"),
                        }), output: None,
                    },
                    "bash_run" => MessagePart::ToolBash {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::BashInput {
                            command: arg(&tc.arguments, "command"),
                            description: if tc.arguments.contains("description") { Some(arg(&tc.arguments, "description")) } else { None },
                        }), output: None,
                    },
                    "grep" => MessagePart::ToolGrep {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::GrepInput {
                            pattern: arg(&tc.arguments, "pattern"),
                            path: Some(arg(&tc.arguments, "path")).filter(|s| !s.is_empty()),
                        }), output: None,
                    },
                    "glob" => MessagePart::ToolGlob {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::GlobInput { pattern: arg(&tc.arguments, "pattern") }), output: None,
                    },
                    "list_directory" => MessagePart::ToolReadFile {
                        tool_call_id: tc.id.clone(), state: crate::ai::chat::ToolState::Running,
                        input: Some(crate::ai::chat::ReadFileInput { path: arg(&tc.arguments, "path") }), output: None,
                    },
                    _ => MessagePart::Text { text: format!("Unknown tool: {}", tc.name) },
                });
            }

            state.messages.push(Message { id: uid(), role: Role::Assistant, parts: parts.clone() });

            // Execute tools
            for tc in &tool_calls {
                let result = execute_tool(&tc.name, &tc.arguments, &self.workspace_root);
                state.messages.push(Message { id: uid(), role: Role::System, parts: vec![MessagePart::Text { text: result }] });
            }

            // Context compaction
            state.messages = compact_messages(state.messages);
        }

        if step >= self.max_steps { state.agent_meta.hit_step_cap = true; }
        state.status = ChatStatus::Ready;
        state.agent_meta.step = None;
        Ok(state)
    }

    fn enrich_message(&self, msg: String) -> String {
        let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
        let root = self.workspace_root.display().to_string();
        let project_memory = read_project_memory(&self.workspace_root);

        let mut env = format!("<env>\nworkspace_root: {root}\nactive_terminal_cwd: {cwd}\n</env>\n\n");
        if !project_memory.is_empty() {
            env.push_str(&format!("<project_memory>\n{project_memory}\n</project_memory>\n\n"));
        }
        env.push_str(&msg);
        env
    }
}

#[derive(Debug, Clone)]
struct ToolCall { id: String, name: String, arguments: String }

fn uid() -> String { uuid::Uuid::new_v4().to_string() }
fn arg(args: &str, key: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args).ok()
        .and_then(|v| v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn convert_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages.iter().map(|m| {
        let parts: Vec<ContentPart> = m.parts.iter().filter_map(|p| match p {
            MessagePart::Text { text } => Some(ContentPart::Text { text: text.clone() }),
            _ => None,
        }).collect();
        let content = if parts.is_empty() { ProviderContent::Text(String::new()) } else { ProviderContent::Parts(parts) };
        ProviderMessage { role: m.role.as_str().to_string(), content }
    }).collect()
}

fn execute_tool(name: &str, args: &str, root: &PathBuf) -> String {
    match name {
        "read_file" | "list_directory" => {
            let path = resolve_path(&arg(args, "path"), root);
            if name == "list_directory" {
                match std::fs::read_dir(&path) {
                    Ok(entries) => entries.filter_map(|e| e.ok()).map(|e| {
                        let ft = e.file_type().map(|t| if t.is_dir() { "/" } else { "" }).unwrap_or("");
                        format!("{}{}", e.file_name().to_string_lossy(), ft)
                    }).collect::<Vec<_>>().join("\n"),
                    Err(e) => format!("Error: {e}"),
                }
            } else {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().collect();
                        if lines.len() > 200 {
                            format!("{} ({} lines total, showing first 200)\n{}", path, lines.len(), lines[..200].join("\n"))
                        } else { content }
                    }
                    Err(e) => format!("Error reading {}: {e}", path),
                }
            }
        }
        "write_file" => {
            let path = resolve_path(&arg(args, "path"), root);
            let content = arg(args, "content");
            if let Some(parent) = std::path::Path::new(&path).parent() { let _ = std::fs::create_dir_all(parent); }
            match std::fs::write(&path, &content) {
                Ok(()) => format!("Wrote {} bytes to {}", content.len(), path),
                Err(e) => format!("Error writing {}: {e}", path),
            }
        }
        "edit" => {
            let path = resolve_path(&arg(args, "path"), root);
            let old = arg(args, "old_string");
            let new = arg(args, "new_string");
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if !content.contains(&old) { return format!("Error: old_string not found in {}", path); }
                    let replaced = content.replacen(&old, &new, 1);
                    match std::fs::write(&path, &replaced) {
                        Ok(()) => format!("Edited {}", path),
                        Err(e) => format!("Error writing {}: {e}", path),
                    }
                }
                Err(e) => format!("Error reading {}: {e}", path),
            }
        }
        "bash_run" => {
            let cmd = arg(args, "command");
            match std::process::Command::new("bash").arg("-c").arg(&cmd)
                .current_dir(root).output()
            {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let mut result = String::new();
                    if !stdout.is_empty() { result.push_str(&format!("stdout:\n{stdout}")); }
                    if !stderr.is_empty() { result.push_str(&format!("stderr:\n{stderr}")); }
                    result.push_str(&format!("exit_code: {}", out.status.code().unwrap_or(-1)));
                    if result.is_empty() { "exit_code: 0".to_string() } else { result }
                }
                Err(e) => format!("Error running command: {e}"),
            }
        }
        "grep" => {
            let pattern = arg(args, "pattern");
            let path = resolve_path(&arg(args, "path"), root);
            // grep via rg or grep
            match std::process::Command::new("rg")
                .args(["--line-number", "--no-heading", "-n", &pattern])
                .arg(&path).output()
            {
                Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                Err(_) => match std::process::Command::new("grep").args(["-rn", &pattern]).arg(&path).output() {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                    Err(e) => format!("Error: {e}"),
                },
            }
        }
        "glob" => {
            let pattern = arg(args, "pattern");
            let path = resolve_path(&arg(args, "path"), root);
            let full = format!("{}/{}", path, pattern);
            match std::process::Command::new("bash").arg("-c").arg(format!("find {} -path '{}' -maxdepth 10 2>/dev/null | head -50", path, full)).output() {
                Ok(out) => {
                    let s = String::from_utf8_lossy(&out.stdout).to_string();
                    if s.is_empty() { "No files found".to_string() } else { s }
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        _ => format!("Unknown tool: {name}"),
    }
}

fn resolve_path(path: &str, root: &PathBuf) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() { path.to_string() } else { root.join(path).display().to_string() }
}

/// Read CLAUDE.md, AGENTS.md, or TERAX.md from workspace root
fn read_project_memory(root: &PathBuf) -> String {
    for name in &["CLAUDE.md", "AGENTS.md", "TERAX.md"] {
        let p = root.join(name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            let truncated: String = content.chars().take(16000).collect();
            return truncated;
        }
    }
    String::new()
}

/// Drop old tool results and system messages to keep context under ~64K chars
fn compact_messages(messages: Vec<Message>) -> Vec<Message> {
    const MAX_CHARS: usize = 48000;
    let total: usize = messages.iter().map(|m| m.parts.iter().map(|p| match p {
        MessagePart::Text { text } => text.len(),
        _ => 200,
    }).sum::<usize>()).sum();

    if total <= MAX_CHARS { return messages; }

    // Keep all user messages and the 3 most recent assistant messages; drop oldest system messages
    let mut kept: Vec<Message> = Vec::new();
    let mut sys_count = 0;
    let mut skipped = 0;

    for m in messages.into_iter().rev() {
        match m.role {
            Role::System => {
                sys_count += 1;
                if sys_count <= 6 { kept.push(m); } else { skipped += 1; }
            }
            _ => kept.push(m),
        }
    }
    kept.reverse();

    if skipped > 0 {
        kept.insert(1, Message {
            id: uid(), role: Role::System,
            parts: vec![MessagePart::Text { text: format!("[Context compacted: {skipped} tool results elided]") }],
        });
    }
    kept
}
