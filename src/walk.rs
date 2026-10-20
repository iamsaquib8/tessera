use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

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
}

#[derive(Debug, Clone)]
pub struct Skipped {
    pub rel_path: String,
    pub reason: SkipReason,
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
        let Some(language) = path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Language::from_extension)
        else {
            continue;
        };
        let rel_path = relative_path(root, path);
        let skip = |discovery: &mut Discovery, reason| {
            discovery.skipped.push(Skipped {
                rel_path: rel_path.clone(),
                reason,
            })
        };
        if !config.includes_path(&rel_path)
            || config.excludes_path(&rel_path)
            || excludes.is_match(&rel_path)
        {
            skip(&mut discovery, SkipReason::Excluded);
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                discovery.warnings.push(IndexWarning {
                    path: rel_path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy();
        if minified_name(&name) {
            skip(&mut discovery, SkipReason::Minified);
            continue;
        }
        if metadata.len() > opts.max_file_bytes {
            skip(&mut discovery, SkipReason::TooLarge);
            continue;
        }
        if opts.sniff_content && read_head(path).is_some_and(|head| minified_content(&head)) {
            skip(&mut discovery, SkipReason::Minified);
            continue;
        }
        let mtime_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        discovery.files.push(SourceFile {
            rel_path,
            abs_path: path.to_path_buf(),
            language,
            size: metadata.len(),
            mtime_ns,
        });
    }
    discovery.files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    discovery
        .skipped
        .sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(discovery)
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
