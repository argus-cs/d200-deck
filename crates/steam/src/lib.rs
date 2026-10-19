//! The Steam addon that comes with D200 Deck: opens games and Steam's pages,
//! and lights a game's key while it runs. D200 Deck starts it like any
//! addon, as `d200-deck.exe --addon steam` (`$app` in its addon.json), so
//! the installer carries no other program.

mod addon;
pub mod library;
mod vdf;

pub use addon::run;

/// The addon's id, as in its addon.json.
pub const ID: &str = "d200deck.steam";

/// The addon.json the installer puts in the app's addons folder.
pub const MANIFEST: &str = include_str!("../addon/addon.json");

/// Whether Steam is installed on this PC.
pub fn installed() -> bool {
    library::steam_dir().is_some()
}

#[cfg(test)]
mod tests {
    use deck_engine::addons::Manifest;

    #[test]
    fn the_manifest_is_valid_and_starts_the_app() {
        let manifest: Manifest = serde_json::from_str(super::MANIFEST).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.id, super::ID);
        assert_eq!(manifest.run, ["$app", "--addon", "steam"]);
        assert!(manifest.action("launch").is_some_and(|a| a.state));
        assert!(manifest.action("open").is_some());
    }
}
