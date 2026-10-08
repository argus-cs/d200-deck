//! Runs one "open" action outside the device loop, to debug app launching.
//! Usage: cargo run -p deck-engine --bin try-open -- <target> [args]

use deck_engine::actions::execute;
use deck_engine::config::Action;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let target = args.next().expect("faltou o alvo");
    let extra = args.next();
    execute(&Action::Open { target, args: extra, watch: false })
}
