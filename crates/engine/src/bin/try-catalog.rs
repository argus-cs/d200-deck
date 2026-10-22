//! Checks an addon catalog with the app's own rules, for the catalog's CI
//! (argus-cs/d200-deck-addons): every entry one D200 Deck can use, and its
//! zip downloaded, unpacked and compared with it. Exits with 1 on any problem.
//! Usage: cargo run -p deck-engine --bin try-catalog -- check <catalog.json>

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use deck_engine::addons::catalog;

const USAGE: &str = "uso: try-catalog check <catalog.json>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, path] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if command != "check" {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match check(Path::new(path)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

/// True when every entry passes.
fn check(path: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(path).with_context(|| format!("não consegui ler {}", path.display()))?;
    let entries = catalog::read(&text)?;
    let staging = std::env::temp_dir().join(format!("d200deck-try-catalog-{}", std::process::id()));
    let mut failed = 0;
    for entry in &entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(why) => {
                failed += 1;
                println!("ERRO  {why}");
                continue;
            }
        };
        let problems = match catalog::fetch_entry(entry, &staging) {
            Ok(staged) => catalog::differences(entry, &staged.manifest),
            Err(e) => vec![format!("{e:#}")],
        };
        if problems.is_empty() {
            println!("ok    {} {}", entry.id, entry.version);
            continue;
        }
        failed += 1;
        println!("ERRO  {} {}", entry.id, entry.version);
        for problem in problems {
            println!("      {problem}");
        }
    }
    let _ = std::fs::remove_dir_all(&staging);
    println!("\n{} addons, {failed} com problema", entries.len());
    Ok(failed == 0)
}
