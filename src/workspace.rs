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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

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
