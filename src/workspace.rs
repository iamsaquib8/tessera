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
        let root = match root {
            Some(root) => {
                let root = cwd.join(root);
                root.canonicalize().unwrap_or(root)
            }
            None => discover_root(cwd),
        };
        let db_path = match db {
            Some(db) => cwd.join(db),
            None => root.join(".tessera").join("tessera.db"),
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
}
