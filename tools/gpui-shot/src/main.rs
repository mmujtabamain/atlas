//! `gpui-shot` binary: parse arguments, run, map the outcome to an exit code.
//! See `lib.rs` for what the tool does and why.

use std::process::ExitCode;

use gpui_shot::{args, logger};
use log::LevelFilter;

fn main() -> ExitCode {
    let options = match args::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    // `-v` has already set RUST_LOG=debug when nothing else did (see `args::parse`).
    logger::init(logger::level_from_env(LevelFilter::Info));

    match gpui_shot::run(&options) {
        Ok(outcome) => {
            println!(
                "{} {}x{} window=0x{:x} settled={} blank={}",
                outcome.path.display(),
                outcome.width,
                outcome.height,
                outcome.window.id,
                outcome.settled,
                outcome.blank
            );
            for extra in &outcome.extra_shots {
                println!("{}", extra.display());
            }
            if outcome.is_good() {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "warning: frame written but {}; app log: {}",
                    if outcome.blank { "it is blank" } else { "it never settled" },
                    outcome.app_log.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "n/a".into())
                );
                ExitCode::from(3)
            }
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
