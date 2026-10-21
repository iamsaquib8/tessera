//! Adds Tessera to one config file at a time, keeping everything already
//! there. A file that cannot be changed safely is left alone, and the caller
//! shows the snippet to add by hand.

use std::path::Path;

use serde_json::{json, Value};
use yaml_rust2::{Yaml, YamlLoader};

use super::{HookConfig, HookEvent, HookLayout, McpFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The file's new content.
    Write(String),
    /// Tessera is already set up there.
    Unchanged,
    /// Left alone: why, and what to add by hand.
    Manual { reason: String, snippet: String },
}

/// The MCP server command an agent should run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub command: String,
    pub args: Vec<String>,
}

pub fn mcp(format: McpFormat, existing: Option<&str>, server: &Server) -> Edit {
    let entry = json!({ "command": server.command, "args": server.args });
    match format {
        McpFormat::McpServers => json_entry(existing, "mcpServers", entry),
        McpFormat::AmpMcpServers => json_entry(existing, "amp.mcpServers", entry),
        McpFormat::ContextServers => json_entry(existing, "context_servers", entry),
        McpFormat::VsCodeServers => json_entry(
            existing,
            "servers",
            json!({ "type": "stdio", "command": server.command, "args": server.args }),
        ),
        McpFormat::OpenCode => {
            let mut command = vec![server.command.clone()];
            command.extend(server.args.iter().cloned());
            json_entry(
                existing,
                "mcp",
                json!({ "type": "local", "command": command, "enabled": true }),
            )
        }
        McpFormat::Toml => toml_entry(existing, server),
        McpFormat::ContinueYaml => match existing {
            Some(_) => Edit::Unchanged,
            None => {
                let text = continue_yaml(server);
                match yaml_document(&text) {
                    Some(Yaml::Hash(_)) => Edit::Write(text),
                    _ => manual("Tessera could not write valid YAML for it", text),
                }
            }
        },
        McpFormat::GooseYaml => goose_entry(existing, server),
    }
}

fn manual(reason: impl Into<String>, snippet: String) -> Edit {
    Edit::Manual {
        reason: reason.into(),
        snippet,
    }
}

/// Parses `existing` as a JSON object (an absent or empty file is `{}`).
fn json_object(existing: Option<&str>) -> Result<Value, &'static str> {
    let text = existing.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err("its top level is not a JSON object"),
        Err(_) => Err("it is not plain JSON (it may have comments or trailing commas)"),
    }
}

/// `{key: {"tessera": entry}}` merged into the file's top-level object.
fn json_entry(existing: Option<&str>, key: &str, entry: Value) -> Edit {
    let snippet = pretty(&json!({ key: { "tessera": entry } }), "  ");
    let mut root = match json_object(existing) {
        Ok(root) => root,
        Err(reason) => return manual(reason, snippet),
    };
    let servers = root
        .as_object_mut()
        .expect("checked above")
        .entry(key)
        .or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return manual(format!("its `{key}` is not an object"), snippet);
    };
    if servers.contains_key("tessera") {
        return Edit::Unchanged;
    }
    servers.insert("tessera".to_string(), entry);
    Edit::Write(pretty(&root, &indent_of(existing.unwrap_or_default())))
}

/// The indent the file already uses, so a rewrite only adds lines.
fn indent_of(text: &str) -> String {
    text.lines()
        .find_map(|line| {
            let indent: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            (!indent.is_empty() && line.len() > indent.len()).then_some(indent)
        })
        .unwrap_or_else(|| "  ".to_string())
}

fn pretty(value: &Value, indent: &str) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(value, &mut serializer).expect("JSON values serialize");
    let mut text = String::from_utf8(out).expect("JSON is UTF-8");
    text.push('\n');
    text
}

/// A TOML basic string; JSON string syntax is valid TOML.
fn quote(text: &str) -> String {
    serde_json::to_string(text).expect("strings serialize")
}

/// `[mcp_servers.tessera]` appended, so the rest of the file is kept byte for
/// byte. The file is parsed to see what is already there, and the result is
/// parsed again: anything that would not come back as the same settings
/// plus Tessera's entry is left for the user to add by hand.
fn toml_entry(existing: Option<&str>, server: &Server) -> Edit {
    let text = existing.unwrap_or_default();
    let args: Vec<String> = server.args.iter().map(|a| quote(a)).collect();
    let block = format!(
        "[mcp_servers.tessera]\ncommand = {}\nargs = [{}]\n",
        quote(&server.command),
        args.join(", ")
    );
    let Ok(before) = text.parse::<toml::Table>() else {
        return manual("it is not valid TOML", block);
    };
    match before.get("mcp_servers") {
        Some(toml::Value::Table(servers)) if servers.contains_key("tessera") => {
            return Edit::Unchanged
        }
        Some(toml::Value::Table(_)) | None => {}
        Some(_) => return manual("its `mcp_servers` is not a table", block),
    }
    let mut out = text.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&block);
    let Ok(mut after) = out.parse::<toml::Table>() else {
        return manual("Tessera's table cannot be appended to it", block);
    };
    let added = match after.get_mut("mcp_servers") {
        Some(toml::Value::Table(servers)) => servers.remove("tessera").is_some(),
        _ => false,
    };
    if after
        .get("mcp_servers")
        .is_some_and(|s| s.as_table().is_some_and(|t| t.is_empty()))
        && !before.contains_key("mcp_servers")
    {
        after.remove("mcp_servers");
    }
    if !added || after != before {
        return manual("Tessera's table cannot be appended to it", block);
    }
    Edit::Write(out)
}

/// Plain YAML when the value needs no quoting, a double-quoted string otherwise.
fn yaml(text: &str) -> String {
    let plain = text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c));
    if plain {
        text.to_string()
    } else {
        quote(text)
    }
}

fn continue_yaml(server: &Server) -> String {
    let mut out = format!(
        "name: Tessera\nversion: 0.0.1\nschema: v1\nmcpServers:\n  - name: tessera\n    command: {}\n    args:\n",
        yaml(&server.command)
    );
    for arg in &server.args {
        out.push_str(&format!("      - {}\n", yaml(arg)));
    }
    out
}

fn goose_block(server: &Server, indent: &str) -> String {
    let inner = format!("{indent}{indent}");
    let mut out = format!(
        "{indent}tessera:\n{inner}name: tessera\n{inner}type: stdio\n{inner}cmd: {}\n{inner}args:\n",
        yaml(&server.command)
    );
    for arg in &server.args {
        out.push_str(&format!("{inner}- {}\n", yaml(arg)));
    }
    out.push_str(&format!("{inner}enabled: true\n{inner}timeout: 300\n"));
    out
}

/// Goose's `extensions:` map with Tessera's entry added as lines, so the
/// rest of the file is kept. Like the TOML edit, the file is parsed first
/// and the result again, and is left alone unless it comes back as the same
/// settings plus Tessera's entry.
fn goose_entry(existing: Option<&str>, server: &Server) -> Edit {
    let text = existing.unwrap_or_default();
    let snippet = format!("extensions:\n{}", goose_block(server, "  "));
    let Some(before) = yaml_document(text) else {
        return manual("it is not a YAML mapping Tessera can read", snippet);
    };
    let tessera = Yaml::String("tessera".to_string());
    let extensions = Yaml::String("extensions".to_string());
    if let Some(Yaml::Hash(map)) = before.as_hash().and_then(|top| top.get(&extensions)) {
        if map.contains_key(&tessera) {
            return Edit::Unchanged;
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let out = match lines.iter().position(|line| extensions_line(line)) {
        None => {
            let mut out = text.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&snippet);
            out
        }
        Some(at) => {
            let indent: String = lines[at + 1..]
                .iter()
                .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                .map(|line| line.chars().take_while(|c| *c == ' ').collect())
                .filter(|indent: &String| !indent.is_empty())
                .unwrap_or_else(|| "  ".to_string());
            let mut out: Vec<String> = lines[..=at].iter().map(|l| l.to_string()).collect();
            out.push(goose_block(server, &indent).trim_end().to_string());
            out.extend(lines[at + 1..].iter().map(|l| l.to_string()));
            out.join("\n") + "\n"
        }
    };
    let Some(Yaml::Hash(mut after)) = yaml_document(&out) else {
        return manual("Tessera's entry cannot be added to it safely", snippet);
    };
    let added = match after.get_mut(&extensions) {
        Some(Yaml::Hash(map)) => map.remove(&tessera).is_some(),
        _ => false,
    };
    // `extensions:` with nothing under it was null before Tessera's entry.
    if after
        .get(&extensions)
        .is_some_and(|e| e.as_hash().is_some_and(|map| map.is_empty()))
    {
        match before.as_hash().and_then(|top| top.get(&extensions)) {
            Some(value) => {
                after.insert(extensions.clone(), value.clone());
            }
            None => {
                after.remove(&extensions);
            }
        }
    }
    let unchanged = match &before {
        Yaml::Hash(top) => *top == after,
        _ => after.is_empty(),
    };
    if !added || !unchanged {
        return manual("Tessera's entry cannot be added to it safely", snippet);
    }
    Edit::Write(out)
}

/// The top-level `extensions:` key with its value on the following lines.
fn extensions_line(line: &str) -> bool {
    line.strip_prefix("extensions:").is_some_and(|rest| {
        let rest = rest.trim();
        rest.is_empty() || rest.starts_with('#')
    })
}

/// The one YAML document in `text`: a mapping, or `Null` when the file is
/// empty; `None` when it is not valid YAML or is something else.
fn yaml_document(text: &str) -> Option<Yaml> {
    let mut docs = YamlLoader::load_from_str(text).ok()?;
    match docs.len() {
        0 => Some(Yaml::Null),
        1 => match docs.remove(0) {
            doc @ (Yaml::Hash(_) | Yaml::Null) => Some(doc),
            _ => None,
        },
        _ => None,
    }
}

/// One handler for `event` in the agent's layout.
fn hook_entry(
    config: &HookConfig,
    event: &HookEvent,
    command: &str,
    scripts: Option<&Path>,
) -> Value {
    let command = match scripts {
        Some(dir) => dir.join(script_name(event)).to_string_lossy().into_owned(),
        None => format!("{command} hook {} {}", config.agent, event.event),
    };
    let handler = match config.layout {
        HookLayout::Nested | HookLayout::Antigravity => {
            json!({ "type": "command", "command": command, "timeout": event.seconds })
        }
        HookLayout::NestedMillis => {
            json!({ "type": "command", "command": command, "timeout": event.seconds * 1000 })
        }
        HookLayout::Copilot => {
            json!({ "type": "command", "command": command, "timeoutSec": event.seconds })
        }
        HookLayout::Cursor => json!({ "command": command, "timeout": event.seconds }),
    };
    let grouped = match config.layout {
        HookLayout::Nested | HookLayout::NestedMillis => true,
        HookLayout::Antigravity => event.matcher.is_some(),
        HookLayout::Copilot | HookLayout::Cursor => false,
    };
    if !grouped {
        return handler;
    }
    let mut group = serde_json::Map::new();
    if let Some(matcher) = event.matcher {
        group.insert("matcher".to_string(), json!(matcher));
    }
    group.insert("hooks".to_string(), json!([handler]));
    Value::Object(group)
}

/// Tessera's hooks added to an agent's hook file, next to whatever hooks
/// are already there.
pub fn hooks(
    existing: Option<&str>,
    config: &HookConfig,
    command: &str,
    scripts: Option<&Path>,
) -> Edit {
    let (wrapper, versioned) = match config.layout {
        HookLayout::Antigravity => ("tessera", false),
        HookLayout::Copilot | HookLayout::Cursor => ("hooks", true),
        HookLayout::Nested | HookLayout::NestedMillis => ("hooks", false),
    };
    let marker = format!("hook {} ", config.agent);
    let snippet = {
        let mut events = serde_json::Map::new();
        for event in config.events {
            events.insert(
                event.name.to_string(),
                json!([hook_entry(config, event, command, scripts)]),
            );
        }
        let mut root = serde_json::Map::new();
        if versioned {
            root.insert("version".to_string(), json!(1));
        }
        root.insert(wrapper.to_string(), Value::Object(events));
        pretty(&Value::Object(root), "  ")
    };
    let mut root = match json_object(existing) {
        Ok(root) => root,
        Err(reason) => return manual(reason, snippet),
    };
    let object = root.as_object_mut().expect("checked above");
    if versioned && !object.contains_key("version") {
        object.insert("version".to_string(), json!(1));
    }
    let events = object.entry(wrapper).or_insert_with(|| json!({}));
    let Some(events) = events.as_object_mut() else {
        return manual(format!("its `{wrapper}` is not an object"), snippet);
    };
    let mut added = false;
    for event in config.events {
        let list = events.entry(event.name).or_insert_with(|| json!([]));
        let Some(list) = list.as_array_mut() else {
            return manual(
                format!("its `{wrapper}.{}` is not a list", event.name),
                snippet,
            );
        };
        let ours = list.iter().any(|entry| {
            let text = entry.to_string();
            text.contains("tessera")
                && (text.contains(&marker) || text.contains(&script_name(event)))
        });
        if !ours {
            list.push(hook_entry(config, event, command, scripts));
            added = true;
        }
    }
    if !added {
        return Edit::Unchanged;
    }
    Edit::Write(pretty(&root, &indent_of(existing.unwrap_or_default())))
}

/// The wrapper script for `event`, for agents whose hooks run scripts.
pub fn script_name(event: &HookEvent) -> String {
    format!("tessera-{}.sh", event.event)
}

/// A wrapper script that runs `tessera hook <agent> <event>` with the hook's
/// input; an existing script that does not run Tessera is left alone.
pub fn hook_script(existing: Option<&str>, command: &str, agent: &str, event: &HookEvent) -> Edit {
    let run = format!("exec {command} hook {agent} {}", event.event);
    let script = format!(
        "#!/bin/sh\n# Added by `tessera init`: {agent} runs hook scripts, so this one runs Tessera's.\n{run}\n"
    );
    match existing {
        None => Edit::Write(script),
        Some(text) if text.contains(&run) => Edit::Unchanged,
        Some(text) if text.contains("Added by `tessera init`") => Edit::Write(script),
        Some(_) => manual("a script of that name already exists", script),
    }
}

pub const INSTRUCTIONS: &str = "## Code graph (Tessera)\n\n\
This repository is indexed by Tessera. Before you rename a function or change \
its signature or behaviour, run `tessera blast <symbol>` (or the `blast` MCP \
tool) to see every caller and the tests that reach it. After editing, run \
`tessera verify --changed` and fix what it reports before you say you are done.\n";

const START: &str = "<!-- tessera:start -->";
const END: &str = "<!-- tessera:end -->";

/// The instructions inside markers, so a rerun replaces only Tessera's part.
pub fn markdown_block(existing: Option<&str>) -> Edit {
    let block = format!("{START}\n{INSTRUCTIONS}{END}\n");
    let Some(text) = existing else {
        return Edit::Write(block);
    };
    if let (Some(start), Some(end)) = (text.find(START), text.find(END)) {
        if start < end {
            let end = end + END.len();
            let end = if text[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            let updated = format!("{}{block}{}", &text[..start], &text[end..]);
            return if updated == text {
                Edit::Unchanged
            } else {
                Edit::Write(updated)
            };
        }
    }
    let mut out = text.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&block);
    Edit::Write(out)
}

/// `@AGENTS.md` in an existing `CLAUDE.md`, which Claude Code reads instead
/// of `AGENTS.md`.
pub fn claude_import(existing: Option<&str>) -> Edit {
    let Some(text) = existing else {
        return Edit::Unchanged;
    };
    if text.lines().any(|line| line.trim() == "@AGENTS.md") {
        return Edit::Unchanged;
    }
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n@AGENTS.md\n");
    Edit::Write(out)
}

pub fn cursor_rule(existing: Option<&str>) -> Edit {
    match existing {
        Some(_) => Edit::Unchanged,
        None => Edit::Write(format!(
            "---\ndescription: Check callers before edits and verify after them with Tessera\nalwaysApply: true\n---\n\n{INSTRUCTIONS}"
        )),
    }
}

pub const PRE_COMMIT_LINE: &str =
    "tessera verify --staged --format agent --quiet; [ $? -ne 1 ] || exit 1";

/// A pre-commit hook that refuses commits with blocking findings; an
/// existing hook is never replaced.
pub fn pre_commit(existing: Option<&str>) -> Edit {
    match existing {
        Some(text) if text.contains("tessera") && text.contains("verify") => Edit::Unchanged,
        Some(_) => manual(
            "a pre-commit hook already exists; add this line to it",
            PRE_COMMIT_LINE.to_string(),
        ),
        None => Edit::Write(format!(
            "#!/bin/sh\n# Added by `tessera init --git-hook`: refuses a commit that breaks call sites or\n# imports. Skip it once with `git commit --no-verify`.\ncommand -v tessera >/dev/null 2>&1 || exit 0\n{PRE_COMMIT_LINE}\n"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            command: "tessera".into(),
            args: vec!["mcp".into()],
        }
    }

    #[test]
    fn json_keeps_key_order_and_indent() {
        let before = "{\n    \"z\": 1,\n    \"a\": 2\n}\n";
        let Edit::Write(after) = mcp(McpFormat::McpServers, Some(before), &server()) else {
            panic!()
        };
        assert!(
            after.starts_with("{\n    \"z\": 1,\n    \"a\": 2,\n    \"mcpServers\""),
            "{after}"
        );
        assert_eq!(
            mcp(McpFormat::McpServers, Some(&after), &server()),
            Edit::Unchanged
        );
    }

    #[test]
    fn toml_appends_once() {
        let Edit::Write(after) = mcp(McpFormat::Toml, Some("model = \"x\""), &server()) else {
            panic!()
        };
        assert_eq!(
            after,
            "model = \"x\"\n\n[mcp_servers.tessera]\ncommand = \"tessera\"\nargs = [\"mcp\"]\n"
        );
        assert_eq!(
            mcp(McpFormat::Toml, Some(&after), &server()),
            Edit::Unchanged
        );
    }

    #[test]
    fn toml_already_set_up_in_any_spelling_is_left_alone() {
        for existing in [
            "[mcp_servers]\ntessera = { command = \"tessera\", args = [\"mcp\"] }\n",
            "mcp_servers = { tessera = { command = \"tessera\" } }\n",
            "[ mcp_servers.tessera ]\ncommand = \"tessera\"\n",
            "[mcp_servers.'tessera']\ncommand = \"tessera\"\n",
        ] {
            assert_eq!(
                mcp(McpFormat::Toml, Some(existing), &server()),
                Edit::Unchanged,
                "{existing}"
            );
        }
    }

    #[test]
    fn toml_output_is_valid_or_the_file_is_left_alone() {
        let Edit::Write(after) = mcp(
            McpFormat::Toml,
            Some("[mcp_servers]\nother = { command = \"o\" }\n"),
            &server(),
        ) else {
            panic!()
        };
        let table: toml::Table = after.parse().unwrap();
        assert!(table["mcp_servers"]["other"].is_table(), "{after}");
        assert_eq!(
            table["mcp_servers"]["tessera"]["command"].as_str(),
            Some("tessera")
        );
        for existing in [
            "mcp_servers = { other = { command = \"o\" } }\n",
            "mcp_servers = \"nope\"\n",
            "model = \n",
        ] {
            assert!(
                matches!(
                    mcp(McpFormat::Toml, Some(existing), &server()),
                    Edit::Manual { .. }
                ),
                "{existing}"
            );
        }
    }

    #[test]
    fn goose_finds_extensions_with_a_comment_and_never_writes_bad_yaml() {
        let before = "extensions:  # mine\n  developer:\n    enabled: true\n";
        let Edit::Write(after) = mcp(McpFormat::GooseYaml, Some(before), &server()) else {
            panic!()
        };
        assert_eq!(after.matches("extensions:").count(), 1, "{after}");
        let docs = yaml_rust2::YamlLoader::load_from_str(&after).unwrap();
        let extensions = &docs[0]["extensions"];
        assert!(!extensions["developer"].is_badvalue(), "{after}");
        assert_eq!(
            extensions["tessera"]["cmd"].as_str(),
            Some("tessera"),
            "{after}"
        );
        for existing in ["extensions: {}\n", "extensions: [\n"] {
            assert!(
                matches!(
                    mcp(McpFormat::GooseYaml, Some(existing), &server()),
                    Edit::Manual { .. }
                ),
                "{existing}"
            );
        }
    }

    #[test]
    fn goose_follows_the_existing_indent() {
        let before = "GOOSE_MODEL: x\nextensions:\n    developer:\n        enabled: true\n";
        let Edit::Write(after) = mcp(McpFormat::GooseYaml, Some(before), &server()) else {
            panic!()
        };
        assert!(
            after.contains("extensions:\n    tessera:\n        name: tessera\n"),
            "{after}"
        );
        assert!(
            after.ends_with("    developer:\n        enabled: true\n"),
            "{after}"
        );
        assert_eq!(
            mcp(McpFormat::GooseYaml, Some(&after), &server()),
            Edit::Unchanged
        );
    }

    #[test]
    fn markdown_block_is_replaced_in_place() {
        let Edit::Write(first) = markdown_block(Some("# A\n")) else {
            panic!()
        };
        assert_eq!(markdown_block(Some(&first)), Edit::Unchanged);
        let edited = first.replace("tessera blast", "old text");
        let Edit::Write(again) = markdown_block(Some(&edited)) else {
            panic!()
        };
        assert_eq!(again, first);
    }

    #[test]
    fn yaml_quotes_only_when_needed() {
        assert_eq!(yaml("tessera"), "tessera");
        assert_eq!(yaml("--db"), "\"--db\"");
        assert_eq!(yaml("${workspaceFolder}"), "\"${workspaceFolder}\"");
    }
}
