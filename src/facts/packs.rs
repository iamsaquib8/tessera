//! Framework packs: tree-sitter queries in `packs/*.scm`, read by one
//! capture convention, so a framework is added without touching this code.
//!
//! Captures: `@route`, `@client`, `@producer`, `@consumer`, `@entity` mark
//! the fact and its line; `@method`, `@path`, `@prefix`, `@topic`, `@table`,
//! `@name` and `@handler` carry its parts; `@topic_args` and `@table_args`
//! are annotation or call arguments holding them; `@args` and `@prefix_args` are argument
//! lists whose first string (or `value =`/`path =` pair) is the path or
//! prefix, `@prefix_path` one whose `path =` pair is. `(#set! method "GET")`
//! gives a default. Other captures only serve predicates.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock};

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
        gate: r"\b(app|router|server|routes?|\w*Router|fastify|App)\s*\.\s*(get|post|put|patch|delete|head|options|all|route|use)\s*\(",
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
        id: "spring-kafka",
        languages: JAVA,
        gate: r"KafkaListener|KafkaTemplate|kafkaTemplate|RabbitListener|JmsListener|SqsListener|\.send\(",
        query: include_str!("../../packs/spring-kafka.scm"),
    },
    Pack {
        id: "kafkajs",
        languages: JS,
        gate: r"subscribe\(|\.send\(|\.sendBatch\(|EventPattern|MessagePattern|\.emit\(",
        query: include_str!("../../packs/kafkajs.scm"),
    },
    Pack {
        id: "jpa",
        languages: JAVA,
        gate: r"@Entity|@Table",
        query: include_str!("../../packs/jpa.scm"),
    },
    Pack {
        id: "typeorm",
        languages: TS,
        gate: r"@Entity",
        query: include_str!("../../packs/typeorm.scm"),
    },
    Pack {
        id: "sequelize",
        languages: JS,
        gate: r"\.define\(|tableName",
        query: include_str!("../../packs/sequelize.scm"),
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
    /// Compiled on first use: a short-lived process (a hook) pays only for
    /// the packs its files need.
    query: OnceLock<Query>,
}

impl Compiled {
    fn query(&self) -> &Query {
        self.query.get_or_init(|| {
            Query::new(&support_for(self.language).grammar(), self.pack.query)
                .unwrap_or_else(|e| panic!("pack {} for {}: {e}", self.pack.id, self.language))
        })
    }
}

/// One query per pack and language: a file runs only the packs whose gate
/// it matches, which is cheaper than one query with every pattern.
static COMPILED: LazyLock<Vec<Compiled>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for pack in PACKS {
        for &language in pack.languages {
            out.push(Compiled {
                pack,
                language,
                gate: Regex::new(pack.gate).expect("valid gate"),
                query: OnceLock::new(),
            });
        }
    }
    out
});

const FACT_CAPTURES: &[(&str, &str)] = &[
    ("route", kind::ROUTE),
    ("client", kind::HTTP_CALL),
    ("producer", kind::PRODUCER),
    ("consumer", kind::CONSUMER),
    ("entity", kind::ENTITY),
    ("mount", kind::MOUNT),
];

static JAVA_CONSTANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(?:static\s+final|final\s+static)\s+String\s+(\w+)\s*=\s*"([^"]*)""#).unwrap()
});
static JS_CONSTANT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?:const|let|var)\s+(\w+)\s*=\s*['"]([^'"]+)['"]"#).unwrap());

pub fn run(language: Language, source: &str, tree: &Tree) -> Vec<IndexedFact> {
    let mut out: Vec<(IndexedFact, u8)> = Vec::new();
    for compiled in COMPILED.iter().filter(|c| c.language == language) {
        if !compiled.gate.is_match(source) {
            continue;
        }
        let query = compiled.query();
        let names = query.capture_names();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
        while let Some(m) = matches.next() {
            let mut captures: HashMap<&str, Node<'_>> = HashMap::new();
            for capture in m.captures {
                captures
                    .entry(names[capture.index as usize])
                    .or_insert(capture.node);
            }
            let defaults: HashMap<&str, &str> = query
                .property_settings(m.pattern_index)
                .iter()
                .filter_map(|p| Some((&*p.key, p.value.as_deref()?)))
                .collect();
            out.extend(facts(
                compiled.pack.id,
                language,
                source,
                &captures,
                &defaults,
            ));
        }
    }
    // Overlapping patterns can match one site; keep the most specific. A
    // listener can name several topics on one line, so topics stay apart.
    let site = |f: &IndexedFact| {
        let topic = if f.kind == kind::PRODUCER || f.kind == kind::CONSUMER {
            f.key.clone()
        } else {
            String::new()
        };
        (f.line, f.kind, f.rule, topic)
    };
    out.sort_by(|a, b| site(&a.0).cmp(&site(&b.0)).then(b.1.cmp(&a.1)));
    out.dedup_by(|b, a| site(&a.0) == site(&b.0));
    out.into_iter().map(|(fact, _)| fact).collect()
}

/// The facts a match describes: one for a route, request or entity, one
/// per topic for a listener.
fn facts(
    pack: &'static str,
    language: Language,
    source: &str,
    captures: &HashMap<&str, Node<'_>>,
    defaults: &HashMap<&str, &str>,
) -> Vec<(IndexedFact, u8)> {
    let Some((capture, kind)) = FACT_CAPTURES
        .iter()
        .find(|(capture, _)| captures.contains_key(capture))
    else {
        return Vec::new();
    };
    match *kind {
        kind::PRODUCER | kind::CONSUMER => {
            topics(pack, language, source, captures, kind, captures[capture])
        }
        kind::ENTITY => entity(pack, source, captures, defaults, captures[capture])
            .into_iter()
            .collect(),
        kind::MOUNT => mount(pack, source, captures).into_iter().collect(),
        _ => fact(pack, source, captures, defaults).into_iter().collect(),
    }
}

fn line_of<'s>(source: &'s str, node: Node<'_>) -> &'s str {
    source
        .lines()
        .nth(node.start_position().row)
        .unwrap_or_default()
}

/// String constants in this file: `static final String ORDERS = "orders"`,
/// `const ORDERS = "orders"`.
fn constants(language: Language, source: &str) -> HashMap<String, String> {
    let regex = match language {
        Language::Java => &JAVA_CONSTANT,
        _ => &JS_CONSTANT,
    };
    regex
        .captures_iter(source)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect()
}

/// A topic as written: a string, a constant from this file, a property
/// placeholder, or an array of those.
fn topic_values(
    source: &str,
    node: Node<'_>,
    constants: &HashMap<String, String>,
) -> Vec<(String, &'static str)> {
    match node.kind() {
        "string" | "string_literal" | "template_string" => {
            let text = text(source, node)
                .trim_matches(['"', '\'', '`'])
                .to_string();
            if text.is_empty() {
                Vec::new()
            } else if text.contains("${") || text.contains("#{") {
                vec![(text, "low")]
            } else {
                vec![(text, "high")]
            }
        }
        "array" | "element_value_array_initializer" => {
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
            children
                .into_iter()
                .flat_map(|child| topic_values(source, child, constants))
                .collect()
        }
        "identifier" | "field_access" | "member_expression" => {
            let name = text(source, node);
            let last = name.rsplit('.').next().unwrap_or(name);
            match constants.get(last) {
                Some(value) => vec![(value.clone(), "high")],
                // Both sides naming the same constant still link.
                None => vec![(format!("${name}"), "low")],
            }
        }
        _ => Vec::new(),
    }
}

fn topics(
    pack: &'static str,
    language: Language,
    source: &str,
    captures: &HashMap<&str, Node<'_>>,
    kind: &'static str,
    at: Node<'_>,
) -> Vec<(IndexedFact, u8)> {
    let constants = constants(language, source);
    let mut values = Vec::new();
    if let Some(topic) = captures.get("topic") {
        values = topic_values(source, *topic, &constants);
    } else if let Some(args) = captures.get("topic_args") {
        let mut cursor = args.walk();
        for child in args.named_children(&mut cursor) {
            match child.kind() {
                "element_value_pair" => {
                    let (Some(key), Some(value)) = (
                        child.child_by_field_name("key"),
                        child.child_by_field_name("value"),
                    ) else {
                        continue;
                    };
                    if matches!(
                        text(source, key),
                        "topics" | "queues" | "destination" | "value"
                    ) {
                        values.extend(topic_values(source, value, &constants));
                    } else if text(source, key) == "topicPattern" {
                        values.extend(
                            topic_values(source, value, &constants)
                                .into_iter()
                                .map(|(topic, _)| (topic, "low")),
                        );
                    }
                }
                _ => values.extend(topic_values(source, child, &constants)),
            }
        }
    }
    values
        .into_iter()
        .map(|(topic, confidence)| {
            let mut fact = IndexedFact::new(
                kind,
                format!("topic {}", topic.to_lowercase()),
                topic.clone(),
                at.start_position().row + 1,
                pack,
                line_of(source, at),
            );
            fact.confidence = confidence;
            (fact, 0)
        })
        .collect()
}

/// `app.use("/api", users)`: the prefix, keyed by the mounted name.
fn mount(
    pack: &'static str,
    source: &str,
    captures: &HashMap<&str, Node<'_>>,
) -> Option<(IndexedFact, u8)> {
    let at = *captures.get("mount")?;
    let (prefix, confidence) = http::path(text(source, *captures.get("path")?))?;
    let local = text(source, *captures.get("handler")?).to_string();
    let mut fact = IndexedFact::new(
        kind::MOUNT,
        format!("mount {local}"),
        prefix,
        at.start_position().row + 1,
        pack,
        line_of(source, at),
    );
    fact.confidence = confidence;
    Some((fact, 0))
}

/// `OwnerPet` → `owner_pet`, the table name ORMs derive by default.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn entity(
    pack: &'static str,
    source: &str,
    captures: &HashMap<&str, Node<'_>>,
    defaults: &HashMap<&str, &str>,
    at: Node<'_>,
) -> Option<(IndexedFact, u8)> {
    let explicit = captures
        .get("table")
        .map(|t| text(source, *t).to_string())
        .or_else(|| {
            let args = captures.get("table_args")?;
            let mut cursor = args.walk();
            let children: Vec<Node<'_>> = args.named_children(&mut cursor).collect();
            children.into_iter().find_map(|child| match child.kind() {
                "string" | "string_literal" => Some(text(source, child).to_string()),
                "element_value_pair" | "pair" => {
                    let key = text(source, child.child_by_field_name("key")?);
                    let value = child.child_by_field_name("value")?;
                    matches!(key, "name" | "tableName").then(|| text(source, value).to_string())
                }
                "object" => {
                    let mut cursor = child.walk();
                    let pairs: Vec<Node<'_>> = child.named_children(&mut cursor).collect();
                    pairs.into_iter().find_map(|pair| {
                        let key = text(source, pair.child_by_field_name("key")?);
                        let value = pair.child_by_field_name("value")?;
                        (key == "name").then(|| text(source, value).to_string())
                    })
                }
                _ => None,
            })
        })
        .map(|t| t.trim_matches(['"', '\'', '`']).to_string())
        .filter(|t| !t.is_empty());
    let (table, confidence, specific) = match explicit {
        Some(table) => (table, "high", 2),
        None if defaults.contains_key("derived") || captures.contains_key("table_args") => (
            snake_case(text(source, *captures.get("name")?)),
            "medium",
            0,
        ),
        None => return None,
    };
    let table = table_name(&table);
    let mut fact = IndexedFact::new(
        kind::ENTITY,
        format!("table {table}"),
        table.clone(),
        at.start_position().row + 1,
        pack,
        line_of(source, at),
    );
    fact.confidence = confidence;
    fact.handler = captures.get("name").map(|n| text(source, *n).to_string());
    Some((fact, specific))
}

/// `"public"."Users"` → `users`.
pub fn table_name(raw: &str) -> String {
    let unquoted: String = raw
        .chars()
        .filter(|c| !matches!(c, '"' | '`' | '[' | ']'))
        .collect();
    unquoted
        .rsplit('.')
        .next()
        .unwrap_or(&unquoted)
        .trim()
        .to_lowercase()
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

/// `getUser` in `app.get("/u", auth, getUser)` or in
/// `app.get("/u", users.getUser)`: a route's named handler.
fn last_identifier_argument(node: Node<'_>) -> Option<Node<'_>> {
    let args = node.child_by_field_name("arguments")?;
    let last = args.named_child(u32::try_from(args.named_child_count().checked_sub(1)?).ok()?)?;
    if args.named_child_count() < 2 {
        return None;
    }
    match last.kind() {
        "identifier" => Some(last),
        "member_expression" => last.child_by_field_name("property"),
        _ => None,
    }
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
        for compiled in COMPILED.iter() {
            assert!(compiled.query().pattern_count() > 0, "{}", compiled.pack.id);
        }
    }
}
