//! Headless D200 Deck: runs the engine without the interface.
//! Usage: deckd [path/to/config.json]   (default: %APPDATA%\D200Deck\config.json)

use std::path::PathBuf;

use deck_engine::config::Config;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(Config::default_path);
    deck_engine::logging::init(&Config::dir_of(&path));
    deck_engine::runtime::run(path)
}
