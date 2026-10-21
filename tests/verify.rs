mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

/// The TypeScript fixture as a committed git repository.
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

fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let file = root.join(path);
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains(from), "{path} lacks {from:?}");
    fs::write(&file, content.replacen(from, to, 1)).unwrap();
}

/// `(exit code, findings as "ID path:line")` for `verify --changed`.
fn verify_changed(root: &Path) -> (i32, Vec<String>, serde_json::Value) {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "--json", "verify", "--changed"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let findings = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{} {}:{}",
                f["id"].as_str().unwrap(),
                f["path"].as_str().unwrap_or("-"),
                f["line"].as_u64().unwrap_or(0)
            )
        })
        .collect();
    (output.status.code().unwrap(), findings, value)
}

#[test]
fn a_clean_change_passes_and_lists_tests_to_run() {
    let temp = repo();
    edit(
        temp.path(),
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    let (code, findings, value) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
    assert!(value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "test/repo.test.ts"));
}

#[test]
fn v1_calls_to_names_that_do_not_exist_where_they_point() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findByIdd(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V1 src/users/repo.ts:17".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v3_and_v4_a_renamed_export_breaks_its_importers() {
    let temp = repo();
    edit(
        temp.path(),
        "src/util/format.ts",
        "export function formatName(",
        "export function formatDisplayName(",
    );
    edit(
        temp.path(),
        "src/util/format.ts",
        "return formatName(id);",
        "return formatDisplayName(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V4 src/users/repo.ts:1".to_string()),
        "{findings:?}"
    );
    assert!(
        findings.contains(&"V3 src/users/repo.ts:7".to_string()),
        "{findings:?}"
    );
}

/// Writes `files` into `root` and commits them.
fn commit(root: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    for args in [
        &["add", "."][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "more",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success());
    }
}

#[test]
fn v3_a_rename_behind_a_barrel_breaks_the_importers() {
    let temp = repo();
    let root = temp.path();
    commit(
        root,
        &[
            (
                "src/lib/name.ts",
                "export function shortName(name: string): string {\n  return name.slice(0, 3);\n}\n",
            ),
            ("src/lib/index.ts", "export { shortName } from \"./name\";\n"),
            (
                "src/app.ts",
                "import { shortName } from \"./lib\";\n\nexport const a = shortName(\"a\");\n",
            ),
            (
                "src/other.ts",
                "import { shortName } from \"./lib\";\n\nexport const b = shortName(\"b\");\n",
            ),
        ],
    );
    edit(root, "src/lib/name.ts", "shortName", "abbreviate");
    let (code, findings, _) = verify_changed(root);
    assert_eq!(code, 1, "{findings:?}");
    for path in ["src/app.ts", "src/other.ts"] {
        assert!(
            findings.iter().any(|f| f.contains(&format!(" {path}:"))),
            "{path}: {findings:?}"
        );
    }
}

#[test]
fn v3_a_call_left_behind_when_the_import_was_updated() {
    let temp = repo();
    let root = temp.path();
    commit(
        root,
        &[(
            "src/app.ts",
            "import { formatName } from \"./util/format\";\n\nexport const a = formatName(\"a\");\nexport const b = formatName(\"b\");\n",
        )],
    );
    for path in ["src/util/format.ts", "src/users/repo.ts"] {
        let content = fs::read_to_string(root.join(path)).unwrap();
        fs::write(
            root.join(path),
            content.replace("formatName", "formatDisplayName"),
        )
        .unwrap();
    }
    edit(
        root,
        "src/app.ts",
        "import { formatName }",
        "import { formatDisplayName }",
    );
    edit(
        root,
        "src/app.ts",
        "formatName(\"a\")",
        "formatDisplayName(\"a\")",
    );
    let (code, findings, _) = verify_changed(root);
    assert_eq!(code, 1, "{findings:?}");
    assert!(
        findings.contains(&"V3 src/app.ts:4".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v2_a_new_required_parameter_breaks_existing_callers() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "export function loadUser(id: string): string {",
        "export function loadUser(id: string, tenant: string): string {",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V2 src/api/handler.ts:4".to_string()),
        "{findings:?}"
    );
    assert!(
        findings.contains(&"V2 test/repo.test.ts:5".to_string()),
        "{findings:?}"
    );
}

#[test]
fn external_calls_and_inherited_members_never_block() {
    let temp = repo();
    fs::write(
        temp.path().join("src/view.ts"),
        "import { useState } from \"react\";\nimport { Component } from \"react\";\n\n\
         export class View extends Component {\n  render() {\n    this.setState({});\n    return useState(0).notAThing();\n  }\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
}

#[test]
fn standard_tier_findings_are_advisory() {
    let temp = repo();
    fs::create_dir_all(temp.path().join("py")).unwrap();
    fs::write(
        temp.path().join("py/util.py"),
        "def helper():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("py/main.py"),
        "from py.util import helperr\n\ndef run():\n    return helperr()\n",
    )
    .unwrap();
    let (code, _, value) = verify_changed(temp.path());
    assert_eq!(code, 0, "{value}");
}

#[test]
fn agent_format_is_terse_and_sarif_is_valid() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findByIdd(id);",
    );
    let agent = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args(["--quiet", "verify", "--changed", "--format", "agent"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&agent.stdout);
    assert!(text.contains("src/users/repo.ts:17"), "{text}");
    assert!(text.lines().count() <= 6, "{text}");
    let sarif = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args(["--quiet", "verify", "--changed", "--format", "sarif"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    assert_eq!(value["version"], "2.1.0");
    assert_eq!(value["runs"][0]["results"][0]["ruleId"], "V1");
}

#[test]
fn comments_typed_arrows_and_tagged_templates_never_make_an_arity_mismatch() {
    let temp = repo();
    fs::write(
        temp.path().join("src/shapes.ts"),
        "export const scale = (\n  value: number,\n  /** optional factor */\n  factor = 2,\n) => value * factor;\n\n\
         export function at(\n  points: number[],\n  index: number, // -1 for last\n): number {\n  return points[index];\n}\n\n\
         type Render = (ui: string, options?: object) => void;\n\
         export const render: Render = (ui, options) => {};\n\n\
         export const tag = (parts: TemplateStringsArray, name: string) => parts[0] + name;\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/use.ts"),
        "import { scale, at, render, tag } from \"./shapes\";\n\n\
         export function use() {\n  scale(1);\n  at([1], /* last */ 0);\n  render(\"x\");\n  return tag`hello ${\"you\"}`;\n}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/Job.java"),
        "class Job {\n  int run(int a, /* b */ int b) { return a + b; }\n  int go() { return run(1, /* two */ 2); }\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
}

#[test]
fn v3_skips_a_function_that_calls_its_own_parameter() {
    let temp = repo();
    fs::write(
        temp.path().join("src/apply.ts"),
        "import { formatName } from \"./util/format\";\n\n\
         export function apply(formatName: (s: string) => string): string {\n  return formatName(\"x\");\n}\n\n\
         export const names = [formatName];\n",
    )
    .unwrap();
    commit_all(temp.path());
    edit(
        temp.path(),
        "src/util/format.ts",
        "export function formatName(",
        "export function formatDisplayName(",
    );
    edit(
        temp.path(),
        "src/util/format.ts",
        "return formatName(id);",
        "return formatDisplayName(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V3 src/users/repo.ts:7".to_string()),
        "{findings:?}"
    );
    assert!(
        !findings.contains(&"V3 src/apply.ts:4".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v4_a_barrel_re_exporting_a_renamed_name_is_broken() {
    let temp = repo();
    fs::write(
        temp.path().join("src/util/index.ts"),
        "export { formatName } from \"./format\";\n",
    )
    .unwrap();
    commit_all(temp.path());
    edit(
        temp.path(),
        "src/util/format.ts",
        "export function formatName(",
        "export function formatDisplayName(",
    );
    edit(
        temp.path(),
        "src/util/format.ts",
        "return formatName(id);",
        "return formatDisplayName(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V4 src/util/index.ts:1".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v4_java_imports_of_generated_classes_are_not_broken() {
    let temp = repo();
    let root = temp.path();
    let files = [
        (".gitignore", "target/\n"),
        (
            "svc/src/main/java/com/acme/domain/User.java",
            "package com.acme.domain;\n\npublic class User {}\n",
        ),
        (
            "svc/target/generated-sources/annotations/com/acme/domain/QUser.java",
            "package com.acme.domain;\n\npublic class QUser {}\n",
        ),
        (
            "api/pom.xml",
            "<project><build><plugins><plugin><artifactId>querydsl-apt</artifactId></plugin></plugins></build></project>\n",
        ),
        (
            "api/src/main/java/com/acme/model/Order.java",
            "package com.acme.model;\n\npublic class Order {}\n",
        ),
        (
            "svc/src/main/java/com/acme/app/App.java",
            "package com.acme.app;\n\nimport com.acme.domain.User;\nimport com.acme.domain.QUser;\nimport com.acme.domain.Usr;\nimport com.acme.model.QOrder;\n\npublic class App {}\n",
        ),
    ];
    for (path, content) in files {
        fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        fs::write(root.join(path), content).unwrap();
    }
    let (_, findings, value) = verify_changed(root);
    let v4: Vec<&String> = findings.iter().filter(|f| f.starts_with("V4")).collect();
    assert_eq!(
        v4,
        vec!["V4 svc/src/main/java/com/acme/app/App.java:5"],
        "{value}"
    );
    let message = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "V4")
        .unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.contains("`com.acme.domain.Usr`")
            && message.contains("not a class in this repository"),
        "{message}"
    );
}

fn commit_all(root: &Path) {
    for args in [
        &["add", "."][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-qm",
            "more",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success());
    }
}

#[test]
fn imports_of_files_the_index_does_not_hold_are_not_broken() {
    let temp = repo();
    let root = temp.path();
    fs::create_dir_all(root.join("src/assets")).unwrap();
    fs::create_dir_all(root.join("generated/client")).unwrap();
    fs::write(root.join(".gitignore"), "generated/\n").unwrap();
    fs::write(root.join("src/data.json"), "{}\n").unwrap();
    fs::write(root.join("src/notes.txt"), "hi\n").unwrap();
    fs::write(root.join("src/assets/logo.svg"), "<svg/>\n").unwrap();
    fs::write(
        root.join("generated/client/index.ts"),
        "export function call() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/big.ts"),
        format!(
            "export function big() {{ return 1; }}\n{}",
            "// padding\n".repeat(120_000)
        ),
    )
    .unwrap();
    fs::write(
        root.join("src/view.tsx"),
        "import data from \"./data.json\";\nimport styles from \"./app.module.css\";\n\
         import logo from \"./assets/logo.svg\";\nimport Logo from \"./assets/logo.svg?react\";\n\
         import notes from \"!raw-loader!./notes.txt\";\nimport pkg from \"../package.json\";\n\
         import { big } from \"./big\";\nimport { call } from \"../generated/client\";\n\n\
         export function View() {\n  big();\n  call();\n  return [data, styles, logo, Logo, notes, pkg];\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(root);
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        !findings.iter().any(|f| f.starts_with("V4")),
        "{findings:?}"
    );
}

#[test]
fn spread_arguments_and_unchecked_files_never_make_an_arity_mismatch() {
    let temp = repo();
    fs::write(
        temp.path().join("src/point.ts"),
        "export function clickAt(x: number, y: number) {\n  return x + y;\n}\n\n\
         export function use(at: [number, number]) {\n  return clickAt(...at);\n}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/bindings.ts"),
        "// @ts-nocheck\nfunction check(condition, text) {\n  return condition || text;\n}\n\n\
         export function run() {\n  return check(true);\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
}

#[test]
fn overloads_getters_and_aliased_methods_take_their_callable_arity() {
    let temp = repo();
    fs::write(
        temp.path().join("src/app.ts"),
        "export class Factory {\n  create(): string;\n  create(name: string, port?: number): string;\n  create(name: string | undefined, port?: number): string {\n    return name ?? String(port);\n  }\n}\n\n\
         export class Stream {\n  get errorHandler(): (err: Error, res: string) => void {\n    return () => {};\n  }\n}\n\n\
         export class Logger {\n  log(message: string, ...rest: string[]) {\n    return message + rest.length;\n  }\n}\n\n\
         const noop = () => {};\nexport class SilentLogger extends Logger {\n  log = noop;\n}\n\n\
         export function use(f: Factory, s: Stream, quiet: SilentLogger) {\n  f.create();\n  s.errorHandler(new Error(\"x\"), \"res\");\n  return quiet.log();\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
}

#[test]
fn plain_javascript_argument_counts_never_make_an_arity_mismatch() {
    let temp = repo();
    fs::write(
        temp.path().join("src/fmt.js"),
        "export function fmt(value, options) {\n  return String(value) + (options || \"\");\n}\n\
         export function sum() {\n  let total = 0;\n  for (const n of arguments) total += n;\n  return total;\n}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/use.js"),
        "import { fmt, sum } from \"./fmt\";\n\nexport function use() {\n  fmt(1);\n  return sum(1, 2, 3);\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
}

#[test]
fn properties_default_expressions_and_destructured_exports_exist() {
    let temp = repo();
    fs::write(
        temp.path().join("src/Holder.ts"),
        "export class Holder {\n  public handler: (() => void) | null = null;\n  private readonly done?: () => void;\n}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/Panel.tsx"),
        "import React from \"react\";\nconst Panel = () => null;\nexport default React.memo(Panel);\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/config.ts"),
        "export default { name: \"x\" };\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/store.ts"),
        "import * as jotai from \"jotai\";\nexport const { useAtom, useSetAtom } = jotai;\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/app.ts"),
        "import { Holder } from \"./Holder\";\nimport Panel from \"./Panel\";\nimport config from \"./config\";\nimport { useAtom } from \"./store\";\n\n\
         export function app() {\n  const h = new Holder();\n  h.handler?.();\n  useAtom();\n  return [Panel, config];\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
}

#[test]
fn a_call_on_a_call_result_has_no_name_of_its_own() {
    let temp = repo();
    fs::write(
        temp.path().join("src/events.ts"),
        "export function handle() {}\nexport function make(event: string) {\n  return (a: number, b: number) => a + b;\n}\n\
         export function fire() {\n  return make(\"handle\")(1, 2);\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
}

fn repo_from(fixture: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(fixture), temp.path());
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

#[test]
fn v5_removing_a_grpc_method_a_client_calls_blocks() {
    let temp = repo_from("contracts-grpc");
    let cart = fs::read_to_string(temp.path().join("cart/CartService.cs")).unwrap();
    let start = cart
        .find("        public override Task<Cart> GetCart")
        .unwrap();
    let end = start + cart[start..].find("        }\n").unwrap() + "        }\n".len();
    fs::write(
        temp.path().join("cart/CartService.cs"),
        format!("{}{}", &cart[..start], &cart[end..]),
    )
    .unwrap();
    let (code, findings, value) = verify_changed(temp.path());
    assert_eq!(code, 1, "{value}");
    assert!(
        findings.contains(&"V5 frontend/rpc.go:10".to_string()),
        "{findings:?}"
    );
    let v5 = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "V5")
        .unwrap();
    assert!(
        v5["message"]
            .as_str()
            .unwrap()
            .contains("CartService/GetCart"),
        "{v5}"
    );
}

#[test]
fn v5_names_the_method_from_its_proto_when_the_server_is_ambiguous() {
    let temp = repo_from("contracts-grpc");
    // Two servers registered in one file leave the handler's service unknown.
    edit(
        temp.path(),
        "checkout/main.go",
        "\tpb.RegisterCheckoutServiceServer(srv, svc)\n",
        "\tpb.RegisterCheckoutServiceServer(srv, svc)\n\thealthpb.RegisterHealthServer(srv, svc)\n",
    );
    fs::write(
        temp.path().join("frontend/checkout.go"),
        "package main\n\nimport (\n\t\"context\"\n\n\tpb \"example.com/frontend/genproto\"\n)\n\n\
         func (fe *frontendServer) placeOrder(ctx context.Context) error {\n\
         \t_, err := pb.NewCheckoutServiceClient(fe.checkoutSvcConn).PlaceOrder(ctx, &pb.PlaceOrderRequest{})\n\
         \treturn err\n}\n",
    )
    .unwrap();
    for args in [
        &["add", "."][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-qm",
            "client",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    }
    edit(
        temp.path(),
        "checkout/main.go",
        "func (cs *checkoutService) PlaceOrder(",
        "func (cs *checkoutService) SubmitOrder(",
    );
    let (code, findings, value) = verify_changed(temp.path());
    assert_eq!(code, 1, "{value}");
    assert!(
        findings.contains(&"V5 frontend/checkout.go:10".to_string()),
        "{findings:?}"
    );
    let message = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "V5")
        .unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.contains("`CheckoutService/PlaceOrder`"),
        "{message}"
    );
}

#[test]
fn v5_a_moved_route_is_reported_for_its_clients() {
    let temp = repo_from("contracts-http");
    edit(
        temp.path(),
        "api/src/app.js",
        "app.get('/api/users/:id', getUser);",
        "app.get('/api/people/:id', getUser);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    // The clients build the path from a variable, so the link is not
    // certain enough to block.
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        findings.contains(&"V5 web/src/client.ts:8".to_string()),
        "{findings:?}"
    );
    assert!(
        findings.contains(&"V5 web/src/client.ts:9".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v5_stays_quiet_when_a_route_moves_to_another_file() {
    let temp = repo_from("contracts-http");
    edit(
        temp.path(),
        "api/src/app.js",
        "router.post('/api/users', (req, res) => res.send(create(req.body)));",
        "",
    );
    fs::write(
        temp.path().join("api/src/users.js"),
        "const router = require('express').Router();\nrouter.post('/api/users', (req, res) => res.send(req.body));\n",
    )
    .unwrap();
    let (_, findings, _) = verify_changed(temp.path());
    assert!(
        !findings.iter().any(|f| f.starts_with("V5")),
        "{findings:?}"
    );
}

fn markdown(root: &Path, args: &[&str]) -> (i32, String) {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "verify", "--format", "markdown"])
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn markdown_report_for_a_pull_request() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findByIdd(id);",
    );
    let (code, text) = markdown(temp.path(), &["--changed"]);
    assert_eq!(code, 1, "{text}");
    assert!(text.starts_with("<!-- tessera -->\n"), "{text}");
    assert!(text.contains("1 problem to fix"), "{text}");
    assert!(text.contains("| V1 | `src/users/repo.ts:17` |"), "{text}");
    assert!(text.contains("Callers of the changed code"), "{text}");
    assert!(text.contains("handler"), "callers are listed: {text}");
    assert!(text.contains("Tests to run"), "{text}");
    assert!(text.contains("`test/repo.test.ts`"), "{text}");
}

#[test]
fn markdown_report_names_the_services_a_contract_change_reaches() {
    let temp = repo_from("contracts-grpc");
    edit(
        temp.path(),
        "cart/CartService.cs",
        "return Task.FromResult(new Cart());",
        "return Task.FromResult(new Cart { });",
    );
    let (code, text) = markdown(temp.path(), &["--changed"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("No problems found"), "{text}");
    assert!(text.contains("Contracts this change touches"), "{text}");
    assert!(
        text.contains("`rpc CartService/GetCart`") && text.contains("frontend"),
        "{text}"
    );
}

/// A committed repository holding just these files.
fn repo_with(files: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    for (path, content) in files {
        let file = temp.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
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
            "-qm",
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

const EXPRESS_APP: &str = "const express = require('express');\nconst app = express();\n";

#[test]
fn v5_is_silent_for_a_call_to_an_external_api() {
    let temp = repo_with(&[
        (
            "api/app.js",
            &format!("{EXPRESS_APP}app.get('/user', (req, res) => res.send('me'));\n"),
        ),
        (
            "web/client.js",
            "export function me() {\n  return fetch('https://api.github.com/user');\n}\n",
        ),
    ]);
    edit(
        temp.path(),
        "api/app.js",
        "app.get('/user', (req, res) => res.send('me'));\n",
        "",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        !findings.iter().any(|f| f.starts_with("V5")),
        "{findings:?}"
    );
}

#[test]
fn v5_does_not_block_on_a_parameter_only_match() {
    let temp = repo_with(&[
        (
            "api/app.js",
            &format!(
                "{EXPRESS_APP}app.get('/users/:id', (req, res) => res.send(req.params.id));\n"
            ),
        ),
        (
            "web/client.js",
            "export function me() {\n  return fetch('/users/me');\n}\n",
        ),
    ]);
    edit(
        temp.path(),
        "api/app.js",
        "app.get('/users/:id', (req, res) => res.send(req.params.id));\n",
        "",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        findings.contains(&"V5 web/client.js:2".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v5_does_not_block_when_a_route_moves_under_a_mounted_router() {
    let temp = repo_with(&[
        (
            "api/app.js",
            &format!("{EXPRESS_APP}app.get('/api/users', (req, res) => res.send([]));\n"),
        ),
        (
            "web/client.js",
            "export function users() {\n  return fetch('/api/users');\n}\n",
        ),
    ]);
    edit(
        temp.path(),
        "api/app.js",
        "app.get('/api/users', (req, res) => res.send([]));\n",
        "const users = express.Router();\nusers.get('/users', (req, res) => res.send([]));\napp.use('/api', users);\n",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
}

#[test]
fn v5_is_silent_when_a_route_is_still_served_for_any_method() {
    let temp = repo_with(&[
        (
            "api/app.js",
            &format!("{EXPRESS_APP}app.get('/api/users', (req, res) => res.send([]));\n"),
        ),
        (
            "web/client.js",
            "export function users() {\n  return fetch('/api/users');\n}\n",
        ),
    ]);
    edit(
        temp.path(),
        "api/app.js",
        "app.get('/api/users'",
        "app.all('/api/users'",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        !findings.iter().any(|f| f.starts_with("V5")),
        "{findings:?}"
    );
}
