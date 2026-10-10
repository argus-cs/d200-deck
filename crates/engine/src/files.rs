//! Writing files that something else may be reading: config.json, which the
//! engine watches, and the Iconify cache.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

/// Writes beside `path` and renames over it, so a reader never sees half a
/// file. Each write has its own temporary name: two writers of one file (two
/// downloads of the same set) don't trip on each other's, and the last rename
/// wins.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!("{name}.{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let written = std::fs::write(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written.with_context(|| format!("não consegui gravar {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writers_of_one_file_dont_collide() {
        let dir = std::env::temp_dir().join("deck-engine-files-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("set.json");
        let payloads: Vec<String> = (0..8).map(|i| format!("{{\"writer\":{i}}}")).collect();
        std::thread::scope(|scope| {
            for payload in &payloads {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..20 {
                        write_atomic(path, payload).unwrap();
                    }
                });
            }
        });
        // Whole, from one of them, and no temporary file left behind.
        assert!(payloads.contains(&std::fs::read_to_string(&path).unwrap()));
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["set.json"]);
    }
}
