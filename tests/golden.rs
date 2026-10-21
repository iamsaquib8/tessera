mod support;

use support::{graph_dump, index_fixture};

macro_rules! golden {
    ($name:ident, $lang:expr) => {
        #[test]
        fn $name() {
            let (_temp, db) = index_fixture($lang);
            insta::assert_json_snapshot!(graph_dump(&db));
        }
    };
}

golden!(golden_typescript, "typescript");
golden!(golden_tsx, "tsx");
golden!(golden_javascript, "javascript");
golden!(golden_python, "python");
golden!(golden_go, "go");
golden!(golden_rust, "rust");
golden!(golden_java, "java");
golden!(golden_c, "c");
golden!(golden_cpp, "cpp");
golden!(golden_csharp, "csharp");
golden!(golden_ruby, "ruby");
golden!(golden_php, "php");
golden!(golden_ts_resolution, "ts-resolution");
golden!(golden_java_resolution, "java-resolution");

#[test]
fn golden_typescript_generic_cast() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("cast.ts"),
        "const f = <T,>(x: T) => x;\nexport function h() {\n  return <any>g();\n}\nexport function g() {\n  return f(<number>value);\n}\n",
    )
    .unwrap();
    let db = temp.path().join("tessera.db");
    tessera_codegraph::Index::build(
        &root,
        &db,
        tessera_codegraph::indexer::IndexOptions {
            full: true,
            ..Default::default()
        },
    )
    .unwrap();
    let dump = graph_dump(&db);
    let symbols = dump["symbols"].as_array().unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.as_str().unwrap().contains(" function g [exported]")),
        "missing g: {symbols:?}"
    );
    let edges = dump["edges"].as_array().unwrap();
    for expected in ["cast.ts g -call-> f [name]", "cast.ts h -call-> g [name]"] {
        assert!(
            edges.iter().any(|e| e.as_str().unwrap() == expected),
            "missing {expected}: {edges:?}"
        );
    }
}
