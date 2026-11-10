//! Services personnalisés : lecture, génération, installation et pilotage
//! d'unités systemd propres à l'administrateur.

use crate::exec;
use crate::util;
use serde::Serialize;
use std::path::PathBuf;

/// Une unité `systemd` telle que listée par `systemctl list-unit-files`.
#[derive(Debug, Clone, Serialize)]
pub struct UnitFile {
    pub name: String,
    pub state: String,
    pub custom: bool,
}

/// Dossier des unités administrateur.
pub const SYSTEM_DIR: &str = "/etc/systemd/system";

/// Analyse la sortie de `systemctl list-unit-files --type=service`.
pub fn parse_unit_files(text: &str) -> Vec<UnitFile> {
    let mut units: Vec<UnitFile> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            if !name.ends_with(".service") || !util::valid_unit_name(name) {
                return None;
            }
            let state = fields.next().unwrap_or("unknown").to_string();
            Some(UnitFile {
                name: name.to_string(),
                state,
                custom: false,
            })
        })
        .collect();
    units.sort_by(|a, b| a.name.cmp(&b.name));
    units.dedup_by(|a, b| a.name == b.name);
    units
}

/// Marque les unités dont le fichier vit dans `/etc/systemd/system`.
pub fn mark_custom(units: &mut [UnitFile]) {
    for unit in units.iter_mut() {
        let path = PathBuf::from(SYSTEM_DIR).join(&unit.name);
        // Un lien symbolique pointe vers une unité du paquet : ce n'est pas
        // une unité écrite par l'administrateur.
        unit.custom = path.is_file() && !path.is_symlink();
    }
}

/// Liste les services et signale ceux qui sont personnalisés.
pub fn list_units() -> Result<Vec<UnitFile>, String> {
    let args = [
        "list-unit-files",
        "--type=service",
        "--no-legend",
        "--no-pager",
    ];
    let captured = exec::run_with_timeout("systemctl", &args, std::time::Duration::from_secs(30))
        .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    let mut units = parse_unit_files(&captured.stdout);
    mark_custom(&mut units);
    Ok(units)
}

/// Modèle d'unité prêt à être édité.
pub fn unit_template(name: &str, description: &str, exec_start: &str, user: &str) -> String {
    let description = if description.trim().is_empty() {
        name.trim_end_matches(".service")
    } else {
        description.trim()
    };
    let exec_start = if exec_start.trim().is_empty() {
        "/usr/bin/true"
    } else {
        exec_start.trim()
    };
    let mut text = format!(
        "[Unit]\nDescription={description}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={exec_start}\nRestart=on-failure\nRestartSec=5\n"
    );
    if !user.trim().is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        text.push_str(&format!("User={}\n", user.trim()));
    }
    text.push_str("\n[Install]\nWantedBy=multi-user.target\n");
    text
}

/// Vérifie un nom d'unité et le contenu du fichier avant installation.
pub fn validate_definition(name: &str, text: &str) -> Result<(), String> {
    if !name.ends_with(".service") {
        return Err("le nom doit se terminer par .service".to_string());
    }
    if name.contains('/') || !util::valid_unit_name(name) {
        return Err(format!("nom d'unité refusé : {name}"));
    }
    if text.len() > 64 * 1024 {
        return Err("fichier d'unité trop volumineux (64 Kio maximum)".to_string());
    }
    if !text.contains("[Service]") {
        return Err("section [Service] absente".to_string());
    }
    let has_exec = text
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with("ExecStart=") && line.len() > "ExecStart=".len());
    if !has_exec {
        return Err("ExecStart= manquant ou vide".to_string());
    }
    Ok(())
}

/// Lit le fichier d'une unité (personnalisée d'abord, puis paquet).
pub fn read_unit(name: &str) -> Result<String, String> {
    if !util::valid_unit_name(name) {
        return Err(format!("nom d'unité refusé : {name}"));
    }
    let candidates = [
        PathBuf::from(SYSTEM_DIR).join(name),
        PathBuf::from("/lib/systemd/system").join(name),
        PathBuf::from("/usr/lib/systemd/system").join(name),
    ];
    for path in candidates {
        if path.is_file() {
            return std::fs::read_to_string(&path)
                .map_err(|error| format!("{} : {error}", path.display()));
        }
    }
    Err(format!("unité introuvable : {name}"))
}

/// Installe une unité dans `/etc/systemd/system` puis recharge systemd.
pub fn install(name: &str, text: &str) -> Result<String, String> {
    validate_definition(name, text)?;
    let temp = util::write_private_temp("um-unit", text)?;
    let target = format!("{SYSTEM_DIR}/{name}");
    let install = exec::run_privileged(
        "install",
        &[
            "-m".to_string(),
            "0644".to_string(),
            temp.display().to_string(),
            target.clone(),
        ],
    );
    let _ = std::fs::remove_file(&temp);
    install?;
    exec::run_privileged("systemctl", &["daemon-reload".to_string()])?;
    Ok(format!("unité installée : {target}"))
}

/// Supprime une unité personnalisée après arrêt et désactivation.
pub fn remove(name: &str) -> Result<String, String> {
    if !util::valid_unit_name(name) {
        return Err(format!("nom d'unité refusé : {name}"));
    }
    let target = format!("{SYSTEM_DIR}/{name}");
    if !PathBuf::from(&target).is_file() {
        return Err(format!("pas une unité personnalisée : {name}"));
    }
    let _ = exec::run_privileged("systemctl", &["disable".to_string(), "--now".to_string(), name.to_string()]);
    exec::run_privileged("rm", &["-f".to_string(), target.clone()])?;
    exec::run_privileged("systemctl", &["daemon-reload".to_string()])?;
    Ok(format!("unité supprimée : {target}"))
}

/// Exécute `systemctl <action> <unité>` avec élévation.
pub fn control(action: &str, name: &str) -> Result<String, String> {
    if !matches!(action, "start" | "stop" | "restart" | "enable" | "disable") {
        return Err(format!("action refusée : {action}"));
    }
    if !util::valid_unit_name(name) {
        return Err(format!("nom d'unité refusé : {name}"));
    }
    exec::run_privileged(
        "systemctl",
        &[action.to_string(), name.to_string()],
    )
    .map(|_| format!("{action} : {name}"))
}

/// État détaillé d'une unité.
pub fn status(name: &str) -> Result<String, String> {
    if !util::valid_unit_name(name) {
        return Err(format!("nom d'unité refusé : {name}"));
    }
    let captured = exec::run_with_timeout(
        "systemctl",
        &["status", name, "--no-pager", "--lines=15"],
        std::time::Duration::from_secs(20),
    )
    .map_err(|error| error.0)?;
    if captured.stdout.trim().is_empty() && !captured.stderr.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(captured.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unit_file_listing() {
        let text = "nginx.service enabled enabled\nssh.service enabled enabled\nbad name here\nfoo.timer disabled enabled\n";
        let units = parse_unit_files(text);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].name, "nginx.service");
        assert_eq!(units[0].state, "enabled");
    }

    #[test]
    fn rejects_invalid_unit_names() {
        assert!(validate_definition("bad;name.service", "[Service]\nExecStart=/bin/true\n").is_err());
        assert!(validate_definition("../../etc/passwd", "[Service]\nExecStart=/bin/true\n").is_err());
        assert!(validate_definition("ok.service", "[Service]\nExecStart=/bin/true\n").is_ok());
    }

    #[test]
    fn requires_exec_start() {
        assert!(validate_definition("ok.service", "[Service]\nExecStart=\n").is_err());
        assert!(validate_definition("ok.service", "[Unit]\nDescription=x\n").is_err());
    }

    #[test]
    fn builds_template_with_sections() {
        let text = unit_template("demo.service", "Démo", "/usr/bin/sleep 10", "amine");
        assert!(text.contains("[Unit]"));
        assert!(text.contains("[Service]"));
        assert!(text.contains("[Install]"));
        assert!(text.contains("ExecStart=/usr/bin/sleep 10"));
        assert!(text.contains("User=amine"));
        assert!(validate_definition("demo.service", &text).is_ok());
    }

    #[test]
    fn ignores_invalid_user_in_template() {
        let text = unit_template("demo.service", "", "", "root; reboot");
        assert!(!text.contains("User="));
    }

    #[test]
    fn rejects_unknown_control_actions() {
        assert!(control("mask", "ok.service").is_err());
        assert!(control("start", "bad name").is_err());
    }
}
