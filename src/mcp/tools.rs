use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

use super::protocol::{RpcError, INVALID_PARAMS};
use super::ServerState;
use crate::output::AgentText;
use crate::query::{
    BlastArgs, ContextArgs, FindArgs, MapArgs, PathArgs, RefsArgs, SearchArgs, VerifyArgs,
};
use crate::Index;

const MCP_DEFAULT_LIMIT: usize = 25;

pub enum ToolError {
    InvalidParams(String),
    Execution(String),
}

pub struct ToolOutput {
    pub text: String,
    pub json: Value,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: fn() -> Value,
    pub run: fn(&Index, Value) -> Result<ToolOutput, ToolError>,
}

pub fn registry() -> &'static [ToolSpec] {
    &REGISTRY
}

static REGISTRY: [ToolSpec; 9] = [
    ToolSpec {
        name: "find",
        title: "Find definition",
        description: "Where a symbol is defined: kind, file:line, signature, and its body with include_body. Pass `path` instead to outline a file or directory with its imports. Accepts `name`, `Class.method`, or `A::B`; fuzzy matches are labelled as such. Use this instead of opening files to locate code.",
        input_schema: find_schema,
        run: |index, args| run_query(args, true, |a: FindArgs| index.find(&a)),
    },
    ToolSpec {
        name: "search",
        title: "Search symbols",
        description: "Search symbol names by fuzzy text or glob (`findBy*`), filtered by kind, language, path prefix, or exported. `unused: true` lists functions and methods nothing calls. Paginated.",
        input_schema: search_schema,
        run: |index, args| run_query(args, true, |a: SearchArgs| index.search(&a)),
    },
    ToolSpec {
        name: "refs",
        title: "Find references",
        description: "Every reference to a symbol with file:line and the enclosing function, or, when `target` is a repo-relative file path, the files that import it. `tests_only` keeps test files. Use instead of grep for call sites. Each reference has a confidence: high (resolved through scope, an import, `this` or a typed receiver), medium (a unique name match), low or name (possible). `min_confidence` drops weaker ones. Paginated.",
        input_schema: refs_schema,
        run: |index, args| run_query(args, true, |a: RefsArgs| index.refs(&a)),
    },
    ToolSpec {
        name: "blast",
        title: "Blast radius",
        description: "Before editing: callers ranked by how much they depend on the target (personalised PageRank over the call graph), plus test files to run. Pass `symbol`, or `diff_from` with a git revision to analyse changed code. Paginated.",
        input_schema: blast_schema,
        run: |index, args| run_query(args, true, |a: BlastArgs| index.blast(&a)),
    },
    ToolSpec {
        name: "verify",
        title: "Verify code",
        description: "After writing code: check that `symbol` exists, or that every call in `snippet` (with `language`) resolves to something in the index. Returns problems with the closest real names. Calls into the standard library and unknown receivers are not flagged.",
        input_schema: verify_schema,
        run: |index, args| run_query(args, false, |a: VerifyArgs| index.verify(&a)),
    },
    ToolSpec {
        name: "context",
        title: "Pre-edit context",
        description: "One budgeted bundle to read before editing a symbol: its body, what it calls, who calls it, related tests, and sibling symbols. Cheaper than opening several files.",
        input_schema: context_schema,
        run: |index, args| run_query(args, false, |a: ContextArgs| index.context(&a)),
    },
    ToolSpec {
        name: "path",
        title: "Call path",
        description: "Shortest call chain from `from` to `to`: answers whether A reaches B, and through which functions.",
        input_schema: path_schema,
        run: |index, args| run_query(args, false, |a: PathArgs| index.path(&a)),
    },
    ToolSpec {
        name: "map",
        title: "Contract map",
        description: "How services talk to each other: HTTP routes with the requests that reach them, gRPC methods with their servers and clients, Kafka topics with producers and consumers, and tables with their migrations, models and queries. `kind` picks one (`route`, `rpc`, `topic`, `table`); `service` lists what each service serves and calls. `query` filters by name or file. Paginated. Use before changing an endpoint, a message, or a table to see every other service it affects.",
        input_schema: map_schema,
        run: |index, args| run_query(args, true, |a: MapArgs| index.map(&a)),
    },
    ToolSpec {
        name: "status",
        title: "Index status",
        description: "Index health: repository root, freshness, file and symbol counts, languages, and setup hints. Call when results look stale or empty.",
        input_schema: status_schema,
        run: |index, args| run_query(args, false, |_: Value| index.status()),
    },
];

fn run_query<A, R>(
    args: Value,
    paged: bool,
    query: impl FnOnce(A) -> anyhow::Result<R>,
) -> Result<ToolOutput, ToolError>
where
    A: DeserializeOwned,
    R: serde::Serialize + AgentText,
{
    let mut args = match args {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        _ => {
            return Err(ToolError::InvalidParams(
                "`arguments` must be an object".into(),
            ))
        }
    };
    let format = match args.remove("format") {
        None => "text".to_string(),
        Some(Value::String(f)) if f == "text" || f == "json" => f,
        Some(other) => {
            return Err(ToolError::InvalidParams(format!(
                "`format` must be \"text\" or \"json\", got {other}"
            )))
        }
    };
    if paged {
        args.entry("limit").or_insert(json!(MCP_DEFAULT_LIMIT));
    }
    let parsed: A = serde_json::from_value(Value::Object(args))
        .map_err(|e| ToolError::InvalidParams(e.to_string()))?;
    let result = query(parsed).map_err(|e| ToolError::Execution(format!("{e:#}")))?;
    let json = serde_json::to_value(&result).map_err(|e| ToolError::Execution(e.to_string()))?;
    let text = if format == "json" {
        json.to_string()
    } else {
        result.agent_text()
    };
    Ok(ToolOutput { text, json })
}

pub(super) fn list() -> Vec<Value> {
    registry()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "title": spec.title,
                "description": spec.description,
                "inputSchema": (spec.input_schema)(),
                "annotations": {
                    "title": spec.title,
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                }
            })
        })
        .collect()
}

pub(super) fn call(state: &ServerState, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call needs `params.name`"))?;
    let spec = registry()
        .iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("Unknown tool: {name}")))?;
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let schema = (spec.input_schema)();
    let missing: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|key| args.get(key).is_none_or(Value::is_null))
        .collect();
    if !missing.is_empty() {
        return Err(RpcError::new(
            INVALID_PARAMS,
            format!(
                "Invalid arguments for {name}: missing {}",
                missing.join(", ")
            ),
        ));
    }
    if let Some(progress) = state.building() {
        return Ok(tool_error(&format!(
            "Index is building ({progress}); retry in a few seconds."
        )));
    }
    let index = match Index::open_workspace(&state.ws) {
        Ok(index) => index,
        Err(error) => return Ok(tool_error(&format!("{error:#}"))),
    };
    match (spec.run)(&index, args) {
        Ok(output) => Ok(json!({
            "content": [{ "type": "text", "text": output.text }],
            "isError": false
        })),
        Err(ToolError::InvalidParams(message)) => Err(RpcError::new(
            INVALID_PARAMS,
            format!("Invalid arguments for {name}: {message}"),
        )),
        Err(ToolError::Execution(message)) => Ok(tool_error(&message)),
    }
}

fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

fn schema(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties["format"] = json!({
        "type": "string",
        "enum": ["text", "json"],
        "description": "Response format. Compact text (default) costs fewer tokens."
    });
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn paging() -> Value {
    json!({
        "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Results per page (default 25)." },
        "cursor": { "type": "string", "description": "next_cursor from a previous page." }
    })
}

fn merge(mut base: Value, extra: Value) -> Value {
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
    base
}

fn find_schema() -> Value {
    schema(
        merge(
            json!({
                "symbol": { "type": "string", "description": "Symbol name: `name`, `Class.method`, or `A::B`." },
                "path": { "type": "string", "description": "Repo-relative file or directory to outline instead." },
                "include_body": { "type": "boolean", "description": "Include each definition's source." }
            }),
            paging(),
        ),
        &[],
    )
}

fn search_schema() -> Value {
    schema(
        merge(
            json!({
                "pattern": { "type": "string", "description": "Fuzzy text or glob such as `findBy*`." },
                "kinds": { "type": "array", "items": { "type": "string" }, "description": "function, method, class, …" },
                "languages": { "type": "array", "items": { "type": "string" } },
                "path": { "type": "string", "description": "Path prefix filter." },
                "exported": { "type": "boolean" },
                "unused": { "type": "boolean", "description": "Only functions and methods with no inbound references." }
            }),
            paging(),
        ),
        &["pattern"],
    )
}

fn refs_schema() -> Value {
    schema(
        merge(
            json!({
                "target": { "type": "string", "description": "Symbol name, or repo-relative file path for importers." },
                "tests_only": { "type": "boolean", "description": "Only references from test files." },
                "min_confidence": { "type": "string", "enum": ["high", "medium", "low"], "description": "Leave out references below this confidence." }
            }),
            paging(),
        ),
        &["target"],
    )
}

fn blast_schema() -> Value {
    schema(
        merge(
            json!({
                "symbol": { "type": "string", "description": "Symbol about to change." },
                "diff_from": { "type": "string", "description": "Git revision; analyse changes since it instead." },
                "diff_to": { "type": "string", "description": "End revision for diff_from (default HEAD)." },
                "depth": { "type": "integer", "minimum": 1, "maximum": 8, "description": "Caller depth (default 4)." }
            }),
            paging(),
        ),
        &[],
    )
}

fn verify_schema() -> Value {
    schema(
        json!({
            "symbol": { "type": "string", "description": "Symbol name to check exists." },
            "snippet": { "type": "string", "description": "Code whose calls should be checked." },
            "language": { "type": "string", "enum": ["typescript", "tsx", "javascript", "python", "go", "rust", "java", "c", "cpp", "csharp", "ruby", "php"] },
            "changed": { "type": "boolean", "description": "Check every change in the working tree against HEAD: calls to names that do not exist, wrong argument counts, references the change broke, broken imports, and tests to run." },
            "staged": { "type": "boolean", "description": "Check the staged changes instead." },
            "diff_from": { "type": "string", "description": "Check the working tree against this git revision instead." }
        }),
        &[],
    )
}

fn context_schema() -> Value {
    schema(
        json!({
            "symbol": { "type": "string" },
            "budget_tokens": { "type": "integer", "minimum": 200, "maximum": 8000, "description": "Approximate size budget (default 2000)." }
        }),
        &["symbol"],
    )
}

fn path_schema() -> Value {
    schema(
        json!({
            "from": { "type": "string" },
            "to": { "type": "string" },
            "depth": { "type": "integer", "minimum": 1, "maximum": 12 }
        }),
        &["from", "to"],
    )
}

fn map_schema() -> Value {
    schema(
        merge(
            json!({
                "kind": { "type": "string", "enum": ["route", "rpc", "topic", "table", "graphql", "config", "service"] },
                "query": { "type": "string" }
            }),
            paging(),
        ),
        &[],
    )
}

fn status_schema() -> Value {
    schema(json!({}), &[])
}
