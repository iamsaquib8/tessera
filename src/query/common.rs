use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Result};

use crate::types::{Language, SymbolRecord};

pub use super::legacy::is_test_path;

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 1_000;
pub const TYPE_KINDS: &[&str] = &[
    "class",
    "struct",
    "interface",
    "type",
    "trait",
    "enum",
    "record",
    "impl",
    "union",
    "annotation",
];
pub const CONTAINER_KINDS: &[&str] = &[
    "class",
    "struct",
    "interface",
    "trait",
    "enum",
    "record",
    "impl",
    "module",
    "namespace",
];

pub fn limit_or_default(limit: usize) -> usize {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

pub fn bare_name(symbol: &str) -> &str {
    symbol
        .rsplit(['.', ':', '#', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(symbol)
}

pub fn read_body(root: &Path, symbol: &SymbolRecord) -> Result<String> {
    let rel = Path::new(&symbol.path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        bail!("refusing to read {} outside the repository", symbol.path);
    }
    let content = fs::read_to_string(root.join(rel))?;
    let start = symbol.start_line.saturating_sub(1);
    let len = symbol.end_line.saturating_sub(symbol.start_line) + 1;
    Ok(content
        .lines()
        .skip(start)
        .take(len)
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn known_global(language: Language, name: &str) -> bool {
    let names: &[&str] = match language {
        Language::JavaScript | Language::TypeScript | Language::Tsx => &[
            "JSON",
            "Math",
            "Object",
            "Array",
            "Promise",
            "console",
            "Number",
            "String",
            "Boolean",
            "Date",
            "Symbol",
            "Reflect",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "Error",
            "RegExp",
            "Intl",
            "BigInt",
            "parseInt",
            "parseFloat",
            "isNaN",
            "isFinite",
            "setTimeout",
            "setInterval",
            "clearTimeout",
            "clearInterval",
            "queueMicrotask",
            "require",
            "import",
            "fetch",
            "structuredClone",
            "encodeURIComponent",
            "decodeURIComponent",
            "encodeURI",
            "decodeURI",
            "describe",
            "it",
            "test",
            "expect",
            "beforeEach",
            "afterEach",
            "beforeAll",
            "afterAll",
            "jest",
            "vi",
            "process",
            "Buffer",
            "window",
            "document",
            "globalThis",
            "module",
            "exports",
            "_",
            "$",
            "React",
            "useState",
            "useEffect",
            "useMemo",
            "useCallback",
            "useRef",
            "useContext",
            "useReducer",
        ],
        Language::Python => &[
            "print",
            "len",
            "range",
            "str",
            "int",
            "float",
            "bool",
            "list",
            "dict",
            "set",
            "tuple",
            "isinstance",
            "issubclass",
            "open",
            "super",
            "enumerate",
            "zip",
            "map",
            "filter",
            "sorted",
            "reversed",
            "sum",
            "min",
            "max",
            "any",
            "all",
            "abs",
            "round",
            "getattr",
            "setattr",
            "hasattr",
            "repr",
            "type",
            "iter",
            "next",
            "id",
            "hash",
            "format",
            "input",
            "vars",
            "callable",
            "Exception",
            "ValueError",
            "TypeError",
            "KeyError",
        ],
        Language::Java => &[
            "System",
            "String",
            "Math",
            "Integer",
            "Long",
            "Double",
            "Boolean",
            "Objects",
            "List",
            "Map",
            "Set",
            "Arrays",
            "Collections",
            "Optional",
            "Stream",
            "super",
            "this",
        ],
        Language::Go => &[
            "fmt", "len", "cap", "make", "new", "append", "copy", "delete", "panic", "recover",
            "errors", "strings", "strconv", "print", "println", "close",
        ],
        Language::Rust => &[
            "Some",
            "None",
            "Ok",
            "Err",
            "Box",
            "Vec",
            "String",
            "Option",
            "Result",
            "println",
            "eprintln",
            "format",
            "vec",
            "panic",
            "assert",
            "assert_eq",
            "todo",
            "unimplemented",
        ],
        Language::Ruby => &[
            "puts",
            "print",
            "p",
            "require",
            "require_relative",
            "raise",
            "attr_accessor",
            "attr_reader",
            "attr_writer",
            "include",
            "extend",
            "lambda",
            "proc",
            "loop",
        ],
        Language::C | Language::Cpp => &[
            "printf", "fprintf", "sprintf", "snprintf", "malloc", "calloc", "realloc", "free",
            "memcpy", "memset", "strlen", "strcmp", "assert", "std",
        ],
        Language::CSharp => &["Console", "Math", "String", "Task", "nameof", "typeof"],
        Language::Php => &["echo", "print", "isset", "empty", "count", "trim", "array"],
    };
    names.contains(&name)
}
