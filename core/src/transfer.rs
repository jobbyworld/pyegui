//! What an app does with files dropped on it: copy or move them, by the rules of the file managers,
//! in a thread with a progress to draw.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The disk of a path: the device number on Unix, the drive (or the share) on Windows
#[cfg(unix)]
fn volume_of(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    // a path that does not exist yet is on the volume of its first parent that does
    path.ancestors().find_map(|p| fs::metadata(p).ok()).map(|m| m.dev().to_string())
}

#[cfg(not(unix))]
fn volume_of(path: &Path) -> Option<String> {
    use std::path::Component;
    let full = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().ok()?.join(path) };
    match full.components().next()? {
        Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_lowercase()),
        _ => Some(String::new()),
    }
}

pub fn same_volume(a: &Path, b: &Path) -> bool {
    match (volume_of(a), volume_of(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// What a drop of files on a folder does with the keys held (shift, ctrl, alt, command): "move" or
/// "copy". On the same volume the files are moved and Option (macOS) or Ctrl copies them instead; on
/// two volumes they are copied and Cmd (macOS) or Shift moves them instead.
pub fn drop_operation_for(held: (bool, bool, bool, bool), sources: &[PathBuf], folder: &Path) -> &'static str {
    let (shift, ctrl, alt, command) = held;
    let mac = cfg!(target_os = "macos");
    let force_move = if mac { command } else { shift };
    let force_copy = if mac { alt } else { ctrl };
    if !sources.is_empty() && sources.iter().all(|s| same_volume(s, folder)) {
        return if force_copy { "copy" } else { "move" };
    }
    if force_move { "move" } else { "copy" }
}

/// path, or "name copy", "name copy 2"... when it exists
fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let suffix = if n == 1 { " copy".to_string() } else { format!(" copy {n}") };
        let candidate = path.with_file_name(format!("{stem}{suffix}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// The bytes of a file, or of all the files of a folder
fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if meta.is_dir() {
        fs::read_dir(path).map(|it| it.flatten().map(|e| tree_size(&e.path())).sum()).unwrap_or(0)
    } else {
        meta.len()
    }
}

#[derive(Default)]
struct State {
    /// the items not done yet
    running: AtomicUsize,
    total: AtomicU64,
    done: AtomicU64,
    /// (folder, target) of the items done, until read
    finished: Mutex<Vec<(PathBuf, PathBuf)>>,
    errors: Mutex<Vec<String>>,
}

fn copy_file(source: &Path, target: &Path, state: &State) -> std::io::Result<()> {
    let mut src = fs::File::open(source)?;
    let mut dst = fs::File::create(target)?;
    let mut block = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = src.read(&mut block)?;
        if n == 0 {
            break;
        }
        dst.write_all(&block[..n])?;
        state.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    // the permissions and the dates, like a copy of the file managers
    if let Ok(meta) = src.metadata() {
        let _ = fs::set_permissions(target, meta.permissions());
        if let Ok(modified) = meta.modified() {
            let _ = dst.set_modified(modified);
        }
    }
    Ok(())
}

/// Copies a file or a folder, adding the bytes copied to the progress
fn copy_tree(source: &Path, target: &Path, state: &State) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()), state)?;
        }
        let _ = fs::set_permissions(target, meta.permissions());
        return Ok(());
    }
    #[cfg(unix)]
    if meta.file_type().is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(source)?, target);
    }
    copy_file(source, target, state)
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn transfer_one(source: &Path, folder: &Path, moving: bool, state: &State) -> Result<PathBuf, String> {
    if !source.exists() && fs::symlink_metadata(source).is_err() {
        return Err("not found".into());
    }
    if folder.starts_with(source) {
        return Err("a folder cannot go into itself".into());
    }
    let name = source.file_name().ok_or("no name")?;
    let target = unique(&folder.join(name));
    let size = tree_size(source);
    state.total.fetch_add(size, Ordering::Relaxed);
    if moving && same_volume(source, folder) {
        fs::rename(source, &target).map_err(|e| e.to_string())?; // no byte moves: done at once
        state.done.fetch_add(size, Ordering::Relaxed);
    } else {
        copy_tree(source, &target, state).map_err(|e| e.to_string())?;
        if moving {
            remove_tree(source).map_err(|e| e.to_string())?;
        }
    }
    Ok(target)
}


/// Copies or moves files into a folder in a thread, and tells how far it is, so that a window does
/// not freeze on a big file and can draw a progress bar. A name that exists is not replaced: the new
/// one is "name copy", "name copy 2"... A move on the same volume is a rename, between two volumes it
/// is a copy followed by the removal of the original. Create it once and read it at each frame.
#[derive(Clone, Default)]
pub struct Transfer {
    state: Arc<State>,
}

impl Transfer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts to copy or move the files into the folder and returns at once. Files started while
    /// others are still going add to the same progress
    pub fn start(&self, sources: Vec<PathBuf>, folder: PathBuf, moving: bool) {
        if sources.is_empty() {
            return;
        }
        let state = self.state.clone();
        if state.running.load(Ordering::SeqCst) == 0 {
            state.total.store(0, Ordering::SeqCst);
            state.done.store(0, Ordering::SeqCst);
        }
        state.running.fetch_add(sources.len(), Ordering::SeqCst);
        std::thread::spawn(move || {
            for source in sources {
                match transfer_one(&source, &folder, moving, &state) {
                    Ok(target) => state.finished.lock().unwrap().push((folder.clone(), target)),
                    Err(error) => state.errors.lock().unwrap().push(format!("{}: {error}", source.display())),
                }
                state.running.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }

    /// The number of items not done yet. 0 when nothing is going on
    pub fn running(&self) -> usize {
        self.state.running.load(Ordering::SeqCst)
    }

    /// The bytes to copy or move since the transfer started (it grows as the items are measured)
    pub fn total(&self) -> u64 {
        self.state.total.load(Ordering::Relaxed)
    }

    /// The bytes done
    pub fn done(&self) -> u64 {
        self.state.done.load(Ordering::Relaxed)
    }

    /// How far it is, from 0 to 1
    pub fn progress(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.done() as f64 / total as f64).min(1.0)
    }

    /// The items done since the last call, as (folder, new path) pairs, oldest first
    pub fn finished(&self) -> Vec<(PathBuf, PathBuf)> {
        self.state.finished.lock().unwrap().drain(..).collect()
    }

    /// The failures since the last call, as "source: reason" texts
    pub fn errors(&self) -> Vec<String> {
        self.state.errors.lock().unwrap().drain(..).collect()
    }
}
