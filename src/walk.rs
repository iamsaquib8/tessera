use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::{Match, WalkBuilder};

use crate::config::TesseraConfig;
use crate::indexer::IndexWarning;
use crate::types::Language;

#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub max_file_bytes: u64,
    pub excludes: Vec<String>,
    pub sniff_content: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 1_048_576,
            excludes: Vec::new(),
            sniff_content: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub language: Language,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    Minified,
    TooLarge,
    Excluded,
    Unreadable,
}

#[derive(Debug, Clone)]
pub struct Skipped {
    pub rel_path: String,
    pub reason: SkipReason,
    pub size: u64,
    pub mtime_ns: i64,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minified => "minified",
            Self::TooLarge => "too_large",
            Self::Excluded => "excluded",
            Self::Unreadable => "unreadable",
        }
    }
}

#[derive(Debug)]
pub enum FileStatus {
    Missing,
    Unsupported,
    Skipped(Skipped),
    Source(SourceFile),
}

#[derive(Debug, Default)]
pub struct Discovery {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<IndexWarning>,
}

const ALWAYS_SKIP: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    ".tessera",
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".gradle",
    ".idea",
    ".cache",
];

const ROOT_SKIP: &[&str] = &[
    "dist",
    "build",
    "target",
    "out",
    "vendor",
    "vendors",
    "third_party",
    "coverage",
    ".next",
    "Pods",
    "DerivedData",
];

const SNIFF_BYTES: usize = 64 * 1024;
const MINIFIED_LINE_BYTES: usize = 5_000;
const MINIFIED_AVG_LINE_BYTES: usize = 300;

pub fn discover(root: &Path, config: &TesseraConfig, opts: &WalkOptions) -> Result<Discovery> {
    let mut patterns = opts.excludes.clone();
    patterns.extend(config.exclude_globs.iter().cloned());
    let excludes = glob_set(&patterns)?;
    let skip_names = config.ignore_names.clone();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .ignore(true)
        .require_git(false)
        .follow_links(false)
        .add_custom_ignore_filename(".tesseraignore")
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if skip_names.iter().any(|skip| skip == name.as_ref()) {
                return false;
            }
            if !entry.file_type().is_some_and(|t| t.is_dir()) {
                return true;
            }
            !(ALWAYS_SKIP.contains(&name.as_ref())
                || (entry.depth() == 1 && ROOT_SKIP.contains(&name.as_ref())))
        });

    let mut discovery = Discovery::default();
    for result in builder.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                discovery.warnings.push(IndexWarning {
                    path: "<walk>".to_string(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(language) = Language::from_path(path) else {
            continue;
        };
        let rel_path = relative_path(root, path);
        match classify(rel_path, path, language, config, &excludes, opts) {
            Ok(FileStatus::Source(file)) => discovery.files.push(file),
            Ok(FileStatus::Skipped(skipped)) => discovery.skipped.push(skipped),
            Ok(_) => {}
            Err(warning) => discovery.warnings.push(warning),
        }
    }
    discovery.files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    discovery
        .skipped
        .sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(discovery)
}

pub fn describe(
    root: &Path,
    rel_path: &str,
    config: &TesseraConfig,
    opts: &WalkOptions,
) -> Result<FileStatus> {
    let path = root.join(rel_path);
    if !path.is_file() {
        return Ok(FileStatus::Missing);
    }
    let Some(language) = Language::from_path(&path).filter(|_| admits(root, rel_path, config))
    else {
        return Ok(FileStatus::Unsupported);
    };
    let mut patterns = opts.excludes.clone();
    patterns.extend(config.exclude_globs.iter().cloned());
    let excludes = glob_set(&patterns)?;
    classify(
        rel_path.to_string(),
        &path,
        language,
        config,
        &excludes,
        opts,
    )
    .map_err(|warning| anyhow::anyhow!("{}: {}", warning.path, warning.message))
}

/// Whether `discover` would visit `rel_path`: the same hidden-name, skip-list,
/// ignore-file and symlink rules, for paths that arrive one at a time (watcher
/// events, hook-supplied paths).
pub fn admits(root: &Path, rel_path: &str, config: &TesseraConfig) -> bool {
    let mut names = Vec::new();
    for component in Path::new(rel_path).components() {
        match component {
            Component::Normal(name) => names.push(name.to_string_lossy().into_owned()),
            _ => return false,
        }
    }
    let Some(last) = names.len().checked_sub(1) else {
        return false;
    };
    for (depth, name) in names.iter().enumerate() {
        let is_dir = depth < last;
        if name.starts_with('.')
            || config.ignore_names.iter().any(|skip| skip == name)
            || (is_dir && ALWAYS_SKIP.contains(&name.as_str()))
            || (is_dir && depth == 0 && ROOT_SKIP.contains(&name.as_str()))
        {
            return false;
        }
    }
    let mut dirs = vec![root.to_path_buf()];
    let mut current = root.to_path_buf();
    for (depth, name) in names.iter().enumerate() {
        current.push(name);
        let is_link = fs::symlink_metadata(&current).map_or(true, |m| m.file_type().is_symlink());
        if is_link {
            return false;
        }
        if depth < last {
            dirs.push(current.clone());
        }
    }
    !ignored_by_files(root, &dirs, &current)
}

// Deeper ignore files win over shallower ones, and within one directory
// .tesseraignore beats .ignore beats .gitignore, as in the walk.
fn ignored_by_files(root: &Path, dirs: &[PathBuf], path: &Path) -> bool {
    for dir in dirs.iter().rev() {
        for name in [".tesseraignore", ".ignore", ".gitignore"] {
            let file = dir.join(name);
            if !file.is_file() {
                continue;
            }
            let (matcher, _) = Gitignore::new(&file);
            match matcher.matched_path_or_any_parents(path, false) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
    }
    let mut exclude = GitignoreBuilder::new(root);
    exclude.add(root.join(".git/info/exclude"));
    let matchers = [
        exclude.build().unwrap_or_else(|_| Gitignore::empty()),
        GitignoreBuilder::new(root).build_global().0,
    ];
    matchers
        .iter()
        .any(|m| matches!(m.matched_path_or_any_parents(path, false), Match::Ignore(_)))
}

fn classify(
    rel_path: String,
    path: &Path,
    language: Language,
    config: &TesseraConfig,
    excludes: &GlobSet,
    opts: &WalkOptions,
) -> std::result::Result<FileStatus, IndexWarning> {
    if !config.includes_path(&rel_path)
        || config.excludes_path(&rel_path)
        || excludes.is_match(&rel_path)
    {
        return Ok(FileStatus::Skipped(Skipped {
            rel_path,
            reason: SkipReason::Excluded,
            size: 0,
            mtime_ns: 0,
        }));
    }
    let metadata = match path.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(IndexWarning {
                path: rel_path,
                message: error.to_string(),
            })
        }
    };
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let reason = if minified_name(&name) {
        Some(SkipReason::Minified)
    } else if metadata.len() > opts.max_file_bytes {
        Some(SkipReason::TooLarge)
    } else if opts.sniff_content && read_head(path).is_some_and(|head| minified_content(&head)) {
        Some(SkipReason::Minified)
    } else {
        None
    };
    Ok(match reason {
        Some(reason) => FileStatus::Skipped(Skipped {
            rel_path,
            reason,
            size: metadata.len(),
            mtime_ns,
        }),
        None => FileStatus::Source(SourceFile {
            rel_path,
            abs_path: path.to_path_buf(),
            language,
            size: metadata.len(),
            mtime_ns,
        }),
    })
}

pub fn is_minified(name: &str, head: &[u8]) -> bool {
    minified_name(name) || minified_content(head)
}

pub fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn minified_name(name: &str) -> bool {
    name.contains(".min.") || name.contains(".bundle.")
}

fn minified_content(head: &[u8]) -> bool {
    let head = &head[..head.len().min(SNIFF_BYTES)];
    if head.is_empty() {
        return false;
    }
    let mut lines = 0usize;
    let mut longest = 0usize;
    for line in head.split(|b| *b == b'\n') {
        lines += 1;
        longest = longest.max(line.len());
    }
    longest > MINIFIED_LINE_BYTES || head.len() / lines > MINIFIED_AVG_LINE_BYTES
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    let mut head = Vec::with_capacity(SNIFF_BYTES);
    File::open(path)
        .ok()?
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn paths(root: &Path, opts: &WalkOptions) -> Vec<String> {
        discover(root, &TesseraConfig::default(), opts)
            .unwrap()
            .files
            .into_iter()
            .map(|f| f.rel_path)
            .collect()
    }

    #[test]
    fn honours_gitignore() {
        let temp = TempDir::new().unwrap();
        write(temp.path(), ".gitignore", "dist/\n");
        write(temp.path(), "dist/a.js", "function a() {}\n");
        write(temp.path(), "src/a.js", "function a() {}\n");
        assert_eq!(
            paths(temp.path(), &WalkOptions::default()),
            vec!["src/a.js"]
        );
    }

    #[test]
    fn keeps_nested_build_and_vendor_package_dirs() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            "src/main/java/com/acme/build/Foo.java",
            "class Foo {}\n",
        );
        write(
            temp.path(),
            "src/main/java/com/acme/vendor/Bar.java",
            "class Bar {}\n",
        );
        write(temp.path(), "vendor/x.go", "package x\n");
        assert_eq!(
            paths(temp.path(), &WalkOptions::default()),
            vec![
                "src/main/java/com/acme/build/Foo.java",
                "src/main/java/com/acme/vendor/Bar.java",
            ]
        );
    }

    #[test]
    fn always_skips_tool_directories() {
        let temp = TempDir::new().unwrap();
        for dir in ["node_modules", ".tessera", ".git", "pkg/node_modules"] {
            write(temp.path(), &format!("{dir}/x.js"), "function x() {}\n");
        }
        write(temp.path(), "pkg/index.js", "function y() {}\n");
        assert_eq!(
            paths(temp.path(), &WalkOptions::default()),
            vec!["pkg/index.js"]
        );
    }

    #[test]
    fn skips_minified_and_oversized_files() {
        let temp = TempDir::new().unwrap();
        write(temp.path(), "app.min.js", "function a(){}\n");
        write(temp.path(), "vendor-lib.js", &"a();".repeat(50_000));
        write(temp.path(), "big.ts", &"// filler\n".repeat(220_000));
        write(temp.path(), "ok.ts", "export function ok() {}\n");
        let discovery = discover(
            temp.path(),
            &TesseraConfig::default(),
            &WalkOptions::default(),
        )
        .unwrap();
        let files: Vec<_> = discovery
            .files
            .iter()
            .map(|f| f.rel_path.as_str())
            .collect();
        assert_eq!(files, vec!["ok.ts"]);
        let mut skipped: Vec<_> = discovery
            .skipped
            .iter()
            .map(|s| (s.rel_path.as_str(), s.reason))
            .collect();
        skipped.sort();
        assert_eq!(
            skipped,
            vec![
                ("app.min.js", SkipReason::Minified),
                ("big.ts", SkipReason::TooLarge),
                ("vendor-lib.js", SkipReason::Minified),
            ]
        );
    }

    #[test]
    fn applies_exclude_globs() {
        let temp = TempDir::new().unwrap();
        write(temp.path(), "legacy/a.ts", "export function a() {}\n");
        write(temp.path(), "src/a.ts", "export function a() {}\n");
        let opts = WalkOptions {
            excludes: vec!["legacy/**".to_string()],
            ..WalkOptions::default()
        };
        let discovery = discover(temp.path(), &TesseraConfig::default(), &opts).unwrap();
        assert_eq!(discovery.files.len(), 1);
        assert_eq!(discovery.files[0].rel_path, "src/a.ts");
        assert_eq!(discovery.skipped[0].reason, SkipReason::Excluded);
    }

    #[test]
    fn output_is_sorted_with_forward_slashes() {
        let temp = TempDir::new().unwrap();
        for rel in ["z.ts", "a/b/c.ts", "a/a.ts", "m.ts"] {
            write(temp.path(), rel, "export const x = () => 1;\n");
        }
        let got = paths(temp.path(), &WalkOptions::default());
        assert_eq!(got, vec!["a/a.ts", "a/b/c.ts", "m.ts", "z.ts"]);
        assert!(got.iter().all(|p| !p.contains('\\')));
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks_or_loop() {
        let temp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        write(outside.path(), "secret.ts", "export function s() {}\n");
        write(temp.path(), "src/a.ts", "export function a() {}\n");
        std::os::unix::fs::symlink(outside.path(), temp.path().join("linked")).unwrap();
        std::os::unix::fs::symlink(temp.path(), temp.path().join("src/loop")).unwrap();
        assert_eq!(
            paths(temp.path(), &WalkOptions::default()),
            vec!["src/a.ts"]
        );
    }

    #[test]
    fn minified_rules() {
        assert!(is_minified("x.min.js", b"function a(){}"));
        assert!(is_minified("app.bundle.js", b"a"));
        assert!(is_minified("lib.js", "a();".repeat(2_000).as_bytes()));
        assert!(!is_minified(
            "lib.js",
            "function a() {\n  return 1;\n}\n".as_bytes()
        ));
    }
}
