//! Sauvegarde cloud via rclone.

use crate::exec;
use crate::panel::*;
use std::path::Path;

/// Analyse `rclone listremotes` : noms sans le `:` final.
pub fn parse_remotes(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_suffix(':'))
        .filter(|name| safe_token(name) && !name.contains('/'))
        .map(str::to_string)
        .collect()
}

/// Chemin distant : pas d'option, pas de retour à la ligne, pas de `..`.
pub fn valid_remote_path(path: &str) -> bool {
    path.len() <= 200
        && !path.starts_with('-')
        && !path.contains("..")
        && !path.chars().any(|ch| ch.is_control() || ch == ':')
}

/// Construit la commande `rclone sync` ; `mode` vaut `simulation` ou `reel`.
pub fn build_sync(remotes: &[String], remote: &str, local: &str, path: &str, mode: &str) -> Result<Cmd, String> {
    if !remotes.iter().any(|known| known == remote) {
        return Err(format!("remote inconnu : {remote}"));
    }
    if !safe_abs_path(local) || !Path::new(local).is_dir() {
        return Err("le dossier local doit exister (chemin absolu)".to_string());
    }
    if !valid_remote_path(path) {
        return Err("chemin distant invalide".to_string());
    }
    let destination = format!("{remote}:{path}");
    let mut args = vec!["sync".to_string(), "-v".to_string()];
    let real = match mode {
        "simulation" => false,
        "reel" => true,
        _ => return Err("mode inconnu : simulation ou reel".to_string()),
    };
    if !real {
        args.push("--dry-run".to_string());
    }
    args.push(local.to_string());
    args.push(destination.clone());
    let cmd = Cmd::owned(if real { "Synchronisation" } else { "Simulation" }, "rclone", args);
    Ok(if real {
        cmd.confirm(&format!(
            "Synchroniser {local} vers {destination} ?\nLes fichiers absents du dossier local seront SUPPRIMÉS à distance."
        ))
    } else {
        cmd
    })
}

fn build_from_form(values: &[String]) -> Result<Cmd, String> {
    let [remote, local, path, mode] = values else {
        return Err("remote, dossier, chemin et mode attendus".to_string());
    };
    let remotes = parse_remotes(&run_lossy("rclone", &["listremotes"]));
    build_sync(&remotes, remote.trim(), local.trim(), path.trim(), mode.trim())
}

pub fn cloud_backup() -> Panel {
    let mut panel = Panel::default();
    if !exec::exists("rclone") {
        panel.error("rclone absent : installez le paquet « rclone » puis configurez un remote avec « rclone config ».");
        return panel;
    }
    let remotes = parse_remotes(&run_lossy("rclone", &["listremotes"]));
    panel.metric("Remotes", remotes.len().to_string(), if remotes.is_empty() { Level::Warn } else { Level::Ok });
    let rows = remotes.iter().map(|name| Row::new(&[name])).collect();
    panel.section(Section::new("Remotes rclone").columns(&["Nom"]).rows(rows));
    panel.section(
        Section::new("Synchroniser un dossier")
            .note("Mode « simulation » par défaut (rclone --dry-run) : rien n'est modifié. Le mode « reel » demande confirmation.")
            .form(Form {
                fields: vec![
                    Field::new("Remote", "nom du remote").with_default(remotes.first().map(String::as_str).unwrap_or("")),
                    Field::new("Dossier local", "/chemin/dossier"),
                    Field::new("Chemin distant", "Sauvegardes/PC"),
                    Field::new("Mode", "simulation ou reel").with_default("simulation"),
                ],
                submit: "Lancer".to_string(),
                build: build_from_form,
            }),
    );
    panel
}

// ---------------------------------------------------------------------------
// Dotfiles : sauvegarde et restauration
// ---------------------------------------------------------------------------

fn default_dotfile_dir() -> std::path::PathBuf {
    dirs::document_dir().unwrap_or_else(home).join("dotfiles")
}

fn do_backup(_: &[String]) -> Result<String, String> {
    crate::backup::backup_dotfiles(&default_dotfile_dir())
}

fn do_restore(args: &[String]) -> Result<String, String> {
    let [folder] = args else {
        return Err("dossier attendu".to_string());
    };
    if !safe_abs_path(folder) {
        return Err("chemin absolu requis".to_string());
    }
    crate::backup::restore_dotfiles(Path::new(folder))
}

fn build_restore(values: &[String]) -> Result<Cmd, String> {
    let folder = values.first().map(|v| v.trim()).unwrap_or("");
    if !safe_abs_path(folder) || !Path::new(folder).is_dir() {
        return Err("dossier existant requis (chemin absolu)".to_string());
    }
    Ok(Cmd::local("Restauration", do_restore, vec![folder.to_string()])
        .confirm(&format!("Restaurer les dotfiles depuis {folder} ?\nLes fichiers actuels sont conservés en .um-bak-<date>.")))
}

/// Sous-dossiers de sauvegarde connus : `Documents/dotfiles*`.
pub fn backup_folders() -> Vec<std::path::PathBuf> {
    let base = dirs::document_dir().unwrap_or_else(home);
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut found: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("dotfiles"))
        })
        .collect();
    found.sort();
    found
}

pub fn dotfiles() -> Panel {
    let mut panel = Panel::default();
    let current = crate::backup::dotfiles();
    panel.metric("Fichiers détectés", current.len().to_string(), Level::Info);
    let rows = current
        .iter()
        .map(|item| Row::new(&[&item.name, &crate::util::human_bytes(item.size)]))
        .collect();
    panel.section(
        Section::new("Configuration actuelle")
            .columns(&["Fichier", "Taille"])
            .rows(rows)
            .action(Cmd::local("Sauvegarder maintenant", do_backup, Vec::new())
                .confirm(&format!("Copier ces fichiers dans {} ?", default_dotfile_dir().display()))),
    );
    let rows = backup_folders()
        .iter()
        .map(|folder| {
            let text = folder.to_string_lossy().to_string();
            Row::new(&[&text]).action(Cmd::local("Restaurer", do_restore, vec![text.clone()])
                .confirm(&format!("Restaurer les dotfiles depuis {text} ?\nLes fichiers actuels sont conservés en .um-bak-<date>.")))
        })
        .collect();
    panel.section(
        Section::new("Sauvegardes trouvées (Documents/dotfiles*)")
            .columns(&["Dossier"])
            .rows(rows)
            .form(Form {
                fields: vec![Field::new("Autre dossier", "/chemin/sauvegarde")],
                submit: "Restaurer ce dossier".to_string(),
                build: build_restore,
            }),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remotes() {
        assert_eq!(parse_remotes("gdrive:\ns3-prod:\nbruit\n"), vec!["gdrive", "s3-prod"]);
        assert!(parse_remotes("").is_empty());
    }

    #[test]
    fn simulation_is_the_default_safe_mode() {
        let remotes = vec!["gdrive".to_string()];
        let tmp = std::env::temp_dir().to_string_lossy().to_string();
        let dry = build_sync(&remotes, "gdrive", &tmp, "Sauvegardes", "simulation").unwrap();
        assert!(dry.args.contains(&"--dry-run".to_string()));
        assert!(dry.confirm.is_none());
        let real = build_sync(&remotes, "gdrive", &tmp, "Sauvegardes", "reel").unwrap();
        assert!(!real.args.contains(&"--dry-run".to_string()));
        assert!(real.confirm.is_some());
    }

    #[test]
    fn rejects_unsafe_inputs() {
        let remotes = vec!["gdrive".to_string()];
        let tmp = std::env::temp_dir().to_string_lossy().to_string();
        assert!(build_sync(&remotes, "autre", &tmp, "x", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", "relatif", "x", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", "/inexistant/dossier", "x", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", &tmp, "--delete-before", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", &tmp, "../x", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", &tmp, "a:b", "simulation").is_err());
        assert!(build_sync(&remotes, "gdrive", &tmp, "x", "oups").is_err());
    }

    #[test]
    fn restore_form_validates_folder() {
        assert!(build_restore(&["relatif".into()]).is_err());
        assert!(build_restore(&["/inexistant/dossier".into()]).is_err());
        let tmp = std::env::temp_dir().to_string_lossy().to_string();
        assert!(build_restore(&[tmp]).unwrap().confirm.is_some());
        assert!(do_restore(&["/tmp/../etc".into()]).is_err());
        assert!(do_restore(&[]).is_err());
    }
}
