//! OrbiSys — point d'entrée.
//!
//! Sans argument : interface graphique. Avec un argument : mode CLI
//! (`inventory`, `updates`, `clean`, `report`, `version`, `help`).

mod gui;

fn main() -> std::process::ExitCode {
    orbisys::util::migrate_legacy_dirs();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.is_empty() {
        return match gui::run() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Interface graphique indisponible : {error}");
                eprintln!("Le mode ligne de commande reste utilisable : orbisys help");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let code = orbisys::cli::run(&arguments);
    std::process::ExitCode::from(code as u8)
}
