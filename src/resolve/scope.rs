use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use super::modules::{ModuleResolver, ModuleTarget};
use crate::types::Language;

/// Kinds that own members: a bare call never reaches a method of these in JS,
/// and `this` walks up to the nearest one.
const CLASS_KINDS: &[&str] = &[
    "class",
    "interface",
    "enum",
    "record",
    "struct",
    "trait",
    "annotation",
];
const TYPE_REF_KINDS: &[&str] = &["extends", "implements", "type"];
/// What a type reference can name: anything with members, or a type alias.
const TYPE_KINDS: &[&str] = &[
    "class",
    "interface",
    "enum",
    "record",
    "struct",
    "trait",
    "annotation",
    "type",
];
/// Objects every JavaScript runtime provides; a call on one is never a call
/// on a repository class unless the file imports or declares the name.
const JS_GLOBALS: &[&str] = &[
    "console",
    "JSON",
    "Math",
    "Object",
    "Array",
    "Promise",
    "Number",
    "String",
    "Boolean",
    "Date",
    "Reflect",
    "Symbol",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "RegExp",
    "BigInt",
    "Atomics",
    "window",
    "document",
    "process",
    "globalThis",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "performance",
    "crypto",
    "Buffer",
    "require",
    "module",
    "exports",
    "Intl",
    "Error",
    "URL",
    "URLSearchParams",
];
/// More name-matched candidates than this is noise, not ambiguity.
const MAX_AMBIGUOUS: usize = 5;
/// Bounds every walk over re-exports, inheritance and call chains.
const MAX_DEPTH: usize = 12;
/// Loaded files kept in memory; a full pass over a large repository would
/// otherwise hold every file's bindings and hints at once.
const FILE_CACHE_LIMIT: usize = 4_096;

#[derive(Debug)]
pub(super) struct Sym {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub qname: String,
    pub kind: String,
    pub container: Option<String>,
    pub exported: bool,
    pub default_export: bool,
    pub arity: Option<(i64, Option<i64>)>,
}

#[derive(Debug)]
pub(super) struct Binding {
    pub id: i64,
    pub local: String,
    pub imported: Option<String>,
    pub source: String,
    pub kind: String,
}

#[derive(Debug)]
struct Hint {
    scope: Option<String>,
    name: String,
    type_name: String,
    origin: String,
}

#[derive(Debug)]
pub(super) struct FileInfo {
    pub path: String,
    pub language: Language,
    module: Option<String>,
    symbols: Vec<Rc<Sym>>,
    by_qname: HashMap<String, Rc<Sym>>,
    pub bindings: Vec<Binding>,
    hints: Vec<Hint>,
}

#[derive(Debug)]
pub(super) struct Ref {
    pub id: i64,
    pub name: String,
    /// `None` for module-level code.
    pub from: Option<i64>,
    pub kind: String,
    receiver: String,
    receiver_name: Option<String>,
    arg_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Resolution {
    pub targets: Vec<i64>,
    pub confidence: &'static str,
    pub resolver: Option<&'static str>,
    /// For resolver `missing`: the repository file that should define the name.
    pub missing_in: Option<String>,
    /// The call's argument count fits none of the candidates.
    pub arity_mismatch: bool,
}

impl Resolution {
    fn unresolved() -> Self {
        Self {
            targets: Vec::new(),
            confidence: "name",
            resolver: None,
            missing_in: None,
            arity_mismatch: false,
        }
    }

    /// The name points into the repository, and nothing there has it.
    fn missing(path: String) -> Self {
        Self {
            confidence: "high",
            resolver: Some("missing"),
            missing_in: Some(path),
            ..Self::unresolved()
        }
    }

    fn external() -> Self {
        Self {
            resolver: Some("external"),
            ..Self::unresolved()
        }
    }

    fn high(targets: Vec<i64>, resolver: &'static str) -> Self {
        Self {
            targets,
            confidence: "high",
            resolver: Some(resolver),
            ..Self::unresolved()
        }
    }

    fn capped(mut self, confidence: &'static str) -> Self {
        if self.confidence == "high" && !self.targets.is_empty() {
            self.confidence = confidence;
        }
        self
    }
}

/// What a name or module path leads to.
#[derive(Debug, Clone)]
enum Found {
    Symbols(Vec<i64>),
    External,
    Missing,
    /// A repository module or class (this file) was searched and lacks the name.
    Absent(String),
}

/// What a receiver expression stands for.
enum Receiver {
    Module(i64),
    Class(Vec<i64>, &'static str, &'static str),
    External,
    Unknown,
}

pub(super) struct Resolver<'c> {
    conn: &'c Connection,
    root: PathBuf,
    pub modules: ModuleResolver,
    files: HashMap<i64, Rc<FileInfo>>,
    symbols: HashMap<i64, Rc<Sym>>,
    by_name: HashMap<String, Rc<Vec<i64>>>,
    supers: HashMap<i64, Rc<Vec<i64>>>,
    module_cache: HashMap<(String, String), ModuleTarget>,
    binding_cache: HashMap<(String, String), Found>,
    candidate_cache: HashMap<(String, bool, bool), Rc<Vec<i64>>>,
}

impl<'c> Resolver<'c> {
    pub fn new(
        conn: &'c Connection,
        root: &Path,
        modules: ModuleResolver,
        preload: bool,
    ) -> Result<Self> {
        let mut resolver = Self {
            conn,
            root: root.to_path_buf(),
            modules,
            files: HashMap::new(),
            symbols: HashMap::new(),
            by_name: HashMap::new(),
            supers: HashMap::new(),
            module_cache: HashMap::new(),
            binding_cache: HashMap::new(),
            candidate_cache: HashMap::new(),
        };
        if preload {
            resolver.preload()?;
        }
        Ok(resolver)
    }

    fn preload(&mut self) -> Result<()> {
        let mut stmt = self.conn.prepare(SYMBOL_COLUMNS_SQL)?;
        let rows = stmt.query_map([], map_sym)?;
        let mut by_name: HashMap<String, Vec<i64>> = HashMap::new();
        for row in rows {
            let sym = Rc::new(row?);
            by_name.entry(sym.name.clone()).or_default().push(sym.id);
            self.symbols.insert(sym.id, sym);
        }
        self.by_name = by_name.into_iter().map(|(k, v)| (k, Rc::new(v))).collect();
        Ok(())
    }

    /// Module resolution is a pure function of the importing directory and the
    /// specifier, and the same pair recurs for every reference in a file.
    fn module(&mut self, from_file: &str, source: &str, language: Language) -> ModuleTarget {
        let dir = from_file.rsplit_once('/').map_or("", |(dir, _)| dir);
        let key = (format!("{}:{dir}", language.family()), source.to_string());
        if let Some(target) = self.module_cache.get(&key) {
            return target.clone();
        }
        let target = self.modules.resolve(from_file, source, language);
        self.module_cache.insert(key, target.clone());
        target
    }

    pub fn file(&mut self, file_id: i64) -> Result<Rc<FileInfo>> {
        if let Some(file) = self.files.get(&file_id) {
            return Ok(Rc::clone(file));
        }
        if self.files.len() >= FILE_CACHE_LIMIT {
            self.files.clear();
        }
        let (path, language, module): (String, String, Option<String>) = self.conn.query_row(
            "SELECT path, language, module FROM files WHERE id = ?1",
            params![file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let mut stmt = self
            .conn
            .prepare_cached(&format!("{SYMBOL_COLUMNS_SQL} WHERE file_id = ?1"))?;
        let symbols: Vec<Rc<Sym>> = stmt
            .query_map(params![file_id], map_sym)?
            .map(|row| row.map(Rc::new))
            .collect::<rusqlite::Result<_>>()?;
        let mut by_qname = HashMap::new();
        for sym in &symbols {
            by_qname
                .entry(sym.qname.clone())
                .or_insert_with(|| Rc::clone(sym));
            self.symbols.entry(sym.id).or_insert_with(|| Rc::clone(sym));
        }
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, local, imported, source, kind FROM import_bindings WHERE file_id = ?1 ORDER BY id",
        )?;
        let bindings = stmt
            .query_map(params![file_id], |row| {
                Ok(Binding {
                    id: row.get(0)?,
                    local: row.get(1)?,
                    imported: row.get(2)?,
                    source: row.get(3)?,
                    kind: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT scope, name, type_name, origin FROM type_hints WHERE file_id = ?1 ORDER BY id",
        )?;
        let hints = stmt
            .query_map(params![file_id], |row| {
                Ok(Hint {
                    scope: row.get(0)?,
                    name: row.get(1)?,
                    type_name: row.get(2)?,
                    origin: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let file = Rc::new(FileInfo {
            path,
            language: Language::from_name(&language).unwrap_or(Language::JavaScript),
            module,
            symbols,
            by_qname,
            bindings,
            hints,
        });
        self.files.insert(file_id, Rc::clone(&file));
        Ok(file)
    }

    pub fn refs(&self, file_id: i64) -> Result<Vec<Ref>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, symbol_name, from_symbol_id, kind, receiver, receiver_name, arg_count
             FROM refs WHERE file_id = ?1 ORDER BY id",
        )?;
        let refs = stmt
            .query_map(params![file_id], |row| {
                Ok(Ref {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    from: row.get(2)?,
                    kind: row.get(3)?,
                    receiver: row.get(4)?,
                    receiver_name: row.get(5)?,
                    arg_count: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(refs)
    }

    fn symbol(&mut self, id: i64) -> Result<Option<Rc<Sym>>> {
        if let Some(sym) = self.symbols.get(&id) {
            return Ok(Some(Rc::clone(sym)));
        }
        let sym = self
            .conn
            .prepare_cached(&format!("{SYMBOL_COLUMNS_SQL} WHERE id = ?1"))?
            .query_row(params![id], map_sym)
            .optional()?
            .map(Rc::new);
        if let Some(sym) = &sym {
            self.symbols.insert(id, Rc::clone(sym));
        }
        Ok(sym)
    }

    fn named(&mut self, name: &str) -> Result<Rc<Vec<i64>>> {
        if let Some(ids) = self.by_name.get(name) {
            return Ok(Rc::clone(ids));
        }
        let ids: Vec<i64> = self
            .conn
            .prepare_cached("SELECT id FROM symbols WHERE name = ?1 ORDER BY id")?
            .query_map(params![name], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let ids = Rc::new(ids);
        self.by_name.insert(name.to_string(), Rc::clone(&ids));
        Ok(ids)
    }

    /// The module a binding's source names, as a root-relative path.
    pub fn binding_target_path(&mut self, file: &FileInfo, binding: &Binding) -> Option<String> {
        let source = match binding.kind.as_str() {
            "java_wildcard" => return None,
            "java_static" => binding.source.clone(),
            _ if binding.source == "." => return None,
            _ => binding.source.clone(),
        };
        match self.module(&file.path, &source, file.language) {
            ModuleTarget::File(id) => self.file(id).ok().map(|f| f.path.clone()),
            _ => None,
        }
    }

    /// `ok`, `missing_module` when a local specifier resolves to no file, or
    /// `missing_export` when a repository module lacks the imported name.
    pub fn binding_status(
        &mut self,
        file: &Rc<FileInfo>,
        binding: &Binding,
    ) -> Result<&'static str> {
        let local = binding.source.starts_with('.');
        match binding.kind.as_str() {
            "named" | "default" | "require_member" | "require" | "namespace" => {
                match self.module(&file.path, &binding.source, file.language) {
                    ModuleTarget::Unresolved if local => Ok("missing_module"),
                    ModuleTarget::File(target)
                        if binding.kind != "namespace" && binding.kind != "require" =>
                    {
                        let export = match binding.kind.as_str() {
                            "default" => "default".to_string(),
                            _ => binding
                                .imported
                                .clone()
                                .unwrap_or_else(|| binding.local.clone()),
                        };
                        Ok(
                            match self.export_lookup(target, &export, &mut HashSet::new(), 0)? {
                                Found::Missing => "missing_export",
                                _ => "ok",
                            },
                        )
                    }
                    _ => Ok("ok"),
                }
            }
            // Source "." is this file (`export default Panel`), not a path.
            "re_export" | "re_export_all" if local && binding.source != "." => Ok(
                match self.module(&file.path, &binding.source, file.language) {
                    ModuleTarget::Unresolved => "missing_module",
                    _ => "ok",
                },
            ),
            "java_single" => {
                let package = binding
                    .source
                    .rsplit_once('.')
                    .map_or("", |(package, _)| package);
                Ok(match self.java_import(&binding.source, &binding.local)? {
                    Found::Absent(_) => "missing_export",
                    Found::External if self.modules.java_package_exists(package) => {
                        "missing_export"
                    }
                    _ => "ok",
                })
            }
            _ => Ok("ok"),
        }
    }

    pub fn resolve_ref(&mut self, file: &Rc<FileInfo>, reference: &Ref) -> Result<Resolution> {
        let chain = self.scope_chain(file, reference.from)?;
        let name = reference.name.as_str();
        let arity = reference.arg_count;
        let resolution = match reference.receiver.as_str() {
            "this" | "super" => {
                let Some(class) = self.enclosing_class(file, &chain) else {
                    return Ok(Resolution::unresolved());
                };
                let start = if reference.receiver == "super" {
                    self.supers(class)?.to_vec()
                } else {
                    vec![class]
                };
                let mut found = Vec::new();
                for class in start {
                    found.extend(self.members(class, name, 0)?);
                }
                self.finish(found, arity, Resolution::high(Vec::new(), "this"))?
            }
            "name" => {
                let receiver = reference.receiver_name.clone().unwrap_or_default();
                match self.receiver(file, &chain, &receiver, 0)? {
                    Receiver::Module(module) => {
                        match self.export_lookup(module, name, &mut HashSet::new(), 0)? {
                            Found::Symbols(ids) => {
                                self.finish(ids, arity, Resolution::high(Vec::new(), "import"))?
                            }
                            Found::External => Resolution::external(),
                            Found::Absent(path) => Resolution::missing(path),
                            Found::Missing => Resolution::missing(self.file(module)?.path.clone()),
                        }
                    }
                    Receiver::Class(classes, confidence, resolver) => {
                        let mut found = Vec::new();
                        for class in &classes {
                            found.extend(self.members(*class, name, 0)?);
                        }
                        if found.is_empty() && confidence == "high" {
                            if let Some(path) = self.closed_class_file(&classes)? {
                                if !self.declares_property(&classes, name, 0)? {
                                    return Ok(Resolution::missing(path));
                                }
                            }
                        }
                        if found.is_empty() {
                            Resolution::unresolved()
                        } else {
                            self.finish(found, arity, Resolution::high(Vec::new(), resolver))?
                                .capped(confidence)
                        }
                    }
                    Receiver::External => Resolution::external(),
                    Receiver::Unknown => self
                        .by_name(name, arity, true, None, reference.from)?
                        .capped("medium"),
                }
            }
            // A call on some other object is rarely a call on the caller itself.
            "other" => self
                .by_name(name, arity, true, None, reference.from)?
                .capped("medium"),
            _ if reference.kind == "new"
                || reference.kind == "jsx"
                || reference.kind == "decorator"
                || TYPE_REF_KINDS.contains(&reference.kind.as_str()) =>
            {
                let types_only = TYPE_REF_KINDS.contains(&reference.kind.as_str());
                self.resolve_type(file, &chain, name, types_only, 0)?
            }
            _ => self.resolve_bare(file, &chain, name, arity)?,
        };
        Ok(resolution)
    }

    fn finish(
        &mut self,
        found: Vec<i64>,
        arity: Option<i64>,
        template: Resolution,
    ) -> Result<Resolution> {
        if found.is_empty() {
            return Ok(Resolution::unresolved());
        }
        let arity_mismatch = self.no_arity_fits(&found, arity)?;
        let targets = self.filter_arity(found, arity)?;
        Ok(Resolution {
            targets,
            arity_mismatch,
            ..template
        })
    }

    /// Drops candidates whose parameter count cannot take `arity` arguments,
    /// unless that would drop them all.
    /// True when every candidate declares its parameters and none accepts
    /// `arity` arguments.
    fn no_arity_fits(&mut self, ids: &[i64], arity: Option<i64>) -> Result<bool> {
        let Some(count) = arity else {
            return Ok(false);
        };
        for id in ids {
            match self.symbol(*id)?.and_then(|sym| sym.arity) {
                Some((min, max)) if count >= min && max.is_none_or(|max| count <= max) => {
                    return Ok(false)
                }
                Some(_) => {}
                None => return Ok(false),
            }
        }
        Ok(!ids.is_empty())
    }

    fn filter_arity(&mut self, mut ids: Vec<i64>, arity: Option<i64>) -> Result<Vec<i64>> {
        ids.sort_unstable();
        ids.dedup();
        let Some(count) = arity else {
            return Ok(ids);
        };
        let mut kept = Vec::new();
        for id in &ids {
            let fits = self.symbol(*id)?.is_none_or(|sym| match sym.arity {
                Some((min, max)) => count >= min && max.is_none_or(|max| count <= max),
                None => true,
            });
            if fits {
                kept.push(*id);
            }
        }
        Ok(if kept.is_empty() { ids } else { kept })
    }

    /// Innermost first: the referencing symbol, its containers, then the file.
    fn scope_chain(&mut self, file: &FileInfo, from: Option<i64>) -> Result<Vec<Option<String>>> {
        let mut chain = Vec::new();
        let mut current = match from {
            Some(from) => self.symbol(from)?,
            None => None,
        };
        while let Some(sym) = current {
            if chain.len() > MAX_DEPTH {
                break;
            }
            chain.push(Some(sym.qname.clone()));
            current = sym
                .container
                .as_ref()
                .and_then(|c| file.by_qname.get(c).cloned());
        }
        chain.push(None);
        Ok(chain)
    }

    fn enclosing_class(&self, file: &FileInfo, chain: &[Option<String>]) -> Option<i64> {
        chain.iter().flatten().find_map(|qname| {
            file.by_qname
                .get(qname)
                .filter(|sym| CLASS_KINDS.contains(&sym.kind.as_str()))
                .map(|sym| sym.id)
        })
    }

    fn is_member(&self, file: &FileInfo, sym: &Sym) -> bool {
        sym.container.as_ref().is_some_and(|c| {
            file.by_qname
                .get(c)
                .is_some_and(|owner| CLASS_KINDS.contains(&owner.kind.as_str()))
        })
    }

    /// A call with no receiver.
    fn resolve_bare(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        name: &str,
        arity: Option<i64>,
    ) -> Result<Resolution> {
        if file.language == Language::Java {
            if let Some(class) = self.enclosing_class(file, chain) {
                let found = self.members(class, name, 0)?;
                if !found.is_empty() {
                    return self.finish(found, arity, Resolution::high(Vec::new(), "this"));
                }
            }
            return Ok(match self.binding_symbols(file, name)? {
                Found::Symbols(ids) => {
                    self.finish(ids, arity, Resolution::high(Vec::new(), "import"))?
                }
                Found::External => Resolution::external(),
                Found::Absent(path) => Resolution::missing(path),
                Found::Missing => Resolution::unresolved(),
            });
        }
        let local = self.local(file, chain, name, false);
        if !local.is_empty() {
            return self.finish(local, arity, Resolution::high(Vec::new(), "local"));
        }
        if name == "require" {
            return Ok(Resolution::external());
        }
        match self.binding_symbols(file, name)? {
            Found::Symbols(ids) => {
                return self.finish(ids, arity, Resolution::high(Vec::new(), "import"))
            }
            Found::External => return Ok(Resolution::external()),
            Found::Absent(path) => return Ok(Resolution::missing(path)),
            Found::Missing if self.has_binding(file, name) => return Ok(Resolution::unresolved()),
            Found::Missing => {}
        }
        self.by_name(name, arity, false, None, None)
    }

    /// A name used as a type, class, component or decorator.
    fn resolve_type(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        name: &str,
        types_only: bool,
        depth: usize,
    ) -> Result<Resolution> {
        if depth > MAX_DEPTH {
            return Ok(Resolution::unresolved());
        }
        let local = self.local(file, chain, name, types_only);
        if !local.is_empty() {
            return self.finish(local, None, Resolution::high(Vec::new(), "local"));
        }
        match self.binding_symbols(file, name)? {
            Found::Symbols(ids) => {
                return self.finish(ids, None, Resolution::high(Vec::new(), "import"))
            }
            Found::External => return Ok(Resolution::external()),
            Found::Absent(path) => return Ok(Resolution::missing(path)),
            Found::Missing if self.has_binding(file, name) => return Ok(Resolution::unresolved()),
            Found::Missing => {}
        }
        if file.language == Language::Java {
            let package = file.module.clone().unwrap_or_default();
            if let Some(id) = self.java_class_symbol(&package, name)? {
                return Ok(Resolution::high(vec![id], "package"));
            }
            return Ok(Resolution::unresolved());
        }
        let kinds = types_only.then_some(TYPE_KINDS);
        self.by_name(name, None, false, kinds, None)
    }

    /// Definitions in this file visible from `chain`, innermost scope first.
    fn local(
        &self,
        file: &FileInfo,
        chain: &[Option<String>],
        name: &str,
        types_only: bool,
    ) -> Vec<i64> {
        for scope in chain {
            let found: Vec<i64> = file
                .symbols
                .iter()
                .filter(|sym| sym.name == name && sym.container == *scope)
                .filter(|sym| !types_only || TYPE_KINDS.contains(&sym.kind.as_str()))
                .filter(|sym| !self.is_member(file, sym))
                // `const x = require("./x")` is an import, not a definition.
                .filter(|sym| sym.kind != "variable" || !self.has_binding(file, &sym.name))
                .map(|sym| sym.id)
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    fn has_binding(&self, file: &FileInfo, name: &str) -> bool {
        file.bindings.iter().any(|b| b.local == name)
    }

    /// The symbols an imported name stands for, cached per file and name.
    fn binding_symbols(&mut self, file: &Rc<FileInfo>, name: &str) -> Result<Found> {
        let key = (file.path.clone(), name.to_string());
        if let Some(found) = self.binding_cache.get(&key) {
            return Ok(found.clone());
        }
        let found = self.binding_symbols_uncached(file, name)?;
        self.binding_cache.insert(key, found.clone());
        Ok(found)
    }

    fn binding_symbols_uncached(&mut self, file: &Rc<FileInfo>, name: &str) -> Result<Found> {
        let bindings: Vec<(String, Option<String>, String, bool)> = file
            .bindings
            .iter()
            .filter(|b| b.local == name || (b.local == "*" && b.kind.starts_with("java")))
            .map(|b| {
                (
                    b.kind.clone(),
                    b.imported.clone(),
                    b.source.clone(),
                    b.local == "*",
                )
            })
            .collect();
        let mut external = false;
        let mut absent = None;
        for (kind, imported, source, wildcard) in bindings {
            let found = match kind.as_str() {
                "named" | "require_member" | "default" | "require" => {
                    let export = if kind == "default" || kind == "require" {
                        "default".to_string()
                    } else {
                        imported.unwrap_or_else(|| name.to_string())
                    };
                    match self.module(&file.path, &source, file.language) {
                        ModuleTarget::File(target) => {
                            match self.export_lookup(target, &export, &mut HashSet::new(), 0)? {
                                Found::Missing => Found::Absent(self.file(target)?.path.clone()),
                                other => other,
                            }
                        }
                        ModuleTarget::External(_) => Found::External,
                        ModuleTarget::Unresolved => Found::Missing,
                    }
                }
                "java_single" => self.java_import(&source, name)?,
                "java_wildcard" => match self.java_class_symbol(&source, name)? {
                    Some(id) => Found::Symbols(vec![id]),
                    None => Found::Missing,
                },
                "java_static" => match self.java_import(&source, "")? {
                    Found::Symbols(classes) => {
                        let mut members = Vec::new();
                        for class in &classes {
                            members.extend(self.members(*class, name, 0)?);
                        }
                        if !members.is_empty() {
                            Found::Symbols(members)
                        } else if wildcard {
                            // `import static a.B.*` only says where a member
                            // might come from.
                            Found::Missing
                        } else {
                            match self.symbol(classes[0])? {
                                Some(class) => {
                                    Found::Absent(self.file(class.file_id)?.path.clone())
                                }
                                None => Found::Missing,
                            }
                        }
                    }
                    other => other,
                },
                _ => Found::Missing,
            };
            match found {
                Found::Symbols(ids) => return Ok(Found::Symbols(ids)),
                // A wildcard only says where a name might come from.
                Found::External if wildcard => {}
                Found::External => external = true,
                Found::Absent(path) => absent = Some(path),
                Found::Missing => {}
            }
        }
        Ok(if external {
            Found::External
        } else if let Some(path) = absent {
            Found::Absent(path)
        } else {
            Found::Missing
        })
    }

    /// A Java import `a.b.C` (or `a.b.Outer.Inner`): the class symbol named
    /// `name`, or the last segment when `name` is empty.
    fn java_import(&mut self, fqn: &str, name: &str) -> Result<Found> {
        let class = if name.is_empty() {
            fqn.rsplit('.').next().unwrap_or(fqn)
        } else {
            name
        };
        match self.module("", fqn, Language::Java) {
            ModuleTarget::File(target) => {
                let file = self.file(target)?;
                let ids: Vec<i64> = file
                    .symbols
                    .iter()
                    .filter(|s| s.name == class && CLASS_KINDS.contains(&s.kind.as_str()))
                    .map(|s| s.id)
                    .collect();
                Ok(if ids.is_empty() {
                    Found::Absent(file.path.clone())
                } else {
                    Found::Symbols(ids)
                })
            }
            _ => Ok(Found::External),
        }
    }

    fn java_class_symbol(&mut self, package: &str, class: &str) -> Result<Option<i64>> {
        let Some(target) = self.modules.java_class(package, class) else {
            return Ok(None);
        };
        let file = self.file(target)?;
        Ok(file
            .symbols
            .iter()
            .find(|s| s.name == class && s.container.is_none())
            .map(|s| s.id))
    }

    /// What `name` exports from `file_id`, following re-exports. `visited`
    /// stops barrel cycles.
    fn export_lookup(
        &mut self,
        file_id: i64,
        name: &str,
        visited: &mut HashSet<(i64, String)>,
        depth: usize,
    ) -> Result<Found> {
        if depth > MAX_DEPTH || !visited.insert((file_id, name.to_string())) {
            return Ok(Found::Missing);
        }
        let file = self.file(file_id)?;
        let top_level = |predicate: &dyn Fn(&Sym) -> bool| -> Vec<i64> {
            file.symbols
                .iter()
                .filter(|s| s.container.is_none() && predicate(s))
                .map(|s| s.id)
                .collect()
        };
        let direct = if name == "default" {
            top_level(&|s| s.default_export)
        } else {
            top_level(&|s| s.name == name && s.exported)
        };
        if !direct.is_empty() {
            return Ok(Found::Symbols(direct));
        }
        let mut external = false;
        let re_exports: Vec<(String, Option<String>, String)> = file
            .bindings
            .iter()
            .filter(|b| (b.kind == "re_export" && b.local == name) || b.kind == "re_export_all")
            .map(|b| (b.kind.clone(), b.imported.clone(), b.source.clone()))
            .collect();
        for (kind, imported, source) in re_exports {
            if kind == "re_export_all" && name == "default" {
                continue;
            }
            let wanted = if kind == "re_export" {
                imported.unwrap_or_else(|| name.to_string())
            } else {
                name.to_string()
            };
            let found = if source == "." {
                let local = top_level(&|s| s.name == wanted);
                if local.is_empty() {
                    self.binding_symbols(&file, &wanted)?
                } else {
                    Found::Symbols(local)
                }
            } else {
                match self.module(&file.path, &source, file.language) {
                    ModuleTarget::File(target) => {
                        self.export_lookup(target, &wanted, visited, depth + 1)?
                    }
                    ModuleTarget::External(_) => Found::External,
                    ModuleTarget::Unresolved => Found::Missing,
                }
            };
            match found {
                Found::Symbols(ids) => return Ok(Found::Symbols(ids)),
                Found::External => external = true,
                Found::Missing | Found::Absent(_) => {}
            }
        }
        Ok(if external {
            Found::External
        } else {
            Found::Missing
        })
    }

    /// What a receiver name refers to: a module, a class (through a type hint
    /// or as a static receiver), something external, or nothing known.
    fn receiver(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        receiver: &str,
        depth: usize,
    ) -> Result<Receiver> {
        if depth > MAX_DEPTH {
            return Ok(Receiver::Unknown);
        }
        if let Some(class) = receiver.strip_prefix("new:") {
            let resolution = self.resolve_qualified_type(file, chain, class, depth)?;
            return Ok(match resolution.confidence {
                _ if resolution.targets.is_empty() => Receiver::External,
                "high" => Receiver::Class(resolution.targets, "high", "new"),
                _ => Receiver::Unknown,
            });
        }
        if let Some((type_name, origin)) = self.hint(file, chain, receiver) {
            return self.typed(file, chain, &type_name, &origin, depth);
        }
        let module_binding = file
            .bindings
            .iter()
            .find(|b| b.local == receiver && matches!(b.kind.as_str(), "namespace" | "require"))
            .map(|b| b.source.clone());
        if let Some(source) = module_binding {
            return Ok(match self.module(&file.path, &source, file.language) {
                ModuleTarget::File(id) => Receiver::Module(id),
                ModuleTarget::External(_) => Receiver::External,
                ModuleTarget::Unresolved => Receiver::Unknown,
            });
        }
        if receiver.contains('.') {
            return self.receiver_path(file, chain, receiver, depth);
        }
        if is_js(file.language)
            && JS_GLOBALS.contains(&receiver)
            && !self.has_binding(file, receiver)
            && self.local(file, chain, receiver, false).is_empty()
        {
            return Ok(Receiver::External);
        }
        if receiver.starts_with(|c: char| c.is_uppercase()) {
            let resolution = self.resolve_type(file, chain, receiver, false, depth + 1)?;
            if resolution.confidence == "high" {
                if let [variable] = resolution.targets[..] {
                    if self
                        .symbol(variable)?
                        .is_some_and(|sym| sym.kind == "variable")
                    {
                        return self.variable_type(variable, depth);
                    }
                }
                return Ok(Receiver::Class(resolution.targets, "high", "static"));
            }
            if resolution.resolver == Some("external") {
                return Ok(Receiver::External);
            }
        }
        Ok(Receiver::Unknown)
    }

    /// The type of a variable defined elsewhere (`export const Factory = new
    /// FactoryStatic()`), from the hint in its own file.
    fn variable_type(&mut self, variable: i64, depth: usize) -> Result<Receiver> {
        let Some(sym) = self.symbol(variable)? else {
            return Ok(Receiver::Unknown);
        };
        let file = self.file(sym.file_id)?;
        let chain = vec![sym.container.clone(), None];
        match self.hint(&file, &chain, &sym.name) {
            Some((type_name, origin)) => self.typed(&file, &chain, &type_name, &origin, depth + 1),
            None => Ok(Receiver::Unknown),
        }
    }

    /// `a.b.c`: resolve `a.b`, then the declared type of its field `c`.
    fn receiver_path(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        path: &str,
        depth: usize,
    ) -> Result<Receiver> {
        let Some((head, field)) = path.rsplit_once('.') else {
            return Ok(Receiver::Unknown);
        };
        let base = if head == "this" {
            match self.enclosing_class(file, chain) {
                Some(class) => Receiver::Class(vec![class], "high", "this"),
                None => Receiver::Unknown,
            }
        } else {
            self.receiver(file, chain, head, depth + 1)?
        };
        match base {
            Receiver::Class(classes, confidence, _) => {
                for class in classes {
                    let Some((owner, owner_chain, type_name, origin)) =
                        self.field_type(class, field, 0)?
                    else {
                        continue;
                    };
                    return Ok(
                        match self.typed(&owner, &owner_chain, &type_name, &origin, depth + 1)? {
                            Receiver::Class(types, field_confidence, _) => {
                                Receiver::Class(types, weaker(confidence, field_confidence), "hint")
                            }
                            other => other,
                        },
                    );
                }
                Ok(Receiver::Unknown)
            }
            Receiver::Module(module) => Ok(
                match self.export_lookup(module, field, &mut HashSet::new(), 0)? {
                    Found::Symbols(ids) => {
                        let mut classes = Vec::new();
                        for id in ids {
                            if self
                                .symbol(id)?
                                .is_some_and(|s| CLASS_KINDS.contains(&s.kind.as_str()))
                            {
                                classes.push(id);
                            }
                        }
                        if classes.is_empty() {
                            Receiver::Unknown
                        } else {
                            Receiver::Class(classes, "high", "import")
                        }
                    }
                    Found::External => Receiver::External,
                    Found::Missing | Found::Absent(_) => Receiver::Unknown,
                },
            ),
            other => Ok(other),
        }
    }

    /// The declared type of `field` on `class` or a supertype, with the file
    /// and scope chain its type name resolves in.
    #[allow(clippy::type_complexity)]
    fn field_type(
        &mut self,
        class: i64,
        field: &str,
        depth: usize,
    ) -> Result<Option<(Rc<FileInfo>, Vec<Option<String>>, String, String)>> {
        if depth > MAX_DEPTH {
            return Ok(None);
        }
        let Some(sym) = self.symbol(class)? else {
            return Ok(None);
        };
        let file = self.file(sym.file_id)?;
        let name = format!("this.{field}");
        let declared = file
            .hints
            .iter()
            .find(|h| h.name == name && h.scope.as_deref() == Some(sym.qname.as_str()))
            .map(|h| (h.type_name.clone(), h.origin.clone()));
        if let Some((type_name, origin)) = declared {
            let chain = vec![Some(sym.qname.clone()), None];
            return Ok(Some((file, chain, type_name, origin)));
        }
        for parent in self.supers(class)?.iter() {
            if let Some(found) = self.field_type(*parent, field, depth + 1)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// The type hint for `name` in the innermost scope that has one. Java
    /// fields are found without `this.`.
    fn hint(
        &self,
        file: &FileInfo,
        chain: &[Option<String>],
        name: &str,
    ) -> Option<(String, String)> {
        let lookup = |name: &str| {
            chain.iter().find_map(|scope| {
                file.hints
                    .iter()
                    .find(|h| h.name == name && h.scope == *scope)
                    .map(|h| (h.type_name.clone(), h.origin.clone()))
            })
        };
        if let Some(found) = lookup(name) {
            return Some(found);
        }
        if file.language == Language::Java && !name.contains('.') {
            return lookup(&format!("this.{name}"));
        }
        None
    }

    /// A receiver whose type is `type_name`.
    fn typed(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        type_name: &str,
        origin: &str,
        depth: usize,
    ) -> Result<Receiver> {
        if let Some(path) = type_name.strip_prefix("path:") {
            return self.receiver(file, chain, path, depth + 1);
        }
        let (classes, confidence) = if let Some(callee) = type_name.strip_prefix("call:") {
            match self.call_return_type(file, chain, callee, depth)? {
                Some(classes) => (classes, "medium"),
                None => return Ok(Receiver::Unknown),
            }
        } else {
            let resolution = self.resolve_qualified_type(file, chain, type_name, depth)?;
            if resolution.targets.is_empty() {
                return Ok(Receiver::External);
            }
            // A type found only by a repo-wide name match may be a global
            // (`parent: Node`) or one of several classes of that name.
            let confidence = if origin == "return" {
                weaker(resolution.confidence, "medium")
            } else {
                resolution.confidence
            };
            (resolution.targets, confidence)
        };
        Ok(Receiver::Class(classes, confidence, "hint"))
    }

    /// `a.b.C` (a namespace import member, or a Java qualified name) or a plain
    /// type name.
    fn resolve_qualified_type(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        type_name: &str,
        depth: usize,
    ) -> Result<Resolution> {
        let Some((head, rest)) = type_name.split_once('.') else {
            return self.resolve_type(file, chain, type_name, false, depth + 1);
        };
        if file.language == Language::Java {
            return Ok(match self.java_import(type_name, "")? {
                Found::Symbols(ids) => Resolution::high(ids, "import"),
                Found::External | Found::Missing | Found::Absent(_) => Resolution::external(),
            });
        }
        match self.receiver(file, chain, head, depth + 1)? {
            Receiver::Module(module) => Ok(
                match self.export_lookup(module, rest, &mut HashSet::new(), 0)? {
                    Found::Symbols(ids) => Resolution::high(ids, "import"),
                    _ => Resolution::external(),
                },
            ),
            _ => Ok(Resolution::external()),
        }
    }

    /// The classes a call returns, from the callee's declared return type.
    fn call_return_type(
        &mut self,
        file: &Rc<FileInfo>,
        chain: &[Option<String>],
        callee: &str,
        depth: usize,
    ) -> Result<Option<Vec<i64>>> {
        let functions = match callee.rsplit_once('.') {
            None => self.resolve_bare(file, chain, callee, None)?,
            Some(("this", method)) => match self.enclosing_class(file, chain) {
                Some(class) => Resolution::high(self.members(class, method, 0)?, "this"),
                None => Resolution::unresolved(),
            },
            Some((receiver, method)) => match self.receiver(file, chain, receiver, depth + 1)? {
                Receiver::Class(classes, _, _) => {
                    let mut found = Vec::new();
                    for class in classes {
                        found.extend(self.members(class, method, 0)?);
                    }
                    Resolution::high(found, "hint")
                }
                Receiver::Module(module) => {
                    match self.export_lookup(module, method, &mut HashSet::new(), 0)? {
                        Found::Symbols(ids) => Resolution::high(ids, "import"),
                        _ => Resolution::unresolved(),
                    }
                }
                _ => Resolution::unresolved(),
            },
        };
        if functions.confidence != "high" {
            return Ok(None);
        }
        for function in functions.targets {
            let Some(sym) = self.symbol(function)? else {
                continue;
            };
            let owner = self.file(sym.file_id)?;
            let declared = owner
                .hints
                .iter()
                .find(|h| {
                    h.origin == "return"
                        && h.name == sym.name
                        && h.scope.as_deref() == Some(sym.qname.as_str())
                        && !h.type_name.starts_with("call:")
                })
                .map(|h| h.type_name.clone());
            if let Some(type_name) = declared {
                let owner_chain = vec![Some(sym.qname.clone()), None];
                let resolution =
                    self.resolve_qualified_type(&owner, &owner_chain, &type_name, depth + 1)?;
                if !resolution.targets.is_empty() {
                    return Ok(Some(resolution.targets));
                }
            }
        }
        Ok(None)
    }

    /// The file of the first class when every class's whole supertype chain is
    /// in the repository and in a typed language, so a member that is not
    /// found really does not exist; `None` when anything is inherited from
    /// outside or the class can grow at runtime (plain JavaScript).
    fn closed_class_file(&mut self, classes: &[i64]) -> Result<Option<String>> {
        let mut first = None;
        for class in classes {
            if !self.closed(*class, 0)? {
                return Ok(None);
            }
            let Some(sym) = self.symbol(*class)? else {
                return Ok(None);
            };
            let file = self.file(sym.file_id)?;
            if !matches!(
                file.language,
                Language::TypeScript | Language::Tsx | Language::Java
            ) {
                return Ok(None);
            }
            first.get_or_insert_with(|| file.path.clone());
        }
        Ok(first)
    }

    /// Whether one of `classes` or its supertypes declares a property
    /// `name` (`handler: Fn | null = null`). Properties are not symbols, so
    /// this reads the class body, which happens only before a member is
    /// reported missing.
    fn declares_property(&mut self, classes: &[i64], name: &str, depth: usize) -> Result<bool> {
        if depth > MAX_DEPTH {
            return Ok(true);
        }
        for &class in classes {
            let Some(sym) = self.symbol(class)? else {
                continue;
            };
            let file = self.file(sym.file_id)?;
            let (start, end): (i64, i64) = self.conn.query_row(
                "SELECT start_line, end_line FROM symbols WHERE id = ?1",
                params![class],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let text = std::fs::read_to_string(self.root.join(&file.path)).unwrap_or_default();
            let body = text
                .lines()
                .skip(start.saturating_sub(1) as usize)
                .take((end - start + 1).max(0) as usize);
            if body.into_iter().any(|line| declares(line, name)) {
                return Ok(true);
            }
            let parents = self.supers(class)?;
            if self.declares_property(&parents, name, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn closed(&mut self, class: i64, depth: usize) -> Result<bool> {
        if depth > MAX_DEPTH {
            return Ok(false);
        }
        let Some(sym) = self.symbol(class)? else {
            return Ok(false);
        };
        let file = self.file(sym.file_id)?;
        // A type alias, a variable, or a TypeScript interface (index
        // signatures, declaration merging) can carry members it does not
        // declare; only classes and Java's types list them all.
        let closable = match sym.kind.as_str() {
            "class" | "enum" | "record" => true,
            "interface" | "annotation" => file.language == Language::Java,
            _ => false,
        };
        if !closable {
            return Ok(false);
        }
        let extends: Vec<(String, Option<String>)> = self
            .conn
            .prepare_cached(
                "SELECT symbol_name, receiver_name FROM refs
                 WHERE from_symbol_id = ?1 AND kind = 'extends' ORDER BY id",
            )?
            .query_map(params![class], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let chain = vec![Some(sym.qname.clone()), None];
        for (name, receiver) in extends {
            let type_name = match receiver {
                Some(receiver) => format!("{receiver}.{name}"),
                None => name,
            };
            let parent = self.resolve_qualified_type(&file, &chain, &type_name, 0)?;
            if parent.confidence != "high" || parent.targets.is_empty() {
                return Ok(false);
            }
            for target in parent.targets {
                if target != class && !self.closed(target, depth + 1)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Members named `name` of `class`, then of its supertypes.
    fn members(&mut self, class: i64, name: &str, depth: usize) -> Result<Vec<i64>> {
        if depth > MAX_DEPTH {
            return Ok(Vec::new());
        }
        let Some(sym) = self.symbol(class)? else {
            return Ok(Vec::new());
        };
        let file = self.file(sym.file_id)?;
        let own: Vec<i64> = file
            .symbols
            .iter()
            .filter(|s| s.name == name && s.container.as_deref() == Some(sym.qname.as_str()))
            .map(|s| s.id)
            .collect();
        if !own.is_empty() {
            return Ok(own);
        }
        for parent in self.supers(class)?.iter() {
            let found = self.members(*parent, name, depth + 1)?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
        Ok(Vec::new())
    }

    /// The classes and interfaces `class` extends or implements, resolved in
    /// its own file.
    fn supers(&mut self, class: i64) -> Result<Rc<Vec<i64>>> {
        if let Some(found) = self.supers.get(&class) {
            return Ok(Rc::clone(found));
        }
        self.supers.insert(class, Rc::new(Vec::new()));
        let Some(sym) = self.symbol(class)? else {
            return Ok(Rc::new(Vec::new()));
        };
        let file = self.file(sym.file_id)?;
        let names: Vec<(String, Option<String>)> = self
            .conn
            .prepare_cached(
                "SELECT symbol_name, receiver_name FROM refs
                 WHERE from_symbol_id = ?1 AND kind IN ('extends', 'implements') ORDER BY id",
            )?
            .query_map(params![class], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let chain = vec![Some(sym.qname.clone()), None];
        let mut parents = Vec::new();
        for (name, receiver) in names {
            let type_name = match receiver {
                Some(receiver) => format!("{receiver}.{name}"),
                None => name,
            };
            let resolution = self.resolve_qualified_type(&file, &chain, &type_name, 0)?;
            if resolution.confidence == "high" {
                parents.extend(resolution.targets.into_iter().filter(|id| *id != class));
            }
        }
        let parents = Rc::new(parents);
        self.supers.insert(class, Rc::clone(&parents));
        Ok(parents)
    }

    /// Repo-wide name match: one candidate is `medium`, a few are `low`.
    fn by_name(
        &mut self,
        name: &str,
        arity: Option<i64>,
        members: bool,
        kinds: Option<&[&str]>,
        exclude: Option<i64>,
    ) -> Result<Resolution> {
        let candidates: Vec<i64> = self
            .candidates(name, members, kinds)?
            .iter()
            .copied()
            .filter(|id| Some(*id) != exclude)
            .collect();
        let candidates = self.filter_arity(candidates, arity)?;
        Ok(match candidates.len() {
            0 => Resolution::unresolved(),
            1 => Resolution {
                targets: candidates,
                confidence: "medium",
                resolver: Some("name"),
                ..Resolution::unresolved()
            },
            n if n <= MAX_AMBIGUOUS => Resolution {
                targets: candidates,
                confidence: "low",
                resolver: Some("name"),
                ..Resolution::unresolved()
            },
            _ => Resolution::unresolved(),
        })
    }

    /// Definitions named `name`, minus callbacks, members unless `members`,
    /// and kinds outside `kinds`.
    fn candidates(
        &mut self,
        name: &str,
        members: bool,
        kinds: Option<&[&str]>,
    ) -> Result<Rc<Vec<i64>>> {
        let key = (name.to_string(), members, kinds.is_some());
        if let Some(found) = self.candidate_cache.get(&key) {
            return Ok(Rc::clone(found));
        }
        let mut candidates = Vec::new();
        for id in self.named(name)?.iter() {
            let Some(sym) = self.symbol(*id)? else {
                continue;
            };
            if kinds.is_some_and(|kinds| !kinds.contains(&sym.kind.as_str())) {
                continue;
            }
            if sym.kind == "callback" {
                continue;
            }
            if !members {
                let file = self.file(sym.file_id)?;
                if self.is_member(&file, &sym) {
                    continue;
                }
            }
            candidates.push(*id);
        }
        let candidates = Rc::new(candidates);
        self.candidate_cache.insert(key, Rc::clone(&candidates));
        Ok(candidates)
    }
}

const SYMBOL_COLUMNS_SQL: &str = "SELECT id, file_id, name, qualified_name, kind, container,
    exported, default_export, arity_min, arity_max FROM symbols";

fn map_sym(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sym> {
    let min: Option<i64> = row.get(8)?;
    let max: Option<i64> = row.get(9)?;
    Ok(Sym {
        id: row.get(0)?,
        file_id: row.get(1)?,
        name: row.get(2)?,
        qname: row.get(3)?,
        kind: row.get(4)?,
        container: row.get(5)?,
        exported: row.get(6)?,
        default_export: row.get(7)?,
        arity: min.map(|min| (min, max)),
    })
}

/// The lower of two confidence levels.
fn weaker(a: &'static str, b: &'static str) -> &'static str {
    let rank = |c: &str| match c {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

fn is_js(language: Language) -> bool {
    matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Tsx
    )
}

/// `public readonly name?: T = …;` and the like: a property declaration of
/// `name` on this line.
fn declares(line: &str, name: &str) -> bool {
    const MODIFIERS: &[&str] = &[
        "public ",
        "private ",
        "protected ",
        "readonly ",
        "static ",
        "declare ",
        "override ",
        "accessor ",
    ];
    let mut rest = line.trim_start();
    while let Some(after) = MODIFIERS.iter().find_map(|m| rest.strip_prefix(m)) {
        rest = after.trim_start();
    }
    let rest = rest.strip_prefix('#').unwrap_or(rest);
    rest.strip_prefix(name).is_some_and(|after| {
        let after = after.trim_start_matches(['?', '!']).trim_start();
        after.starts_with(':') || after.starts_with('=') || after.starts_with(';')
    })
}
