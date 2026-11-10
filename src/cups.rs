//! Imprimantes avancées : files CUPS, travaux bloqués, pilotes et partage.

use crate::exec;
use serde::Serialize;

/// Une imprimante déclarée dans CUPS.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Printer {
    pub name: String,
    pub state: String,
    pub enabled: bool,
    pub accepting: bool,
    pub make_model: String,
    pub location: String,
    pub device: String,
}

/// Un travail d'impression.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: String,
    pub printer: String,
    pub owner: String,
    pub size: String,
    pub submitted: String,
    pub held: bool,
}

/// Configuration de partage CUPS.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Sharing {
    pub service_installed: bool,
    pub service_enabled: bool,
    pub browsing: bool,
    pub port_631: bool,
    pub listen_all: bool,
}

impl Sharing {
    /// Le partage n'est réellement actif que si CUPS écoute et diffuse.
    pub fn exposed(&self) -> bool {
        self.service_enabled && self.port_631 && (self.browsing || self.listen_all)
    }
}

/// Nom d'imprimante acceptable sur la ligne de commande.
pub fn valid_printer(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 127
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Identifiant de travail `imprimante-numéro`.
pub fn valid_job_id(id: &str) -> bool {
    match id.rsplit_once('-') {
        Some((printer, number)) => {
            valid_printer(printer)
                && !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Analyse `lpstat -p -l`.
pub fn parse_printers(text: &str) -> Vec<Printer> {
    let mut printers: Vec<Printer> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("printer ") {
            let mut fields = rest.splitn(2, ' ');
            let name = fields.next().unwrap_or("").to_string();
            let remainder = fields.next().unwrap_or("");
            let state = if remainder.contains("is idle") {
                "idle"
            } else if remainder.contains("now printing") {
                "printing"
            } else if remainder.contains("disabled") {
                "disabled"
            } else {
                "unknown"
            };
            if valid_printer(&name) {
                printers.push(Printer {
                    name,
                    state: state.to_string(),
                    enabled: remainder.contains("enabled"),
                    accepting: !remainder.contains("not accepting"),
                    ..Printer::default()
                });
            }
            continue;
        }
        let Some(printer) = printers.last_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Make and Model:") {
            printer.make_model = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("Location:") {
            printer.location = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("Interface:") {
            printer.device = value.trim().to_string();
        }
    }
    printers
}

/// Analyse `lpstat -v` et complète l'URI des périphériques.
pub fn parse_devices(text: &str, printers: &mut [Printer]) {
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("device for ") else {
            continue;
        };
        let Some((name, device)) = rest.split_once(':') else {
            continue;
        };
        if let Some(printer) = printers.iter_mut().find(|item| item.name == name.trim()) {
            printer.device = device.trim().to_string();
        }
    }
}

/// Analyse `lpstat -o -W all -l` (travaux, y compris terminés en attente).
pub fn parse_jobs(text: &str) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("no entries") {
            continue;
        }
        let mut fields = trimmed.splitn(4, ' ');
        let Some(id) = fields.next() else {
            continue;
        };
        if !valid_job_id(id) {
            continue;
        }
        let printer = id.rsplit_once('-').map(|(name, _)| name.to_string()).unwrap_or_default();
        let owner = fields.next().unwrap_or("").to_string();
        let size = fields.next().unwrap_or("").to_string();
        let submitted = fields.next().unwrap_or("").trim().to_string();
        jobs.push(Job {
            id: id.to_string(),
            printer,
            owner,
            size,
            held: submitted.to_lowercase().contains("held")
                || trimmed.to_lowercase().contains("held for authentication"),
            submitted,
        });
    }
    jobs
}

/// Analyse `/etc/cups/cupsd.conf`.
pub fn parse_cupsd(text: &str) -> (bool, bool, bool) {
    let mut browsing = false;
    let mut port_631 = false;
    let mut listen_all = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("browsing ") {
            browsing = lower.ends_with("on");
        } else if lower.starts_with("port ") {
            port_631 = lower.split_whitespace().nth(1) == Some("631");
        } else if lower.starts_with("listen ") {
            if lower.contains("0.0.0.0:631") || lower.contains("[::]:631") {
                listen_all = true;
            }
        }
    }
    (browsing, port_631, listen_all)
}

fn run_lines(program: &str, args: &[&str]) -> Result<String, String> {
    let captured = exec::run_with_timeout(program, args, std::time::Duration::from_secs(20))
        .map_err(|error| error.0)?;
    if !captured.ok && captured.stdout.trim().is_empty() {
        return Err(captured.error_text());
    }
    Ok(captured.stdout)
}

/// Liste les imprimantes avec leurs URIs.
pub fn printers() -> Result<Vec<Printer>, String> {
    let text = run_lines("lpstat", &["-p", "-l"])?;
    let mut printers = parse_printers(&text);
    if let Ok(devices) = run_lines("lpstat", &["-v"]) {
        parse_devices(&devices, &mut printers);
    }
    Ok(printers)
}

/// Travaux en cours ou en attente.
pub fn jobs() -> Vec<Job> {
    run_lines("lpstat", &["-o", "-W", "all", "-l"])
        .map(|text| parse_jobs(&text))
        .unwrap_or_default()
}

/// État de partage CUPS.
pub fn sharing() -> Sharing {
    let config = std::fs::read_to_string("/etc/cups/cupsd.conf").unwrap_or_default();
    let (browsing, port_631, listen_all) = parse_cupsd(&config);
    let enabled = exec::run("systemctl", &["is-enabled", "cups"])
        .map(|captured| captured.text().trim() == "enabled")
        .unwrap_or(false);
    Sharing {
        service_installed: !config.is_empty(),
        service_enabled: enabled,
        browsing,
        port_631,
        listen_all,
    }
}

/// Annule un travail précis.
pub fn cancel_job(id: &str) -> Result<String, String> {
    if !valid_job_id(id) {
        return Err(format!("travail refusé : {id}"));
    }
    exec::run_privileged("cancel", &[id.to_string()])?;
    Ok(format!("travail annulé : {id}"))
}

/// Vide la file d'une imprimante.
pub fn cancel_all(printer: &str) -> Result<String, String> {
    if !valid_printer(printer) {
        return Err(format!("imprimante refusée : {printer}"));
    }
    exec::run_privileged("cancel", &["-a".to_string(), printer.to_string()])?;
    Ok(format!("file vidée : {printer}"))
}

/// Active ou désactive une imprimante.
pub fn set_enabled(printer: &str, enabled: bool) -> Result<String, String> {
    if !valid_printer(printer) {
        return Err(format!("imprimante refusée : {printer}"));
    }
    let program = if enabled { "cupsenable" } else { "cupsdisable" };
    exec::run_privileged(program, &[printer.to_string()])?;
    Ok(format!(
        "{} : {printer}",
        if enabled { "activée" } else { "désactivée" }
    ))
}

/// Supprime définitivement une imprimante.
pub fn remove(printer: &str) -> Result<String, String> {
    if !valid_printer(printer) {
        return Err(format!("imprimante refusée : {printer}"));
    }
    exec::run_privileged(
        "lpadmin",
        &["-x".to_string(), printer.to_string()],
    )?;
    Ok(format!("imprimante supprimée : {printer}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_printer_listing() {
        let text = "printer HP_LaserJet is idle.  enabled since jeu. 30 oct. 2026 10:00:00\n\tLocation: bureau\n\tMake and Model: HP LaserJet 1020, hpcups 3.22.10\n\tInterface: /etc/cups/ppd/HP_LaserJet.ppd\nprinter PDF disabled since jeu. 30 oct. 2026 09:00:00 -\n\tMake and Model: Generic PDF printer\n";
        let printers = parse_printers(text);
        assert_eq!(printers.len(), 2);
        assert_eq!(printers[0].name, "HP_LaserJet");
        assert_eq!(printers[0].state, "idle");
        assert!(printers[0].enabled);
        assert_eq!(printers[0].location, "bureau");
        assert!(printers[0].make_model.contains("hpcups"));
        assert!(!printers[1].enabled);
        assert_eq!(printers[1].state, "disabled");
    }

    #[test]
    fn merges_device_uris() {
        let mut printers = vec![Printer {
            name: "HP".to_string(),
            ..Printer::default()
        }];
        parse_devices("device for HP: ipp://192.168.1.5/ipp/print\n", &mut printers);
        assert_eq!(printers[0].device, "ipp://192.168.1.5/ipp/print");
        parse_devices("device for autre: usb://x\n", &mut printers);
        assert_eq!(printers[0].device, "ipp://192.168.1.5/ipp/print");
    }

    #[test]
    fn parses_jobs() {
        let text = "HP-42 amine 1024 jeu. 30 oct. 2026 10:00:00\nHP-43 root 512 held for authentication\n";
        let jobs = parse_jobs(text);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "HP-42");
        assert_eq!(jobs[0].printer, "HP");
        assert_eq!(jobs[0].owner, "amine");
        assert!(jobs[1].held);
    }

    #[test]
    fn parses_cupsd_configuration() {
        let text = "Browsing On\nPort 631\nListen 0.0.0.0:631\n# Listen localhost:631\n";
        assert_eq!(parse_cupsd(text), (true, true, true));
        let local = "Browsing Off\nListen localhost:631\n";
        assert_eq!(parse_cupsd(local), (false, false, false));
    }

    #[test]
    fn flags_exposed_sharing() {
        let exposed = Sharing {
            service_installed: true,
            service_enabled: true,
            browsing: true,
            port_631: true,
            listen_all: false,
        };
        assert!(exposed.exposed());
        let closed = Sharing {
            service_enabled: false,
            ..exposed.clone()
        };
        assert!(!closed.exposed());
    }

    #[test]
    fn rejects_invalid_names_and_jobs() {
        assert!(valid_printer("HP_LaserJet"));
        assert!(valid_printer("Canon-MF.2"));
        assert!(!valid_printer("-o"));
        assert!(!valid_printer("a b"));
        assert!(valid_job_id("HP-42"));
        assert!(!valid_job_id("HP"));
        assert!(!valid_job_id("HP-42x"));
        assert!(cancel_job("HP-42; reboot").is_err());
        assert!(remove("-x").is_err());
        assert!(set_enabled("a b", true).is_err());
    }
}
