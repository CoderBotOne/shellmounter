use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single chat message part — mirrors the Vercel AI SDK UIMessagePart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "reasoning")]
    Reasoning { text: String },
    #[serde(rename = "tool-bash")]
    ToolBash {
        tool_call_id: String,
        state: ToolState,
        input: Option<BashInput>,
        output: Option<String>,
    },
    #[serde(rename = "tool-read_file")]
    ToolReadFile {
        tool_call_id: String,
        state: ToolState,
        input: Option<ReadFileInput>,
        output: Option<String>,
    },
    #[serde(rename = "tool-write_file")]
    ToolWriteFile {
        tool_call_id: String,
        state: ToolState,
        input: Option<WriteFileInput>,
        output: Option<String>,
    },
    #[serde(rename = "tool-edit")]
    ToolEdit {
        tool_call_id: String,
        state: ToolState,
        input: Option<EditInput>,
        output: Option<String>,
    },
    #[serde(rename = "tool-glob")]
    ToolGlob {
        tool_call_id: String,
        state: ToolState,
        input: Option<GlobInput>,
        output: Option<String>,
    },
    #[serde(rename = "tool-grep")]
    ToolGrep {
        tool_call_id: String,
        state: ToolState,
        input: Option<GrepInput>,
        output: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Pending,
    Running,
    #[serde(rename = "output-available")]
    OutputAvailable,
    Done,
    Error,
    #[serde(rename = "approval-requested")]
    ApprovalRequested,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashInput {
    pub command: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileInput {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileInput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditInput {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobInput {
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepInput {
    pub pattern: String,
    pub path: Option<String>,
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

/// Chat session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
}

/// The full chat state
#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub session_id: Option<String>,
    pub messages: Vec<Message>,
    pub status: ChatStatus,
    pub error: Option<String>,
    pub agent_meta: AgentMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatStatus {
    #[default]
    Ready,
    Submitted,
    Streaming,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct AgentMeta {
    pub step: Option<String>,
    pub hit_step_cap: bool,
    pub compaction_notice: Option<CompactionNotice>,
    pub tokens: TokenUsage,
    pub last_input_tokens: u64,
    pub last_cached_tokens: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CompactionNotice {
    pub dropped_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

/// An AI agent definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub model_id: Option<String>,
}

/// A model available for selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub context_limit: u64,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Composer state — files, snippets, commands attached to the prompt
#[derive(Debug, Clone, Default)]
pub struct ComposerState {
    pub value: String,
    pub files: Vec<AttachedFile>,
    pub snippets: Vec<AttachedSnippet>,
    pub commands: Vec<AttachedCommand>,
}

#[derive(Debug, Clone)]
pub struct AttachedFile {
    pub path: String,
    pub name: String,
    pub lines: usize,
}

#[derive(Debug, Clone)]
pub struct AttachedSnippet {
    pub id: String,
    pub handle: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct AttachedCommand {
    pub name: String,
    pub label: String,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            session_id: None,
            messages: Vec::new(),
            status: ChatStatus::Ready,
            error: None,
            agent_meta: AgentMeta::default(),
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.status, ChatStatus::Submitted | ChatStatus::Streaming)
    }

    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last()
    }
}
