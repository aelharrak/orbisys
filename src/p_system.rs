//! Système : démarrage, journal des actions, maintenance, dépannage,
//! profils de poste et comptes utilisateurs.

use crate::disk;
use crate::exec;
use crate::journal;
use crate::p_monitor;
use crate::panel::*;
use crate::util;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Démarrage
// ---------------------------------------------------------------------------

/// Une entrée de démarrage automatique.
#[derive(Debug, Clone, PartialEq)]
pub struct Autostart {
    pub name: String,
    pub exec: String,
    pub enabled: bool,
}

/// Analyse un fichier `.desktop` de démarrage.
pub fn parse_autostart(text: &str, fallback: &str) -> Autostart {
    let mut name = String::new();
    let mut exec = String::new();
    let mut enabled = true;
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            if name.is_empty() {
                name = value.to_string();
            }
        } else if let Some(value) = line.strip_prefix("Exec=") {
            exec = value.to_string();
        } else if let Some(value) = line.strip_prefix("Hidden=") {
            if value.eq_ignore_ascii_case("true") {
                enabled = false;
            }
        } else if let Some(value) = line.strip_prefix("X-GNOME-Autostart-enabled=") {
            if value.eq_ignore_ascii_case("false") {
                enabled = false;
            }
        }
    }
    Autostart {
        name: if name.is_empty() { fallback.to_string() } else { name },
        exec,
        enabled,
    }
}

/// Réécrit un fichier `.desktop` avec la clé d'activation voulue.
pub fn set_autostart_text(text: &str, enabled: bool) -> String {
    let flag = format!("X-GNOME-Autostart-enabled={}", if enabled { "true" } else { "false" });
    let mut out = Vec::new();
    let mut replaced = false;
    for line in text.lines() {
        if line.starts_with("X-GNOME-Autostart-enabled=") {
            if !replaced {
                out.push(flag.clone());
                replaced = true;
            }
        } else if line.starts_with("Hidden=") {
            continue;
        } else {
            out.push(line.to_string());
        }
    }
    if !replaced {
        out.push(flag);
    }
    out.join("\n") + "\n"
}

pub fn autostart_dir() -> PathBuf {
    home().join(".config/autostart")
}

/// Le chemin désigne-t-il un `.desktop` du dossier d'autostart utilisateur ?
pub fn autostart_path_ok(path: &str) -> bool {
    let dir = autostart_dir();
    safe_abs_path(path) && path.ends_with(".desktop") && util::path_within(&dir, std::path::Path::new(path))
}

pub fn toggle_autostart(args: &[String]) -> Result<String, String> {
    let [path, state] = args else {
        return Err("chemin et état attendus".to_string());
    };
    if !autostart_path_ok(path) {
        return Err(format!("chemin refusé : {path}"));
    }
    let enabled = state == "on";
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    std::fs::write(path, set_autostart_text(&text, enabled)).map_err(|error| error.to_string())?;
    Ok(if enabled { "Activé au démarrage" } else { "Désactivé au démarrage" }.to_string())
}

fn save_boot_plot(args: &[String]) -> Result<String, String> {
    let [target] = args else {
        return Err("fichier attendu".to_string());
    };
    let svg = run_text("systemd-analyze", &["plot"])?;
    std::fs::write(target, svg).map_err(|error| error.to_string())?;
    Ok(format!("Graphe enregistré : {target}"))
}

pub fn startup() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    let user_dir = autostart_dir();
    for (dir, editable) in [(user_dir.clone(), true), (PathBuf::from("/etc/xdg/autostart"), false)] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
            .collect();
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let entry = parse_autostart(&text, &stem);
            let mut row = Row::owned(vec![
                entry.name.clone(),
                if editable { "utilisateur" } else { "système (lecture seule)" }.to_string(),
                if entry.enabled { "activé" } else { "désactivé" }.to_string(),
                util::ellipsis(&entry.exec, 50),
            ]);
            row.bad = !entry.enabled;
            if editable {
                let path_text = path.to_string_lossy().to_string();
                row = row.action(Cmd::local(
                    if entry.enabled { "Désactiver" } else { "Activer" },
                    toggle_autostart,
                    vec![path_text, if entry.enabled { "off" } else { "on" }.to_string()],
                ));
            }
            rows.push(row);
        }
    }
    panel.metric("Applications au démarrage", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Applications lancées à l'ouverture de session")
            .columns(&["Nom", "Origine", "État", "Commande"])
            .rows(rows),
    );

    if exec::exists("systemd-analyze") {
        let time = run_lossy("systemd-analyze", &["time"]);
        if let Some(line) = time.lines().next() {
            panel.metric("Démarrage", util::ellipsis(line.trim(), 90), Level::Info);
        }
        let chain = run_lossy("systemd-analyze", &["critical-chain", "--no-pager"]);
        let plot_target = home().join("orbisys-demarrage.svg").to_string_lossy().to_string();
        panel.section(
            Section::new("Chaîne critique")
                .text(chain.trim().to_string())
                .action(Cmd::local("Enregistrer le graphe (SVG)", save_boot_plot, vec![plot_target])),
        );
        let blame = parse_blame(&run_lossy("systemd-analyze", &["blame", "--no-pager"]));
        let rows = blame
            .into_iter()
            .take(20)
            .map(|(time, unit)| Row::owned(vec![unit, time]))
            .collect();
        panel.section(
            Section::new("Unités les plus lentes")
                .columns(&["Unité", "Durée"])
                .rows(rows),
        );
    }
    panel
}

/// Analyse `systemd-analyze blame` : `(durée, unité)`.
pub fn parse_blame(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (time, unit) = line.rsplit_once(char::is_whitespace)?;
            if unit.contains('.') && !time.is_empty() {
                Some((time.trim().to_string(), unit.to_string()))
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Journal des actions
// ---------------------------------------------------------------------------

fn export_journal(_: &[String]) -> Result<String, String> {
    let path = home().join("orbisys-journal.csv");
    std::fs::write(&path, journal::to_csv(&journal::recent(100_000))).map_err(|error| error.to_string())?;
    Ok(format!("Exporté : {}", path.display()))
}

fn purge_journal(_: &[String]) -> Result<String, String> {
    match std::fs::remove_file(journal::path()) {
        Ok(()) => Ok("Journal purgé".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("Journal déjà vide".to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn action_log() -> Panel {
    let mut panel = Panel::default();
    let entries = journal::recent(300);
    let failed = entries.iter().filter(|entry| !entry.ok).count();
    panel.metric("Entrées", entries.len().to_string(), Level::Info);
    panel.metric("Échecs", failed.to_string(), if failed > 0 { Level::Warn } else { Level::Ok });
    let rows = entries
        .iter()
        .map(|entry| {
            Row::owned(vec![
                entry.timestamp.clone(),
                entry.action.clone(),
                entry.target.clone(),
                if entry.ok { "réussi" } else { "échec" }.to_string(),
                util::ellipsis(&entry.detail, 70),
            ])
            .bad(!entry.ok)
        })
        .collect();
    panel.section(
        Section::new("Historique des actions")
            .columns(&["Date", "Action", "Cible", "Résultat", "Détail"])
            .rows(rows)
            .action(Cmd::local("Exporter en CSV", export_journal, Vec::new()))
            .action(
                Cmd::local("Purger le journal", purge_journal, Vec::new())
                    .confirm("Effacer tout l'historique des actions ?"),
            ),
    );
    panel
}

// ---------------------------------------------------------------------------
// Maintenance
// ---------------------------------------------------------------------------

pub fn maintenance() -> Panel {
    let mut panel = Panel::default();
    let tools = ["apt-get", "snap", "flatpak", "pkexec", "journalctl", "du", "systemctl"];
    let rows = tools
        .iter()
        .map(|tool| {
            let present = exec::exists(tool);
            Row::owned(vec![tool.to_string(), if present { "disponible" } else { "absent" }.to_string()])
                .bad(!present)
        })
        .collect();
    panel.section(Section::new("Outils").columns(&["Outil", "État"]).rows(rows));
    panel.section(
        Section::new("Tâches privilégiées")
            .note("Chaque tâche demande une confirmation puis l'authentification administrateur (Polkit).")
            .action(
                Cmd::new("Supprimer les paquets orphelins", "apt-get", &["-y", "autoremove"])
                    .privileged()
                    .confirm("Exécuter apt-get autoremove ?"),
            )
            .action(
                Cmd::new("Vider le cache APT", "apt-get", &["clean"])
                    .privileged()
                    .confirm("Exécuter apt-get clean ?"),
            )
            .action(
                Cmd::new("Réduire les journaux (7 jours)", "journalctl", &["--vacuum-time=7d"])
                    .privileged()
                    .confirm("Supprimer les journaux systemd de plus de 7 jours ?"),
            )
            .action(
                Cmd::new("Réduire les journaux (500 Mo)", "journalctl", &["--vacuum-size=500M"])
                    .privileged()
                    .confirm("Limiter les journaux systemd à 500 Mo ?"),
            ),
    );
    panel
}

// ---------------------------------------------------------------------------
// Dépannage
// ---------------------------------------------------------------------------

/// Un diagnostic.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub label: String,
    pub ok: bool,
    pub detail: String,
    pub advice: String,
}

pub fn diagnostics() -> Vec<Diagnostic> {
    let mut list = Vec::new();
    if exec::exists("dpkg") {
        let audit = run_lossy("dpkg", &["--audit"]);
        list.push(Diagnostic {
            label: "Base dpkg".into(),
            ok: audit.trim().is_empty(),
            detail: if audit.trim().is_empty() { "cohérente".into() } else { util::ellipsis(audit.trim(), 120) },
            advice: "Exécutez « dpkg --configure -a » puis « apt-get -f install ».".into(),
        });
    }
    let used = p_monitor::disk_percent();
    list.push(Diagnostic {
        label: "Espace disque".into(),
        ok: used < 90.0,
        detail: format!("{used:.0} % au maximum"),
        advice: "Libérez de l'espace via Nettoyage ou Espace disque.".into(),
    });
    let memory = disk::memory().percent();
    list.push(Diagnostic {
        label: "Mémoire".into(),
        ok: memory < 92.0,
        detail: format!("{memory:.0} % utilisée"),
        advice: "Fermez des applications ou ajoutez du swap/zram.".into(),
    });
    let failed = p_monitor::count_failed_units(&run_lossy("systemctl", &["--failed", "--no-legend", "--plain"]));
    list.push(Diagnostic {
        label: "Services en échec".into(),
        ok: failed == 0,
        detail: format!("{failed} unité(s)"),
        advice: "Consultez « systemctl --failed » et les journaux de l'unité.".into(),
    });
    let sync = run_lossy("timedatectl", &["show", "-p", "NTPSynchronized", "--value"]);
    let synced = sync.trim() == "yes";
    list.push(Diagnostic {
        label: "Horloge".into(),
        ok: synced,
        detail: if synced { "synchronisée".into() } else { "non synchronisée".into() },
        advice: "Activez la synchronisation : « timedatectl set-ntp true ».".into(),
    });
    let dns = run_text("getent", &["hosts", "ubuntu.com"]);
    list.push(Diagnostic {
        label: "Résolution DNS".into(),
        ok: dns.as_ref().is_ok_and(|text| !text.trim().is_empty()),
        detail: match &dns {
            Ok(text) if !text.trim().is_empty() => "ubuntu.com résolu".into(),
            _ => "échec de résolution".into(),
        },
        advice: "Vérifiez le réseau et les serveurs DNS (page Réseau).".into(),
    });
    list
}

pub fn troubleshoot() -> Panel {
    let mut panel = Panel::default();
    let list = diagnostics();
    let problems = list.iter().filter(|d| !d.ok).count();
    panel.metric(
        "Problèmes",
        problems.to_string(),
        if problems == 0 { Level::Ok } else { Level::Bad },
    );
    let rows = list
        .iter()
        .map(|d| {
            Row::owned(vec![
                d.label.clone(),
                if d.ok { "OK" } else { "problème" }.to_string(),
                d.detail.clone(),
                if d.ok { String::new() } else { d.advice.clone() },
            ])
            .bad(!d.ok)
        })
        .collect();
    panel.section(
        Section::new("Diagnostic automatique")
            .columns(&["Contrôle", "État", "Détail", "Conseil"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Profils de poste
// ---------------------------------------------------------------------------

pub struct Profile {
    pub name: &'static str,
    pub description: &'static str,
    pub packages: &'static [&'static str],
    pub power: &'static str,
}

pub const PROFILES: [Profile; 4] = [
    Profile {
        name: "Développement",
        description: "Outils de compilation, gestion de versions et conteneurs.",
        packages: &["build-essential", "git", "curl", "docker.io"],
        power: "performance",
    },
    Profile {
        name: "Gaming",
        description: "Pilotes graphiques et outils de performance (vulkan, gamemode).",
        packages: &["mesa-vulkan-drivers", "vulkan-tools", "gamemode", "mangohud"],
        power: "performance",
    },
    Profile {
        name: "Serveur",
        description: "Services réseau et outils d'administration système.",
        packages: &["openssh-server", "ufw", "htop", "tmux"],
        power: "balanced",
    },
    Profile {
        name: "Bureau minimal",
        description: "Outillage bureautique léger et nettoyage des paquets.",
        packages: &["gnome-tweaks", "bleachbit"],
        power: "balanced",
    },
];

/// Commande d'installation d'un profil ; refuse tout nom de paquet invalide.
pub fn profile_install(profile: &Profile) -> Result<Cmd, String> {
    if let Some(bad) = profile.packages.iter().find(|name| !util::valid_package_name(name)) {
        return Err(format!("nom de paquet refusé : {bad}"));
    }
    let mut args = vec!["install".to_string(), "-y".to_string()];
    args.extend(profile.packages.iter().map(|name| name.to_string()));
    Ok(Cmd::owned(&format!("Installer {}", profile.name), "apt-get", args)
        .privileged()
        .confirm(&format!(
            "Installer {} ?\nProfil « {} »",
            profile.packages.join(", "),
            profile.name
        )))
}

pub fn profiles() -> Panel {
    let mut panel = Panel::default();
    let apt = exec::exists("apt-get");
    if !apt {
        panel.error("L'installation des profils passe par apt-get : indisponible sur ce système (le profil d'énergie reste applicable).");
    }
    let installed = run_lossy("dpkg-query", &["-W", "-f", "${Package} ${db:Status-Abbrev}\n"]);
    let installed: std::collections::HashSet<&str> = installed
        .lines()
        .filter_map(|line| {
            let (name, status) = line.split_once(' ')?;
            status.starts_with("ii").then_some(name)
        })
        .collect();
    let rows = PROFILES
        .iter()
        .map(|profile| {
            let present = profile.packages.iter().filter(|name| installed.contains(**name)).count();
            let mut row = Row::owned(vec![
                profile.name.to_string(),
                profile.description.to_string(),
                format!("{present} / {}", profile.packages.len()),
                profile.power.to_string(),
            ]);
            if apt && present < profile.packages.len() {
                if let Ok(cmd) = profile_install(profile) {
                    row = row.action(cmd);
                }
            }
            if exec::exists("powerprofilesctl") {
                row = row.action(Cmd::new("Profil d'énergie", "powerprofilesctl", &["set", profile.power]));
            }
            row
        })
        .collect();
    panel.section(
        Section::new("Profils de poste")
            .columns(&["Profil", "Contenu", "Installés", "Énergie"])
            .rows(rows)
            .note("L'installation passe par apt-get et Polkit, après confirmation."),
    );
    panel
}

// ---------------------------------------------------------------------------
// Utilisateurs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub name: String,
    pub uid: u32,
    pub home: String,
    pub shell: String,
}

/// Comptes locaux : root et UID ≥ 1000 (hors `nobody`).
pub fn parse_passwd(text: &str) -> Vec<User> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 7 {
                return None;
            }
            let uid: u32 = fields[2].parse().ok()?;
            if (uid != 0 && !(1000..60000).contains(&uid)) || fields[0] == "nobody" {
                return None;
            }
            Some(User {
                name: fields[0].to_string(),
                uid,
                home: fields[5].to_string(),
                shell: fields[6].to_string(),
            })
        })
        .collect()
}

/// Membres d'un groupe d'après `/etc/group`.
pub fn group_members(text: &str, group: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            (fields.len() >= 4 && fields[0] == group).then(|| {
                fields[3]
                    .split(',')
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        })
        .next()
        .unwrap_or_default()
}

/// Statut `passwd -S` : `Some(true)` si verrouillé.
pub fn parse_passwd_status(text: &str) -> Option<bool> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    match fields.get(1) {
        Some(&"L") => Some(true),
        Some(&"P") | Some(&"NP") => Some(false),
        _ => None,
    }
}

/// Actions possibles sur un compte (jamais sur `root` ni sur un nom invalide).
pub fn user_actions(user: &User, admin: bool, locked: Option<bool>) -> Vec<Cmd> {
    if user.name == "root" || user.uid == 0 || !valid_username(&user.name) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if admin {
        actions.push(
            Cmd::new("Retirer de sudo", "gpasswd", &["-d", &user.name, "sudo"])
                .privileged()
                .confirm(&format!("Retirer {} du groupe sudo ?", user.name)),
        );
    } else {
        actions.push(
            Cmd::new("Ajouter à sudo", "usermod", &["-aG", "sudo", &user.name])
                .privileged()
                .confirm(&format!("Donner les droits d'administration à {} ?", user.name)),
        );
    }
    if locked != Some(true) {
        actions.push(
            Cmd::new("Verrouiller", "usermod", &["-L", &user.name])
                .privileged()
                .confirm(&format!("Verrouiller le compte {} ?", user.name)),
        );
    }
    if locked != Some(false) {
        actions.push(Cmd::new("Déverrouiller", "usermod", &["-U", &user.name]).privileged());
    }
    actions
}

pub fn users() -> Panel {
    let mut panel = Panel::default();
    let users = parse_passwd(&read("/etc/passwd"));
    let sudoers = group_members(&read("/etc/group"), "sudo");
    let rows = users
        .iter()
        .map(|user| {
            let admin = user.uid == 0 || sudoers.contains(&user.name);
            let locked = run_text("passwd", &["-S", &user.name]).ok().and_then(|t| parse_passwd_status(&t));
            let mut row = Row::owned(vec![
                user.name.clone(),
                user.uid.to_string(),
                if admin { "administrateur" } else { "standard" }.to_string(),
                match locked {
                    Some(true) => "verrouillé",
                    Some(false) => "actif",
                    None => "inconnu",
                }
                .to_string(),
                user.shell.clone(),
            ]);
            row.bad = locked == Some(true);
            row.actions = user_actions(user, admin, locked);
            row
        })
        .collect::<Vec<_>>();
    panel.metric("Comptes", rows.len().to_string(), Level::Info);
    panel.metric("Administrateurs", sudoers.len().to_string(), Level::Info);
    panel.section(
        Section::new("Comptes locaux")
            .columns(&["Compte", "UID", "Rôle", "État", "Shell"])
            .rows(rows)
            .note("Le compte root n'est pas modifiable depuis cette page."),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autostart_entries() {
        let text = "[Desktop Entry]\nName=Dropbox\nExec=dropbox start\nX-GNOME-Autostart-enabled=false\n";
        let entry = parse_autostart(text, "dropbox");
        assert_eq!(entry.name, "Dropbox");
        assert!(!entry.enabled);
        let hidden = parse_autostart("[Desktop Entry]\nExec=x\nHidden=true\n", "repli");
        assert_eq!(hidden.name, "repli");
        assert!(!hidden.enabled);
        assert!(parse_autostart("[Desktop Entry]\nExec=x\n", "a").enabled);
    }

    #[test]
    fn rewrites_autostart_flag() {
        let text = "[Desktop Entry]\nName=A\nHidden=true\nX-GNOME-Autostart-enabled=false\n";
        let on = set_autostart_text(text, true);
        assert!(on.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!on.contains("Hidden"));
        assert_eq!(on.matches("X-GNOME-Autostart-enabled").count(), 1);
        let added = set_autostart_text("[Desktop Entry]\nName=A\n", false);
        assert!(added.ends_with("X-GNOME-Autostart-enabled=false\n"));
    }

    #[test]
    fn autostart_toggle_refuses_foreign_paths() {
        assert!(toggle_autostart(&["/etc/passwd".into(), "on".into()]).is_err());
        assert!(toggle_autostart(&["/etc/xdg/autostart/x.desktop".into(), "on".into()]).is_err());
        let escape = format!("{}/../../etc/x.desktop", autostart_dir().display());
        assert!(toggle_autostart(&[escape, "on".into()]).is_err());
        assert!(toggle_autostart(&["seul".into()]).is_err());
    }

    #[test]
    fn parses_blame() {
        let text = "5.123s NetworkManager-wait-online.service\n1.2s snapd.service\nfoo\n";
        let parsed = parse_blame(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].1, "NetworkManager-wait-online.service");
    }

    #[test]
    fn profiles_use_valid_package_names() {
        for profile in &PROFILES {
            let cmd = profile_install(profile).unwrap();
            assert_eq!(cmd.program, "apt-get");
            assert!(cmd.privileged && cmd.confirm.is_some());
            assert_eq!(cmd.args.len(), 2 + profile.packages.len());
        }
        let bad = Profile { name: "x", description: "", packages: &["a b"], power: "balanced" };
        assert!(profile_install(&bad).is_err());
    }

    #[test]
    fn parses_users_and_groups() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\ndaemon:x:1:1::/usr/sbin:/usr/sbin/nologin\namine:x:1000:1000:A:/home/amine:/bin/bash\nnobody:x:65534:65534::/nonexistent:/usr/sbin/nologin\nsnap_daemon:x:584788:584788::/nonexistent:/usr/bin/false\n";
        let users = parse_passwd(passwd);
        assert_eq!(users.len(), 2);
        assert_eq!(users[1].name, "amine");
        let group = "adm:x:4:amine\nsudo:x:27:amine,bob\n";
        assert_eq!(group_members(group, "sudo"), vec!["amine", "bob"]);
        assert!(group_members(group, "absent").is_empty());
    }

    #[test]
    fn user_actions_protect_root_and_bad_names() {
        let root = User { name: "root".into(), uid: 0, home: "/root".into(), shell: "/bin/bash".into() };
        assert!(user_actions(&root, true, None).is_empty());
        let bad = User { name: "a;b".into(), uid: 1001, home: "/".into(), shell: "/bin/sh".into() };
        assert!(user_actions(&bad, false, None).is_empty());
        let ok = User { name: "bob".into(), uid: 1001, home: "/home/bob".into(), shell: "/bin/sh".into() };
        let actions = user_actions(&ok, false, Some(false));
        let labels: Vec<&str> = actions.iter().map(|cmd| cmd.label.as_str()).collect();
        assert_eq!(labels, vec!["Ajouter à sudo", "Verrouiller"]);
        assert!(actions.iter().all(|cmd| cmd.privileged));
    }

    #[test]
    fn parses_passwd_status() {
        assert_eq!(parse_passwd_status("bob L 2026-01-01 0 99999 7 -1"), Some(true));
        assert_eq!(parse_passwd_status("bob P 2026-01-01 0 99999 7 -1"), Some(false));
        assert_eq!(parse_passwd_status(""), None);
    }
}
