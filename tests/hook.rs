mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::{json, Value};
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
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
