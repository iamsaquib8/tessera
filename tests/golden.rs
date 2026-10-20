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
