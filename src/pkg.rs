//! Backends de gestion de paquets : APT, DNF, Pacman, Zypper, APK.
//!
//! Chaque backend expose la même interface (inventaire, mises à jour, aperçu
//! de suppression) et délègue l'action réelle au gestionnaire natif : on ne
//! réimplémente ni le téléchargement ni la résolution de dépendances.

use crate::exec;
use crate::util;
use serde::Serialize;

/// Origine d'un logiciel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Source {
    Native,
    Snap,
    Flatpak,
    Manual,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::Native => "Paquet",
            Source::Snap => "Snap",
            Source::Flatpak => "Flatpak",
            Source::Manual => "Manuel",
        }
    }
}

/// Un logiciel installé, toutes sources confondues.
#[derive(Debug, Clone, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub size_bytes: u64,
    pub section: String,
    pub description: String,
    pub source: Source,
}

impl Package {
    pub fn new(name: &str, version: &str, source: Source) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            size_bytes: 0,
            section: String::new(),
            description: String::new(),
            source,
        }
    }
}

/// Une mise à jour disponible.
#[derive(Debug, Clone, Serialize)]
pub struct Update {
    pub name: String,
    pub current: String,
    pub available: String,
    pub source: Source,
}

/// Aperçu d'une suppression : ce qui partirait et ce qui serait installé.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RemovalPreview {
    pub requested: Vec<String>,
    pub remove: Vec<String>,
    pub install: Vec<String>,
    pub raw: String,
}

impl RemovalPreview {
    /// `true` si l'aperçu n'a rien pu être calculé (simulation indisponible).
    pub fn is_empty(&self) -> bool {
        self.remove.is_empty() && self.install.is_empty()
    }
}

/// Gestionnaire natif détecté sur la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PackageManager {
    Apt,
    Dnf,
    Pacman,
    Zypper,
    Apk,
}

impl PackageManager {
    /// Détecte le gestionnaire natif à partir des binaires disponibles.
    pub fn detect() -> Option<Self> {
        if exec::exists("apt-get") && exec::exists("dpkg-query") {
            Some(PackageManager::Apt)
        } else if exec::exists("dnf") {
            Some(PackageManager::Dnf)
        } else if exec::exists("pacman") {
            Some(PackageManager::Pacman)
        } else if exec::exists("zypper") {
            Some(PackageManager::Zypper)
        } else if exec::exists("apk") {
            Some(PackageManager::Apk)
        } else {
            None
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PackageManager::Apt => "APT / dpkg",
            PackageManager::Dnf => "DNF / RPM",
            PackageManager::Pacman => "Pacman",
            PackageManager::Zypper => "Zypper",
            PackageManager::Apk => "APK",
        }
    }

    /// Commande de mise à jour (affichée à l'utilisateur, pas exécutée par nous).
    pub fn update_command(&self) -> &'static str {
        match self {
            PackageManager::Apt => "apt-get upgrade",
            PackageManager::Dnf => "dnf upgrade",
            PackageManager::Pacman => "pacman -Syu",
            PackageManager::Zypper => "zypper update",
            PackageManager::Apk => "apk upgrade",
        }
    }

    /// Inventaire des paquets natifs installés.
    pub fn installed(&self) -> Result<Vec<Package>, String> {
        match self {
            PackageManager::Apt => {
                // `db:Status-Abbrev` est indispensable : `dpkg-query -W` liste
                // aussi les paquets désinstallés dont la configuration reste
                // (état « rc »), que l'application ne doit pas compter.
                let format = "${db:Status-Abbrev}\t${binary:Package}\t${Version}\t${Installed-Size}\t${Section}\t${binary:Summary}\n";
                let captured = exec::run_with_timeout(
                    "dpkg-query",
                    &["-W", "-f", format],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                if !captured.ok {
                    return Err(captured.error_text());
                }
                Ok(parse_dpkg_query(&captured.stdout))
            }
            PackageManager::Dnf | PackageManager::Zypper => {
                if !exec::exists("rpm") {
                    return self.installed_via_manager();
                }
                let query = "%{NAME}\t%{VERSION}-%{RELEASE}\t%{SIZE}\t%{GROUP}\t%{SUMMARY}\n";
                let captured = exec::run_with_timeout(
                    "rpm",
                    &["-qa", "--qf", query],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                if !captured.ok {
                    return self.installed_via_manager();
                }
                Ok(parse_tab_packages(&captured.stdout))
            }
            PackageManager::Pacman => {
                let captured = exec::run_with_timeout(
                    "pacman",
                    &["-Q"],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                if !captured.ok {
                    return Err(captured.error_text());
                }
                Ok(parse_pacman_q(&captured.stdout))
            }
            PackageManager::Apk => {
                let captured = exec::run_with_timeout(
                    "apk",
                    &["info", "-v"],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                if !captured.ok {
                    return Err(captured.error_text());
                }
                Ok(parse_apk_listing(&captured.stdout))
            }
        }
    }

    /// Repli quand `rpm` est absent (base interrogée par le gestionnaire).
    fn installed_via_manager(&self) -> Result<Vec<Package>, String> {
        let captured = match self {
            PackageManager::Dnf => exec::run("dnf", &["-q", "list", "--installed"]),
            PackageManager::Zypper => exec::run(
                "zypper",
                &["--non-interactive", "--no-refresh", "se", "-i", "-t", "package"],
            ),
            _ => return Err("gestionnaire inconnu".to_string()),
        }
        .map_err(|error| error.0)?;
        Ok(parse_zypper_packages(&captured.stdout))
    }

    /// Mises à jour disponibles.
    pub fn updates(&self) -> Result<Vec<Update>, String> {
        match self {
            PackageManager::Apt => {
                let captured = exec::run_with_timeout(
                    "apt",
                    &["list", "--upgradable"],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                // `apt list` écrit un avertissement sur stderr : le code de
                // sortie n'est pas significatif, on analyse la sortie standard.
                Ok(parse_apt_upgradable(&captured.stdout))
            }
            PackageManager::Dnf => {
                let captured = exec::run_with_timeout(
                    "dnf",
                    &["-q", "check-update"],
                    std::time::Duration::from_secs(180),
                )
                .map_err(|error| error.0)?;
                Ok(parse_dnf_check_update(&captured.stdout))
            }
            PackageManager::Pacman => {
                let captured = exec::run_with_timeout(
                    "pacman",
                    &["-Qu"],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                Ok(parse_pacman_updates(&captured.stdout))
            }
            PackageManager::Zypper => {
                let captured = exec::run_with_timeout(
                    "zypper",
                    &["--non-interactive", "--no-refresh", "list-updates"],
                    std::time::Duration::from_secs(180),
                )
                .map_err(|error| error.0)?;
                Ok(parse_zypper_updates(&captured.stdout))
            }
            PackageManager::Apk => {
                let captured = exec::run_with_timeout(
                    "apk",
                    &["version", "-l", "<"],
                    std::time::Duration::from_secs(120),
                )
                .map_err(|error| error.0)?;
                Ok(parse_apk_updates(&captured.stdout))
            }
        }
    }

    /// Simule une suppression pour savoir ce qui serait retiré.
    pub fn removal_preview(&self, names: &[String]) -> Result<RemovalPreview, String> {
        if names.is_empty() {
            return Ok(RemovalPreview::default());
        }
        for name in names {
            if !util::valid_package_name(name) {
                return Err(format!("nom de paquet refusé : {name}"));
            }
        }
        let mut args: Vec<String> = Vec::new();
        let program = match self {
            PackageManager::Apt => {
                args.extend(["remove".to_string(), "-s".to_string()]);
                "apt-get"
            }
            PackageManager::Dnf => {
                args.extend(["-q".to_string(), "remove".to_string(), "--assumeno".to_string()]);
                "dnf"
            }
            PackageManager::Pacman => {
                args.extend([
                    "-R".to_string(),
                    "--print".to_string(),
                    "--print-format".to_string(),
                    "%n".to_string(),
                ]);
                "pacman"
            }
            PackageManager::Zypper => {
                args.extend([
                    "--non-interactive".to_string(),
                    "remove".to_string(),
                    "--dry-run".to_string(),
                ]);
                "zypper"
            }
            PackageManager::Apk => {
                args.extend(["del".to_string(), "--simulate".to_string()]);
                "apk"
            }
        };
        args.extend(names.iter().cloned());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let captured = exec::run_with_timeout(program, &refs, std::time::Duration::from_secs(180))
            .map_err(|error| error.0)?;
        let raw = format!("{}\n{}", captured.stdout, captured.stderr);
        let mut preview = match self {
            PackageManager::Apt => parse_apt_simulation(&raw),
            PackageManager::Pacman => RemovalPreview {
                requested: names.to_vec(),
                remove: captured
                    .stdout
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty() && !line.starts_with('-'))
                    .collect(),
                install: Vec::new(),
                raw,
            },
            _ => RemovalPreview {
                requested: names.to_vec(),
                remove: parse_generic_removal(&raw),
                install: Vec::new(),
                raw,
            },
        };
        preview.requested = names.to_vec();
        // La simulation APT renvoie aussi les lignes « Remv » des dépendances :
        // on garde l'ordre stable et sans doublon.
        preview.remove.sort();
        preview.remove.dedup();
        Ok(preview)
    }

    /// Supprime réellement les paquets (via Polkit).
    pub fn remove(&self, names: &[String]) -> Result<String, String> {
        if names.is_empty() {
            return Err("aucun paquet sélectionné".to_string());
        }
        for name in names {
            if !util::valid_package_name(name) {
                return Err(format!("nom de paquet refusé : {name}"));
            }
        }
        let (program, args) = match self {
            PackageManager::Apt => {
                let mut args = vec!["remove".to_string(), "-y".to_string()];
                args.extend(names.iter().cloned());
                ("apt-get", args)
            }
            PackageManager::Dnf => {
                let mut args = vec!["-y".to_string(), "remove".to_string()];
                args.extend(names.iter().cloned());
                ("dnf", args)
            }
            PackageManager::Pacman => {
                let mut args = vec!["-R".to_string(), "--noconfirm".to_string()];
                args.extend(names.iter().cloned());
                ("pacman", args)
            }
            PackageManager::Zypper => {
                let mut args = vec!["--non-interactive".to_string(), "remove".to_string()];
                args.extend(names.iter().cloned());
                ("zypper", args)
            }
            PackageManager::Apk => {
                let mut args = vec!["del".to_string()];
                args.extend(names.iter().cloned());
                ("apk", args)
            }
        };
        exec::run_privileged(program, &args)
    }
}

/// Lignes `état<TAB>nom<TAB>version<TAB>taille<TAB>section<TAB>description`.
///
/// Seuls les paquets réellement installés (`ii`) sont retenus : un paquet en
/// état `rc` n'a plus de fichiers, seulement des restes de configuration.
pub fn parse_dpkg_query(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 2 {
                return None;
            }
            let status = fields[0].trim();
            if !status.starts_with("ii") {
                return None;
            }
            let name = fields.get(1)?.trim();
            if name.is_empty() {
                return None;
            }
            let size_bytes = fields
                .get(3)
                .and_then(|value| value.trim().parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
            Some(Package {
                name: name.to_string(),
                version: fields.get(2).map(|v| v.trim().to_string()).unwrap_or_default(),
                size_bytes,
                section: fields.get(4).map(|v| v.trim().to_string()).unwrap_or_default(),
                description: fields.get(5).map(|v| v.trim().to_string()).unwrap_or_default(),
                source: Source::Native,
            })
        })
        .collect()
}

/// Lignes `nom<TAB>version<TAB>taille<TAB>section<TAB>description` (RPM).
pub fn parse_tab_packages(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let name = fields.first()?.trim();
            if name.is_empty() {
                return None;
            }
            let size_bytes = fields
                .get(2)
                .and_then(|value| value.trim().parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
            Some(Package {
                name: name.to_string(),
                version: fields.get(1).map(|v| v.trim().to_string()).unwrap_or_default(),
                size_bytes,
                section: fields.get(3).map(|v| v.trim().to_string()).unwrap_or_default(),
                description: fields.get(4).map(|v| v.trim().to_string()).unwrap_or_default(),
                source: Source::Native,
            })
        })
        .collect()
}

/// `pacman -Q` : une ligne par paquet, `nom version`.
pub fn parse_pacman_q(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let version = fields.next().unwrap_or("");
            (!name.is_empty()).then(|| {
                let mut package = Package::new(name, version, Source::Native);
                package.section = "pacman".to_string();
                package
            })
        })
        .collect()
}

/// `apk info -v` : `nom-version-rN`.
pub fn parse_apk_listing(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(split_apk_entry)
        .map(|(name, version)| {
            let mut package = Package::new(&name, &version, Source::Native);
            package.section = "apk".to_string();
            package
        })
        .collect()
}

fn split_apk_entry(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.rsplitn(3, '-');
    let revision = parts.next()?;
    let version = parts.next()?;
    let name = parts.next()?;
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_string(), format!("{version}-{revision}")))
}

/// `zypper se -i -t package` : colonnes séparées par `|`.
pub fn parse_zypper_packages(text: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() < 4 {
            continue;
        }
        let name = fields[1];
        if name.is_empty() || name.eq_ignore_ascii_case("name") || name.starts_with('-') {
            continue;
        }
        if !name.chars().any(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let mut package = Package::new(name, fields[3], Source::Native);
        package.section = fields.get(5).copied().unwrap_or("").to_string();
        packages.push(package);
    }
    packages
}

/// `apt list --upgradable`.
pub fn parse_apt_upgradable(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            if line.starts_with("Listing") || !line.contains('/') {
                return None;
            }
            let (name, rest) = line.split_once('/')?;
            let fields: Vec<&str> = rest.split_whitespace().collect();
            // `nom/suite  version  arch  [upgradable from: ancienne]`
            let available = fields.get(1).copied().unwrap_or("").to_string();
            let current = line
                .split("upgradable from:")
                .nth(1)
                .map(|value| value.trim().trim_end_matches(']').trim().to_string())
                .unwrap_or_default();
            (!name.is_empty()).then(|| Update {
                name: name.to_string(),
                current,
                available,
                source: Source::Native,
            })
        })
        .collect()
}

/// `dnf check-update` : `nom.arch  version  dépôt`.
pub fn parse_dnf_check_update(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("Last metadata")
                || trimmed.starts_with("Obsoleting")
                || trimmed.starts_with("Security:")
                || trimmed.starts_with('=')
            {
                return None;
            }
            let fields: Vec<&str> = trimmed.split_whitespace().collect();
            if fields.len() < 2 {
                return None;
            }
            let (name, arch) = match fields[0].rsplit_once('.') {
                Some((name, arch)) => (name, arch),
                None => (fields[0], ""),
            };
            // Une ligne d'entête de tableau contient des mots, pas des versions.
            if arch.is_empty() && fields.len() < 3 {
                return None;
            }
            if !name.chars().any(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            Some(Update {
                name: name.to_string(),
                current: String::new(),
                available: fields[1].to_string(),
                source: Source::Native,
            })
        })
        .collect()
}

/// `pacman -Qu` : `nom 1.0-1 -> 1.1-1`.
pub fn parse_pacman_updates(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once("->")?;
            let mut before = left.split_whitespace();
            let name = before.next()?;
            let current = before.next().unwrap_or("").to_string();
            Some(Update {
                name: name.to_string(),
                current,
                available: right.trim().to_string(),
                source: Source::Native,
            })
        })
        .collect()
}

/// `zypper list-updates` : `S | dépôt | nom | actuel | disponible | arch`.
pub fn parse_zypper_updates(text: &str) -> Vec<Update> {
    let mut updates = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() < 5 {
            continue;
        }
        let name = fields[2];
        if name.is_empty() || name.starts_with('-') || name.eq_ignore_ascii_case("name") {
            continue;
        }
        updates.push(Update {
            name: name.to_string(),
            current: fields[3].to_string(),
            available: fields[4].to_string(),
            source: Source::Native,
        });
    }
    updates
}

/// `apk version -l '<'` : `nom-1.2-r0 < 1.3-r0`.
pub fn parse_apk_updates(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once('<')?;
            let (name, current) = split_apk_entry(left)?;
            Some(Update {
                name,
                current,
                available: right.trim().split_whitespace().next()?.to_string(),
                source: Source::Native,
            })
        })
        .collect()
}

/// Simulation APT : lignes `Remv nom [version]` et `Inst nom (version …)`.
pub fn parse_apt_simulation(text: &str) -> RemovalPreview {
    let mut preview = RemovalPreview::default();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Remv ") {
            if let Some(name) = rest.split_whitespace().next() {
                preview.remove.push(name.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("Inst ") {
            if let Some(name) = rest.split_whitespace().next() {
                preview.install.push(name.to_string());
            }
        }
    }
    preview.raw = text.to_string();
    preview
}

/// Sortie générique (DNF, Zypper, APK) : on retient les noms plausibles.
pub fn parse_generic_removal(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.ends_with(':') {
            continue;
        }
        let first = trimmed.split_whitespace().next().unwrap_or("");
        let name = first.split('.').next().unwrap_or(first);
        if (!name.is_empty())
            && name.len() < 120
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.' | ':'))
            && name.chars().any(|c| c.is_ascii_alphabetic())
        {
            names.push(name.trim_end_matches(':').to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Snap : `snap list`.
pub fn snap_installed() -> Result<Vec<Package>, String> {
    let captured = exec::run("snap", &["list"])
        .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(captured.error_text());
    }
    Ok(parse_snap_list(&captured.stdout))
}

pub fn parse_snap_list(text: &str) -> Vec<Package> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 2 {
                return None;
            }
            let mut package = Package::new(fields[0], fields[1], Source::Snap);
            package.section = fields.get(4).copied().unwrap_or("").to_string();
            Some(package)
        })
        .collect()
}

/// Flatpak : `flatpak list --app --columns=application,version,size`.
pub fn flatpak_installed() -> Result<Vec<Package>, String> {
    let captured = exec::run(
        "flatpak",
        &["list", "--app", "--columns=application,version,size,origin"],
    )
    .map_err(|error| error.0)?;
    if !captured.ok {
        return Err(captured.error_text());
    }
    Ok(parse_flatpak_list(&captured.stdout))
}

pub fn parse_flatpak_list(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let name = fields.first()?.trim();
            if name.is_empty() || !util::valid_app_id(name) {
                return None;
            }
            let mut package = Package::new(name, fields.get(1).copied().unwrap_or(""), Source::Flatpak);
            package.size_bytes = fields.get(2).map(|size| util::parse_human_size(size)).unwrap_or(0);
            package.section = fields.get(3).copied().unwrap_or("").to_string();
            Some(package)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Mises à jour Snap et Flatpak, application groupée
// ---------------------------------------------------------------------------

/// Analyse `snap refresh --list` (colonnes : Name Version Rev Size Publisher Notes).
pub fn parse_snap_refresh_list(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 || fields[0] == "Name" || !crate::util::valid_package_name(fields[0]) {
                return None;
            }
            // Une ligne de données a une révision numérique en troisième colonne.
            fields[2].chars().all(|ch| ch.is_ascii_digit()).then(|| Update {
                name: fields[0].to_string(),
                current: String::new(),
                available: fields[1].to_string(),
                source: Source::Snap,
            })
        })
        .collect()
}

/// Analyse `flatpak remote-ls --updates --columns=application,version`.
pub fn parse_flatpak_updates(text: &str) -> Vec<Update> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?.trim();
            if id.is_empty() || id == "Application ID" || !crate::util::valid_app_id(id) || !id.contains('.') {
                return None;
            }
            Some(Update {
                name: id.to_string(),
                current: String::new(),
                available: fields.next().unwrap_or("").trim().to_string(),
                source: Source::Flatpak,
            })
        })
        .collect()
}

/// Mises à jour Snap et Flatpak disponibles (outils absents : liste vide).
pub fn extra_updates() -> Vec<Update> {
    let mut found = Vec::new();
    if exec::exists("snap") {
        if let Ok(captured) = exec::run_with_timeout("snap", &["refresh", "--list"], std::time::Duration::from_secs(60)) {
            found.extend(parse_snap_refresh_list(&captured.stdout));
        }
    }
    if exec::exists("flatpak") {
        if let Ok(captured) = exec::run_with_timeout(
            "flatpak",
            &["remote-ls", "--updates", "--columns=application,version"],
            std::time::Duration::from_secs(90),
        ) {
            found.extend(parse_flatpak_updates(&captured.stdout));
        }
    }
    found
}

/// Toutes les mises à jour : gestionnaire natif, Snap et Flatpak.
pub fn all_updates() -> Result<Vec<Update>, String> {
    let extra = extra_updates();
    match PackageManager::detect() {
        Some(manager) => match manager.updates() {
            Ok(mut native) => {
                native.extend(extra);
                Ok(native)
            }
            Err(error) if extra.is_empty() => Err(error),
            Err(_) => Ok(extra),
        },
        None => Ok(extra),
    }
}

impl PackageManager {
    /// Commande qui applique toutes les mises à jour natives (sans question).
    pub fn upgrade_command(&self) -> (&'static str, &'static [&'static str]) {
        match self {
            PackageManager::Apt => ("apt-get", &["-y", "upgrade"]),
            PackageManager::Dnf => ("dnf", &["-y", "upgrade"]),
            PackageManager::Pacman => ("pacman", &["-Syu", "--noconfirm"]),
            PackageManager::Zypper => ("zypper", &["-n", "update"]),
            PackageManager::Apk => ("apk", &["upgrade"]),
        }
    }
}

/// Applique les mises à jour listées : natives et Snap via Polkit, Flatpak par
/// l'outil lui-même. Renvoie un résumé, une ligne par source.
pub fn apply_updates(manager: Option<PackageManager>, updates: &[Update]) -> Result<String, String> {
    let mut lines = Vec::new();
    let mut failures = 0usize;
    let has = |source: Source| updates.iter().any(|update| update.source == source);
    if let (Some(manager), true) = (manager, has(Source::Native)) {
        let (program, args) = manager.upgrade_command();
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        match exec::run_privileged(program, &args) {
            Ok(_) => lines.push(format!("{} : à jour", manager.label())),
            Err(error) => {
                failures += 1;
                lines.push(format!("{} : échec — {error}", manager.label()));
            }
        }
    }
    if has(Source::Snap) {
        match exec::run_privileged("snap", &["refresh".to_string()]) {
            Ok(_) => lines.push("Snap : à jour".to_string()),
            Err(error) => {
                failures += 1;
                lines.push(format!("Snap : échec — {error}"));
            }
        }
    }
    if has(Source::Flatpak) {
        match exec::run_with_timeout("flatpak", &["update", "-y", "--noninteractive"], std::time::Duration::from_secs(1800)) {
            Ok(captured) if captured.ok => lines.push("Flatpak : à jour".to_string()),
            Ok(captured) => {
                failures += 1;
                lines.push(format!("Flatpak : échec — {}", captured.error_text()));
            }
            Err(error) => {
                failures += 1;
                lines.push(format!("Flatpak : échec — {}", error.0));
            }
        }
    }
    if lines.is_empty() {
        return Err("aucune mise à jour à appliquer".to_string());
    }
    if failures == lines.len() {
        Err(lines.join("\n"))
    } else {
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dpkg_query_output_and_skips_removed_packages() {
        let text = "ii \tbash\t5.2.21-2ubuntu4\t1836\tshells\tGNU Bourne Again SHell\nrc \told-package\t1.0\t120\tmisc\tAncien paquet\nii \tfirefox\t1:128.0\t245760\tweb\tMozilla Firefox\n";
        let packages = parse_dpkg_query(text);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "bash");
        assert_eq!(packages[0].size_bytes, 1836 * 1024);
        assert_eq!(packages[1].section, "web");
        assert!(packages.iter().all(|p| p.source == Source::Native));
    }

    #[test]
    fn tolerates_empty_dpkg_fields() {
        let packages = parse_tab_packages("\n\npaquet\t1.0\t\t\t\n");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].size_bytes, 0);
        assert!(packages[0].section.is_empty());
    }

    #[test]
    fn parses_apt_upgradable() {
        let text = "Listing...\nfirefox/jammy-updates 1:129.0+build1 amd64 [upgradable from: 1:128.0]\nlibc6/jammy-updates 2.35-0ubuntu3.8 amd64 [upgradable from: 2.35-0ubuntu3.7]\n";
        let updates = parse_apt_upgradable(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "firefox");
        assert_eq!(updates[0].current, "1:128.0");
        assert_eq!(updates[0].available, "1:129.0+build1");
    }

    #[test]
    fn parses_apt_simulation() {
        let text = "Reading package lists...\nThe following packages will be REMOVED:\n  firefox firefox-locale-fr\n0 upgraded, 0 newly installed, 2 to remove and 0 not upgraded.\nRemv firefox [1:128.0]\nRemv firefox-locale-fr [1:128.0]\nInst libdbus-1-3 (1.14.10-1ubuntu3 amd64) []\n";
        let preview = parse_apt_simulation(text);
        assert_eq!(preview.remove, vec!["firefox", "firefox-locale-fr"]);
        assert_eq!(preview.install, vec!["libdbus-1-3"]);
    }

    #[test]
    fn parses_pacman_updates() {
        let text = "bash 5.2.026-1 -> 5.2.026-2\nfirefox 128.0-1 -> 129.0-1\n";
        let updates = parse_pacman_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[1].name, "firefox");
        assert_eq!(updates[1].current, "128.0-1");
        assert_eq!(updates[1].available, "129.0-1");
    }

    #[test]
    fn parses_pacman_installed() {
        let packages = parse_pacman_q("bash 5.2.026-1\nzlib 1:1.3.1-1\n");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[1].name, "zlib");
    }

    #[test]
    fn parses_alpine_listing() {
        let packages = parse_apk_listing("musl-1.2.4-r2\nbusybox-1.36.1-r5\n");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "musl");
        assert_eq!(packages[0].version, "1.2.4-r2");
    }

    #[test]
    fn parses_dnf_check_update() {
        let text = "Last metadata expiration check: 0:10:00 ago.\nkernel.x86_64   6.8.0-45.fc40   updates\nfirefox.x86_64  128.0-1.fc40  updates\nObsoleting Packages\n";
        let updates = parse_dnf_check_update(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "kernel");
        assert_eq!(updates[1].available, "128.0-1.fc40");
    }

    #[test]
    fn parses_zypper_updates_table() {
        let text = "S | Repository | Name | Current Version | Available Version | Arch\n--+------------+------+-----------------+-------------------+------\nv | Update | bash | 5.2.21-1.1 | 5.2.21-2.1 | x86_64\n";
        let updates = parse_zypper_updates(text);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "bash");
        assert_eq!(updates[0].available, "5.2.21-2.1");
    }

    #[test]
    fn parses_flatpak_list() {
        let text = "org.gnome.Builder\t44.2\t245,4 MB\tflathub\n";
        let packages = parse_flatpak_list(text);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "org.gnome.Builder");
        assert_eq!(packages[0].source, Source::Flatpak);
        assert!(packages[0].size_bytes > 200_000_000);
    }

    #[test]
    fn pacman_preview_rejects_bad_names() {
        let manager = PackageManager::Pacman;
        let error = manager.removal_preview(&["--force".to_string()]).unwrap_err();
        assert!(error.contains("refusé"));
    }

    #[test]
    fn parses_snap_refresh_list() {
        let text = "Name     Version  Rev   Size   Publisher   Notes\nfirefox  132.0    5002  250MB  mozilla✓    -\ncore22   20241001 1663  70MB   canonical✓  base\n";
        let updates = parse_snap_refresh_list(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "firefox");
        assert_eq!(updates[0].available, "132.0");
        assert_eq!(updates[0].source, Source::Snap);
        assert!(parse_snap_refresh_list("All snaps up to date.\n").is_empty());
        assert!(parse_snap_refresh_list("").is_empty());
    }

    #[test]
    fn parses_flatpak_updates() {
        let text = "Application ID\tVersion\norg.gnome.Calculator\t47.0\ncom.spotify.Client\t1.2.3\n\nrien\n";
        let updates = parse_flatpak_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[1].name, "com.spotify.Client");
        assert_eq!(updates[0].source, Source::Flatpak);
        assert!(parse_flatpak_updates("").is_empty());
    }

    #[test]
    fn upgrade_commands_are_non_interactive() {
        for manager in [PackageManager::Apt, PackageManager::Dnf, PackageManager::Zypper] {
            let (program, args) = manager.upgrade_command();
            assert!(!program.is_empty());
            assert!(args.iter().any(|arg| *arg == "-y" || *arg == "-n"), "{program} doit être non interactif");
        }
        assert_eq!(PackageManager::Pacman.upgrade_command().1, &["-Syu", "--noconfirm"]);
        assert!(apply_updates(None, &[]).is_err());
    }
}
