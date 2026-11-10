//! Mode ligne de commande : `inventory`, `updates`, `clean`, `report`.
//! Le même noyau que l'interface graphique, sans dépendance à l'affichage.

use crate::cleanup;
use crate::distro::Capabilities;
use crate::inventory::Inventory;
use crate::pkg::Source;
use crate::report::{self, Report};
use crate::util;
use std::path::{Path, PathBuf};

/// Point d'entrée CLI. Renvoie le code de sortie du processus.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("inventory") => inventory(&args[1..]),
        Some("updates") => updates(),
        Some("clean") => clean(args.iter().any(|arg| arg == "--apply")),
        Some("report") => report(args.get(1).map(String::as_str)),
        Some("panel") => panel(args.get(1).map(String::as_str)),
        Some("version") | Some("--version") => {
            println!("OrbiSys {}", crate::APP_VERSION);
            0
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("Commande inconnue : {other}\n");
            print_help();
            2
        }
    }
}

fn print_help() {
    println!(
        "OrbiSys {} — maintenance et administration Linux\n\n\
         Utilisation : orbisys [commande] [options]\n\n\
         Sans commande, l'interface graphique se lance. Sinon :\n\n\
         \tinventory [--txt F] [--csv F] [--json F]  Liste des logiciels installés\n\
         \tupdates                                  Mises à jour disponibles\n\
         \tclean [--apply]                          Cibles de nettoyage (et taille)\n\
         \treport [DOSSIER]                        Rapport TXT, CSV, JSON et PDF\n\
         \tpanel [NOM]                              Affiche une rubrique (sans NOM : la liste)\n\
         \tversion | help\n\n\
         Les actions privilégiées (suppression, nettoyage système) passent par\n\
         Polkit et ne sont jamais exécutées sans confirmation.",
        crate::APP_VERSION
    );
}

fn panel(name: Option<&str>) -> i32 {
    use crate::panels::PanelId;
    match name {
        None => {
            for id in PanelId::ALL {
                println!("{}", id.name());
            }
            0
        }
        Some(name) => match PanelId::from_name(name) {
            Some(id) => {
                print!("{}", id.collect().render_text());
                0
            }
            None => {
                eprintln!("Rubrique inconnue : {name} (liste : orbisys panel)");
                2
            }
        },
    }
}

fn load_inventory() -> (Inventory, Capabilities) {
    let capabilities = Capabilities::probe();
    let inventory = Inventory::load(&capabilities);
    (inventory, capabilities)
}

fn inventory(args: &[String]) -> i32 {
    let (inventory, _) = load_inventory();
    let (native, snap, flatpak, manual) = inventory.counts();
    println!("Distribution : {}", crate::distro::current().display_name());
    println!(
        "Gestionnaire : {}",
        inventory
            .manager
            .map(|manager| manager.label())
            .unwrap_or("inconnu")
    );
    println!(
        "Logiciels    : {native} paquets, {snap} snap, {flatpak} flatpak, {manual} manuels"
    );
    for error in &inventory.errors {
        eprintln!("avertissement : {error}");
    }
    println!();
    for package in inventory.filtered("", None) {
        println!(
            "{:<34} {:<18} {:<8} {}",
            util::ellipsis(&package.name, 34),
            util::ellipsis(&package.version, 18),
            package.source.label(),
            util::human_bytes(package.size_bytes)
        );
    }
    for app in &inventory.manual {
        println!(
            "{:<34} {:<18} {:<8} {}",
            util::ellipsis(&app.name, 34),
            "—",
            app.kind.label(),
            app.path.display()
        );
    }

    let mut export_error = false;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let target = args.get(index + 1).map(PathBuf::from);
        let result = match (flag, target) {
            ("--txt", Some(path)) => report::write_inventory_txt(&inventory, &path),
            ("--csv", Some(path)) => report::write_inventory_csv(&inventory, &path),
            ("--json", Some(path)) => report::write_inventory_json(&inventory, &path),
            ("--txt" | "--csv" | "--json", None) => {
                eprintln!("{flag} attend un chemin de fichier");
                return 2;
            }
            _ => {
                index += 1;
                continue;
            }
        };
        match result {
            Ok(()) => {
                if let Some(path) = &args.get(index + 1) {
                    println!("\nExport écrit : {path}");
                }
            }
            Err(error) => {
                eprintln!("écriture impossible : {error}");
                export_error = true;
            }
        }
        index += 2;
    }
    if export_error {
        1
    } else {
        0
    }
}

fn updates() -> i32 {
    let Some(manager) = crate::pkg::PackageManager::detect() else {
        eprintln!("Aucun gestionnaire de paquets reconnu.");
        return 1;
    };
    match manager.updates() {
        Ok(updates) => {
            println!("{} mise(s) à jour disponible(s) via {}.", updates.len(), manager.label());
            for update in &updates {
                if update.current.is_empty() {
                    println!("  {:<34} → {}", util::ellipsis(&update.name, 34), update.available);
                } else {
                    println!(
                        "  {:<34} {} → {}",
                        util::ellipsis(&update.name, 34),
                        update.current,
                        update.available
                    );
                }
            }
            println!("\nAppliquer : sudo {}", manager.update_command());
            0
        }
        Err(error) => {
            eprintln!("Lecture des mises à jour impossible : {error}");
            1
        }
    }
}

fn clean(apply: bool) -> i32 {
    let capabilities = Capabilities::probe();
    let items = cleanup::scan(&capabilities);
    if items.is_empty() {
        println!("Aucune cible de nettoyage détectée.");
        return 0;
    }
    let mut total = 0u64;
    for item in &items {
        total += item.size;
        let size = if item.size > 0 {
            util::human_bytes(item.size)
        } else {
            "à traiter".to_string()
        };
        println!(
            "{:<38} {:>10}  {}{}",
            util::ellipsis(&item.label, 38),
            size,
            if item.requires_root { "[root] " } else { "" },
            util::ellipsis(&item.description, 70)
        );
    }
    println!("\nTotal mesuré : {}", util::human_bytes(total));
    if !apply {
        println!("Rien n'a été supprimé. Relancer avec --apply pour les cibles personnelles,");
        println!("ou utiliser l'interface graphique pour les cibles système.");
        return 0;
    }
    let mut failures = 0;
    for item in &items {
        if item.requires_root {
            println!("{} : ignoré en CLI (privilèges), à faire depuis l'interface.", item.label);
            continue;
        }
        match cleanup::run(item) {
            Ok(message) => {
                println!("{} : {message}", item.label);
                crate::journal::record("clean", &item.label, true, &message);
            }
            Err(error) => {
                println!("{} : échec — {error}", item.label);
                crate::journal::record("clean", &item.label, false, &error);
                failures += 1;
            }
        }
    }
    if failures > 0 {
        1
    } else {
        0
    }
}

fn report(directory: Option<&str>) -> i32 {
    let directory: PathBuf = directory
        .map(PathBuf::from)
        .or_else(dirs::document_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let report = Report::collect();
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let base = format!("orbisys-{stamp}");
    let targets: [(&str, PathBuf); 4] = [
        ("txt", directory.join(format!("{base}.txt"))),
        ("csv", directory.join(format!("{base}.csv"))),
        ("json", directory.join(format!("{base}.json"))),
        ("pdf", directory.join(format!("{base}.pdf"))),
    ];
    let mut failures = 0;
    for (kind, path) in &targets {
        let result = match *kind {
            "txt" => report.write_txt(path),
            "csv" => report.write_csv(path),
            "json" => report.write_json(path),
            _ => report.write_pdf(path),
        };
        match result {
            Ok(()) => {
                println!("Écrit : {}", path.display());
                crate::journal::record("report", &path.display().to_string(), true, kind);
            }
            Err(error) => {
                eprintln!("Échec {} : {error}", path.display());
                failures += 1;
            }
        }
    }
    if failures > 0 {
        1
    } else {
        0
    }
}

/// Résumé d'une ligne de tableau pour le mode CLI (`nom source`).
pub fn summarize(package: &crate::pkg::Package) -> String {
    let origin = match package.source {
        Source::Native => "paquet",
        Source::Snap => "snap",
        Source::Flatpak => "flatpak",
        Source::Manual => "manuel",
    };
    format!("{} ({origin})", package.name)
}

/// Écrit un fichier de rapport vers un chemin explicite (`--out`).
pub fn write_report_to(path: &Path) -> std::io::Result<()> {
    let report = Report::collect();
    if path.extension().map(|ext| ext == "pdf").unwrap_or(false) {
        report.write_pdf(path)
    } else {
        report.write_txt(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_labels_sources() {
        let package = crate::pkg::Package::new("code", "1.0", Source::Snap);
        assert_eq!(summarize(&package), "code (snap)");
    }

    #[test]
    fn unknown_command_returns_error_code() {
        assert_eq!(run(&["commande-inconnue".to_string()]), 2);
    }
}
