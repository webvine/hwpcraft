// Adapted from WordCraft (MIT). Copyright (c) 2026 ArtCraft Team and the WordCraft contributors.
// License: ../../../assets/licenses/LICENSE-WordCraft-MIT.txt
//! The folders automation may read and write (#769).
//!
//! `hwpcraft-cli mcp --automation-read-root <dir> --automation-write-root <dir>` confines the
//! files an agent's commands reach. [`AutomationRoots`] holds the two folders. While they are in force, every path a command
//! reads goes through [`check_read`] and every path it writes through [`check_write`]:
//!
//! - The path is made absolute against the working directory, as the file system reads it. Its
//!   links are followed: for a file that doesn't exist yet, the links of its nearest existing
//!   folder, the rest of the path being plain names (`..` below a missing folder is refused, and so
//!   is a link that leads nowhere).
//! - The result must lie inside the root, compared by whole components (`/work2` isn't inside
//!   `/work`), without regard to case on Windows.
//! - Read and write access are separate: a root given alone grants only its own kind of access.
//!
//! Roots are in force on a thread inside [`confine`], and in the whole process after
//! [`confine_process`] (`hwpcraft-cli mcp`), except inside [`unconfined`]. Without roots
//! nothing is checked.
//!
//! A path is checked, then opened: a process that can change the folders in between (swapping in
//! a link) can still win that race. The roots keep an agent to the files it was given; they don't
//! guard against other programs on the computer.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Read access or write access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// The folders automation may read and write; a root left out grants no access of its kind.
#[derive(Debug)]
pub struct AutomationRoots {
    read: Option<Root>,
    write: Option<Root>,
}

#[derive(Debug)]
struct Root {
    /// The folder made absolute, links followed: what paths are compared with.
    resolved: PathBuf,
    /// The same without Windows' `\\?\` prefix, for people.
    shown: PathBuf,
}

impl Root {
    /// The folder at `path`, which must exist.
    fn open(path: &Path, access: Access) -> Result<Self, String> {
        let kind = match access {
            Access::Read => "read",
            Access::Write => "write",
        };
        let resolved = std::fs::canonicalize(path)
            .and_then(|p| {
                if p.is_dir() {
                    Ok(p)
                } else {
                    Err(std::io::ErrorKind::NotADirectory.into())
                }
            })
            .map_err(|e| {
                format!(
                    "cannot open the automation {kind} root `{}`: {e}",
                    path.display()
                )
            })?;
        Ok(Self {
            shown: plain(resolved.clone()),
            resolved,
        })
    }
}

impl AutomationRoots {
    /// The roots `read` and `write`, which must be existing folders; `None` when neither is given
    /// (nothing is confined then).
    pub fn new(read: Option<&Path>, write: Option<&Path>) -> Result<Option<Arc<Self>>, String> {
        if read.is_none() && write.is_none() {
            return Ok(None);
        }
        let read = read.map(|p| Root::open(p, Access::Read)).transpose()?;
        let write = write.map(|p| Root::open(p, Access::Write)).transpose()?;
        Ok(Some(Arc::new(Self { read, write })))
    }

    /// The read root, absolute with its links followed.
    pub fn read_root(&self) -> Option<&Path> {
        self.read.as_ref().map(|r| r.shown.as_path())
    }

    /// The write root, absolute with its links followed.
    pub fn write_root(&self) -> Option<&Path> {
        self.write.as_ref().map(|r| r.shown.as_path())
    }

    /// May the file or folder at `path` be read (or written)? `Err` says why not.
    pub fn check(&self, path: &str, access: Access) -> Result<(), String> {
        let root = match access {
            Access::Read => self.read.as_ref(),
            Access::Write => self.write.as_ref(),
        };
        let Some(Root {
            resolved: folder,
            shown,
        }) = root
        else {
            return match access {
                Access::Read => Err(format!(
                    "automation filesystem access is not granted: read authority is absent: {path}"
                )),
                Access::Write => Err(format!(
                    "automation filesystem access is not granted: write authority is absent: {path}"
                )),
            };
        };
        if inside(&resolve(path)?, folder) {
            return Ok(());
        }
        let root = shown.display();
        match access {
            Access::Read => Err(format!(
                "automation path rejected: outside the read root {root}: {path}"
            )),
            Access::Write => Err(format!(
                "automation path rejected: outside the write root {root}: {path}"
            )),
        }
    }
}

/// Where the roots are in force on a thread ([`confine`], [`unconfined`]).
#[derive(Clone, Debug)]
enum Scope {
    Confined(Arc<AutomationRoots>),
    Unconfined,
}

thread_local! {
    /// This thread's scope; `None`: the process's roots ([`confine_process`]).
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// The roots in force in every thread outside [`confine`] and [`unconfined`].
static PROCESS: OnceLock<Arc<AutomationRoots>> = OnceLock::new();

/// Put `roots` in force in the whole process, for good. `false` when roots were in force already
/// (they stay as they were).
pub fn confine_process(roots: Arc<AutomationRoots>) -> bool {
    PROCESS.set(roots).is_ok()
}

/// Puts the scope it replaced back when dropped (also when `f` panics).
struct Restore(Option<Scope>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCOPE.with(|s| *s.borrow_mut() = previous);
    }
}

fn with_scope<T>(scope: Option<Scope>, f: impl FnOnce() -> T) -> T {
    let _restore = Restore(SCOPE.with(|s| s.replace(scope)));
    f()
}

/// Run `f` with `roots` in force on this thread (`None`: as things stand).
pub fn confine<T>(roots: Option<&Arc<AutomationRoots>>, f: impl FnOnce() -> T) -> T {
    match roots {
        Some(r) => with_scope(Some(Scope::Confined(r.clone())), f),
        None => f(),
    }
}

/// Run `f` with no roots in force on this thread: what the person at the keyboard does, and the
/// app's own folders.
pub fn unconfined<T>(f: impl FnOnce() -> T) -> T {
    with_scope(Some(Scope::Unconfined), f)
}

/// The roots in force on this thread, if any.
pub fn current() -> Option<Arc<AutomationRoots>> {
    match SCOPE.with(|s| s.borrow().clone()) {
        Some(Scope::Confined(r)) => Some(r),
        Some(Scope::Unconfined) => None,
        None => PROCESS.get().cloned(),
    }
}

/// `path` without Windows' `\\?\` prefix (which `canonicalize` adds), for people to read.
fn plain(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// May `path` be read here? Always, without roots in force.
pub fn check_read(path: &str) -> Result<(), String> {
    current().map_or(Ok(()), |r| r.check(path, Access::Read))
}

thread_local! {
    /// Nesting depth of [`forbid_writes`] guards alive on this thread.
    static WRITES_FORBIDDEN: Cell<usize> = const { Cell::new(0) };
}

/// Refuses every write on this thread, whatever roots are in force, until dropped.
pub struct ForbidWrites(());

impl Drop for ForbidWrites {
    fn drop(&mut self) {
        WRITES_FORBIDDEN.with(|n| n.set(n.get().saturating_sub(1)));
    }
}

/// Refuse file and settings writes while the guard lives: an atomic batch can roll back
/// document changes, never what it already wrote to disk.
pub fn forbid_writes() -> ForbidWrites {
    WRITES_FORBIDDEN.with(|n| n.set(n.get() + 1));
    ForbidWrites(())
}

/// May `path` be written here? Always, without roots in force, unless writes are forbidden.
pub fn check_write(path: &str) -> Result<(), String> {
    if WRITES_FORBIDDEN.with(Cell::get) > 0 {
        return Err("automation path rejected: writes are not allowed here (atomic batch)".into());
    }
    current().map_or(Ok(()), |r| r.check(path, Access::Write))
}

/// `path` made absolute with its links followed; for a path that doesn't exist (yet), its nearest
/// existing folder's links followed and the rest appended.
fn resolve(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("automation path rejected: path is empty".into());
    }
    // Checked as given: making `…\NUL` absolute gives the device path `\\.\NUL` on Windows.
    if cfg!(windows)
        && Path::new(path)
            .components()
            .any(|c| matches!(c, Component::Normal(n) if is_device_name(&n.to_string_lossy())))
    {
        return Err(format!(
            "automation path rejected: reserved device names are not allowed: {path}"
        ));
    }
    let absolute = std::path::absolute(path)
        .map_err(|e| format!("automation path rejected: cannot resolve {path}: {e}"))?;
    // The names below the nearest existing folder, last first.
    let mut missing: Vec<OsString> = vec![];
    let mut at = absolute.as_path();
    loop {
        match std::fs::canonicalize(at) {
            Ok(mut found) => {
                found.extend(missing.iter().rev());
                return Ok(found);
            }
            Err(e) => match std::fs::symlink_metadata(at) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(format!(
                        "automation path rejected: a link that leads nowhere: {path}"
                    ));
                }
                // There, yet its real path can't be had (no permission): nothing to compare.
                Ok(_) => {
                    return Err(format!(
                        "automation path rejected: cannot resolve {path}: {e}"
                    ));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(format!(
                        "automation path rejected: cannot resolve {path}: {e}"
                    ));
                }
            },
        }
        let parent = at.parent();
        match (at.components().next_back(), parent) {
            (Some(Component::Normal(name)), Some(parent)) => {
                missing.push(name.to_os_string());
                at = parent;
            }
            (Some(Component::ParentDir), _) => {
                return Err(format!(
                    "automation path rejected: parent traversal below a folder that doesn't exist: {path}"
                ));
            }
            // Down to the root (or drive) and it doesn't exist either.
            _ => {
                let e = std::io::Error::from(std::io::ErrorKind::NotFound);
                return Err(format!(
                    "automation path rejected: cannot resolve {path}: {e}"
                ));
            }
        }
    }
}

/// Is `path` (resolved) `root` or inside it, component by component (without regard to case on
/// Windows, whose file systems ignore it)?
fn inside(path: &Path, root: &Path) -> bool {
    let mut path = path.components();
    root.components()
        .all(|r| path.next().is_some_and(|p| same_component(p, r)))
}

fn same_component(a: Component, b: Component) -> bool {
    if a == b {
        return true;
    }
    if !cfg!(windows) {
        return false;
    }
    match (a.as_os_str().to_str(), b.as_os_str().to_str()) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    }
}

/// A Windows device name (`CON`, `NUL`, `COM1`… also with an extension), which names no file.
fn is_device_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
    };
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) || numbered("COM")
        || numbered("LPT")
}
