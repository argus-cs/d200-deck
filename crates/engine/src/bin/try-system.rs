//! Reads or changes the Windows settings of "system" actions, outside the device loop.
//! Usage: cargo run -p deck-engine --bin try-system -- state
//!        cargo run -p deck-engine --bin try-system -- outputs
//!        cargo run -p deck-engine --bin try-system -- set <setting> [toggle|on|off] [value]

use std::time::Instant;

use anyhow::{bail, Context, Result};
use deck_engine::system::{apply, audio_outputs, describe, state, validate, Kind, Setting, Switch};

const USAGE: &str = "uso: try-system state | outputs | set <ajuste> [toggle|on|off] [valor]";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("state") => {
            // The runtime reads these every second: the later rounds show the cached cost.
            for round in 1..=3 {
                let start = Instant::now();
                print_states(round == 1)?;
                println!("rodada {round}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
            }
            Ok(())
        }
        Some("outputs") => {
            for name in audio_outputs()? {
                println!("{name}");
            }
            Ok(())
        }
        Some("set") => set(&args[1..]),
        _ => bail!(USAGE),
    }
}

fn print_states(show: bool) -> Result<()> {
    for setting in Setting::ALL {
        let values: Vec<String> = match setting.kind() {
            Kind::Choice if setting == Setting::AudioOutput => audio_outputs()?,
            Kind::Choice => setting.choices().iter().map(|v| v.to_string()).collect(),
            _ => vec![String::new()],
        };
        for value in values {
            let value = (!value.is_empty()).then_some(value);
            let start = Instant::now();
            let now = state(setting, value.as_deref());
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            if show {
                let label = match (setting.kind(), now) {
                    (_, None) => "desconhecido",
                    (Kind::Choice, Some(true)) => "atual",
                    (Kind::Choice, Some(false)) => "-",
                    (_, Some(true)) => "ligado",
                    (_, Some(false)) => "desligado",
                };
                let name = serde_json::to_value(setting)?;
                let name = name.as_str().unwrap_or_default();
                match &value {
                    Some(value) => println!("{name} [{value}]: {label} ({ms:.1} ms)"),
                    None => println!("{name}: {label} ({ms:.1} ms)"),
                }
            }
        }
    }
    Ok(())
}

fn set(args: &[String]) -> Result<()> {
    let name = args.first().context(USAGE)?;
    let setting: Setting = serde_json::from_value(name.as_str().into()).with_context(|| format!("ajuste desconhecido {name:?}"))?;
    let mut rest = &args[1..];
    let mut switch = Switch::Toggle;
    if let Some(first) = rest.first() {
        if let Ok(parsed) = serde_json::from_value::<Switch>(first.as_str().into()) {
            switch = parsed;
            rest = &rest[1..];
        }
    }
    // Device names have spaces.
    let value = (!rest.is_empty()).then(|| rest.join(" "));
    validate(setting, value.as_deref())?;
    println!("{}", describe(setting, switch, value.as_deref()));
    apply(setting, switch, value.as_deref())?;
    println!("agora: {:?}", state(setting, value.as_deref()));
    Ok(())
}
