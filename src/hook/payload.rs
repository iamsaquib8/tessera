//! Reads whatever shape of hook payload arrived. Several agents run each
//! other's hook files (Cursor loads Claude hooks, Grok reads
//! `.claude/settings.json`), so the shape is detected, never assumed.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::Agent;

/// Tools that write files, across agents.
const EDIT_TOOLS: &[&str] = &[
    "edit",
    "write",
    "multiedit",
    "notebookedit",
    "apply_patch",
    "write_file",
    "replace",
    "edit_file",
    "create_file",
    "write_to_file",
    "replace_file_content",
    "multi_replace_file_content",
    "str_replace_editor",
    "str_replace_based_edit_tool",
    "create",
    "insert",
    "save_file",
    "remove_files",
];
const PATH_KEYS: &[&str] = &[
    "file_path",
    "filePath",
    "path",
    "notebook_path",
    "TargetFile",
    "target_file",
    "absolutePath",
];

#[derive(Debug, Clone, Default)]
pub struct Payload {
    pub session: String,
    pub cwd: Option<PathBuf>,
    pub tool: Option<String>,
    /// Files the tool writes, as given (absolute or relative to `cwd`).
    pub paths: Vec<String>,
    /// For string-replacing edits, the text being replaced.
    pub old_texts: Vec<String>,
    /// `None` when the agent does not report whether a stop hook is already
    /// continuing the turn.
    pub stop_hook_active: Option<bool>,
    /// Why the agent stopped, when it says: `completed`/`end_turn`, or an
    /// interruption or error.
    pub stop_cause: Option<String>,
    /// The agent reported changed files directly (Augment's `file_changes`).
    pub file_changes: bool,
    /// The client that sent the payload, when its fields tell.
    pub client: Option<Agent>,
    /// The client's own name for the event (`hook_event_name`).
    pub event_name: Option<String>,
    /// The client's id for the turn, when it sends one.
    pub turn: Option<String>,
}

impl Payload {
    pub fn parse(raw: &str) -> Self {
        let value: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
        let client = client(&value);
        // Cursor sends a `session_id` only at session start; its other events
        // carry the conversation id.
        let ids: &[&str] = if client == Some(Agent::Cursor) {
            &["conversation_id", "session_id"]
        } else {
            &[
                "session_id",
                "sessionId",
                "conversation_id",
                "conversationId",
            ]
        };
        let session = ids
            .iter()
            .find_map(|key| value[*key].as_str())
            .unwrap_or("default")
            .to_string();
        let cwd = value["cwd"]
            .as_str()
            .or_else(|| value["workspace_roots"][0].as_str())
            .or_else(|| value["workspaceRoots"][0].as_str())
            .or_else(|| value["workspacePaths"][0].as_str())
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from);
        let tool = value["tool_name"]
            .as_str()
            .or_else(|| value["toolName"].as_str())
            .or_else(|| value["toolCall"]["name"].as_str())
            .map(ToOwned::to_owned);
        let mut input = value["tool_input"].clone();
        for alternative in [
            &value["toolArgs"],
            &value["toolCall"]["args"],
            &value["args"],
        ] {
            if !input.is_null() {
                break;
            }
            input = match alternative {
                Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
                other => other.clone(),
            };
        }
        if input.is_null() && value["file_path"].is_string() {
            input = value.clone();
        }
        let mut paths = Vec::new();
        collect_paths(&input, &mut paths);
        let file_changes = match &value["file_changes"] {
            Value::Array(changes) if !changes.is_empty() => {
                collect_paths(&value["file_changes"], &mut paths);
                true
            }
            _ => false,
        };
        for patch in patches(&input) {
            paths.extend(patch_paths(&patch));
        }
        paths.sort();
        paths.dedup();
        let mut old_texts = Vec::new();
        for edit in std::iter::once(&input).chain(input["edits"].as_array().into_iter().flatten()) {
            if let Some(text) = edit["old_string"]
                .as_str()
                .or_else(|| edit["oldString"].as_str())
            {
                if !text.is_empty() {
                    old_texts.push(text.to_string());
                }
            }
        }
        Self {
            session,
            cwd,
            tool,
            paths,
            old_texts,
            stop_hook_active: value["stop_hook_active"]
                .as_bool()
                .or_else(|| value["loop_count"].as_u64().map(|count| count > 0)),
            stop_cause: ["status", "agent_stop_cause"]
                .iter()
                .find_map(|key| value[*key].as_str())
                .map(ToOwned::to_owned),
            file_changes,
            client,
            event_name: value["hook_event_name"]
                .as_str()
                .or_else(|| value["hookEventName"].as_str())
                .map(ToOwned::to_owned),
            turn: ["generation_id", "turn_id"]
                .iter()
                .find_map(|key| value[*key].as_str())
                .map(ToOwned::to_owned),
        }
    }

    /// Whether the tool edits files: a known edit tool, or any tool that
    /// names a file when the agent does not report the tool name.
    pub fn is_edit(&self) -> bool {
        if self.file_changes {
            return true;
        }
        match &self.tool {
            Some(tool) => EDIT_TOOLS.contains(&tool.to_lowercase().replace('-', "_").as_str()),
            None => !self.paths.is_empty(),
        }
    }

    /// The turn ended normally, not by an interruption or an error.
    pub fn finished(&self) -> bool {
        self.stop_cause
            .as_deref()
            .is_none_or(|cause| matches!(cause, "completed" | "end_turn"))
    }

    /// The edited files relative to `root`, dropping any outside it.
    pub fn paths_in(&self, root: &Path) -> Vec<String> {
        let base = self.cwd.clone().unwrap_or_else(|| root.to_path_buf());
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut out: Vec<String> = self
            .paths
            .iter()
            .filter_map(|path| {
                let absolute = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    base.join(path)
                };
                let absolute = canonical_or_parent(&absolute);
                absolute
                    .strip_prefix(&root)
                    .ok()
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Gemini's event names; Claude-shaped agents use `PreToolUse` and so on.
const GEMINI_EVENTS: &[&str] = &[
    "BeforeTool",
    "AfterTool",
    "BeforeAgent",
    "AfterAgent",
    "BeforeModel",
    "AfterModel",
    "BeforeToolSelection",
    "PreCompress",
];

/// The client that sent `value`, from fields only it sends (docs/agents.md),
/// since clients run each other's hook files: Cursor and Copilot CLI both
/// load `.claude/settings*.json`. `None` when nothing tells.
pub fn client(value: &Value) -> Option<Agent> {
    let has = |key: &str| !value[key].is_null();
    // Cursor and Augment both send these; Cursor's event names are camelCase.
    let workspace = has("conversation_id") && has("workspace_roots");
    let camel_event = value["hook_event_name"]
        .as_str()
        .and_then(|e| e.chars().next())
        .is_some_and(|c| c.is_ascii_lowercase());
    if has("cursor_version") || has("generation_id") || (workspace && camel_event) {
        return Some(Agent::Cursor);
    }
    if [
        "toolCall",
        "conversationId",
        "workspacePaths",
        "terminationReason",
        "fullyIdle",
    ]
    .iter()
    .any(|key| has(key))
    {
        return Some(Agent::Antigravity);
    }
    // Copilot CLI: camelCase for its own hook files, snake_case with an ISO
    // timestamp (and neither a transcript nor a permission mode, which Qwen
    // and Gemini send) for hooks it reads from Claude's files.
    let camel = ["toolName", "toolArgs", "toolResult", "stopReason"]
        .iter()
        .any(|key| has(key))
        || (has("sessionId") && value["timestamp"].is_number());
    let snake = has("tool_result")
        || has("stop_reason")
        || (value["timestamp"].is_string()
            && !has("transcript_path")
            && !has("permission_mode")
            && has("session_id"));
    if camel || snake {
        return Some(Agent::Copilot);
    }
    if value["hook_event_name"]
        .as_str()
        .is_some_and(|event| GEMINI_EVENTS.contains(&event))
    {
        return Some(Agent::Gemini);
    }
    if [
        "is_mcp_tool",
        "file_changes",
        "agent_stop_cause",
        "tool_error",
    ]
    .iter()
    .any(|key| has(key))
        || workspace
    {
        return Some(Agent::Augment);
    }
    if has("turn_id") {
        return Some(Agent::Codex);
    }
    None
}

/// A deleted file cannot be canonicalised; its parent can.
fn canonical_or_parent(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

fn collect_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match child {
                    Value::String(text) if PATH_KEYS.contains(&key.as_str()) => {
                        out.push(text.clone())
                    }
                    _ => collect_paths(child, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_paths(item, out)),
        _ => {}
    }
}

/// `apply_patch` bodies, wherever the agent put them.
fn patches(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut visit = |text: &str| {
        if text.contains("*** Begin Patch") {
            out.push(text.to_string());
        }
    };
    match input {
        Value::String(text) => visit(text),
        Value::Object(map) => {
            for child in map.values() {
                match child {
                    Value::String(text) => visit(text),
                    Value::Array(items) => {
                        items.iter().filter_map(Value::as_str).for_each(&mut visit)
                    }
                    _ => {}
                }
            }
        }
        Value::Array(items) => items.iter().filter_map(Value::as_str).for_each(visit),
        _ => {}
    }
    out
}

fn patch_paths(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|line| {
            [
                "*** Update File: ",
                "*** Add File: ",
                "*** Delete File: ",
                "*** Move to: ",
            ]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        })
        .map(|path| path.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_edit() {
        let p = Payload::parse(
            r#"{"session_id":"s","cwd":"/r","tool_name":"Edit","tool_input":{"file_path":"/r/a.ts","old_string":"x","new_string":"y"}}"#,
        );
        assert_eq!(p.session, "s");
        assert_eq!(p.paths, vec!["/r/a.ts"]);
        assert_eq!(p.old_texts, vec!["x"]);
        assert!(p.is_edit());
    }

    #[test]
    fn copilot_tool_args_are_a_json_string() {
        let p = Payload::parse(
            r#"{"sessionId":"c","toolName":"edit","toolArgs":"{\"path\":\"src/a.ts\"}"}"#,
        );
        assert_eq!(p.session, "c");
        assert_eq!(p.paths, vec!["src/a.ts"]);
    }

    #[test]
    fn antigravity_tool_call() {
        let p = Payload::parse(
            r#"{"toolCall":{"name":"write_to_file","args":{"TargetFile":"/r/b.ts"}}}"#,
        );
        assert_eq!(p.paths, vec!["/r/b.ts"]);
        assert!(p.is_edit());
    }

    #[test]
    fn codex_apply_patch() {
        let p = Payload::parse(
            r#"{"tool_name":"apply_patch","tool_input":{"command":["apply_patch","*** Begin Patch\n*** Update File: src/a.ts\n*** Add File: src/b.ts\n*** End Patch"]}}"#,
        );
        assert_eq!(p.paths, vec!["src/a.ts", "src/b.ts"]);
    }

    #[test]
    fn multi_edit_collects_every_replaced_text() {
        let p = Payload::parse(
            r#"{"tool_name":"MultiEdit","tool_input":{"file_path":"a.ts","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]},"stop_hook_active":false}"#,
        );
        assert_eq!(p.old_texts, vec!["a", "c"]);
        assert_eq!(p.stop_hook_active, Some(false));
    }

    #[test]
    fn the_client_is_told_by_its_own_fields() {
        let cases = [
            (
                r#"{"session_id":"s","transcript_path":"/t","cwd":"/r","permission_mode":"default","hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{},"tool_response":{},"tool_use_id":"t"}"#,
                None,
            ),
            (
                r#"{"session_id":"s","transcript_path":"/t","cwd":"/r","hook_event_name":"PostToolUse","timestamp":"2026-10-21T10:00:00Z","permission_mode":"default","tool_name":"edit"}"#,
                None,
            ),
            (
                r#"{"conversation_id":"c","generation_id":"g","cursor_version":"2.1","workspace_roots":["/r"],"hook_event_name":"postToolUse"}"#,
                Some(Agent::Cursor),
            ),
            (
                r#"{"sessionId":"c","timestamp":1,"cwd":"/r","toolName":"edit","toolArgs":"{}"}"#,
                Some(Agent::Copilot),
            ),
            (
                r#"{"hook_event_name":"PostToolUse","session_id":"c","timestamp":"2026-10-21T10:00:00Z","cwd":"/r","tool_name":"edit","tool_input":{},"tool_result":{"result_type":"success"}}"#,
                Some(Agent::Copilot),
            ),
            (
                r#"{"hook_event_name":"PreToolUse","session_id":"c","timestamp":"2026-10-21T10:00:00Z","cwd":"/r","tool_name":"edit","tool_input":{}}"#,
                Some(Agent::Copilot),
            ),
            (
                r#"{"session_id":"g","transcript_path":"/t","cwd":"/r","hook_event_name":"AfterTool","timestamp":"2026-10-21T10:00:00Z"}"#,
                Some(Agent::Gemini),
            ),
            (
                r#"{"conversation_id":"a","workspace_roots":["/r"],"hook_event_name":"PostToolUse","is_mcp_tool":false}"#,
                Some(Agent::Augment),
            ),
            (
                r#"{"conversationId":"v","workspacePaths":["/r"],"toolCall":{"name":"x"}}"#,
                Some(Agent::Antigravity),
            ),
            (
                r#"{"session_id":"x","turn_id":"t","cwd":"/r","hook_event_name":"Stop","model":"gpt"}"#,
                Some(Agent::Codex),
            ),
        ];
        for (raw, client) in cases {
            assert_eq!(Payload::parse(raw).client, client, "{raw}");
        }
    }

    #[test]
    fn reads_are_not_edits() {
        let p = Payload::parse(r#"{"tool_name":"Read","tool_input":{"file_path":"/r/a.ts"}}"#);
        assert!(!p.is_edit());
    }
}
