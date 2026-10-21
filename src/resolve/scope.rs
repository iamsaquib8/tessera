use std::collections::{HashMap, HashSet};
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
/// More name-matched candidates than this is noise, not ambiguity.
const MAX_AMBIGUOUS: usize = 5;
/// Bounds every walk over re-exports, inheritance and call chains.
const MAX_DEPTH: usize = 12;

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
    pub from: i64,
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
}

impl Resolution {
    fn unresolved() -> Self {
        Self {
            targets: Vec::new(),
            confidence: "name",
            resolver: None,
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
    pub modules: ModuleResolver,
    files: HashMap<i64, Rc<FileInfo>>,
    symbols: HashMap<i64, Rc<Sym>>,
    by_name: HashMap<String, Rc<Vec<i64>>>,
    supers: HashMap<i64, Rc<Vec<i64>>>,
}

impl<'c> Resolver<'c> {
    pub fn new(conn: &'c Connection, modules: ModuleResolver, preload: bool) -> Result<Self> {
        let mut resolver = Self {
            conn,
            modules,
            files: HashMap::new(),
            symbols: HashMap::new(),
            by_name: HashMap::new(),
            supers: HashMap::new(),
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

    pub fn file(&mut self, file_id: i64) -> Result<Rc<FileInfo>> {
        if let Some(file) = self.files.get(&file_id) {
            return Ok(Rc::clone(file));
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
             FROM refs WHERE file_id = ?1 AND from_symbol_id IS NOT NULL ORDER BY id",
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
        match self.modules.resolve(&file.path, &source, file.language) {
            ModuleTarget::File(id) => self.file(id).ok().map(|f| f.path.clone()),
            _ => None,
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
                            Found::Missing => Resolution::unresolved(),
                        }
                    }
                    Receiver::Class(classes, confidence, resolver) => {
                        let mut found = Vec::new();
                        for class in classes {
                            found.extend(self.members(class, name, 0)?);
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
                        .by_name(name, arity, true, None, Some(reference.from))?
                        .capped("medium"),
                }
            }
            // A call on some other object is rarely a call on the caller itself.
            "other" => self
                .by_name(name, arity, true, None, Some(reference.from))?
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
        let targets = self.filter_arity(found, arity)?;
        Ok(Resolution {
            targets,
            ..template
        })
    }

    /// Drops candidates whose parameter count cannot take `arity` arguments,
    /// unless that would drop them all.
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
    fn scope_chain(&mut self, file: &FileInfo, from: i64) -> Result<Vec<Option<String>>> {
        let mut chain = Vec::new();
        let mut current = self.symbol(from)?;
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
                Found::Missing => Resolution::unresolved(),
            });
        }
        let local = self.local(file, chain, name, false);
        if !local.is_empty() {
            return self.finish(local, arity, Resolution::high(Vec::new(), "local"));
        }
        match self.binding_symbols(file, name)? {
            Found::Symbols(ids) => {
                return self.finish(ids, arity, Resolution::high(Vec::new(), "import"))
            }
            Found::External => return Ok(Resolution::external()),
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
        let kinds = types_only.then_some(CLASS_KINDS);
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
                .filter(|sym| !types_only || CLASS_KINDS.contains(&sym.kind.as_str()))
                .filter(|sym| !self.is_member(file, sym))
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

    /// The symbols an imported name stands for.
    fn binding_symbols(&mut self, file: &Rc<FileInfo>, name: &str) -> Result<Found> {
        let bindings: Vec<(String, Option<String>, String)> = file
            .bindings
            .iter()
            .filter(|b| b.local == name || (b.local == "*" && b.kind.starts_with("java")))
            .map(|b| (b.kind.clone(), b.imported.clone(), b.source.clone()))
            .collect();
        let mut external = false;
        for (kind, imported, source) in bindings {
            let found = match kind.as_str() {
                "named" | "require_member" | "default" => {
                    let export = if kind == "default" {
                        "default".to_string()
                    } else {
                        imported.unwrap_or_else(|| name.to_string())
                    };
                    match self.modules.resolve(&file.path, &source, file.language) {
                        ModuleTarget::File(target) => {
                            self.export_lookup(target, &export, &mut HashSet::new(), 0)?
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
                        for class in classes {
                            members.extend(self.members(class, name, 0)?);
                        }
                        if members.is_empty() {
                            Found::Missing
                        } else {
                            Found::Symbols(members)
                        }
                    }
                    other => other,
                },
                _ => Found::Missing,
            };
            match found {
                Found::Symbols(ids) => return Ok(Found::Symbols(ids)),
                Found::External => external = true,
                Found::Missing => {}
            }
        }
        Ok(if external {
            Found::External
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
        match self.modules.resolve("", fqn, Language::Java) {
            ModuleTarget::File(target) => {
                let file = self.file(target)?;
                let ids: Vec<i64> = file
                    .symbols
                    .iter()
                    .filter(|s| s.name == class && CLASS_KINDS.contains(&s.kind.as_str()))
                    .map(|s| s.id)
                    .collect();
                Ok(if ids.is_empty() {
                    Found::Missing
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
                match self.modules.resolve(&file.path, &source, file.language) {
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
                Found::Missing => {}
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
        if let Some((type_name, origin)) = self.hint(file, chain, receiver) {
            return self.typed(file, chain, &type_name, &origin, depth);
        }
        let module_binding = file
            .bindings
            .iter()
            .find(|b| b.local == receiver && matches!(b.kind.as_str(), "namespace" | "require"))
            .map(|b| b.source.clone());
        if let Some(source) = module_binding {
            return Ok(
                match self.modules.resolve(&file.path, &source, file.language) {
                    ModuleTarget::File(id) => Receiver::Module(id),
                    ModuleTarget::External(_) => Receiver::External,
                    ModuleTarget::Unresolved => Receiver::Unknown,
                },
            );
        }
        if receiver.contains('.') && !receiver.starts_with("this.") {
            return Ok(Receiver::Unknown);
        }
        if receiver.starts_with(|c: char| c.is_uppercase()) {
            let resolution = self.resolve_type(file, chain, receiver, false, depth + 1)?;
            if resolution.confidence == "high" {
                return Ok(Receiver::Class(resolution.targets, "high", "static"));
            }
            if resolution.resolver == Some("external") {
                return Ok(Receiver::External);
            }
        }
        Ok(Receiver::Unknown)
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
            let confidence = if origin == "return" { "medium" } else { "high" };
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
                Found::External | Found::Missing => Resolution::external(),
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
        let mut candidates = Vec::new();
        for id in self.named(name)?.iter() {
            if Some(*id) == exclude {
                continue;
            }
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
        let candidates = self.filter_arity(candidates, arity)?;
        Ok(match candidates.len() {
            0 => Resolution::unresolved(),
            1 => Resolution {
                targets: candidates,
                confidence: "medium",
                resolver: Some("name"),
            },
            n if n <= MAX_AMBIGUOUS => Resolution {
                targets: candidates,
                confidence: "low",
                resolver: Some("name"),
            },
            _ => Resolution::unresolved(),
        })
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
