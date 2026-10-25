//! `map`: contracts between services, from the facts layer. A contract is
//! one key (a gRPC method, a route, a topic, a table) with the places that
//! define it, provide it, and consume it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::facts::kind;
use crate::output::AgentText;

/// Files that make a directory a service of its own.
const MANIFESTS: &[&str] = &[
    "package.json",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "Cargo.toml",
    "Gemfile",
    "composer.json",
    "Dockerfile",
];
pub const KINDS: &[&str] = &["route", "rpc", "topic", "table", "graphql", "service"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapArgs {
    /// `route`, `rpc`, `topic`, `table`, or `service`; `None` for every
    /// contract kind.
    pub kind: Option<String>,
    /// Keeps contracts whose name, or a file of theirs, contains this.
    pub query: Option<String>,
    /// Contracts (or services) per page; 0 for all of them.
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    pub path: String,
    pub line: usize,
    /// The handler or caller around the site.
    pub symbol: Option<String>,
    #[serde(skip)]
    pub symbol_id: Option<i64>,
    pub service: String,
    pub confidence: String,
    pub rule: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Contract {
    pub kind: String,
    pub name: String,
    pub key: String,
    /// Where the contract is declared (`.proto`, OpenAPI).
    pub definitions: Vec<Site>,
    /// Servers, route handlers, producers, owners.
    pub providers: Vec<Site>,
    /// Clients, callers, consumers.
    pub consumers: Vec<Site>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapResult {
    pub contracts: Vec<Contract>,
    /// With kind `service`: each service and its links to others.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceView>,
    /// Present when a `limit` was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<crate::output::Page>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceView {
    pub name: String,
    /// Contracts this service provides that other services use.
    pub serves: Vec<ServiceLink>,
    /// Contracts of other services this service uses.
    pub calls: Vec<ServiceLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceLink {
    pub kind: String,
    pub contract: String,
    /// The services on the other side.
    pub services: Vec<String>,
    /// This service's sites.
    pub sites: Vec<Site>,
}

#[derive(Debug, Clone)]
struct Fact {
    kind: String,
    key: String,
    name: String,
    path: String,
    line: usize,
    symbol: Option<String>,
    symbol_id: Option<i64>,
    confidence: String,
    rule: String,
    evidence: String,
}

/// Where facts come from: each repository's index, with the prefix that
/// makes its paths unique in a workspace.
pub struct Sources {
    repos: Vec<(String, Connection)>,
    /// The directory paths and services are relative to.
    root: PathBuf,
}

impl Sources {
    /// This repository, or every repository of its workspace.
    pub fn open(conn: &Connection, root: &Path) -> Result<Self> {
        let Some((dir, repos)) = crate::workspace::workspace_repos(root) else {
            return Ok(Self {
                repos: vec![(String::new(), reopen(conn)?)],
                root: root.to_path_buf(),
            });
        };
        let here = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut out = Vec::new();
        for repo in repos {
            let prefix = repo
                .strip_prefix(&dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| repo.to_string_lossy().into_owned());
            let prefix = if prefix.is_empty() {
                String::new()
            } else {
                format!("{prefix}/")
            };
            let conn = if repo == here {
                reopen(conn)?
            } else {
                let ws = crate::workspace::Workspace::resolve(Some(&repo), None, &repo);
                crate::fresh::ensure(&ws)?;
                crate::db::open_ro(&ws.db_path)?
            };
            out.push((prefix, conn));
        }
        Ok(Self {
            repos: out,
            root: dir,
        })
    }

    /// Each repository's path prefix (empty outside a workspace) and index.
    pub fn repos(&self) -> &[(String, Connection)] {
        &self.repos
    }

    fn load(&self, key_prefix: &str) -> Result<Vec<Fact>> {
        let mut out = Vec::new();
        for (path_prefix, conn) in &self.repos {
            let mut facts = load(conn, key_prefix)?;
            if "http ".starts_with(key_prefix) || key_prefix.starts_with("http ") {
                facts = mounted(conn, facts)?;
            }
            if !path_prefix.is_empty() {
                for fact in &mut facts {
                    fact.path = format!("{path_prefix}{}", fact.path);
                }
            }
            out.extend(facts);
        }
        Ok(out)
    }
}

/// A second handle on the same database, so `Sources` owns all of its
/// connections.
fn reopen(conn: &Connection) -> Result<Connection> {
    let path = conn.path().unwrap_or_default().to_string();
    Ok(crate::db::open_ro(Path::new(&path))?)
}

/// Routes registered on a router mounted under a prefix
/// (`app.use("/api", users)`) answer under that prefix, once per mount; the
/// unprefixed copy is not served.
fn mounted(conn: &Connection, facts: Vec<Fact>) -> Result<Vec<Fact>> {
    static RECEIVER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\s*(?:this\.)?(\w+)\s*\.\s*(?:get|post|put|patch|delete|head|options|all|route)\s*\(")
            .unwrap()
    });
    let mut stmt = conn.prepare(
        "SELECT fl.path, substr(f.key, 7), f.name, f.confidence,
                (SELECT b.target_path FROM import_bindings b
                  WHERE b.file_id = f.file_id AND b.local = substr(f.key, 7)
                    AND b.target_path IS NOT NULL LIMIT 1)
         FROM facts f JOIN files fl ON fl.id = f.file_id WHERE f.kind = ?1",
    )?;
    let mounts: Vec<(String, String, String, String, Option<String>)> = stmt
        .query_map([kind::MOUNT], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    if mounts.is_empty() {
        return Ok(facts);
    }
    let mut out = Vec::with_capacity(facts.len());
    for fact in facts {
        let receiver = RECEIVER.captures(&fact.evidence).map(|c| c[1].to_string());
        let parts = fact
            .key
            .strip_prefix("http ")
            .and_then(|k| k.split_once(' '));
        let (Some(receiver), Some((method, path)), true) =
            (receiver, parts, fact.kind == kind::ROUTE)
        else {
            out.push(fact);
            continue;
        };
        let under: Vec<&(String, String, String, String, Option<String>)> = mounts
            .iter()
            .filter(|(file, local, _, _, target)| match target {
                // An imported router: its file's routes, except the app's own.
                Some(target) => {
                    target == &fact.path
                        && !matches!(receiver.as_str(), "app" | "server" | "fastify" | "App")
                }
                None => file == &fact.path && local == &receiver,
            })
            .collect();
        if under.is_empty() {
            out.push(fact);
            continue;
        }
        for (_, _, prefix, confidence, _) in under {
            let full = crate::facts::http::join(prefix, path);
            let mut copy = fact.clone();
            copy.key = crate::facts::http::key(method, &full);
            copy.name = format!("{method} {full}");
            copy.confidence = lower(&fact.confidence, confidence);
            out.push(copy);
        }
    }
    Ok(out)
}

fn load(conn: &Connection, prefix: &str) -> Result<Vec<Fact>> {
    let mut stmt = conn.prepare(
        "SELECT f.kind, f.key, f.name, fl.path, f.line, s.qualified_name, f.confidence, f.rule,
                f.evidence, f.symbol_id
         FROM facts f JOIN files fl ON fl.id = f.file_id
         LEFT JOIN symbols s ON s.id = f.symbol_id
         WHERE f.key LIKE ?1 || '%'
         ORDER BY fl.path, f.line, f.id",
    )?;
    let rows = stmt.query_map([prefix], |row| {
        Ok(Fact {
            kind: row.get(0)?,
            key: row.get(1)?,
            name: row.get(2)?,
            path: row.get(3)?,
            line: row.get::<_, i64>(4)? as usize,
            symbol: row.get(5)?,
            confidence: row.get(6)?,
            rule: row.get(7)?,
            evidence: row.get(8)?,
            symbol_id: row.get(9)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The nearest directory above `path` with a build manifest, relative to
/// the root; the root's own name when there is none.
pub struct Services {
    root: Option<PathBuf>,
    cache: HashMap<String, String>,
}

impl Services {
    pub fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            cache: HashMap::new(),
        }
    }

    /// No labels, for callers that only follow links.
    fn none() -> Self {
        Self {
            root: None,
            cache: HashMap::new(),
        }
    }

    pub fn of(&mut self, path: &str) -> String {
        if self.root.is_none() {
            return String::new();
        }
        let dir = Path::new(path)
            .parent()
            .map(|d| d.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        self.of_dir(&dir)
    }

    fn of_dir(&mut self, dir: &str) -> String {
        if let Some(found) = self.cache.get(dir) {
            return found.clone();
        }
        let root = self.root.clone().unwrap_or_default();
        let absolute = root.join(dir);
        let has_manifest = !dir.is_empty()
            && (MANIFESTS.iter().any(|m| absolute.join(m).is_file())
                || std::fs::read_dir(&absolute).is_ok_and(|entries| {
                    entries
                        .flatten()
                        .any(|e| e.file_name().to_string_lossy().ends_with(".csproj"))
                }));
        let found = if has_manifest {
            dir.to_string()
        } else if dir.is_empty() {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            let parent = Path::new(dir)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.of_dir(&parent)
        };
        self.cache.insert(dir.to_string(), found.clone());
        found
    }
}

impl Fact {
    fn site(&self, services: &mut Services) -> Site {
        Site {
            path: self.path.clone(),
            line: self.line,
            symbol: self.symbol.clone(),
            symbol_id: self.symbol_id,
            service: services.of(&self.path),
            confidence: self.confidence.clone(),
            rule: self.rule.clone(),
            evidence: self.evidence.clone(),
        }
    }

    /// `(service, method)` of a `grpc service/method` key.
    fn rpc(&self) -> Option<(&str, &str)> {
        self.key.strip_prefix("grpc ")?.split_once('/')
    }

    fn dir(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(dir, _)| dir)
    }
}

pub fn run(conn: &Connection, root: &Path, args: &MapArgs) -> Result<MapResult> {
    if let Some(kind) = &args.kind {
        if !KINDS.contains(&kind.as_str()) {
            bail!(
                "unknown contract kind `{kind}`; use one of {}",
                KINDS.join(", ")
            );
        }
    }
    let sources = Sources::open(conn, root)?;
    let mut services = Services::new(&sources.root);
    if args.kind.as_deref() == Some("service") {
        let contracts = all(&sources, &mut services, None)?;
        let (services, page) = page_of(service_views(&contracts, args.query.as_deref()), args)?;
        return Ok(MapResult {
            contracts: Vec::new(),
            services,
            page,
        });
    }
    let mut contracts = all(&sources, &mut services, args.kind.as_deref())?;
    if let Some(query) = &args.query {
        let query = query.to_lowercase();
        contracts.retain(|c| {
            c.name.to_lowercase().contains(&query)
                || c.key.contains(&query)
                || c.providers
                    .iter()
                    .chain(&c.consumers)
                    .any(|s| s.path.to_lowercase().contains(&query))
        });
    }
    let (contracts, page) = page_of(contracts, args)?;
    Ok(MapResult {
        contracts,
        services: Vec::new(),
        page,
    })
}

fn page_of<T>(items: Vec<T>, args: &MapArgs) -> Result<(Vec<T>, Option<crate::output::Page>)> {
    if args.limit == 0 {
        return Ok((items, None));
    }
    let (items, page) = crate::output::paginate(items, args.limit, args.cursor.as_deref())?;
    Ok((items, Some(page)))
}

/// A caller → callee hop a contract adds to the call graph: a client to the
/// server's handler, a producer to the consumer. Each end is a site's path
/// (repository-prefixed in a workspace) and symbol id, so a hop can cross
/// repositories.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hop {
    pub from: (String, i64),
    pub to: (String, i64),
    /// The contract, as `kind name` (`route GET /api/users/{}`).
    pub via: String,
}

/// Every hop the contracts of these sources add.
pub fn hops(sources: &Sources) -> Result<Vec<Hop>> {
    let mut out = Vec::new();
    for contract in all(sources, &mut Services::none(), None)? {
        let (from, to) = match contract.kind.as_str() {
            "table" => continue,
            "topic" => (&contract.providers, &contract.consumers),
            _ => (&contract.consumers, &contract.providers),
        };
        let via = format!("{} {}", contract.kind, contract.name);
        let ends = |sites: &[Site]| -> Vec<(String, i64)> {
            sites
                .iter()
                .filter_map(|s| Some((s.path.clone(), s.symbol_id?)))
                .collect()
        };
        for source in ends(from) {
            for target in ends(to) {
                if source != target {
                    out.push(Hop {
                        from: source.clone(),
                        to: target,
                        via: via.clone(),
                    });
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Each service's links to other services, for `map service [name]`.
/// Upstream is the provider side (servers, producers), downstream the
/// consumer side.
fn service_views(contracts: &[Contract], query: Option<&str>) -> Vec<ServiceView> {
    let mut views: BTreeMap<String, ServiceView> = BTreeMap::new();
    let services =
        |sites: &[Site]| -> BTreeSet<String> { sites.iter().map(|s| s.service.clone()).collect() };
    for contract in contracts.iter().filter(|c| c.kind != "table") {
        let up = services(&contract.providers);
        let down = services(&contract.consumers);
        let sides = [
            (&up, &down, &contract.providers, true),
            (&down, &up, &contract.consumers, false),
        ];
        for (mine, theirs, sites, serves) in sides {
            for service in mine {
                // Calls within the service itself are not links to others.
                let others: Vec<String> =
                    theirs.iter().filter(|s| *s != service).cloned().collect();
                if others.is_empty() && theirs.contains(service) {
                    continue;
                }
                let link = ServiceLink {
                    kind: contract.kind.clone(),
                    contract: contract.name.clone(),
                    services: others,
                    sites: sites
                        .iter()
                        .filter(|s| &s.service == service)
                        .cloned()
                        .collect(),
                };
                let view = views.entry(service.clone()).or_insert_with(|| ServiceView {
                    name: service.clone(),
                    serves: Vec::new(),
                    calls: Vec::new(),
                });
                if serves {
                    view.serves.push(link);
                } else {
                    view.calls.push(link);
                }
            }
        }
    }
    let query = query.map(str::to_lowercase);
    views
        .into_values()
        .filter(|v| {
            query
                .as_ref()
                .is_none_or(|q| v.name.to_lowercase().contains(q))
        })
        .collect()
}

fn all(sources: &Sources, services: &mut Services, only: Option<&str>) -> Result<Vec<Contract>> {
    let wants = |kind: &str| only.is_none_or(|k| k == kind);
    let mut contracts = Vec::new();
    if wants("route") {
        contracts.extend(route_contracts(&sources.load("http ")?, services));
    }
    if wants("rpc") {
        contracts.extend(rpc_contracts(&sources.load("grpc ")?, services));
    }
    if wants("topic") {
        let facts = sources.load("topic ")?;
        contracts.extend(keyed(
            "topic",
            &facts,
            &[],
            &[kind::PRODUCER],
            &[kind::CONSUMER],
            services,
        ));
    }
    if wants("graphql") {
        contracts.extend(keyed(
            "graphql",
            &sources.load("graphql ")?,
            &[kind::GQL_DEF],
            &[kind::GQL_RESOLVER],
            &[kind::GQL_OP],
            services,
        ));
    }
    if wants("table") {
        let mut facts = sources.load("table ")?;
        // JPQL and HQL name the entity class (`FROM PetType`), not its table.
        let tables: BTreeSet<String> = facts
            .iter()
            .filter(|f| f.kind == kind::TABLE || f.kind == kind::ENTITY)
            .map(|f| f.key.clone())
            .collect();
        let by_class: HashMap<String, (String, String)> = facts
            .iter()
            .filter(|f| f.kind == kind::ENTITY)
            .filter_map(|f| {
                let class = f.symbol.as_deref()?.rsplit('.').next()?.to_lowercase();
                Some((format!("table {class}"), (f.key.clone(), f.name.clone())))
            })
            .collect();
        for fact in facts.iter_mut().filter(|f| f.kind == kind::SQL) {
            if !tables.contains(&fact.key) {
                if let Some((key, name)) = by_class.get(&fact.key) {
                    fact.key = key.clone();
                    fact.name = name.clone();
                }
            }
        }
        contracts.extend(keyed(
            "table",
            &facts,
            &[kind::TABLE],
            &[kind::ENTITY],
            &[kind::SQL],
            services,
        ));
    }
    Ok(contracts)
}

/// gRPC: `.proto` methods, with servers and clients matched to them. A
/// side that names no service (a Go method of the right shape, a call on a
/// client variable) is matched by method name among the services its
/// package registers or its file creates clients for.
fn rpc_contracts(facts: &[Fact], services: &mut Services) -> Vec<Contract> {
    let mut contracts: BTreeMap<String, Contract> = BTreeMap::new();
    let mut methods: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut declared_services: BTreeSet<String> = BTreeSet::new();
    for fact in facts.iter().filter(|f| f.kind == kind::RPC_DEF) {
        let Some((service, method)) = fact.rpc() else {
            continue;
        };
        methods
            .entry(method.to_string())
            .or_default()
            .insert(service.to_string());
        declared_services.insert(service.to_string());
        let site = fact.site(services);
        contracts
            .entry(fact.key.clone())
            .or_insert_with(|| contract("rpc", &fact.name, &fact.key))
            .definitions
            .push(site);
    }
    let in_files =
        |kind: &str, by: &dyn Fn(&Fact) -> String| -> HashMap<String, BTreeSet<String>> {
            let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
            for fact in facts.iter().filter(|f| f.kind == kind) {
                let service = fact.key.trim_start_matches("grpc ").to_string();
                out.entry(by(fact)).or_default().insert(service);
            }
            out
        };
    let registered = in_files(kind::RPC_SERVER, &|f: &Fact| f.dir().to_string());
    let clients = in_files(kind::RPC_CLIENT, &|f: &Fact| f.path.clone());
    for fact in facts {
        let (side, scope) = match fact.kind.as_str() {
            k if k == kind::RPC_IMPL => (Side::Provider, registered.get(fact.dir())),
            k if k == kind::RPC_CALL => (Side::Consumer, clients.get(&fact.path)),
            _ => continue,
        };
        let Some((service, method)) = fact.rpc() else {
            continue;
        };
        let declared = methods.get(method);
        let targets: Vec<String> = if service == "*" {
            let candidates: Vec<&String> = declared
                .map(|services| services.iter().collect())
                .unwrap_or_default();
            let scoped: Vec<&String> = candidates
                .iter()
                .copied()
                .filter(|s| scope.is_some_and(|scope| scope.contains(*s)))
                .collect();
            match (side, scoped.is_empty(), candidates.len()) {
                (_, false, _) => scoped.into_iter().cloned().collect(),
                // A Go server's registration can sit in another package.
                (Side::Provider, true, 1) => candidates.into_iter().cloned().collect(),
                _ => Vec::new(),
            }
        } else if declared.is_some_and(|d| d.contains(service))
            // Without a `.proto` for the service, trust the server's own
            // claim; a helper method of a declared service is not an rpc.
            || (side == Side::Provider && !declared_services.contains(service))
        {
            vec![service.to_string()]
        } else {
            Vec::new()
        };
        for target in targets {
            let key = format!("grpc {target}/{method}");
            let name = contracts
                .get(&key)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| fact.name.clone());
            let site = fact.site(services);
            let contract = contracts
                .entry(key.clone())
                .or_insert_with(|| contract("rpc", &name, &key));
            match side {
                Side::Provider => contract.providers.push(site),
                Side::Consumer => contract.consumers.push(site),
            }
        }
    }
    let mut out: Vec<Contract> = contracts.into_values().collect();
    for contract in &mut out {
        for sites in [
            &mut contract.definitions,
            &mut contract.providers,
            &mut contract.consumers,
        ] {
            sites.sort();
            sites.dedup();
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Contracts whose sides share a key exactly: topics (producers and
/// consumers) and tables (migrations, mapped models, SQL in code).
fn keyed(
    contract_kind: &str,
    facts: &[Fact],
    definitions: &[&str],
    providers: &[&str],
    consumers: &[&str],
    services: &mut Services,
) -> Vec<Contract> {
    let mut contracts: BTreeMap<String, Contract> = BTreeMap::new();
    for fact in facts {
        let list = if definitions.contains(&fact.kind.as_str()) {
            0
        } else if providers.contains(&fact.kind.as_str()) {
            1
        } else if consumers.contains(&fact.kind.as_str()) {
            2
        } else {
            continue;
        };
        let site = fact.site(services);
        let contract = contracts
            .entry(fact.key.clone())
            .or_insert_with(|| contract(contract_kind, &fact.name, &fact.key));
        match list {
            0 => contract.definitions.push(site),
            1 => contract.providers.push(site),
            _ => contract.consumers.push(site),
        }
    }
    let mut out: Vec<Contract> = contracts.into_values().collect();
    for contract in &mut out {
        for sites in [
            &mut contract.definitions,
            &mut contract.providers,
            &mut contract.consumers,
        ] {
            sites.sort();
            sites.dedup();
        }
    }
    out
}

/// `(METHOD, path segments)` of an `http METHOD /path` key.
fn http_parts(key: &str) -> Option<(&str, Vec<&str>)> {
    let (method, path) = key.strip_prefix("http ")?.split_once(' ')?;
    Some((method, path.split('/').filter(|s| !s.is_empty()).collect()))
}

/// Whether a request for `call` can reach `route`: the same number of
/// segments, each equal or a parameter on either side. `Some(true)` when
/// every segment is equal.
pub(crate) fn path_match(route: &[&str], call: &[&str]) -> Option<bool> {
    if route.len() != call.len() {
        return None;
    }
    let mut exact = true;
    for (r, c) in route.iter().zip(call) {
        if r == c {
            continue;
        }
        if *r == "{}" || *c == "{}" {
            exact = false;
        } else {
            return None;
        }
    }
    Some(exact)
}

fn lower(a: &str, b: &str) -> String {
    let rank = |c: &str| match c {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    if rank(a) >= rank(b) { a } else { b }.to_string()
}

/// HTTP: routes with their OpenAPI operations and the requests that reach
/// them. A request reaches a route with the same path, segment by segment
/// (`/orders/42` reaches `/orders/{}`), and the same method, or any method
/// when either side does not say.
fn route_contracts(facts: &[Fact], services: &mut Services) -> Vec<Contract> {
    let mut contracts: BTreeMap<String, Contract> = BTreeMap::new();
    for fact in facts.iter().filter(|f| f.kind == kind::ROUTE) {
        let site = fact.site(services);
        contracts
            .entry(fact.key.clone())
            .or_insert_with(|| contract("route", &fact.name, &fact.key))
            .providers
            .push(site);
    }
    let routes: Vec<(String, String, Vec<String>)> = contracts
        .keys()
        .filter_map(|key| {
            let (method, segments) = http_parts(key)?;
            Some((
                key.clone(),
                method.to_string(),
                segments.into_iter().map(str::to_string).collect(),
            ))
        })
        .collect();
    let reach = |method: &str, segments: &[&str]| -> Vec<(String, bool)> {
        let found: Vec<(String, bool)> = routes
            .iter()
            .filter(|(_, m, _)| m == method || m == "ANY" || method == "ANY")
            .filter_map(|(key, _, route)| {
                let route: Vec<&str> = route.iter().map(String::as_str).collect();
                path_match(&route, segments).map(|exact| (key.clone(), exact))
            })
            .collect();
        // An exact path beats a parameter match (`/users/new` over `/users/{}`).
        if found.iter().any(|(_, exact)| *exact) {
            found.into_iter().filter(|(_, exact)| *exact).collect()
        } else {
            found
        }
    };
    let mut extra: BTreeMap<String, Contract> = BTreeMap::new();
    for fact in facts
        .iter()
        .filter(|f| f.kind == kind::ROUTE_DEF || f.kind == kind::HTTP_CALL)
    {
        let Some((method, segments)) = http_parts(&fact.key) else {
            continue;
        };
        let targets = reach(method, &segments);
        let definition = fact.kind == kind::ROUTE_DEF;
        if targets.is_empty() {
            let site = fact.site(services);
            let contract = extra
                .entry(fact.key.clone())
                .or_insert_with(|| contract("route", &fact.name, &fact.key));
            if definition {
                contract.definitions.push(site);
            } else {
                contract.consumers.push(site);
            }
            continue;
        }
        for (key, exact) in targets {
            let mut site = fact.site(services);
            if !exact {
                site.confidence = lower(&site.confidence, "medium");
            }
            let contract = contracts
                .get_mut(&key)
                .expect("route keys come from contracts");
            if definition {
                contract.definitions.push(site);
            } else {
                contract.consumers.push(site);
            }
        }
    }
    let mut out: Vec<Contract> = contracts.into_values().chain(extra.into_values()).collect();
    for contract in &mut out {
        for sites in [
            &mut contract.definitions,
            &mut contract.providers,
            &mut contract.consumers,
        ] {
            sites.sort();
            sites.dedup();
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Provider,
    Consumer,
}

fn contract(kind: &str, name: &str, key: &str) -> Contract {
    Contract {
        kind: kind.to_string(),
        name: name.to_string(),
        key: key.to_string(),
        definitions: Vec::new(),
        providers: Vec::new(),
        consumers: Vec::new(),
    }
}

impl fmt::Display for MapResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for MapResult {
    fn agent_text(&self) -> String {
        let mut out = self.listing();
        if let Some(page) = &self.page {
            out.push_str(&super::find::page_footer(page));
        }
        out
    }
}

impl MapResult {
    fn listing(&self) -> String {
        if !self.services.is_empty() {
            let mut out = String::new();
            for view in &self.services {
                out.push_str(&format!("service {}\n", view.name));
                for (label, links) in [("serves", &view.serves), ("calls", &view.calls)] {
                    for link in links {
                        let at = link
                            .sites
                            .first()
                            .map(|s| format!("  {}:{}", s.path, s.line))
                            .unwrap_or_default();
                        let others = if link.services.is_empty() {
                            String::new()
                        } else if label == "serves" {
                            format!(" to {}", link.services.join(", "))
                        } else {
                            format!(" on {}", link.services.join(", "))
                        };
                        out.push_str(&format!(
                            "  {label:<6} {} {}{others}{at}\n",
                            link.kind, link.contract
                        ));
                    }
                }
            }
            return out;
        }
        if self.contracts.is_empty() {
            return "No contracts found.\n".to_string();
        }
        let mut out = String::new();
        for contract in &self.contracts {
            out.push_str(&format!("{} {}\n", contract.kind, contract.name));
            let labels = match contract.kind.as_str() {
                "topic" => ["declared", "produced by", "consumed by"],
                "table" => ["migrated", "mapped by", "queried by"],
                _ => ["defined", "served by", "called by"],
            };
            let rows = [
                (labels[0], &contract.definitions),
                (labels[1], &contract.providers),
                (labels[2], &contract.consumers),
            ];
            let width = rows
                .iter()
                .flat_map(|(_, sites)| sites.iter())
                .map(|site| site.service.chars().count())
                .max()
                .unwrap_or(0);
            for (label, sites) in rows {
                for site in sites {
                    let symbol = site
                        .symbol
                        .as_deref()
                        .map(|s| format!("  {s}"))
                        .unwrap_or_default();
                    let confidence = if site.confidence == "high" {
                        String::new()
                    } else {
                        format!(" ({})", site.confidence)
                    };
                    out.push_str(&format!(
                        "  {label:<11} {:<width$}  {}:{}{symbol}{confidence}\n",
                        site.service, site.path, site.line
                    ));
                }
            }
            if contract.kind == "rpc"
                && contract.providers.is_empty()
                && !contract.definitions.is_empty()
            {
                out.push_str("  (no server found in this repository)\n");
            }
        }
        out
    }
}
