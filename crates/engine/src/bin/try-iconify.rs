//! Tries the Iconify downloads outside the app, in a folder of your choice
//! (it stands for the config folder: downloads go to <folder>/iconify).
//! Usage: cargo run -p deck-engine --bin try-iconify -- <folder> sets
//!        cargo run -p deck-engine --bin try-iconify -- <folder> icons <set> [filter]
//!        cargo run -p deck-engine --bin try-iconify -- <folder> search <text>
//!        cargo run -p deck-engine --bin try-iconify -- <folder> samples <set:icon>...
//!        cargo run -p deck-engine --bin try-iconify -- <folder> save <set:icon>

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use deck_engine::iconify;
use deck_engine::icons::{render_key, Look, ICON_COLOR};

const USAGE: &str = "uso: try-iconify <pasta> sets | icons <coleção> [filtro] | search <texto> | samples <coleção:ícone>... | save <coleção:ícone>";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let base = PathBuf::from(args.first().context(USAGE)?);
    let rest = &args[1..];
    let start = Instant::now();
    match rest.first().map(String::as_str) {
        Some("sets") => {
            let sets = iconify::sets(&base)?;
            let total: u64 = sets.iter().map(|s| s.total).sum();
            println!("{} coleções, {total} ícones", sets.len());
            for set in sets.iter().take(5) {
                println!("  {} · {} · {} ícones · {} · amostras {:?}", set.prefix, set.name, set.total, set.license, set.samples);
            }
        }
        Some("icons") => {
            let prefix = rest.get(1).context(USAGE)?;
            let filter = rest[2..].join(" ");
            let page = iconify::icons(&base, prefix, &filter, 0, 10)?;
            println!("{} ícones casam; os primeiros: {:?}", page.total, page.icons.iter().map(|i| &i.name).collect::<Vec<_>>());
        }
        Some("search") => {
            let found = iconify::search(&rest[1..].join(" "))?;
            println!("{} achados; os primeiros: {:?}", found.len(), found.iter().take(10).map(|i| &i.name).collect::<Vec<_>>());
        }
        Some("samples") => {
            let found = iconify::samples(&base, &rest[1..])?;
            for icon in &found {
                println!("  {} · {} bytes", icon.name, icon.svg.len());
            }
            println!("{} de {} (guardadas em {})", found.len(), rest.len() - 1, base.join("iconify").join("samples.json").display());
        }
        Some("save") => {
            let path = iconify::save(&base, rest.get(1).context(USAGE)?)?;
            let look = Look {
                icon: Some(&path),
                background: "#24262B",
                icon_color: ICON_COLOR,
                border: None,
                label: "Exemplo",
                text_color: "#FFFFFF",
                label_px: 24,
                dim: false,
            };
            let png = base.join("key.png");
            std::fs::write(&png, render_key(&look, &base)?)?;
            println!("salvo em {path}; tecla de exemplo em {}", png.display());
        }
        _ => bail!(USAGE),
    }
    println!("({:.0} ms)", start.elapsed().as_secs_f64() * 1000.0);
    Ok(())
}
