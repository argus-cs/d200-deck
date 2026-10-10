//! Where the log goes: stderr, as before (seen in `tauri dev` and when
//! running deckd), and a file beside config.json, since the installed app has
//! no console and its warnings and crashes would leave no trace otherwise.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Past this the file starts over, keeping the previous one beside it, also
/// while the app runs (it may stay in the tray for weeks).
const MAX_LOG: u64 = 2 * 1024 * 1024;

/// The log file in `dir` (the config folder).
pub fn file(dir: &Path) -> PathBuf {
    dir.join("d200deck.log")
}

/// Sets up logging (level from RUST_LOG, `info` by default) and records
/// panics from any thread, such as the engine's.
pub fn init(dir: &Path) {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .target(env_logger::Target::Pipe(Box::new(Both::new(file(dir)))))
        .init();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!("panic na thread {}: {info}", thread.name().unwrap_or("sem nome"));
        default_hook(info);
    }));
}

/// The log file, after moving a full one aside; `None` if it can't be opened
/// (logging then goes to stderr only).
fn open(path: &Path) -> Option<File> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG) {
        move_aside(path);
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// The full log becomes d200deck.old.log, replacing the one before. Another
/// instance may hold it open: then it just keeps growing.
fn move_aside(path: &Path) {
    let old = path.with_extension("old.log");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(path, &old);
}

/// Writes each record to stderr and to the file.
struct Both {
    path: PathBuf,
    file: Option<File>,
    /// What the file holds, to start over past `MAX_LOG`.
    written: u64,
}

impl Both {
    fn new(path: PathBuf) -> Self {
        let file = open(&path);
        let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Both { path, file, written }
    }
}

impl Write for Both {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if self.file.is_some() && self.written + buf.len() as u64 > MAX_LOG {
            // Closed before moving it, which Windows needs.
            self.file = None;
            move_aside(&self.path);
            self.file = open(&self.path);
            self.written = 0;
        }
        if let Some(file) = &mut self.file {
            if file.write_all(buf).is_ok() {
                self.written += buf.len() as u64;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        match &mut self.file {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_starts_over_while_the_app_runs() {
        let dir = std::env::temp_dir().join("deck-engine-log-running-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = file(&dir);
        std::fs::write(&path, vec![b'x'; MAX_LOG as usize - 10]).unwrap();
        let mut log = Both::new(path.clone());
        log.write_all(b"cabe
").unwrap();
        // This one would pass the limit: the full file moves aside first.
        log.write_all(b"passa do limite
").unwrap();
        drop(log);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "passa do limite
");
        assert_eq!(std::fs::metadata(dir.join("d200deck.old.log")).unwrap().len(), MAX_LOG - 10 + 5);
    }

    #[test]
    fn a_full_log_is_moved_aside() {
        let dir = std::env::temp_dir().join("deck-engine-log-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = file(&dir);
        std::fs::write(&path, vec![b'x'; MAX_LOG as usize + 1]).unwrap();
        let mut log = open(&path).unwrap();
        log.write_all(b"nova linha\n").unwrap();
        drop(log);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nova linha\n");
        assert_eq!(std::fs::metadata(dir.join("d200deck.old.log")).unwrap().len(), MAX_LOG + 1);
        // A small one is kept and appended to.
        let mut log = open(&path).unwrap();
        log.write_all(b"outra\n").unwrap();
        drop(log);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nova linha\noutra\n");
    }
}
