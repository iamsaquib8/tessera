mod support;

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use support::index_fixture;
use tessera_codegraph::indexer::{self, IndexOptions};
use tessera_codegraph::{Index, Workspace};

/// `from -kind-> to [confidence/resolver]`, where `to` is `path:qualified_name`
/// for resolved edges and the bare name otherwise.
fn edges(db: &Path) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT fs.qualified_name || ' -' || e.kind || '-> '
                || COALESCE(tf.path || ':' || ts.qualified_name, e.to_symbol_name)
                || ' [' || e.confidence || COALESCE('/' || e.resolver, '') || ']'
             FROM edges e JOIN symbols fs ON fs.id = e.from_symbol_id
             LEFT JOIN symbols ts ON ts.id = e.to_symbol_id
             LEFT JOIN files tf ON tf.id = ts.file_id",
        )
        .unwrap();
    let mut out: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    out.sort();
    out
}

/// `path local status` for every import binding that is not `ok`.
fn broken_bindings(db: &Path) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT f.path || ' ' || b.local || ' ' || b.status
             FROM import_bindings b JOIN files f ON f.id = b.file_id
             WHERE b.status != 'ok' ORDER BY f.path, b.line",
        )
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn assert_has(all: &[String], expected: &[&str]) {
    let missing: Vec<&&str> = expected
        .iter()
        .filter(|e| !all.iter().any(|a| a == **e))
        .collect();
    assert!(missing.is_empty(), "missing {missing:#?}\nin {all:#?}");
}

fn assert_lacks(all: &[String], prefixes: &[&str]) {
    let found: Vec<&String> = all
        .iter()
        .filter(|a| prefixes.iter().any(|p| a.starts_with(p)))
        .collect();
    assert!(found.is_empty(), "unexpected {found:#?}");
}

#[test]
fn typescript_edges_resolve_through_imports_hints_and_scope() {
    let (_temp, db) = index_fixture("ts-resolution");
    let all = edges(&db);
    assert_has(
        &all,
        &[
            // field hint from a constructor parameter property
            "UserService.name -call-> src/data/repo.ts:UserRepo.require [high/hint]",
            // default import through a tsconfig path alias
            "makeService -call-> src/data/repo.ts:createRepo [high/import]",
            "makeService -new-> src/data/repo.ts:UserRepo [high/import]",
            // local variable typed by `new`
            "makeService -call-> src/data/repo.ts:UserRepo.get [high/hint]",
            "makeService -new-> src/services/users.ts:UserService [high/local]",
            // namespace import, then a barrel re-export
            "isUser -type-> src/domain/user.ts:User [high/import]",
            "UserRepo.require -call-> src/data/repo.ts:UserRepo.get [high/this]",
            // `export * from` in the barrel
            "UserRepo.require -call-> src/domain/errors.ts:notFound [high/import]",
            "UserRepo -extends-> src/data/repo.ts:Repo [high/local]",
            "User -implements-> src/domain/user.ts:Entity [high/local]",
            // CommonJS: destructured require with a rename, and a namespace require
            "label -call-> src/legacy/strings.js:title [high/import]",
            "label -call-> src/legacy/strings.js:slug [high/import]",
            "label -call-> src/legacy/strings.js:suffix [high/import]",
            "default -jsx-> src/ui/Badge.tsx:Badge [high/local]",
            // a type alias is a type
            "Badge -type-> src/ui/Badge.tsx:Props [high/local]",
            // a module-level variable typed by `new`
            "useShared -call-> src/data/repo.ts:UserRepo.get [high/hint]",
            // a field's declared type, directly and through a local copy
            "Shop.direct -call-> src/services/users.ts:UserService.name [high/hint]",
            "Shop.label -call-> src/services/users.ts:UserService.name [high/hint]",
            "describe(\"UserService\").it(\"names a user\") -call-> src/services/users.ts:makeService [high/import]",
            // unknown receiver: unique name match, capped at medium
            "UserService.name -call-> src/domain/user.ts:User.display [medium/name]",
        ],
    );
    // A receiver typed as an external type, or a call imported from an
    // external package, never falls back to a repo symbol: it gets no edge.
    assert_lacks(
        &all,
        &[
            "UserRepo.get -call->",
            "Badge -call-> memo",
            "describe(\"UserService\") -call-> src/",
        ],
    );
}

#[test]
fn java_edges_resolve_through_imports_packages_hints_and_overloads() {
    let (_temp, db) = index_fixture("java-resolution");
    let all = edges(&db);
    let repo = "src/main/java/com/acme/repo/UserRepository.java";
    let user = "src/main/java/com/acme/domain/User.java";
    let service = "src/main/java/com/acme/service";
    assert_has(
        &all,
        &[
            // field type through a single-type import, overload picked by arity
            &format!("UserService.describe -call-> {repo}:UserRepository.require [high/hint]"),
            &format!("UserService.describe -new-> {user}:User [high/import]"),
            &format!("UserService.describe -call-> {user}:User.id [high/hint]"),
            &format!("UserService.describe -call-> {service}/Format.java:Format.label [high/import]"),
            &format!("UserService.describe -call-> {service}/Format.java:Format.shout [high/import]"),
            &format!("UserService -extends-> {service}/BaseService.java:BaseService [high/package]"),
            &format!("UserRepository.require -call-> {repo}:UserRepository.find [high/this]"),
            // wildcard import, static call
            "UserRepository.require -call-> src/main/java/com/acme/domain/Errors.java:Errors.notFound [high/static]",
            &format!(
                "UserServiceTest.describesAUser -call-> {service}/UserService.java:UserService.describe [high/hint]"
            ),
            &format!("User.compareTo -call-> {user}:User.id [high/hint]"),
        ],
    );
    // `require(key)` inside `require(key, strict)` picks the one-argument overload.
    let arities: Vec<(i64, i64)> = Connection::open(&db)
        .unwrap()
        .prepare(
            "SELECT fs.arity_min, ts.arity_min FROM edges e
             JOIN symbols fs ON fs.id = e.from_symbol_id
             JOIN symbols ts ON ts.id = e.to_symbol_id
             WHERE fs.qualified_name = 'UserRepository.require'
               AND ts.qualified_name = 'UserRepository.require'",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(arities, vec![(2, 1)]);
    assert_lacks(&all, &["UserRepository.find -call-> src/"]);
}

fn repo_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    for (path, content) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let ws = Workspace {
        root: root.canonicalize().unwrap(),
        db_path: temp.path().join("t.db"),
    };
    Index::build(&ws.root, &ws.db_path, IndexOptions::default()).unwrap();
    (temp, ws)
}

#[test]
fn barrels_that_re_export_each_other_terminate() {
    let (_temp, ws) = repo_with(&[
        ("a/index.ts", "export * from \"../b\";\nexport * from \"./thing\";\n"),
        ("a/thing.ts", "export function thing() { return 1; }\n"),
        ("b/index.ts", "export * from \"../a\";\n"),
        (
            "main.ts",
            "import { thing, missing } from \"./b\";\nexport function run() { thing(); missing(); }\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_has(
        &all,
        &[
            "run -call-> a/thing.ts:thing [high/import]",
            // the barrels resolve, and none of them exports `missing`
            "run -call-> missing [high/missing]",
        ],
    );
}

#[test]
fn a_local_definition_shadows_an_import() {
    let (_temp, ws) = repo_with(&[
        ("repo.ts", "export function loadUser() { return 1; }\n"),
        (
            "main.ts",
            "import { loadUser } from \"./repo\";\nexport function run() {\n  const loadUser = () => 2;\n  return loadUser();\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_has(&all, &["run -call-> main.ts:run.loadUser [high/local]"]);
}

#[test]
fn parameters_and_local_variables_shadow_an_import() {
    let (_temp, ws) = repo_with(&[
        (
            "repo.ts",
            "export function loadUser() { return 1; }\nexport function Panel() { return null; }\n",
        ),
        (
            "main.tsx",
            "import { loadUser, Panel } from \"./repo\";\n\
             export function param(loadUser: () => number) {\n  return loadUser();\n}\n\
             export function destructured(deps: any) {\n  const { loadUser } = deps;\n  return loadUser();\n}\n\
             export function variable() {\n  const loadUser = makeLoader();\n  return loadUser();\n}\n\
             export function callback(items: Array<() => number>) {\n  return items.map((loadUser) => loadUser());\n}\n\
             export function render({ Panel }: any) {\n  return <Panel />;\n}\n\
             export function outer() {\n  loadUser();\n  return <Panel />;\n}\n\
             function makeLoader() { return () => 2; }\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    let to_import: Vec<&String> = all
        .iter()
        .filter(|e| e.contains("repo.ts:") && !e.starts_with("outer "))
        .collect();
    assert!(to_import.is_empty(), "{to_import:#?}");
    assert_has(
        &all,
        &[
            "outer -call-> repo.ts:loadUser [high/import]",
            "outer -jsx-> repo.ts:Panel [high/import]",
        ],
    );
}

#[test]
fn renaming_an_export_re_resolves_its_importers() {
    let (_temp, ws) = repo_with(&[
        ("a.ts", "export function foo() { return 1; }\n"),
        (
            "b.ts",
            "import { foo } from \"./a\";\nexport function run() { return foo(); }\n",
        ),
    ]);
    assert_has(&edges(&ws.db_path), &["run -call-> a.ts:foo [high/import]"]);

    fs::write(
        ws.root.join("a.ts"),
        "export function bar() { return 1; }\n",
    )
    .unwrap();
    indexer::index_files(&ws, &["a.ts".to_string()]).unwrap();
    let all = edges(&ws.db_path);
    assert_lacks(&all, &["run -call-> a.ts:"]);

    fs::write(
        ws.root.join("a.ts"),
        "export function bar() { return 1; }\nexport function foo() { return 2; }\n",
    )
    .unwrap();
    indexer::index_files(&ws, &["a.ts".to_string()]).unwrap();
    assert_has(&edges(&ws.db_path), &["run -call-> a.ts:foo [high/import]"]);
}

#[test]
fn a_call_on_a_new_expression_resolves_to_the_class() {
    let (_temp, ws) = repo_with(&[
        (
            "repo.ts",
            "export class UserRepo {\n  findById(id: string) { return id; }\n}\n",
        ),
        (
            "main.ts",
            "import { UserRepo } from \"./repo\";\nexport function run() {\n  new UserRepo().findById(\"1\");\n  new UserRepo().findByIdd(\"1\");\n  return new Map().get(\"k\");\n}\n",
        ),
        (
            "Repo.java",
            "class Repo {\n  String find(String id) { return id; }\n}\n",
        ),
        (
            "Main.java",
            "class Main {\n  String run() { return new Repo().find(\"1\"); }\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_has(
        &all,
        &[
            "run -call-> repo.ts:UserRepo.findById [high/new]",
            "run -call-> findByIdd [high/missing]",
            "Main.run -call-> Repo.java:Repo.find [high/new]",
        ],
    );
    assert_lacks(
        &all,
        &["run -call-> get", "run -call-> repo.ts:UserRepo.get"],
    );
}

#[test]
fn only_closed_classes_make_a_missing_member() {
    let (_temp, ws) = repo_with(&[
        (
            "types.ts",
            "export type ElementsMap = Map<string, number>;\nexport interface Route {\n  path: string;\n}\nexport interface Callable extends Function {}\n",
        ),
        (
            "factory.ts",
            "class FactoryStatic {\n  create() { return 1; }\n}\nexport const Factory = new FactoryStatic();\n",
        ),
        (
            "schema.ts",
            "import { z } from \"zod\";\nexport const Schema = z.object({});\n",
        ),
        (
            "main.ts",
            "import { ElementsMap, Route, Callable } from \"./types\";\nimport { Factory } from \"./factory\";\nimport { Schema } from \"./schema\";\n\
             export function run(m: ElementsMap, r: Route, c: Callable) {\n  m.get(\"a\");\n  r.handler();\n  c.bind(null);\n  Schema.parse({});\n  Factory.create();\n  Factory.creat();\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_has(
        &all,
        &[
            "run -call-> factory.ts:FactoryStatic.create [high/hint]",
            "run -call-> creat [high/missing]",
        ],
    );
    let missing: Vec<&String> = all
        .iter()
        .filter(|e| e.ends_with("[high/missing]") && !e.contains("creat"))
        .collect();
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn calls_on_js_globals_never_match_repository_methods_by_name() {
    let (_temp, ws) = repo_with(&[
        (
            "logger.ts",
            "export class Logger {\n  log(m: string) { return m; }\n  parse(s: string) { return s; }\n}\n",
        ),
        (
            "storage.ts",
            "export function getItem(key: string) { return key; }\n",
        ),
        (
            "main.ts",
            "import * as localStorage from \"./storage\";\nexport function run() {\n  console.log(\"x\");\n  JSON.parse(\"{}\");\n  process.env.HOME.trim();\n  return localStorage.getItem(\"k\");\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_lacks(&all, &["run -call-> logger.ts:"]);
    // An import of the same name is not the global.
    assert_has(&all, &["run -call-> storage.ts:getItem [high/import]"]);
}

#[test]
fn a_receiver_typed_by_a_name_match_is_no_stronger_than_the_match() {
    let (_temp, ws) = repo_with(&[
        ("a/foo.ts", "class Foo {\n  run() { return 1; }\n}\n"),
        ("b/foo.ts", "class Foo {\n  run() { return 2; }\n}\n"),
        (
            "tree.ts",
            "export class Node {\n  id() { return 1; }\n}\n",
        ),
        (
            "main.ts",
            "export function go(x: Foo) {\n  return x.run();\n}\n\
             export function attach(parent: Node, child: Node) {\n  parent.appendChild(child);\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    let high: Vec<&String> = all
        .iter()
        .filter(|e| (e.starts_with("go ") || e.starts_with("attach ")) && e.contains("[high/"))
        .collect();
    assert!(high.is_empty(), "{high:#?}");
    assert_has(
        &all,
        &[
            "go -call-> a/foo.ts:Foo.run [low/hint]",
            "go -call-> b/foo.ts:Foo.run [low/hint]",
        ],
    );
}

#[test]
fn java_overloads_with_the_same_argument_count_are_ambiguous() {
    let (_temp, ws) = repo_with(&[
        (
            "Repo.java",
            "class Repo {\n  String findById(String id) { return id; }\n  String findById(Long id) { return null; }\n  String findById(Long id, boolean strict) { return null; }\n}\n",
        ),
        (
            "Main.java",
            "class Main {\n  Repo repo;\n  String run(Long id) { repo.findById(id, true); return repo.findById(id); }\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    let run: Vec<&String> = all.iter().filter(|e| e.starts_with("Main.run ")).collect();
    assert_eq!(
        run,
        vec![
            "Main.run -call-> Repo.java:Repo.findById [high/hint]",
            "Main.run -call-> Repo.java:Repo.findById [low/hint]",
            "Main.run -call-> Repo.java:Repo.findById [low/hint]",
        ],
        "{all:#?}"
    );
}

#[test]
fn a_namespace_re_export_is_a_module_named_by_its_alias() {
    let (_temp, ws) = repo_with(&[
        ("x.ts", "export function foo() { return 1; }\n"),
        (
            "barrel.ts",
            "export * as ns from \"./x\";\nimport * as other from \"./x\";\nexport { other };\n",
        ),
        (
            "main.ts",
            "import { ns, other } from \"./barrel\";\nexport function run() {\n  ns.foo();\n  return other.foo();\n}\n",
        ),
    ]);
    assert_eq!(broken_bindings(&ws.db_path), Vec::<String>::new());
    let all = edges(&ws.db_path);
    let run: Vec<&String> = all.iter().filter(|e| e.starts_with("run ")).collect();
    assert_eq!(
        run,
        vec![
            "run -call-> x.ts:foo [high/import]",
            "run -call-> x.ts:foo [high/import]",
        ],
        "{all:#?}"
    );
}

#[test]
fn enum_members_and_implicit_members_are_never_missing() {
    let (_temp, ws) = repo_with(&[
        ("Color.java", "enum Color { RED, GREEN }\n"),
        ("Point.java", "record Point(int x, int y) {}\n"),
        ("Box.java", "class Box {\n  int size() { return 1; }\n}\n"),
        (
            "Main.java",
            "class Main {\n  String run(Point p, Box b) {\n    Color c = Color.valueOf(\"RED\");\n    c.ordinal();\n    b.toString();\n    b.sizee();\n    return \"\" + p.x() + Color.values().length + b.hashCode();\n  }\n}\n",
        ),
        ("kind.ts", "export enum Kind { A, B = 2 }\n"),
        (
            "shape.ts",
            "export class Shape {\n  area() { return 1; }\n}\n",
        ),
        (
            "main.ts",
            "import { Kind } from \"./kind\";\nimport { Shape } from \"./shape\";\n\
             export function f(k: Kind.A, s: Shape): string {\n  s.aera();\n  return s.toString() + Kind.B;\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    let missing: Vec<&String> = all
        .iter()
        .filter(|e| e.ends_with("[high/missing]"))
        .collect();
    // Only the two real typos remain.
    assert_eq!(
        missing,
        vec![
            "Main.run -call-> sizee [high/missing]",
            "f -call-> aera [high/missing]"
        ],
        "{all:#?}"
    );
}

#[test]
fn a_java_call_on_a_this_field_uses_the_field_type() {
    let (_temp, ws) = repo_with(&[
        (
            "VetRepository.java",
            "interface VetRepository {\n  java.util.List<String> findAll();\n}\n",
        ),
        (
            "OwnerRepository.java",
            "interface OwnerRepository {\n  java.util.List<String> findAll();\n}\n",
        ),
        (
            "VetController.java",
            "class VetController {\n  private final VetRepository vetRepository;\n  VetController(VetRepository vetRepository) { this.vetRepository = vetRepository; }\n  Object list() { return this.vetRepository.findAll(); }\n}\n",
        ),
    ]);
    let all = edges(&ws.db_path);
    assert_has(
        &all,
        &["VetController.list -call-> VetRepository.java:VetRepository.findAll [high/hint]"],
    );
    assert_lacks(&all, &["VetController.list -call-> OwnerRepository.java:"]);
}

#[test]
fn commonjs_exports_the_resolver_cannot_list_are_never_missing() {
    let (_temp, ws) = repo_with(&[
        ("anon.js", "module.exports = function () { return 1; };\n"),
        (
            "named.js",
            "module.exports = function named(a) { return a; };\n",
        ),
        (
            "klass.js",
            "module.exports = class Store {\n  get() { return 1; }\n};\n",
        ),
        (
            "inst.js",
            "class Svc {\n  run() { return 1; }\n}\nmodule.exports = new Svc();\n",
        ),
        (
            "made.js",
            "function makeThing() { return {}; }\nmodule.exports = makeThing();\n",
        ),
        (
            "getters.js",
            "module.exports = {\n  get version() { return 1; },\n  helper() { return 2; },\n  plain: 1,\n};\n",
        ),
        (
            "ident.js",
            "function main() { return 1; }\nmain.extra = 1;\nmodule.exports = main;\n",
        ),
        ("legacy.ts", "function legacy() { return 1; }\nexport = legacy;\n"),
        ("dyn.js", "Object.assign(module.exports, { a: 1 });\n"),
        ("config.js", "const a = 1;\nmodule.exports = { a };\n"),
        (
            "main.js",
            "const anon = require(\"./anon\");\nconst named = require(\"./named\");\nconst Store = require(\"./klass\");\n\
             const svc = require(\"./inst\");\nconst made = require(\"./made\");\n\
             const { version, helper, plain } = require(\"./getters\");\nconst { extra } = require(\"./ident\");\n\
             const ident = require(\"./ident\");\nconst legacy = require(\"./legacy\");\nconst { a } = require(\"./dyn\");\n\
             import config from \"./config\";\n\
             function run() {\n  anon();\n  named(1);\n  new Store().get();\n  svc.run();\n  made.go();\n  helper();\n  ident();\n  legacy();\n  return [version, plain, extra, a, config];\n}\nmodule.exports = { run };\n",
        ),
    ]);
    assert_eq!(broken_bindings(&ws.db_path), Vec::<String>::new());
    let all = edges(&ws.db_path);
    let missing: Vec<&String> = all
        .iter()
        .filter(|e| e.ends_with("[high/missing]"))
        .collect();
    assert!(missing.is_empty(), "{all:#?}");
    assert_has(
        &all,
        &[
            "run -call-> anon.js:default [high/import]",
            "run -call-> named.js:named [high/import]",
            "run -new-> klass.js:Store [high/import]",
            "run -call-> klass.js:Store.get [high/new]",
            "run -call-> ident.js:main [high/import]",
            "run -call-> legacy.ts:legacy [high/import]",
        ],
    );
}
