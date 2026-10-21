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
pub const KINDS: &[&str] = &["rpc"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapArgs {
    /// `rpc`; `None` for every kind.
    pub kind: Option<String>,
    /// Keeps contracts whose name, or a file of theirs, contains this.
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    pub path: String,
    pub line: usize,
    /// The handler or caller around the site.
    pub symbol: Option<String>,
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
}

#[derive(Debug, Clone)]
struct Fact {
    kind: String,
    key: String,
    name: String,
    path: String,
    line: usize,
    symbol: Option<String>,
    confidence: String,
    rule: String,
    evidence: String,
}

fn load(conn: &Connection, prefix: &str) -> Result<Vec<Fact>> {
    let mut stmt = conn.prepare(
        "SELECT f.kind, f.key, f.name, fl.path, f.line, s.qualified_name, f.confidence, f.rule, f.evidence
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
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The nearest directory above `path` with a build manifest, relative to
/// the root; the root's own name when there is none.
pub struct Services {
    root: PathBuf,
    cache: HashMap<String, String>,
}

impl Services {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            cache: HashMap::new(),
        }
    }

    pub fn of(&mut self, path: &str) -> String {
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
        let absolute = self.root.join(dir);
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
            self.root
                .file_name()
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
    let mut services = Services::new(root);
    let mut contracts = rpc_contracts(&load(conn, "grpc ")?, &mut services);
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
    Ok(MapResult { contracts })
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
        if self.contracts.is_empty() {
            return "No contracts found.\n".to_string();
        }
        let mut out = String::new();
        for contract in &self.contracts {
            out.push_str(&format!("{} {}\n", contract.kind, contract.name));
            let rows = [
                ("defined", &contract.definitions),
                ("served by", &contract.providers),
                ("called by", &contract.consumers),
            ];
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
                        "  {label:<9} {:<14} {}:{}{symbol}{confidence}\n",
                        site.service, site.path, site.line
                    ));
                }
            }
            if contract.providers.is_empty() && !contract.definitions.is_empty() {
                out.push_str("  (no server found in this repository)\n");
            }
        }
        out
    }
}
