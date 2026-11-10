//! Sécurité : clés SSH/GPG, certificats, Secure Boot, AppArmor/SELinux,
//! intégrité des paquets et ACL.

use crate::exec;
use crate::panel::*;
use crate::util;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Clés SSH et GPG
// ---------------------------------------------------------------------------

/// Une clé SSH publique décrite par `ssh-keygen -lf`.
#[derive(Debug, Clone, PartialEq)]
pub struct SshKey {
    pub bits: String,
    pub fingerprint: String,
    pub comment: String,
    pub kind: String,
}

/// Analyse `256 SHA256:abc… commentaire (ED25519)`.
pub fn parse_ssh_fingerprint(line: &str) -> Option<SshKey> {
    let line = line.trim();
    let mut parts = line.splitn(3, ' ');
    let bits = parts.next()?.to_string();
    let fingerprint = parts.next()?.to_string();
    let rest = parts.next().unwrap_or("");
    if !bits.chars().all(|ch| ch.is_ascii_digit()) || !fingerprint.contains(':') {
        return None;
    }
    let (comment, kind) = match rest.rfind(" (") {
        Some(index) => (
            rest[..index].to_string(),
            rest[index + 2..].trim_end_matches(')').to_string(),
        ),
        None => (rest.to_string(), String::new()),
    };
    Some(SshKey { bits, fingerprint, comment, kind })
}

/// Une clé GPG.
#[derive(Debug, Clone, PartialEq)]
pub struct GpgKey {
    pub id: String,
    pub algo: String,
    pub created: String,
    pub uid: String,
}

/// Nom lisible de l'algorithme GPG (numéro RFC 4880).
pub fn gpg_algo_name(code: &str) -> &str {
    match code {
        "1" | "2" | "3" => "RSA",
        "16" | "20" => "ElGamal",
        "17" => "DSA",
        "18" => "ECDH",
        "19" => "ECDSA",
        "22" => "EdDSA",
        other => other,
    }
}

/// Date `AAAA-MM-JJ` d'un horodatage Unix en secondes.
pub fn epoch_date(epoch: &str) -> String {
    epoch
        .parse::<i64>()
        .ok()
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| epoch.to_string())
}

/// Analyse `gpg --list-keys --with-colons`.
pub fn parse_gpg_colons(text: &str) -> Vec<GpgKey> {
    let mut keys: Vec<GpgKey> = Vec::new();
    let mut expect_fingerprint = false;
    for line in text.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first().copied() {
            Some("pub") if fields.len() > 5 => {
                keys.push(GpgKey {
                    id: fields[4].to_string(),
                    algo: fields[3].to_string(),
                    created: fields[5].to_string(),
                    uid: String::new(),
                });
                expect_fingerprint = true;
            }
            Some("fpr") if expect_fingerprint => {
                if let (Some(key), Some(fingerprint)) = (keys.last_mut(), fields.get(9)) {
                    key.id = (*fingerprint).to_string();
                }
                expect_fingerprint = false;
            }
            Some("uid") if fields.len() > 9 => {
                if let Some(key) = keys.last_mut() {
                    if key.uid.is_empty() {
                        key.uid = fields[9].to_string();
                    }
                }
            }
            _ => {}
        }
    }
    keys
}

pub fn ssh_dir() -> PathBuf {
    home().join(".ssh")
}

/// Nom de fichier de clé : lettres, chiffres, `._-`, 1 à 40 caractères.
pub fn valid_key_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && !name.starts_with('.')
        && !name.starts_with('-')
        && name.chars().all(|ch| ch.is_ascii_alphanumeric() || "._-".contains(ch))
}

fn valid_comment(comment: &str) -> bool {
    comment.len() <= 80 && comment.chars().all(|ch| ch.is_ascii_alphanumeric() || "@._- ".contains(ch))
}

fn build_ssh_keygen(values: &[String]) -> Result<Cmd, String> {
    let [kind, name, comment] = values else {
        return Err("type, nom et commentaire attendus".to_string());
    };
    let (kind, name, comment) = (kind.trim(), name.trim(), comment.trim());
    let bits: &[&str] = match kind {
        "ed25519" => &["-t", "ed25519"],
        "rsa" => &["-t", "rsa", "-b", "4096"],
        _ => return Err("type inconnu : ed25519 ou rsa".to_string()),
    };
    if !valid_key_name(name) {
        return Err("nom de fichier invalide (lettres, chiffres, ._-)".to_string());
    }
    if !valid_comment(comment) {
        return Err("commentaire invalide".to_string());
    }
    let path = ssh_dir().join(name);
    if path.exists() || path.with_extension("pub").exists() {
        return Err(format!("{} existe déjà", path.display()));
    }
    std::fs::create_dir_all(ssh_dir()).map_err(|error| error.to_string())?;
    let mut args: Vec<String> = bits.iter().map(|arg| arg.to_string()).collect();
    args.extend(["-f".into(), path.to_string_lossy().to_string(), "-N".into(), String::new(), "-C".into(), comment.to_string(), "-q".into()]);
    Ok(Cmd::owned("Clé générée", "ssh-keygen", args))
}

/// Copie un texte dans le presse-papiers (Wayland puis X11).
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    for (program, args) in [("wl-copy", &[][..]), ("xclip", &["-selection", "clipboard"][..])] {
        if !exec::exists(program) {
            continue;
        }
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).map_err(|error| error.to_string())?;
        }
        return child.wait().map(|_| ()).map_err(|error| error.to_string());
    }
    Err("wl-copy ou xclip requis pour copier".to_string())
}

fn copy_public_key(args: &[String]) -> Result<String, String> {
    let [path] = args else {
        return Err("clé attendue".to_string());
    };
    if !path.ends_with(".pub") || !util::path_within(&ssh_dir(), Path::new(path)) {
        return Err("seules les clés publiques de ~/.ssh peuvent être copiées".to_string());
    }
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    copy_to_clipboard(text.trim())?;
    Ok("Clé publique copiée".to_string())
}

pub fn ssh_gpg() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ssh_dir()) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "pub"))
            .collect();
        files.sort();
        for path in files {
            let text = path.to_string_lossy().to_string();
            let info = run_lossy("ssh-keygen", &["-lf", &text]);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let row = match parse_ssh_fingerprint(&info) {
                Some(key) => Row::owned(vec![name, format!("{} {}", key.kind, key.bits), util::ellipsis(&key.fingerprint, 50), key.comment]),
                None => Row::owned(vec![name, "illisible".into(), String::new(), String::new()]).bad(true),
            };
            rows.push(row.action(Cmd::local("Copier la clé publique", copy_public_key, vec![text])));
        }
    }
    panel.metric("Clés SSH", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Clés SSH (~/.ssh)")
            .columns(&["Fichier", "Type", "Empreinte", "Commentaire"])
            .rows(rows)
            .form(Form {
                fields: vec![
                    Field::new("Type", "ed25519 ou rsa").with_default("ed25519"),
                    Field::new("Nom du fichier", "id_ed25519_travail"),
                    Field::new("Commentaire", "adresse ou machine"),
                ],
                submit: "Générer".to_string(),
                build: build_ssh_keygen,
            })
            .note("La clé est créée sans phrase secrète ; ajoutez-en une avec « ssh-keygen -p -f <fichier> »."),
    );
    if exec::exists("gpg") {
        let keys = parse_gpg_colons(&run_lossy("gpg", &["--list-keys", "--with-colons", "--with-fingerprint"]));
        panel.metric("Clés GPG", keys.len().to_string(), Level::Info);
        let rows = keys
            .iter()
            .map(|key| Row::owned(vec![util::ellipsis(&key.uid, 50), util::ellipsis(&key.id, 44), gpg_algo_name(&key.algo).to_string(), epoch_date(&key.created)]))
            .collect();
        panel.section(
            Section::new("Trousseau GPG")
                .columns(&["Identité", "Empreinte", "Algo", "Création"])
                .rows(rows),
        );
    }
    panel
}

// ---------------------------------------------------------------------------
// Certificats
// ---------------------------------------------------------------------------

pub const CA_DIR: &str = "/usr/local/share/ca-certificates";

/// Date de fin extraite de `notAfter=Dec 31 23:59:59 2026 GMT`.
pub fn parse_not_after(text: &str) -> Option<chrono::NaiveDateTime> {
    let value = text.trim().strip_prefix("notAfter=")?.trim().trim_end_matches(" GMT");
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    chrono::NaiveDateTime::parse_from_str(&normalized, "%b %d %H:%M:%S %Y").ok()
}

/// Jours restants (négatif si expiré).
pub fn days_until(date: chrono::NaiveDateTime, now: chrono::NaiveDateTime) -> i64 {
    (date - now).num_days()
}

/// Fichier de certificat valide dans le dossier des CA locales.
pub fn ca_file_ok(path: &str) -> bool {
    safe_abs_path(path) && path.starts_with(&format!("{CA_DIR}/")) && path.ends_with(".crt")
}

fn build_ca_add(values: &[String]) -> Result<Cmd, String> {
    let [source] = values else {
        return Err("fichier attendu".to_string());
    };
    let source = source.trim();
    let lower = source.to_ascii_lowercase();
    if !safe_abs_path(source) || !(lower.ends_with(".crt") || lower.ends_with(".pem")) {
        return Err("certificat .crt ou .pem avec chemin absolu requis".to_string());
    }
    if !Path::new(source).is_file() {
        return Err(format!("fichier introuvable : {source}"));
    }
    let stem = Path::new(source)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if !valid_key_name(&stem) {
        return Err("nom de fichier de certificat invalide (lettres, chiffres, ._-)".to_string());
    }
    let target = format!("{CA_DIR}/{stem}.crt");
    Ok(Cmd::new("Certificat ajouté", "cp", &["--", source, &target])
        .privileged()
        .confirm(&format!("Copier {source} dans {CA_DIR} ?\nPensez ensuite à mettre à jour le magasin.")))
}

pub fn certificates() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    let now = chrono::Local::now().naive_local();
    if let Ok(entries) = std::fs::read_dir(CA_DIR) {
        let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            let text = path.to_string_lossy().to_string();
            if !ca_file_ok(&text) {
                continue;
            }
            let info = run_lossy("openssl", &["x509", "-enddate", "-noout", "-in", &text]);
            let (expiry, bad) = match parse_not_after(&info) {
                Some(date) => {
                    let days = days_until(date, now);
                    (format!("{} ({} j)", date.format("%Y-%m-%d"), days), days < 30)
                }
                None => ("illisible".to_string(), true),
            };
            rows.push(
                Row::owned(vec![
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    expiry,
                ])
                .bad(bad)
                .action(
                    Cmd::new("Supprimer", "rm", &["--", &text])
                        .privileged()
                        .confirm(&format!("Supprimer {text} ? Mettez ensuite le magasin à jour.")),
                ),
            );
        }
    }
    panel.metric("CA locales", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Autorités de certification locales")
            .columns(&["Certificat", "Expiration"])
            .rows(rows)
            .action(
                Cmd::new("Mettre à jour le magasin", "update-ca-certificates", &["--fresh"])
                    .privileged()
                    .confirm("Reconstruire le magasin de certificats ?"),
            )
            .form(Form {
                fields: vec![Field::new("Fichier", "/chemin/autorite.crt")],
                submit: "Ajouter".to_string(),
                build: build_ca_add,
            }),
    );
    panel
}

// ---------------------------------------------------------------------------
// Secure Boot
// ---------------------------------------------------------------------------

/// Sujets des clés MOK d'après `mokutil --list-enrolled`.
pub fn parse_mok_subjects(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("Subject:").map(|s| s.trim().to_string()))
        .filter(|subject| !subject.is_empty())
        .collect()
}

pub fn secure_boot() -> Panel {
    let mut panel = Panel::default();
    let uefi = Path::new("/sys/firmware/efi").exists();
    panel.metric("Mode de démarrage", if uefi { "UEFI" } else { "BIOS (hérité)" }, Level::Info);
    if !exec::exists("mokutil") {
        panel.error("mokutil absent : installez le paquet « mokutil » pour lire l'état de Secure Boot.");
        return panel;
    }
    let state = run_lossy("mokutil", &["--sb-state"]).trim().to_string();
    let enabled = state.to_ascii_lowercase().contains("enabled");
    panel.metric(
        "Secure Boot",
        if state.is_empty() { "inconnu".to_string() } else { state.clone() },
        if enabled { Level::Ok } else { Level::Warn },
    );
    let keys = parse_mok_subjects(&run_lossy("mokutil", &["--list-enrolled"]));
    let rows = keys.iter().map(|subject| Row::owned(vec![util::ellipsis(subject, 110)])).collect();
    panel.section(
        Section::new("Clés MOK enregistrées (lecture seule)")
            .columns(&["Sujet"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// AppArmor / SELinux
// ---------------------------------------------------------------------------

/// Profils AppArmor `nom (mode)` : `(nom, mode)`.
pub fn parse_apparmor_profiles(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let open = line.rfind(" (")?;
            let mode = line[open + 2..].trim_end_matches(')').to_string();
            matches!(mode.as_str(), "enforce" | "complain" | "kill" | "unconfined" | "prompt")
                .then(|| (line[..open].to_string(), mode))
        })
        .collect()
}

/// Nom de profil AppArmor sûr (chemin ou identifiant, sans espace).
pub fn valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && !name.contains("..")
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/._:-+".contains(ch))
}

pub fn apparmor() -> Panel {
    let mut panel = Panel::default();
    let mode = if exec::exists("getenforce") {
        run_lossy("getenforce", &[]).trim().to_string()
    } else {
        String::new()
    };
    // « Disabled » : SELinux n'est pas utilisé sur ce système, rien à afficher.
    let selinux = mode == "Enforcing" || mode == "Permissive";
    if selinux {
        panel.metric("SELinux", mode.clone(), if mode == "Enforcing" { Level::Ok } else { Level::Warn });
        let mut section = Section::new("SELinux");
        if mode != "Enforcing" {
            section = section.action(Cmd::new("Passer en enforcing", "setenforce", &["1"]).privileged().confirm("Passer SELinux en mode enforcing (immédiat) ?"));
        }
        if mode != "Permissive" {
            section = section.action(Cmd::new("Passer en permissive", "setenforce", &["0"]).privileged().confirm("Passer SELinux en mode permissive (immédiat) ?"));
        }
        panel.section(section);
    }
    let raw = read("/sys/kernel/security/apparmor/profiles");
    if raw.is_empty() {
        if !selinux {
            panel.error("Profils AppArmor illisibles : les droits administrateur sont requis (ou AppArmor est inactif).");
        }
        return panel;
    }
    let profiles = parse_apparmor_profiles(&raw);
    let enforcing = profiles.iter().filter(|(_, mode)| mode == "enforce").count();
    panel.metric("Profils chargés", profiles.len().to_string(), Level::Info);
    panel.metric("En enforce", enforcing.to_string(), Level::Ok);
    let rows = profiles
        .iter()
        .filter(|(name, _)| valid_profile_name(name))
        .map(|(name, mode)| {
            let mut row = Row::owned(vec![name.clone(), mode.clone()]).bad(mode == "complain");
            if mode != "complain" {
                row = row.action(Cmd::new("Complain", "aa-complain", &[name]).privileged().confirm(&format!("Passer {name} en mode complain (journalise sans bloquer) ?")));
            }
            if mode != "enforce" {
                row = row.action(Cmd::new("Enforce", "aa-enforce", &[name]).privileged());
            }
            row
        })
        .collect();
    panel.section(
        Section::new("Profils AppArmor")
            .columns(&["Profil", "Mode"])
            .rows(rows)
            .note("aa-complain / aa-enforce proviennent du paquet apparmor-utils."),
    );
    panel
}

// ---------------------------------------------------------------------------
// Intégrité
// ---------------------------------------------------------------------------

/// Une anomalie de `dpkg -V` : `(drapeaux, fichier de configuration ?, chemin)`.
pub fn parse_dpkg_verify(text: &str) -> Vec<(String, bool, String)> {
    text.lines()
        .filter_map(|line| {
            if line.len() < 12 {
                return None;
            }
            let flags = line.get(..9)?;
            if !flags.chars().all(|ch| ch == '?' || ch == '.' || ch.is_ascii_alphanumeric()) {
                return None;
            }
            let rest = line.get(9..)?.trim_start();
            let (conffile, path) = match rest.split_once(' ') {
                Some((attr, path)) if attr.len() == 1 => (attr == "c", path.trim()),
                _ => (false, rest),
            };
            path.starts_with('/').then(|| (flags.to_string(), conffile, path.to_string()))
        })
        .collect()
}

pub fn integrity() -> Panel {
    let mut panel = Panel::default();
    // dpkg -V (Debian/Ubuntu) et rpm -Va (Fedora, RHEL, openSUSE) ont le même format de sortie.
    let (program, args): (&str, &[&str]) = if exec::exists("dpkg") {
        ("dpkg", &["-V"])
    } else if exec::exists("rpm") {
        ("rpm", &["-Va"])
    } else {
        panel.error("Ni dpkg ni rpm : la vérification de l'intégrité des paquets n'est pas disponible sur ce système (Arch : « pacman -Qkk », Alpine : « apk verify »).");
        return panel;
    };
    let captured = exec::run_with_timeout(program, args, Duration::from_secs(300));
    let text = match captured {
        Ok(captured) => captured.stdout,
        Err(error) => {
            panel.error(format!("{program} : {}", error.0));
            return panel;
        }
    };
    let (unreadable, issues): (Vec<_>, Vec<_>) = parse_dpkg_verify(&text)
        .into_iter()
        .partition(|(flags, _, _)| flags.chars().all(|ch| ch == '?'));
    let binaries = issues.iter().filter(|(_, conf, _)| !conf).count();
    if !unreadable.is_empty() {
        panel.metric("Illisibles (droits)", unreadable.len().to_string(), Level::Info);
    }
    panel.metric("Fichiers modifiés", issues.len().to_string(), if issues.is_empty() { Level::Ok } else { Level::Warn });
    panel.metric("Hors configuration", binaries.to_string(), if binaries == 0 { Level::Ok } else { Level::Bad });
    let rows = issues
        .iter()
        .take(300)
        .map(|(flags, conf, path)| {
            Row::owned(vec![path.clone(), flags.clone(), if *conf { "configuration" } else { "programme/donnée" }.to_string()]).bad(!conf)
        })
        .collect();
    panel.section(
        Section::new(if program == "dpkg" { "dpkg -V" } else { "rpm -Va" })
            .columns(&["Fichier", "Écarts", "Nature"])
            .rows(rows)
            .note("Les fichiers de configuration modifiés sont normaux ; un programme modifié mérite une vérification."),
    );
    if exec::exists("debsums") {
        let sums = run_lossy("debsums", &["-s"]);
        let rows = sums.lines().filter(|l| !l.trim().is_empty()).take(200).map(|l| Row::owned(vec![util::ellipsis(l.trim(), 110)])).collect::<Vec<_>>();
        panel.metric("debsums", if rows.is_empty() { "aucun écart".to_string() } else { format!("{} écart(s)", rows.len()) }, if rows.is_empty() { Level::Ok } else { Level::Warn });
        panel.section(Section::new("debsums -s").columns(&["Anomalie"]).rows(rows));
    }
    panel
}

// ---------------------------------------------------------------------------
// ACL et permissions
// ---------------------------------------------------------------------------

/// Droits ACL valides : combinaison de r, w, x ou `-` (ex. `rwx`, `r-x`).
pub fn valid_acl_perms(perms: &str) -> bool {
    let chars: Vec<char> = perms.chars().collect();
    chars.len() == 3
        && ["r", "w", "x"]
            .iter()
            .enumerate()
            .all(|(index, letter)| chars[index] == '-' || chars[index].to_string() == *letter)
}

fn existing_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if !safe_abs_path(path) {
        return Err("chemin absolu requis".to_string());
    }
    if !Path::new(path).exists() {
        return Err(format!("introuvable : {path}"));
    }
    Ok(path.to_string())
}

fn build_acl_show(values: &[String]) -> Result<Cmd, String> {
    let path = existing_path(values.first().map(String::as_str).unwrap_or(""))?;
    Ok(Cmd::new("ACL", "getfacl", &["-p", "--", &path]))
}

fn build_acl_set(values: &[String]) -> Result<Cmd, String> {
    let [path, user, perms] = values else {
        return Err("chemin, utilisateur et droits attendus".to_string());
    };
    let path = existing_path(path)?;
    let user = user.trim();
    let perms = perms.trim();
    if !valid_username(user) {
        return Err(format!("utilisateur invalide : {user}"));
    }
    if !valid_acl_perms(perms) {
        return Err("droits invalides (ex. rwx, r-x, r--)".to_string());
    }
    Ok(Cmd::new("ACL modifiée", "setfacl", &["-m", &format!("u:{user}:{perms}"), "--", &path])
        .privileged()
        .confirm(&format!("Donner {perms} à {user} sur {path} ?")))
}

fn build_acl_clear(values: &[String]) -> Result<Cmd, String> {
    let path = existing_path(values.first().map(String::as_str).unwrap_or(""))?;
    Ok(Cmd::new("ACL retirées", "setfacl", &["-b", "--", &path])
        .privileged()
        .confirm(&format!("Supprimer toutes les ACL étendues de {path} ?")))
}

pub fn acl() -> Panel {
    let mut panel = Panel::default();
    for tool in ["getfacl", "setfacl"] {
        if !exec::exists(tool) {
            panel.error(format!("{tool} absent : installez le paquet « acl »."));
        }
    }
    panel.section(
        Section::new("Analyser un fichier ou dossier")
            .note("Le résultat s'affiche dans la zone de sortie en bas de l'écran.")
            .form(Form {
                fields: vec![Field::new("Chemin", "/chemin/absolu")],
                submit: "Afficher les ACL".to_string(),
                build: build_acl_show,
            }),
    );
    panel.section(
        Section::new("Définir une ACL utilisateur").form(Form {
            fields: vec![
                Field::new("Chemin", "/chemin/absolu"),
                Field::new("Utilisateur", "nom"),
                Field::new("Droits", "rwx").with_default("r-x"),
            ],
            submit: "Appliquer".to_string(),
            build: build_acl_set,
        }),
    );
    panel.section(
        Section::new("Retirer les ACL").form(Form {
            fields: vec![Field::new("Chemin", "/chemin/absolu")],
            submit: "Supprimer les ACL".to_string(),
            build: build_acl_clear,
        }),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_fingerprints() {
        let key = parse_ssh_fingerprint("256 SHA256:abcDEF+/12 amine@pc (ED25519)").unwrap();
        assert_eq!(key.bits, "256");
        assert_eq!(key.comment, "amine@pc");
        assert_eq!(key.kind, "ED25519");
        let no_comment = parse_ssh_fingerprint("4096 SHA256:xyz  (RSA)").unwrap();
        assert_eq!(no_comment.kind, "RSA");
        assert!(parse_ssh_fingerprint("n'importe quoi").is_none());
        assert!(parse_ssh_fingerprint("").is_none());
    }

    #[test]
    fn parses_gpg_colons() {
        let text = "tru::1:1:1:0:3:1:5\npub:u:255:22:ABCDEF0123456789:1700000000:::u:::scESC:::::ed25519:::0:\nfpr:::::::::AAAA1111BBBB2222CCCC3333DDDD4444EEEE5555:\nuid:u::::1700000000::HASH::Amine <a@exemple.fr>::::::::::0:\nsub:u:255:18:1111222233334444:1700000000::::::e::::::cv25519::\n";
        let keys = parse_gpg_colons(text);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "AAAA1111BBBB2222CCCC3333DDDD4444EEEE5555");
        assert_eq!(keys[0].uid, "Amine <a@exemple.fr>");
    }

    #[test]
    fn validates_key_generation_input() {
        let ok = |k: &str, n: &str, c: &str| build_ssh_keygen(&[k.into(), n.into(), c.into()]);
        assert!(ok("dsa", "cle", "x").is_err());
        assert!(ok("ed25519", "../evil", "x").is_err());
        assert!(ok("ed25519", "-oProxyCommand", "x").is_err());
        assert!(ok("ed25519", "cle", "a;b").is_err());
        assert!(ok("ed25519", "", "x").is_err());
        assert!(valid_key_name("id_ed25519_travail"));
    }

    #[test]
    fn copies_only_public_keys_of_ssh_dir() {
        assert!(copy_public_key(&["/etc/passwd".into()]).is_err());
        assert!(copy_public_key(&[format!("{}/id_ed25519", ssh_dir().display())]).is_err());
        assert!(copy_public_key(&[format!("{}/../x.pub", ssh_dir().display())]).is_err());
        assert!(copy_public_key(&[]).is_err());
    }

    #[test]
    fn parses_certificate_dates() {
        let date = parse_not_after("notAfter=Dec 31 23:59:59 2026 GMT\n").unwrap();
        assert_eq!(date.format("%Y-%m-%d").to_string(), "2026-12-31");
        let padded = parse_not_after("notAfter=Jan  2 03:04:05 2030 GMT").unwrap();
        assert_eq!(padded.format("%Y-%m-%d").to_string(), "2030-01-02");
        assert!(parse_not_after("erreur").is_none());
        let now = parse_not_after("notAfter=Dec 01 00:00:00 2026 GMT").unwrap();
        assert_eq!(days_until(date, now), 30);
        assert_eq!(days_until(now, date), -30);
    }

    #[test]
    fn ca_paths_are_restricted() {
        assert!(ca_file_ok("/usr/local/share/ca-certificates/mon-ca.crt"));
        assert!(!ca_file_ok("/etc/ssl/certs/ca-certificates.crt"));
        assert!(!ca_file_ok("/usr/local/share/ca-certificates/../../../etc/passwd.crt"));
        assert!(build_ca_add(&["relatif.crt".into()]).is_err());
        assert!(build_ca_add(&["/tmp/inexistant.crt".into()]).is_err());
        assert!(build_ca_add(&["/tmp/a.txt".into()]).is_err());
    }

    #[test]
    fn parses_mok_and_apparmor() {
        let mok = "[key 1]\nSHA1 Fingerprint: aa\nCertificate:\n  Subject: CN=Canonical Ltd. Master Certificate Authority\n";
        assert_eq!(parse_mok_subjects(mok), vec!["CN=Canonical Ltd. Master Certificate Authority"]);
        let profiles = parse_apparmor_profiles("/usr/bin/man (enforce)\nsnap.firefox.firefox (complain)\nligne invalide\n");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[1], ("snap.firefox.firefox".to_string(), "complain".to_string()));
        assert!(valid_profile_name("/usr/bin/man"));
        assert!(!valid_profile_name("-x"));
        assert!(!valid_profile_name("a b"));
        assert!(!valid_profile_name("/usr/../etc"));
    }

    #[test]
    fn formats_gpg_fields() {
        assert_eq!(gpg_algo_name("1"), "RSA");
        assert_eq!(gpg_algo_name("22"), "EdDSA");
        assert_eq!(gpg_algo_name("99"), "99");
        assert_eq!(epoch_date("1700000000"), "2023-11-14");
        assert_eq!(epoch_date("pas un nombre"), "pas un nombre");
    }

    #[test]
    fn parses_dpkg_verify() {
        let text = "??5??????  c /etc/ssh/sshd_config\n..5......    /usr/bin/ls\nmauvais\n";
        let issues = parse_dpkg_verify(text);
        assert_eq!(issues.len(), 2);
        assert!(issues[0].1);
        assert_eq!(issues[0].2, "/etc/ssh/sshd_config");
        assert!(!issues[1].1);
        assert_eq!(issues[1].2, "/usr/bin/ls");
    }

    #[test]
    fn validates_acl_forms() {
        assert!(valid_acl_perms("rwx"));
        assert!(valid_acl_perms("r--"));
        assert!(valid_acl_perms("-w-"));
        assert!(!valid_acl_perms("xwr"));
        assert!(!valid_acl_perms("rwxx"));
        assert!(build_acl_show(&["/tmp".into()]).is_ok());
        assert!(build_acl_show(&["relatif".into()]).is_err());
        assert!(build_acl_set(&["/tmp".into(), "Root".into(), "rwx".into()]).is_err());
        assert!(build_acl_set(&["/tmp".into(), "bob".into(), "777".into()]).is_err());
        assert!(build_acl_set(&["/tmp".into(), "bob".into(), "r-x".into()]).is_ok());
        assert!(build_acl_set(&["/tmp/../etc".into(), "bob".into(), "r-x".into()]).is_err());
        assert!(build_acl_clear(&["/tmp".into()]).unwrap().privileged);
    }
}
