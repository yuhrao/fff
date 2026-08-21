//! Filesystem traversal backend. Selects one implementation at compile time:
//! - `zlob`: zlob's native parallel walker (requires the Zig toolchain).
//! - `ripgrep`: the `ignore` crate (ripgrep's walker), used by default.
//!
//! Both expose [`walk_collect_files`] with identical semantics so the rest of
//! the crate stays backend-agnostic.

use crate::types::FileItem;
use std::path::Path;

#[cfg(feature = "zlob")]
mod zlob;
#[cfg(feature = "zlob")]
pub(crate) use zlob::walk_collect_files;

#[cfg(all(not(feature = "zlob"), feature = "ripgrep"))]
mod ripgrep;
#[cfg(all(not(feature = "zlob"), feature = "ripgrep"))]
pub(crate) use ripgrep::walk_collect_files;

pub(crate) struct WalkOutput {
    pub(crate) pairs: Vec<(FileItem, String)>,
    /// Every non-ignored directory the walk visited, relative, ending with /
    pub(crate) dirs: Vec<String>,
    pub(crate) ignore_rules: Option<WalkIgnoreRules>,
}

pub(crate) struct WalkIgnoreRules {
    #[cfg(feature = "zlob")]
    inner: ::zlob::walk::WalkerOutcomeRules,
    #[cfg(not(feature = "zlob"))]
    _never: std::convert::Infallible,
}

// SAFETY: the underlying storage is immutable, heap-owned, and thread-safe to
// read from concurrently (mirrors zlob's `IgnoreRules: Send + Sync`).
unsafe impl Send for WalkIgnoreRules {}
unsafe impl Sync for WalkIgnoreRules {}

impl std::fmt::Debug for WalkIgnoreRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalkIgnoreRules")
    }
}

// In ripgrep builds `WalkIgnoreRules` is never constructed (the `_never`
// field is uninhabited), so its methods are legitimately dead there.
#[cfg_attr(not(feature = "zlob"), allow(dead_code))]
impl WalkIgnoreRules {
    /// Returns `true` if the provided path is ignored by the collected rule set
    ///
    /// `relative_path` has to be relative to the walker's provided base path
    pub(crate) fn is_ignored(&self, relative_path: &Path) -> bool {
        #[cfg(feature = "zlob")]
        {
            self.inner
                .rules()
                .is_some_and(|rules| rules.is_ignored(relative_path))
        }
        #[cfg(not(feature = "zlob"))]
        {
            let _ = relative_path;
            match self._never {}
        }
    }

    // The old `is_ignored_untrusted` variant was folded away when zlob's
    // ignore matcher moved to full ancestor enumeration — trailing-slash
    // sniffing on the input is now sufficient for external queries.
}

#[cfg(test)]
mod tests {
    use super::walk_collect_files;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Backend-agnostic parity check: both the zlob and ripgrep walkers must
    // respect .gitignore, skip hidden files in a git repo, and surface the
    // expected file set with a correct synced count.
    #[test]
    fn collects_files_respecting_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::write(root.join("Cargo.toml"), "x").unwrap();
        fs::write(root.join("debug.log"), "").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("target/out.bin"), "bin").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, true, false, false, 1, &counter).unwrap();

        let mut names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();
        names.sort();

        assert!(names.contains(&"Cargo.toml".to_string()));
        assert!(names.iter().any(|n| n.ends_with("main.rs")));
        // target/ and *.log are gitignored; .git/ is skipped.
        assert!(!names.iter().any(|n| n.contains("target")));
        assert!(!names.iter().any(|n| n.ends_with(".log")));
        assert!(!names.iter().any(|n| n.contains(".git/")));
        assert_eq!(counter.load(Ordering::Relaxed), names.len());
    }

    // Non-git roots prune known non-code directories (node_modules).
    #[test]
    fn prunes_non_code_dirs_for_non_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules/lib.js"), "x").unwrap();
        fs::write(root.join("index.js"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, false, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with("index.js")));
        assert!(!names.iter().any(|n| n.contains("node_modules")));
    }

    // show_hidden=false (the default) on a non-git root excludes dotfiles
    // and hidden directories, same as today.
    #[test]
    fn show_hidden_false_excludes_dotfiles_on_non_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".env"), "SECRET=1").unwrap();
        fs::create_dir(root.join(".config")).unwrap();
        fs::write(root.join(".config/settings.json"), "{}").unwrap();
        fs::write(root.join("index.js"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, false, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with("index.js")));
        assert!(!names.iter().any(|n| n.ends_with(".env")));
        assert!(!names.iter().any(|n| n.contains(".config")));
    }

    // show_hidden=true on a non-git root includes dotfiles and files under
    // hidden directories.
    #[test]
    fn show_hidden_true_includes_dotfiles_on_non_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".env"), "SECRET=1").unwrap();
        fs::create_dir(root.join(".config")).unwrap();
        fs::write(root.join(".config/settings.json"), "{}").unwrap();
        fs::write(root.join("index.js"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, true, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with("index.js")));
        assert!(names.iter().any(|n| n.ends_with(".env")));
        assert!(names.iter().any(|n| n.ends_with("settings.json")));
        assert_eq!(counter.load(Ordering::Relaxed), names.len());
    }

    // show_hidden=true still respects standalone `.ignore` files (the ripgrep
    // convention that, unlike `.gitignore`, applies without needing an actual
    // `.git` directory nearby — the only ignore-file mechanism this walker
    // honors on a genuinely non-git root today). A hidden file matched by
    // `.ignore` stays excluded; a non-matched hidden file is included.
    #[test]
    fn show_hidden_true_still_respects_dot_ignore_file_on_non_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".ignore"), ".env.ignored\n").unwrap();
        fs::write(root.join(".env.ignored"), "SECRET=1").unwrap();
        fs::write(root.join(".env.local"), "OTHER=1").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, true, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with(".env.local")));
        assert!(!names.iter().any(|n| n.ends_with(".env.ignored")));
    }

    // show_hidden must not weaken the non-code-directory pruning (the actual
    // ignore mechanism a genuinely non-git root relies on today) even when
    // the pruned directory is itself hidden.
    #[test]
    fn show_hidden_true_still_prunes_non_code_dirs_under_hidden_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".config/node_modules")).unwrap();
        fs::write(root.join(".config/node_modules/lib.js"), "x").unwrap();
        fs::write(root.join(".config/settings.json"), "{}").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, true, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with("settings.json")));
        assert!(!names.iter().any(|n| n.contains("node_modules")));
    }

    // show_hidden=true never surfaces .git internals, even on a non-git-repo
    // walk (e.g. a stray/unrelated .git directory under the indexed root).
    #[test]
    fn show_hidden_true_never_includes_git_internals() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "[core]").unwrap();
        fs::write(root.join("index.js"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, false, false, true, 1, &counter).unwrap();
        let names: Vec<String> = out.pairs.into_iter().map(|(_, rel)| rel).collect();

        assert!(names.iter().any(|n| n.ends_with("index.js")));
        assert!(!names.iter().any(|n| n.contains(".git/")));
    }

    // show_hidden must not change git-repo behavior: git roots already show
    // hidden (non-ignored) files today regardless of this flag.
    #[test]
    fn show_hidden_does_not_change_git_root_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".env"), "SECRET=1").unwrap();
        fs::write(root.join("Cargo.toml"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let off = walk_collect_files(root, true, false, false, 1, &counter).unwrap();
        let mut off_names: Vec<String> = off.pairs.into_iter().map(|(_, rel)| rel).collect();
        off_names.sort();

        let on = walk_collect_files(root, true, false, true, 1, &counter).unwrap();
        let mut on_names: Vec<String> = on.pairs.into_iter().map(|(_, rel)| rel).collect();
        on_names.sort();

        assert_eq!(
            off_names, on_names,
            "show_hidden must be a no-op on git roots"
        );
        assert!(off_names.iter().any(|n| n.ends_with(".env")));
    }

    // Only the zlob backend surfaces reusable ignore rules; they must match
    // the same tree the walk respected.
    #[cfg(feature = "zlob")]
    #[test]
    fn surfaces_reusable_ignore_rules() {
        use std::path::Path;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::write(root.join("Cargo.toml"), "x").unwrap();

        let counter = Arc::new(AtomicUsize::new(0));
        let out = walk_collect_files(root, true, false, false, 1, &counter).unwrap();

        let rules = out.ignore_rules.expect("zlob surfaces ignore rules");
        assert!(rules.is_ignored(Path::new("target/")));
        assert!(rules.is_ignored(Path::new("debug.log")));
        assert!(!rules.is_ignored(Path::new("Cargo.toml")));
    }
}
