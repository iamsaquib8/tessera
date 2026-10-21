//! HTTP keys: `http METHOD /path`, with every path parameter as `{}`.

use std::sync::LazyLock;

use regex::Regex;

static SCHEME_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*://[^/]*").unwrap());
static PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?::\w+\??|\{[^}]*\}|\[\[?\.{0,3}[^\]]*\]\]?|<[^>]*>|\*\*?)$").unwrap()
});

/// `GET`, `POST`, … from a method name, an annotation, or a decorator;
/// `ANY` for `all`, `RequestMapping` and anything unknown.
pub fn method(text: &str) -> String {
    let text = text.trim_matches(['"', '\'', '`']);
    // `getForObject`, `postForEntity`: the verb leads.
    let text = match text.find(|c: char| c.is_ascii_uppercase()) {
        Some(at) if at > 0 && text.starts_with(|c: char| c.is_ascii_lowercase()) => &text[..at],
        _ => text,
    };
    let base = text
        .trim_end_matches("Mapping")
        .rsplit('.')
        .next()
        .unwrap_or(text)
        .to_ascii_uppercase();
    match base.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => base,
        _ => "ANY".to_string(),
    }
}

/// A path as written in source: a string, a template, or a concatenation.
/// Returns the normalised path and a confidence, or `None` when no path can
/// be read from it.
pub fn path(source: &str) -> Option<(String, &'static str)> {
    let source = source.trim();
    let parts = top_level_parts(source);
    let (text, confidence) = if parts.len() > 1 {
        let (text, holes) = concatenation(&parts)?;
        (text, if holes { "low" } else { "high" })
    } else if let Some(inner) = quoted(source, '`') {
        let (text, holes) = template(inner);
        (text, if holes { "medium" } else { "high" })
    } else {
        let inner = quoted(source, '"').or_else(|| quoted(source, '\''))?;
        (inner.to_string(), "high")
    };
    normalise(&text).map(|path| (path, confidence))
}

fn quoted(text: &str, quote: char) -> Option<&str> {
    text.strip_prefix(quote)?.strip_suffix(quote)
}

/// `/users/${id}` becomes `/users/{}`; a leading `${BASE}` is a base URL.
fn template(inner: &str) -> (String, bool) {
    let mut out = String::new();
    let mut holes = false;
    let mut rest = inner;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        // A hole at the very start is a base URL, not a path segment.
        if !(out.is_empty() && start == 0) {
            out.push_str("{}");
        }
        holes = true;
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    (out, holes)
}

/// The operands of a `+` outside quotes: `"/a/" + id` → `"/a/"`, `id`.
fn top_level_parts(source: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;
    for (at, c) in source.char_indices() {
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if matches!(c, '"' | '\'' | '`') => quote = Some(c),
            None if c == '+' => {
                parts.push(source[start..at].trim());
                start = at + 1;
            }
            None => {}
        }
    }
    parts.push(source[start..].trim());
    parts
}

/// `BASE + "/users/" + id` becomes `/users/{}`.
fn concatenation(parts: &[&str]) -> Option<(String, bool)> {
    let mut out = String::new();
    let mut holes = false;
    let mut literal = false;
    for part in parts.iter().copied() {
        if let Some(inner) = quoted(part, '"').or_else(|| quoted(part, '\'')) {
            out.push_str(inner);
            literal = true;
        } else {
            if literal || !out.is_empty() {
                out.push_str("{}");
            }
            holes = true;
        }
    }
    literal.then_some((out, holes))
}

/// Drops scheme, host, query and fragment; parameters become `{}`.
pub fn normalise(text: &str) -> Option<String> {
    let text = SCHEME_HOST.replace(text, "");
    let text = text.split(['?', '#']).next().unwrap_or_default();
    if text.is_empty() {
        return Some("/".to_string());
    }
    if !text.starts_with('/') && !text.starts_with('{') {
        // Relative names (`users`) are paths under a prefix; anything with
        // spaces or no letters is not a path.
        if text.contains(char::is_whitespace) || !text.chars().any(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
    }
    let segments: Vec<String> = text
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|segment| {
            if PARAM.is_match(segment) {
                "{}".to_string()
            } else {
                segment.to_string()
            }
        })
        .collect();
    Some(format!("/{}", segments.join("/")))
}

/// `prefix` and `path` joined: `/users` + `/{}` is `/users/{}`.
pub fn join(prefix: &str, path: &str) -> String {
    let joined = format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    normalise(&joined).unwrap_or(joined)
}

pub fn key(method: &str, path: &str) -> String {
    format!("http {method} {path}")
}

static NEXT_EXPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^export\s+(?:async\s+)?(?:function\s+|const\s+)(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b",
    )
    .unwrap()
});

/// Next.js file routes: `pages/api/**` handles every method at its path;
/// `app/**/route.ts` handles the methods it exports.
pub fn next_routes(path: &str, source: &str) -> Vec<super::IndexedFact> {
    let segments: Vec<&str> = path.split('/').collect();
    let (stem, _) = path.rsplit_once('.').unwrap_or((path, ""));
    let stem_segments: Vec<&str> = stem.split('/').collect();
    let pages = segments.windows(2).position(|w| w == ["pages", "api"]);
    let app = segments.iter().position(|s| *s == "app");
    let route = |parts: &[&str]| -> String {
        let parts: Vec<&str> = parts
            .iter()
            .copied()
            .filter(|p| !(p.starts_with('(') && p.ends_with(')')) && *p != "index")
            .collect();
        normalise(&parts.join("/")).unwrap_or_else(|| "/".to_string())
    };
    let fact = |method: &str, path: String, line: usize, evidence: &str| {
        super::IndexedFact::new(
            super::kind::ROUTE,
            key(method, &path),
            format!("{method} {path}"),
            line,
            "nextjs",
            evidence,
        )
    };
    if let Some(at) = pages {
        let path = route(&stem_segments[at + 1..]);
        let line = source
            .lines()
            .position(|l| l.contains("export default"))
            .map_or(1, |i| i + 1);
        return vec![fact("ANY", path, line, "export default")];
    }
    if let (Some(at), true) = (app, stem_segments.last() == Some(&"route")) {
        let path = route(&stem_segments[at + 1..stem_segments.len() - 1]);
        return NEXT_EXPORT
            .captures_iter(source)
            .map(|c| {
                let offset = c.get(0).map_or(0, |m| m.start());
                let line = source[..offset].matches('\n').count() + 1;
                fact(
                    &c[1],
                    path.clone(),
                    line,
                    c.get(0).map_or("", |m| m.as_str()),
                )
            })
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_normalise_parameters_hosts_and_bases() {
        assert_eq!(path("'/users/:id'"), Some(("/users/{}".into(), "high")));
        assert_eq!(
            path("\"/owners/{ownerId}/pets\""),
            Some(("/owners/{}/pets".into(), "high"))
        );
        assert_eq!(
            path("`/api/users/${id}?x=1`"),
            Some(("/api/users/{}".into(), "medium"))
        );
        assert_eq!(
            path("`${BASE}/api/cart`"),
            Some(("/api/cart".into(), "medium"))
        );
        assert_eq!(
            path("'http://cart:8080/api/cart/'"),
            Some(("/api/cart".into(), "high"))
        );
        assert_eq!(
            path("API + '/users/' + id"),
            Some(("/users/{}".into(), "low"))
        );
        assert_eq!(path("url"), None);
        assert_eq!(
            path("\"/api/\" + id + \"/items\""),
            Some(("/api/{}/items".into(), "low"))
        );
        assert_eq!(
            path("\"http://owners/owners/\" + ownerId + \"/pets\""),
            Some(("/owners/{}/pets".into(), "low"))
        );
        assert_eq!(path("'/a+b'"), Some(("/a+b".into(), "high")));
        assert_eq!(normalise("pages/[id]"), Some("/pages/{}".into()));
        assert_eq!(
            normalise("/owners/*/pets/{petId}"),
            Some("/owners/{}/pets/{}".into())
        );
        assert_eq!(join("/owners", ""), "/owners");
        assert_eq!(join("users", ":id"), "/users/{}");
    }

    #[test]
    fn next_file_routes() {
        let keys = |path: &str, source: &str| -> Vec<String> {
            next_routes(path, source)
                .into_iter()
                .map(|f| f.key)
                .collect()
        };
        assert_eq!(
            keys("web/pages/api/users/[id].ts", "export default h"),
            vec!["http ANY /api/users/{}"]
        );
        assert_eq!(
            keys("pages/api/index.js", "export default h"),
            vec!["http ANY /api"]
        );
        assert_eq!(
            keys(
                "src/app/(shop)/api/items/[id]/route.ts",
                "export async function GET() {}\nexport const POST = h;\n"
            ),
            vec!["http GET /api/items/{}", "http POST /api/items/{}"]
        );
        assert!(keys("src/app/page.tsx", "export default P").is_empty());
    }

    #[test]
    fn methods_from_names_annotations_and_decorators() {
        assert_eq!(method("get"), "GET");
        assert_eq!(method("PostMapping"), "POST");
        assert_eq!(method("RequestMapping"), "ANY");
        assert_eq!(method("RequestMethod.DELETE"), "DELETE");
        assert_eq!(method("all"), "ANY");
        assert_eq!(method("getForObject"), "GET");
        assert_eq!(method("exchange"), "ANY");
    }
}
