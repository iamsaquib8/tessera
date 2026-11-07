use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub db_path: PathBuf,
}

pub fn discover_root(start: &Path) -> PathBuf {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    start
        .ancestors()
        .find(|dir| dir.join(".tessera").is_dir() || dir.join(".git").exists())
        .map(Path::to_path_buf)
        .unwrap_or(start)
}

/// A path argument as the repository-relative, `/`-separated form the index
/// stores. `\` is read as `/`; a relative `input` is taken from `cwd`; `.`
/// and `..` are resolved; an absolute `input` must lie under `root` (both are
/// compared after resolving symlinks). A path that leaves the repository is
/// an error that says so. The repository itself is `.`.
pub fn repo_relative(root: &Path, cwd: &Path, input: &str) -> Result<String, String> {
    let input = input.trim().replace('\\', "/");
    let given = Path::new(if input.is_empty() { "." } else { &input });
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        cwd.join(given)
    };
    let abs = resolve_existing_prefix(&lexically_normal(&joined));
    let root_abs = resolve_existing_prefix(&lexically_normal(root));
    let outside = || format!("{} is outside the repository {}", input, root_abs.display());
    let rel = abs.strip_prefix(&root_abs).map_err(|_| outside())?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Ok(if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    })
}

/// `input` as a repository-relative file path when it names an existing file
/// under `root` (relative to `cwd`); `None` for a symbol name or any other
/// argument.
pub fn repo_file_argument(root: &Path, cwd: &Path, input: &str) -> Option<String> {
    let rel = repo_relative(root, cwd, input).ok()?;
    resolve_existing_prefix(&lexically_normal(root))
        .join(&rel)
        .is_file()
        .then_some(rel)
}

/// `path` with `.` removed and `..` applied to the component before it.
fn lexically_normal(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `path` with symlinks resolved as far as it exists; the part that does not
/// exist yet is appended unchanged.
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut head = path;
    loop {
        if let Ok(real) = head.canonicalize() {
            return tail.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (head.parent(), head.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                head = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

impl Workspace {
    /// True when the root is the home directory or the filesystem root, where an
    /// auto-built index would cover far more than one project.
    pub fn is_too_broad(&self) -> bool {
        if self.root.parent().is_none() {
            return true;
        }
        ["HOME", "USERPROFILE"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .any(|home| home.canonicalize().unwrap_or(home) == self.root)
    }

    pub fn resolve(root: Option<&Path>, db: Option<&Path>, cwd: &Path) -> Workspace {
        let explicit_db = db.map(|db| {
            if db.is_absolute() {
                db.to_path_buf()
            } else {
                cwd.join(db)
            }
        });
        let root = match root {
            Some(root) => {
                let root = cwd.join(root);
                root.canonicalize().unwrap_or(root)
            }
            None => {
                let discovered = discover_root(cwd);
                let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
                if discovered == canonical_cwd {
                    if let Some(db_p) = explicit_db.as_ref() {
                        if db_p.exists() {
                            if let Some(parent) = db_p.parent() {
                                if parent.file_name().and_then(|n| n.to_str()) == Some(".tessera") {
                                    if let Some(repo_root) = parent.parent() {
                                        if repo_root.is_dir() {
                                            repo_root
                                                .canonicalize()
                                                .unwrap_or_else(|_| repo_root.to_path_buf())
                                        } else {
                                            discovered
                                        }
                                    } else {
                                        discovered
                                    }
                                } else {
                                    discovered
                                }
                            } else {
                                discovered
                            }
                        } else {
                            discovered
                        }
                    } else {
                        discovered
                    }
                } else {
                    discovered
                }
            }
        };
        let db_path = match explicit_db {
            Some(p) => p,
            None => {
                let config = crate::config::TesseraConfig::load(&root);
                if let Some(custom_db) = config.db {
                    let p = PathBuf::from(custom_db);
                    if p.is_absolute() {
                        p
                    } else {
                        root.join(p)
                    }
                } else {
                    root.join(".tessera").join("tessera.db")
                }
            }
        };
        Workspace { root, db_path }
    }
}

/// The file that lists repositories mapped together.
pub const WORKSPACE_FILE: &str = "tessera.workspace.toml";

/// The nearest `tessera.workspace.toml` at or above `root`: its directory
/// and the repositories it lists (absolute), `root` among them.
pub fn workspace_repos(root: &Path) -> Option<(PathBuf, Vec<PathBuf>)> {
    let file = root
        .ancestors()
        .map(|dir| dir.join(WORKSPACE_FILE))
        .find(|file| file.is_file())?;
    let dir = file.parent()?.to_path_buf();
    let content = std::fs::read_to_string(&file).ok()?;
    let listed = match toml::from_str::<WorkspaceFile>(&content) {
        Ok(listed) => listed,
        Err(error) => {
            eprintln!(
                "tessera: {} is not valid TOML, mapping this repository alone: {}",
                file.display(),
                error.message()
            );
            WorkspaceFile::default()
        }
    };
    let mut repos: Vec<PathBuf> = Vec::new();
    for value in listed
        .repos
        .into_iter()
        .chain(listed.repo.into_iter().map(|r| r.path))
    {
        let path = dir.join(&value);
        if !path.is_dir() {
            eprintln!(
                "tessera: {} lists {value}, which is not a directory; skipping it",
                file.display()
            );
            continue;
        }
        repos.push(path.canonicalize().unwrap_or(path));
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !repos.contains(&root) {
        repos.push(root);
    }
    repos.sort();
    repos.dedup();
    Some((dir.canonicalize().unwrap_or(dir), repos))
}

/// The prefix of this repository's paths in a workspace map (`web/`), or
/// nothing outside a workspace.
pub fn repo_prefix(root: &Path) -> String {
    let Some((dir, _)) = workspace_repos(root) else {
        return String::new();
    };
    let here = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    match here.strip_prefix(&dir) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            format!("{}/", rel.to_string_lossy().replace('\\', "/"))
        }
        _ => String::new(),
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct WorkspaceFile {
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    repo: Vec<RepoEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct RepoEntry {
    path: String,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn repo_relative_handles_cwd_absolute_and_backslash_inputs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join("src/api")).unwrap();
        fs::write(root.join("src/api/index.ts"), "").unwrap();
        let src = root.join("src");
        let abs = root.canonicalize().unwrap().join("src/api/index.ts");
        let rel = |cwd: &Path, input: &str| repo_relative(&root, cwd, input);
        for (cwd, input) in [
            (&root, "src/api/index.ts"),
            (&root, "./src/api/index.ts"),
            (&root, ".\\src\\api\\index.ts"),
            (&root, "src//api/../api/index.ts"),
            (&root, abs.to_str().unwrap()),
            (&src, "api/index.ts"),
            (&src, "./api/index.ts"),
            (&src, "../src/api/index.ts"),
            (&src, abs.to_str().unwrap()),
        ] {
            assert_eq!(rel(cwd, input).unwrap(), "src/api/index.ts", "{input}");
        }
        assert_eq!(rel(&root, ".").unwrap(), ".");
        assert_eq!(rel(&root, "").unwrap(), ".");
        assert_eq!(rel(&src, "api/").unwrap(), "src/api");
        assert_eq!(rel(&src, "..").unwrap(), ".");
        // a file that is not there yet keeps its place under the root
        assert_eq!(rel(&src, "api/new.ts").unwrap(), "src/api/new.ts");
        let err = rel(&root, "../elsewhere.ts").unwrap_err();
        assert!(err.contains("outside the repository"), "{err}");
        assert!(rel(&root, "/etc/hosts").is_err());
    }

    #[test]
    fn repo_file_argument_only_accepts_existing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.ts"), "").unwrap();
        assert_eq!(
            repo_file_argument(root, root, "./src/a.ts").as_deref(),
            Some("src/a.ts")
        );
        assert_eq!(repo_file_argument(root, root, "src"), None);
        assert_eq!(repo_file_argument(root, root, "Index::find"), None);
    }

    #[test]
    fn workspace_files_are_toml() {
        let temp = TempDir::new().unwrap();
        for repo in ["api", "web", "docs"] {
            fs::create_dir_all(temp.path().join(repo)).unwrap();
        }
        fs::write(
            temp.path().join(WORKSPACE_FILE),
            "repos = [\n  \"api\",\n  \"missing\", # typo\n]\n\n[[repo]]\npath = 'docs'\n",
        )
        .unwrap();
        let (dir, repos) = workspace_repos(&temp.path().join("web")).unwrap();
        let names: Vec<String> = repos
            .iter()
            .map(|r| r.strip_prefix(&dir).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["api", "docs", "web"]);
        assert!(!temp.path().join("missing").exists());
        assert_eq!(repo_prefix(&temp.path().join("web")), "web/");
        assert_eq!(repo_prefix(temp.path()), "");
    }

    #[test]
    fn discover_root_walks_up_to_git() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src/deep")).unwrap();
        assert_eq!(
            discover_root(&repo.join("src/deep")),
            repo.canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_root_prefers_nearest_tessera_dir() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("packages/app/.tessera")).unwrap();
        fs::create_dir_all(repo.join("packages/app/src")).unwrap();
        assert_eq!(
            discover_root(&repo.join("packages/app/src")),
            repo.join("packages/app").canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_root_falls_back_to_start() {
        let temp = TempDir::new().unwrap();
        let start = temp.path().join("plain");
        fs::create_dir_all(&start).unwrap();
        assert_eq!(discover_root(&start), start.canonicalize().unwrap());
    }

    #[test]
    fn resolve_defaults_db_under_root() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        let ws = Workspace::resolve(None, None, &repo.join("src"));
        let root = repo.canonicalize().unwrap();
        assert_eq!(ws.root, root);
        assert_eq!(ws.db_path, root.join(".tessera/tessera.db"));
    }

    #[test]
    fn resolve_relative_db_against_cwd() {
        let temp = TempDir::new().unwrap();
        let cwd = temp.path().canonicalize().unwrap();
        let ws = Workspace::resolve(Some(Path::new(".")), Some(Path::new("x/t.db")), &cwd);
        assert_eq!(ws.db_path, cwd.join("x/t.db"));
        assert_eq!(ws.root, cwd);
    }

    #[test]
    fn resolve_uses_config_db_when_unspecified() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".tessera")).unwrap();
        fs::write(
            repo.join(".tessera/config.toml"),
            "[index]\ndb = \"custom_index.db\"\n",
        )
        .unwrap();

        let ws = Workspace::resolve(Some(Path::new(".")), None, &repo);
        assert_eq!(
            ws.db_path,
            repo.canonicalize().unwrap().join("custom_index.db")
        );
    }

    #[test]
    fn resolve_derives_root_from_explicit_db_when_outside_repo() {
        let temp = TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        let repo = temp.path().join("my_repo");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(repo.join(".tessera")).unwrap();
        let db_path = repo.join(".tessera").join("tessera.db");
        fs::write(&db_path, "").unwrap();

        let ws = Workspace::resolve(None, Some(&db_path), &outside);
        assert_eq!(ws.root, repo.canonicalize().unwrap());
        assert_eq!(ws.db_path, db_path);
    }
}
