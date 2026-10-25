mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::{json, Value};
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn repo() -> TempDir {
    repo_with(&[])
}

/// The TypeScript fixture plus `extra` files, committed.
fn repo_with(extra: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
    for (path, content) in extra {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    }
    temp
}

/// Runs `tessera hook <agent> <event>` with `payload` on stdin; returns the
/// parsed stdout (Null when the hook printed nothing).
fn hook(agent: &str, event: &str, payload: Value) -> Value {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .args(["hook", agent, event])
        .write_stdin(payload.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"))
    }
}

fn claude(root: &Path, event: &str, extra: Value) -> Value {
    let mut payload = json!({
        "session_id": "s1",
        "cwd": root.to_string_lossy(),
        "hook_event_name": event,
    });
    payload
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    hook("claude", event, payload)
}

fn write(root: &Path, path: &str, content: &str) {
    fs::write(root.join(path), content).unwrap();
}

fn edit_payload(root: &Path, path: &str, old: &str, new: &str) -> Value {
    json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": root.join(path).to_string_lossy(),
            "old_string": old,
            "new_string": new,
        }
    })
}

#[test]
fn pre_edit_announces_cross_file_callers_once() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    let payload = edit_payload(
        root,
        "src/util/format.ts",
        "return name.trim();",
        "return name;",
    );
    let first = claude(root, "PreToolUse", payload.clone());
    let context = first["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default();
    assert!(context.contains("formatName"), "{first}");
    assert!(context.contains("src/users/repo.ts"), "{first}");
    assert!(context.split_whitespace().count() < 120, "{context}");
    assert!(
        first.get("decision").is_none(),
        "pre-edit never blocks: {first}"
    );
    assert_eq!(claude(root, "PreToolUse", payload), Value::Null);
}

#[test]
fn the_pre_edit_note_stays_within_its_budget() {
    let dir = "src/a_rather_deeply/nested_directory_structure/for_the_purposes_of/testing_notes";
    let names = [
        "computeTheExtraordinarilyLongAndDescriptiveFunctionNameNumberOne",
        "computeTheExtraordinarilyLongAndDescriptiveFunctionNameNumberTwo",
        "computeTheExtraordinarilyLongAndDescriptiveFunctionNameNumberThree",
    ];
    let module: String = names
        .iter()
        .map(|name| {
            format!("export function {name}(value: string): string {{\n  return value;\n}}\n\n")
        })
        .collect();
    let mut files = vec![(format!("{dir}/module_with_a_long_file_name.ts"), module)];
    for i in 0..5 {
        let calls: String = names
            .iter()
            .map(|name| format!("export const use_{name} = {name}(\"{i}\");\n"))
            .collect();
        let import = format!(
            "import {{ {} }} from \"./module_with_a_long_file_name\";\n",
            names.join(", ")
        );
        files.push((
            format!("{dir}/consumer_number_{i}_with_a_long_name.ts"),
            format!("{import}{calls}"),
        ));
        files.push((
            format!("{dir}/consumer_number_{i}_with_a_long_name.test.ts"),
            format!("import {{ use_{} }} from \"./consumer_number_{i}_with_a_long_name\";\ntest(\"x\", () => use_{});\n", names[0], names[0]),
        ));
    }
    let extra: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let temp = repo_with(&extra);
    let root = temp.path();
    start_indexed(root);
    let pre = claude(
        root,
        "PreToolUse",
        json!({
            "tool_name": "Write",
            "tool_input": { "file_path": path_of(root, &format!("{dir}/module_with_a_long_file_name.ts")), "content": "" }
        }),
    );
    let note = pre["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default();
    assert!(note.contains("computeTheExtraordinarily"), "{pre}");
    assert!(note.len() <= 600, "{} chars:\n{note}", note.len());
}

#[test]
fn post_edit_reports_first_and_blocks_when_the_problem_persists() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let first = claude(
        root,
        "PostToolUse",
        edit_payload(
            root,
            "src/users/repo.ts",
            "repo.findById(id)",
            "repo.findByIdd(id)",
        ),
    );
    assert!(first.get("decision").is_none(), "{first}");
    let context = first["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default();
    assert!(context.contains("findByIdd"), "{first}");

    // An unrelated second edit leaves the same problem in place.
    let content = fs::read_to_string(root.join("src/util/format.ts")).unwrap();
    write(root, "src/util/format.ts", &format!("{content}// note\n"));
    let second = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/util/format.ts", "", "// note"),
    );
    assert_eq!(second["decision"], "block", "{second}");
    assert!(second["reason"]
        .as_str()
        .unwrap()
        .contains("src/users/repo.ts:17"));
}

#[test]
fn a_two_step_rename_ends_clean() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    let format = fs::read_to_string(root.join("src/util/format.ts")).unwrap();
    write(
        root,
        "src/util/format.ts",
        &format.replace("formatName", "formatDisplayName"),
    );
    let step1 = claude(
        root,
        "PostToolUse",
        edit_payload(
            root,
            "src/util/format.ts",
            "formatName",
            "formatDisplayName",
        ),
    );
    assert!(step1.get("decision").is_none(), "{step1}");
    let repo_ts = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &repo_ts.replace("formatName", "formatDisplayName"),
    );
    let step2 = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/users/repo.ts", "formatName", "formatDisplayName"),
    );
    assert!(step2.get("decision").is_none(), "{step2}");
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );
}

/// Indexes `root` before the session starts, so no background index races
/// the test's edits.
fn start_indexed(root: &Path) {
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "index"])
        .assert()
        .success();
    claude(root, "SessionStart", json!({}));
}

fn replace_in(root: &Path, path: &str, from: &str, to: &str) {
    let content = fs::read_to_string(root.join(path)).unwrap();
    assert!(content.contains(from), "{path} lacks {from:?}");
    write(root, path, &content.replacen(from, to, 1));
}

const THREE_CALLS: &str = "import { UserRepo } from \"./users/repo\";\n\nconst repo = new UserRepo();\nexport const a = repo.findById(\"a\");\nexport const b = repo.findById(\"b\");\nexport const c = repo.findById(\"c\");\n";

#[test]
fn a_rename_fixed_one_call_site_at_a_time_is_never_interrupted() {
    let temp = repo_with(&[("src/c.ts", THREE_CALLS)]);
    let root = temp.path();
    start_indexed(root);
    let repo_ts = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &repo_ts.replace("findById", "findUser"),
    );
    let rename = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/users/repo.ts", "findById", "findUser"),
    );
    assert!(rename.get("decision").is_none(), "{rename}");
    for (step, call) in ["\"a\"", "\"b\"", "\"c\""].iter().enumerate() {
        let from = format!("repo.findById({call})");
        let to = format!("repo.findUser({call})");
        replace_in(root, "src/c.ts", &from, &to);
        let out = claude(
            root,
            "PostToolUse",
            edit_payload(root, "src/c.ts", &from, &to),
        );
        assert!(out.get("decision").is_none(), "step {step}: {out}");
    }
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );
}

#[test]
fn problems_already_there_when_the_session_began_never_block() {
    let base = fs::read_to_string(fixture_dir("typescript").join("src/users/repo.ts")).unwrap();
    let broken = base.replace("repo.findById(id)", "repo.findByIdd(id)");
    let temp = repo_with(&[("src/users/repo.ts", &broken)]);
    let root = temp.path();
    start_indexed(root);
    // Two edits to the file with the old problem, the first shifting its line.
    write(root, "src/users/repo.ts", &format!("// one\n{broken}"));
    let first = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/users/repo.ts", "", "// one"),
    );
    assert!(first.get("decision").is_none(), "{first}");
    replace_in(root, "src/users/repo.ts", "// one", "// one\n// two");
    let second = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/users/repo.ts", "// one", "// one\n// two"),
    );
    assert!(second.get("decision").is_none(), "{second}");
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );

    // A problem the agent adds still blocks, and only it is named.
    replace_in(
        root,
        "src/users/repo.ts",
        "formatName(this.lookup(id))",
        "formatName(this.lookup(id), 2)",
    );
    let stop = claude(root, "Stop", json!({ "stop_hook_active": false }));
    assert_eq!(stop["decision"], "block", "{stop}");
    let reason = stop["reason"].as_str().unwrap();
    assert!(reason.contains("formatName"), "{reason}");
    assert!(!reason.contains("findByIdd"), "{reason}");
}

#[test]
fn stop_blocks_on_problems_at_most_three_times() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    for attempt in 0..3 {
        let stop = claude(root, "Stop", json!({ "stop_hook_active": attempt > 0 }));
        assert_eq!(stop["decision"], "block", "attempt {attempt}: {stop}");
    }
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": true })),
        Value::Null
    );
}

#[test]
fn problems_the_agent_gave_up_on_do_not_block_later_turns() {
    let temp = repo();
    let root = temp.path();
    start_indexed(root);
    replace_in(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    claude(
        root,
        "PostToolUse",
        edit_payload(
            root,
            "src/users/repo.ts",
            "repo.findById(id)",
            "repo.findByIdd(id)",
        ),
    );
    for attempt in 0..3 {
        let stop = claude(root, "Stop", json!({ "stop_hook_active": attempt > 0 }));
        assert_eq!(stop["decision"], "block", "attempt {attempt}: {stop}");
    }
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": true })),
        Value::Null
    );
    // The next turn starts with an unrelated edit.
    let format = fs::read_to_string(root.join("src/util/format.ts")).unwrap();
    write(root, "src/util/format.ts", &format!("{format}// later\n"));
    let edit = claude(
        root,
        "PostToolUse",
        edit_payload(root, "src/util/format.ts", "", "// later"),
    );
    assert!(edit.get("decision").is_none(), "{edit}");
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );
}

/// Stop checks everything changed since the session began, so its time
/// must not grow with one git process per changed file.
#[test]
fn stop_over_five_hundred_changed_files_is_quick() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    support::synth_repo(root, 1_000);
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success());
    }
    start_indexed(root);
    for i in 0..500 {
        let path = format!("module_{i:05}.ts");
        replace_in(root, &path, &format!("x + {i})"), &format!("x + {i} + 1)"));
    }
    replace_in(
        root,
        "module_00001.ts",
        "sharedHelper(x) +",
        "sharedHelper(x, 2) +",
    );
    let started = std::time::Instant::now();
    let stop = claude(root, "Stop", json!({ "stop_hook_active": false }));
    let took = started.elapsed();
    assert_eq!(stop["decision"], "block", "{stop}");
    assert!(
        // Unbounded, this took 12 s in a debug build; loaded runners need slack.
        took < std::time::Duration::from_secs(10),
        "Stop over 500 changed files took {took:?}"
    );
}

#[test]
fn edits_are_found_in_every_payload_shape() {
    let temp = repo();
    let root = temp.path();
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let file = root
        .join("src/users/repo.ts")
        .to_string_lossy()
        .into_owned();
    let cwd = root.to_string_lossy().into_owned();
    let shapes = [
        (
            "copilot",
            json!({ "sessionId": "c1", "cwd": cwd, "toolName": "edit", "toolArgs": json!({ "path": file }).to_string() }),
        ),
        (
            "cursor",
            json!({ "conversation_id": "u1", "workspace_roots": [cwd], "tool_name": "Edit", "tool_input": { "file_path": file } }),
        ),
        (
            "codex",
            json!({ "session_id": "x1", "cwd": cwd, "tool_name": "apply_patch", "tool_input": { "command": ["apply_patch", "*** Begin Patch\n*** Update File: src/users/repo.ts\n@@\n-a\n+b\n*** End Patch\n"] } }),
        ),
    ];
    for (agent, payload) in shapes {
        let output = Command::cargo_bin("tessera")
            .unwrap()
            .args(["hook", agent, "post-edit"])
            .write_stdin(payload.to_string())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("findByIdd"), "{agent}: {stdout}");
    }
}

#[test]
fn edits_to_ignored_files_are_not_checked() {
    let temp = repo();
    let root = temp.path();
    write(root, ".gitignore", "generated/\n");
    fs::create_dir_all(root.join("generated")).unwrap();
    write(
        root,
        "generated/client.ts",
        "import { UserRepo } from \"../src/users/repo\";\nexport const x = new UserRepo().findByIdd(\"1\");\n",
    );
    claude(root, "SessionStart", json!({}));
    let post = claude(
        root,
        "PostToolUse",
        json!({
            "tool_name": "Write",
            "tool_input": { "file_path": root.join("generated/client.ts").to_string_lossy() }
        }),
    );
    assert_eq!(post, Value::Null);
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );
}

#[test]
fn deleting_an_ignored_file_is_not_checked() {
    let temp = repo();
    let root = temp.path();
    write(root, ".gitignore", "generated/\n");
    fs::create_dir_all(root.join("generated")).unwrap();
    write(root, "generated/kept.ts", "export const kept = 1;\n");
    claude(root, "SessionStart", json!({}));
    for path in ["generated/old.ts", "src/gone.ts"] {
        claude(
            root,
            "PostToolUse",
            json!({
                "tool_name": "apply_patch",
                "tool_input": { "command": format!("*** Begin Patch\n*** Delete File: {path}\n*** End Patch\n") }
            }),
        );
    }
    let edited = session_file(root, "s1")["edited"].clone();
    assert_eq!(edited, json!(["src/gone.ts"]), "{edited}");
}

#[test]
fn breakage_from_before_the_session_does_not_block_stop() {
    let temp = repo();
    let root = temp.path();
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    claude(root, "SessionStart", json!({}));
    let format = fs::read_to_string(root.join("src/util/format.ts")).unwrap();
    write(
        root,
        "src/util/format.ts",
        &format.replace("name.trim()", "name.trim().toLowerCase()"),
    );
    assert_eq!(
        claude(root, "Stop", json!({ "stop_hook_active": false })),
        Value::Null
    );
}

#[test]
fn a_huge_change_reports_the_top_findings() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    for i in 0..40 {
        write(
            root,
            &format!("src/broken{i}.ts"),
            "import { UserRepo } from \"./users/repo\";\nexport const x = new UserRepo().findByIdd(\"1\");\n",
        );
    }
    let stop = claude(root, "Stop", json!({ "stop_hook_active": false }));
    let reason = stop["reason"].as_str().unwrap_or_default();
    assert_eq!(stop["decision"], "block", "{stop}");
    assert!(reason.contains("more"), "{reason}");
    assert!(reason.lines().count() <= 12, "{reason}");
}

/// The TypeScript fixture with `findById` misspelled at src/users/repo.ts:17,
/// already indexed by a session start.
fn broken(agent: &str, start: Value) -> TempDir {
    let temp = repo();
    let root = temp.path();
    hook(agent, "session-start", start);
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    temp
}

fn path_of(root: &Path, rel: &str) -> String {
    root.join(rel).to_string_lossy().into_owned()
}

#[test]
fn codex_answers_in_claude_shape_and_always_prints_json_at_stop() {
    let temp = repo();
    let root = temp.path();
    let cwd = root.to_string_lossy().into_owned();
    let base = json!({ "session_id": "x", "cwd": cwd, "model": "gpt" });
    assert_eq!(
        hook(
            "codex",
            "Stop",
            json!({ "session_id": "x", "cwd": cwd, "stop_hook_active": false })
        ),
        json!({})
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let mut post = base.clone();
    post.as_object_mut().unwrap().extend(
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "apply_patch",
            "tool_input": { "command": "*** Begin Patch\n*** Update File: src/users/repo.ts\n*** End Patch\n" }
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    let out = hook("codex", "PostToolUse", post);
    assert_eq!(
        out["hookSpecificOutput"]["hookEventName"], "PostToolUse",
        "{out}"
    );
    assert!(out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("findByIdd"));
}

#[test]
fn gemini_gets_context_after_a_tool_and_a_retry_after_the_agent() {
    let temp = repo();
    let root = temp.path();
    let cwd = root.to_string_lossy().into_owned();
    hook(
        "gemini",
        "SessionStart",
        json!({ "session_id": "g", "cwd": cwd, "source": "startup" }),
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let after_tool = hook(
        "gemini",
        "AfterTool",
        json!({
            "session_id": "g", "cwd": cwd, "hook_event_name": "AfterTool",
            "tool_name": "replace",
            "tool_input": { "file_path": path_of(root, "src/users/repo.ts"), "old_string": "repo.findById(id)", "new_string": "repo.findByIdd(id)" },
            "tool_response": {}
        }),
    );
    assert!(
        after_tool["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap_or_default()
            .contains("findByIdd"),
        "{after_tool}"
    );
    assert!(after_tool.get("decision").is_none(), "{after_tool}");
    let after_agent = hook(
        "gemini",
        "AfterAgent",
        json!({ "session_id": "g", "cwd": cwd, "hook_event_name": "AfterAgent", "prompt": "p", "prompt_response": "done", "stop_hook_active": false }),
    );
    assert_eq!(after_agent["decision"], "deny", "{after_agent}");
    assert!(after_agent["reason"]
        .as_str()
        .unwrap()
        .contains("src/users/repo.ts:17"));
}

#[test]
fn copilot_blocks_agent_stop() {
    let temp = broken("copilot", json!({ "sessionId": "c", "cwd": "" }));
    let cwd = temp.path().to_string_lossy().into_owned();
    let stop = hook(
        "copilot",
        "agentStop",
        json!({ "sessionId": "c2", "timestamp": 0, "cwd": cwd, "stopReason": "end_turn", "stop_hook_active": false }),
    );
    // A session first seen at stop has no baseline for what the agent changed.
    assert_eq!(stop, Value::Null);
    hook(
        "copilot",
        "sessionStart",
        json!({ "sessionId": "c3", "timestamp": 0, "cwd": cwd }),
    );
    let content = fs::read_to_string(temp.path().join("src/users/repo.ts")).unwrap();
    write(
        temp.path(),
        "src/users/repo.ts",
        &content.replace("repo.findByIdd(id)", "repo.findByIddd(id)"),
    );
    let stop = hook(
        "copilot",
        "agentStop",
        json!({ "sessionId": "c3", "timestamp": 0, "cwd": cwd, "stopReason": "end_turn", "stop_hook_active": false }),
    );
    assert_eq!(stop["decision"], "block", "{stop}");
}

#[test]
fn cursor_follows_up_at_stop_unless_the_user_aborted() {
    let temp = repo();
    let root = temp.path();
    let roots = json!([root.to_string_lossy()]);
    hook(
        "cursor",
        "sessionStart",
        json!({ "conversation_id": "u", "workspace_roots": roots }),
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let aborted = hook(
        "cursor",
        "stop",
        json!({ "conversation_id": "u", "workspace_roots": roots, "hook_event_name": "stop", "status": "aborted", "loop_count": 0 }),
    );
    assert_eq!(aborted, Value::Null);
    let stop = hook(
        "cursor",
        "stop",
        json!({ "conversation_id": "u", "workspace_roots": roots, "hook_event_name": "stop", "status": "completed", "loop_count": 0 }),
    );
    assert!(
        stop["followup_message"]
            .as_str()
            .unwrap_or_default()
            .contains("findByIdd"),
        "{stop}"
    );
}

#[test]
fn augment_reads_file_changes_and_wraps_its_stop_decision() {
    let temp = repo();
    let root = temp.path();
    let roots = json!([root.to_string_lossy()]);
    hook(
        "augment",
        "SessionStart",
        json!({ "conversation_id": "a", "workspace_roots": roots, "hook_event_name": "SessionStart" }),
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let post = hook(
        "augment",
        "PostToolUse",
        json!({
            "conversation_id": "a", "workspace_roots": roots, "hook_event_name": "PostToolUse",
            "tool_name": "str-replace-editor", "tool_input": {}, "is_mcp_tool": false,
            "file_changes": [{ "path": "src/users/repo.ts", "changeType": "edit" }]
        }),
    );
    assert_eq!(
        post["hookSpecificOutput"]["hookEventName"], "PostToolUse",
        "{post}"
    );
    let interrupted = hook(
        "augment",
        "Stop",
        json!({ "conversation_id": "a", "workspace_roots": roots, "hook_event_name": "Stop", "agent_stop_cause": "interrupted" }),
    );
    assert_eq!(interrupted, Value::Null);
    let stop = hook(
        "augment",
        "Stop",
        json!({ "conversation_id": "a", "workspace_roots": roots, "hook_event_name": "Stop", "agent_stop_cause": "end_turn" }),
    );
    assert_eq!(stop["hookSpecificOutput"]["decision"], "block", "{stop}");
    assert_eq!(stop["hookSpecificOutput"]["hookEventName"], "Stop");
}

#[test]
fn antigravity_gets_empty_objects_and_a_continue_at_stop() {
    let temp = repo();
    let root = temp.path();
    let paths = json!([root.to_string_lossy()]);
    assert_eq!(
        hook(
            "antigravity",
            "SessionStart",
            json!({ "conversationId": "v", "workspacePaths": paths })
        ),
        json!({})
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let post = hook(
        "antigravity",
        "PostToolUse",
        json!({ "conversationId": "v", "workspacePaths": paths, "toolCall": { "name": "replace_file_content", "args": { "TargetFile": path_of(root, "src/users/repo.ts") } }, "error": "" }),
    );
    assert_eq!(post, json!({}));
    let stop = hook(
        "antigravity",
        "Stop",
        json!({ "conversationId": "v", "workspacePaths": paths, "executionNum": 1, "terminationReason": "", "fullyIdle": true }),
    );
    assert_eq!(stop["decision"], "continue", "{stop}");
    assert!(stop["reason"].as_str().unwrap().contains("findByIdd"));
}

#[test]
fn antigravity_gets_a_decision_at_every_stop_and_no_continue_after_an_error() {
    let temp = repo();
    let root = temp.path();
    index_now(root);
    let paths = json!([root.to_string_lossy()]);
    let stop = |extra: Value| {
        let mut payload = json!({ "conversationId": "v", "workspacePaths": paths, "executionNum": 1, "fullyIdle": true });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        hook("antigravity", "Stop", payload)
    };
    hook(
        "antigravity",
        "SessionStart",
        json!({ "conversationId": "v", "workspacePaths": paths }),
    );
    let clean = stop(json!({ "terminationReason": "model_stop" }));
    assert!(clean["decision"].is_string(), "{clean}");
    assert_ne!(clean["decision"], "continue", "{clean}");
    replace_in(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    for failed in [
        json!({ "terminationReason": "error", "error": "model unavailable" }),
        json!({ "terminationReason": "max_steps_exceeded" }),
        json!({ "terminationReason": "", "error": "tool crashed" }),
    ] {
        let out = stop(failed.clone());
        assert!(out["decision"].is_string(), "{failed}: {out}");
        assert_ne!(out["decision"], "continue", "{failed}: {out}");
    }
    assert_eq!(
        stop(json!({ "terminationReason": "model_stop" }))["decision"],
        "continue"
    );
}

#[test]
fn qwen_gets_no_pre_edit_note_but_post_edit_context() {
    let temp = repo();
    let root = temp.path();
    let cwd = root.to_string_lossy().into_owned();
    hook(
        "qwen",
        "SessionStart",
        json!({ "session_id": "q", "cwd": cwd }),
    );
    let pre = hook(
        "qwen",
        "PreToolUse",
        json!({ "session_id": "q", "cwd": cwd, "tool_name": "edit", "tool_input": { "file_path": path_of(root, "src/util/format.ts"), "old_string": "return name.trim();", "new_string": "x" } }),
    );
    assert_eq!(
        pre,
        Value::Null,
        "qwen requires a permission decision with context"
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    let post = hook(
        "qwen",
        "PostToolUse",
        json!({ "session_id": "q", "cwd": cwd, "tool_name": "write_file", "tool_input": { "file_path": path_of(root, "src/users/repo.ts") } }),
    );
    assert_eq!(
        post["hookSpecificOutput"]["hookEventName"], "PostToolUse",
        "{post}"
    );
}

#[test]
fn the_session_file_counts_what_the_hooks_did() {
    let temp = repo();
    let root = temp.path();
    claude(root, "SessionStart", json!({}));
    claude(
        root,
        "PreToolUse",
        edit_payload(
            root,
            "src/util/format.ts",
            "return name.trim();",
            "return name;",
        ),
    );
    let content = fs::read_to_string(root.join("src/users/repo.ts")).unwrap();
    write(
        root,
        "src/users/repo.ts",
        &content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    );
    claude(
        root,
        "PostToolUse",
        edit_payload(
            root,
            "src/users/repo.ts",
            "repo.findById(id)",
            "repo.findByIdd(id)",
        ),
    );
    claude(root, "Stop", json!({ "stop_hook_active": false }));
    let session: Value =
        serde_json::from_str(&fs::read_to_string(root.join(".tessera/sessions/s1.json")).unwrap())
            .unwrap();
    assert_eq!(session["counts"]["notes"], 1, "{session}");
    assert_eq!(session["counts"]["reports"], 1, "{session}");
    assert_eq!(session["counts"]["post_blocks"], 0, "{session}");
    assert_eq!(session["counts"]["stop_blocks"], 1, "{session}");
    let blocked = session["blocked"].as_array().unwrap();
    assert!(
        blocked
            .iter()
            .any(|f| f.as_str().unwrap().contains("findByIdd")),
        "{session}"
    );
}

fn session_file(root: &Path, id: &str) -> Value {
    let path = root.join(format!(".tessera/sessions/{id}.json"));
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn index_now(root: &Path) {
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "index"])
        .assert()
        .success();
}

/// Cursor loads `.claude/settings.local.json` too, so with Claude and Cursor
/// set up each Cursor event reaches `hook claude` and `hook cursor`.
#[test]
fn a_cursor_event_through_both_adapters_gets_one_answer_in_cursor_shape() {
    let temp = repo();
    let root = temp.path();
    index_now(root);
    let common = json!({
        "conversation_id": "u", "generation_id": "g1", "cursor_version": "2.1.0",
        "workspace_roots": [root.to_string_lossy()], "model": "m",
    });
    let with = |extra: Value| {
        let mut payload = common.clone();
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        payload
    };
    hook(
        "cursor",
        "sessionStart",
        with(json!({ "hook_event_name": "sessionStart", "session_id": "u" })),
    );
    replace_in(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    let post = with(json!({
        "hook_event_name": "postToolUse", "tool_name": "Edit", "tool_use_id": "t1",
        "tool_input": { "file_path": path_of(root, "src/users/repo.ts") }, "tool_output": "ok",
    }));
    let first = hook("claude", "post-edit", post.clone());
    assert!(
        first["additional_context"]
            .as_str()
            .unwrap_or_default()
            .contains("findByIdd"),
        "{first}"
    );
    assert!(first.get("hookSpecificOutput").is_none(), "{first}");
    assert_eq!(hook("cursor", "post-edit", post), first);

    let stop = with(json!({ "hook_event_name": "stop", "status": "completed", "loop_count": 0 }));
    let first = hook("claude", "stop", stop.clone());
    assert!(
        first["followup_message"]
            .as_str()
            .unwrap_or_default()
            .contains("findByIdd"),
        "{first}"
    );
    assert_eq!(hook("cursor", "stop", stop), first);
    assert_eq!(session_file(root, "u")["counts"]["stop_blocks"], 1);
}

/// Copilot CLI reads `.claude/settings*.json` as well, and sends hooks
/// configured there its snake_case payload.
#[test]
fn a_copilot_event_through_both_adapters_gets_one_answer_in_copilot_shape() {
    let temp = repo();
    let root = temp.path();
    index_now(root);
    let cwd = root.to_string_lossy().into_owned();
    let file = path_of(root, "src/users/repo.ts");
    hook(
        "copilot",
        "sessionStart",
        json!({ "sessionId": "c", "timestamp": 1_760_000_000_000_u64, "cwd": cwd, "source": "new" }),
    );
    replace_in(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    let snake = json!({
        "hook_event_name": "PostToolUse", "session_id": "c", "timestamp": "2026-10-21T10:00:00Z",
        "cwd": cwd, "tool_name": "edit", "tool_input": { "path": file },
        "tool_result": { "result_type": "success", "text_result_for_llm": "ok" },
    });
    let camel = json!({
        "sessionId": "c", "timestamp": 1_760_000_000_100_u64, "cwd": cwd, "toolName": "edit",
        "toolArgs": json!({ "path": file }).to_string(),
        "toolResult": { "resultType": "success", "textResultForLlm": "ok" },
    });
    let first = hook("claude", "post-edit", snake);
    assert!(
        first["additionalContext"]
            .as_str()
            .unwrap_or_default()
            .contains("findByIdd"),
        "{first}"
    );
    assert!(first.get("hookSpecificOutput").is_none(), "{first}");
    assert_eq!(hook("copilot", "post-edit", camel), first);

    let snake = json!({
        "hook_event_name": "Stop", "session_id": "c", "timestamp": "2026-10-21T10:00:01Z",
        "cwd": cwd, "transcript_path": "/t", "stop_reason": "end_turn", "stop_hook_active": false,
    });
    let camel = json!({
        "sessionId": "c", "timestamp": 1_760_000_001_000_u64, "cwd": cwd, "transcriptPath": "/t",
        "stopReason": "end_turn", "stop_hook_active": false,
    });
    let first = hook("claude", "stop", snake);
    assert_eq!(first["decision"], "block", "{first}");
    assert_eq!(hook("copilot", "agentStop", camel), first);
    assert_eq!(session_file(root, "c")["counts"]["stop_blocks"], 1);
}

/// The plugin and `init` both wired: Claude Code runs the two identical
/// commands at once with the same input.
#[test]
fn the_same_event_run_twice_at_once_is_answered_once() {
    use std::io::Write;
    use std::process::Stdio;

    let temp = repo();
    let root = temp.path();
    start_indexed(root);
    replace_in(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    let mut payload = edit_payload(
        root,
        "src/users/repo.ts",
        "repo.findById(id)",
        "repo.findByIdd(id)",
    );
    let ids = json!({ "session_id": "s1", "cwd": root.to_string_lossy() });
    payload
        .as_object_mut()
        .unwrap()
        .extend(ids.as_object().unwrap().clone());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(root.join(".tessera/sessions/s1.lock"))
        .unwrap();
    lock.lock().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("tessera");
    let children: Vec<_> = (0..2)
        .map(|_| {
            let mut child = StdCommand::new(&bin)
                .args(["hook", "claude", "post-edit"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.to_string().as_bytes())
                .unwrap();
            child
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(300));
    drop(lock);
    let outputs: Vec<Value> = children
        .into_iter()
        .map(|child| {
            let out = child.wait_with_output().unwrap();
            serde_json::from_slice(&out.stdout).unwrap()
        })
        .collect();
    assert!(outputs[0].get("decision").is_none(), "{outputs:?}");
    assert_eq!(outputs[0], outputs[1]);
}
