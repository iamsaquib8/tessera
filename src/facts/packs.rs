//! Framework packs: tree-sitter queries in `packs/*.scm`, read by one
//! capture convention, so a framework is added without touching this code.
//!
//! Captures: `@route`, `@client`, `@producer`, `@consumer`, `@entity` mark
//! the fact and its line; `@method`, `@path`, `@prefix`, `@topic`, `@table`
//! and `@handler` carry its parts; `@args` and `@prefix_args` are argument
//! lists whose first string (or `value =`/`path =` pair) is the path or
//! prefix, `@prefix_path` one whose `path =` pair is. `(#set! method "GET")`
//! gives a default. Other captures only serve predicates.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

use super::{http, kind, IndexedFact};
use crate::extract::support_for;
use crate::types::Language;

pub struct Pack {
    pub id: &'static str,
    pub languages: &'static [Language],
    /// The file must match this for the pack to run: a cheap scan that
    /// skips the query on files that cannot match.
    pub gate: &'static str,
    pub query: &'static str,
}

const JS: &[Language] = &[Language::JavaScript, Language::TypeScript, Language::Tsx];
const TS: &[Language] = &[Language::TypeScript, Language::Tsx];
const JAVA: &[Language] = &[Language::Java];

pub const PACKS: &[Pack] = &[
    Pack {
        id: "express",
        languages: JS,
        gate: r"\b(app|router|server|routes?|\w*Router|fastify|App)\s*\.\s*(get|post|put|patch|delete|head|options|all|route)\s*\(",
        query: include_str!("../../packs/express.scm"),
    },
    Pack {
        id: "nestjs",
        languages: TS,
        gate: r"@Controller",
        query: include_str!("../../packs/nestjs.scm"),
    },
    Pack {
        id: "http-clients",
        languages: JS,
        gate: r"\b(fetch|axios|useSWR\w*)\s*\(|\b(axios|ky|api|apiClient|client|http|httpClient|request|instance|\$http|\$api)\s*\.\s*(get|post|put|patch|delete|head|options)\s*\(",
        query: include_str!("../../packs/http-clients.scm"),
    },
    Pack {
        id: "spring-mvc",
        languages: JAVA,
        gate: r"Mapping\b",
        query: include_str!("../../packs/spring-mvc.scm"),
    },
    Pack {
        id: "spring-clients",
        languages: JAVA,
        gate: r"FeignClient|RestTemplate|restTemplate|WebClient|webClient|RestClient|restClient|MockMvc|WebTestClient",
        query: include_str!("../../packs/spring-clients.scm"),
    },
];

struct Compiled {
    pack: &'static Pack,
    language: Language,
    gate: Regex,
    query: Query,
}

/// One query per pack and language: a file runs only the packs whose
/// needles it contains, which is cheaper than one query with every pattern.
static COMPILED: LazyLock<Vec<Compiled>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for pack in PACKS {
        for &language in pack.languages {
            let query = Query::new(&support_for(language).grammar(), pack.query)
                .unwrap_or_else(|e| panic!("pack {} for {language}: {e}", pack.id));
            out.push(Compiled {
                pack,
                language,
                gate: Regex::new(pack.gate).expect("valid gate"),
                query,
            });
        }
    }
    out
});

const FACT_CAPTURES: &[(&str, &str)] = &[("route", kind::ROUTE), ("client", kind::HTTP_CALL)];

pub fn run(language: Language, source: &str, tree: &Tree) -> Vec<IndexedFact> {
    let mut out: Vec<(IndexedFact, u8)> = Vec::new();
    for compiled in COMPILED.iter().filter(|c| c.language == language) {
        if !compiled.gate.is_match(source) {
            continue;
        }
        let names = compiled.query.capture_names();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&compiled.query, tree.root_node(), source.as_bytes());
        while let Some(m) = matches.next() {
            let mut captures: HashMap<&str, Node<'_>> = HashMap::new();
            for capture in m.captures {
                captures
                    .entry(names[capture.index as usize])
                    .or_insert(capture.node);
            }
            let defaults: HashMap<&str, &str> = compiled
                .query
                .property_settings(m.pattern_index)
                .iter()
                .filter_map(|p| Some((&*p.key, p.value.as_deref()?)))
                .collect();
            if let Some(fact) = fact(compiled.pack.id, source, &captures, &defaults) {
                out.push(fact);
            }
        }
    }
    // Overlapping patterns can match one site; keep the most specific.
    out.sort_by(|a, b| {
        (a.0.line, a.0.kind, a.0.rule)
            .cmp(&(b.0.line, b.0.kind, b.0.rule))
            .then(b.1.cmp(&a.1))
    });
    out.dedup_by(|b, a| a.0.line == b.0.line && a.0.kind == b.0.kind && a.0.rule == b.0.rule);
    out.into_iter().map(|(fact, _)| fact).collect()
}

fn text<'s>(source: &'s str, node: Node<'_>) -> &'s str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// The fact a match describes, with how specific it is (a method and a
/// prefix make it more so).
fn fact(
    pack: &'static str,
    source: &str,
    captures: &HashMap<&str, Node<'_>>,
    defaults: &HashMap<&str, &str>,
) -> Option<(IndexedFact, u8)> {
    let (capture, kind) = FACT_CAPTURES
        .iter()
        .find(|(capture, _)| captures.contains_key(capture))?;
    let at = captures[capture];
    let (path_args, method_args) = captures
        .get("args")
        .map(|args| arguments(source, *args, ArgKeys::ValueOrPath))
        .unwrap_or_default();
    let path_text = captures
        .get("path")
        .map(|p| text(source, *p).to_string())
        .or(path_args);
    let (path, confidence) = match path_text {
        Some(text) => http::path(&text)?,
        // `@GetMapping` or `@Get()` with no path: the prefix itself.
        None if *kind == kind::ROUTE => ("/".to_string(), "high"),
        None => return None,
    };
    let prefix = captures
        .get("prefix")
        .map(|p| text(source, *p).to_string())
        .or_else(|| {
            captures
                .get("prefix_args")
                .and_then(|a| arguments(source, *a, ArgKeys::ValueOrPath).0)
        })
        .or_else(|| {
            captures
                .get("prefix_path")
                .and_then(|a| arguments(source, *a, ArgKeys::PathOnly).0)
        })
        .and_then(|p| http::path(&p))
        .map(|(p, _)| p);
    let method = captures
        .get("method")
        .map(|m| http::method(text(source, *m)))
        .filter(|m| m != "ANY")
        .or(method_args.map(|m| http::method(&m)))
        .or_else(|| defaults.get("method").map(|m| http::method(m)))
        .unwrap_or_else(|| "ANY".to_string());
    let full = match &prefix {
        Some(prefix) => http::join(prefix, &path),
        None => path,
    };
    let specific = u8::from(prefix.is_some()) + u8::from(method != "ANY");
    let line_text = source
        .lines()
        .nth(at.start_position().row)
        .unwrap_or_default();
    let mut fact = IndexedFact::new(
        kind,
        http::key(&method, &full),
        format!("{method} {full}"),
        at.start_position().row + 1,
        pack,
        line_text,
    );
    fact.confidence = confidence;
    fact.handler = captures
        .get("handler")
        .copied()
        .or_else(|| last_identifier_argument(at))
        .map(|h| text(source, h).to_string());
    Some((fact, specific))
}

/// `getUser` in `app.get("/u", auth, getUser)`: a route's named handler.
fn last_identifier_argument(node: Node<'_>) -> Option<Node<'_>> {
    let args = node.child_by_field_name("arguments")?;
    let last = args.named_child(u32::try_from(args.named_child_count().checked_sub(1)?).ok()?)?;
    (last.kind() == "identifier" && args.named_child_count() > 1).then_some(last)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArgKeys {
    ValueOrPath,
    PathOnly,
}

/// The path (first string, or a `value`/`path` pair) and the `method` pair
/// of an argument list, in JavaScript or a Java annotation.
fn arguments(source: &str, args: Node<'_>, keys: ArgKeys) -> (Option<String>, Option<String>) {
    let mut path = None;
    let mut method = None;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        match child.kind() {
            "string" | "string_literal" | "template_string" if keys == ArgKeys::ValueOrPath => {
                path.get_or_insert_with(|| text(source, child).to_string());
            }
            // `@GetMapping({"/vets", "/vets.html"})`: the first path.
            "element_value_array_initializer" if keys == ArgKeys::ValueOrPath => {
                if let Some(first) = child.named_child(0) {
                    path.get_or_insert_with(|| text(source, first).to_string());
                }
            }
            "element_value_pair" | "pair" => {
                let (Some(key), Some(value)) = (
                    child.child_by_field_name("key"),
                    child.child_by_field_name("value"),
                ) else {
                    continue;
                };
                let value_text = match value.kind() {
                    "element_value_array_initializer" | "array" => value
                        .named_child(0)
                        .map(|first| text(source, first).to_string()),
                    _ => Some(text(source, value).to_string()),
                };
                match text(source, key) {
                    "value" if keys == ArgKeys::ValueOrPath => {
                        path = path.or(value_text);
                    }
                    "path" => path = value_text.or(path),
                    "method" => method = value_text,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    (path, method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_compiles_for_its_languages() {
        assert_eq!(
            COMPILED.len(),
            PACKS.iter().map(|p| p.languages.len()).sum::<usize>()
        );
    }
}
