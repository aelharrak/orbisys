//! Logiciels : dépôts APT, paquets avancés, runtimes, conteneurs.

use crate::exec;
use crate::panel::*;
use crate::util;

/// Découpe une sortie séparée par des tabulations.
pub fn tsv(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split('\t').map(|cell| cell.trim().to_string()).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Dépôts APT
// ---------------------------------------------------------------------------

/// Une source APT.
#[derive(Debug, Clone, PartialEq)]
pub struct AptSource {
    pub file: String,
    /// Numéro de ligne (1-based) pour le format ligne ; 0 pour deb822.
    pub line: usize,
    pub kind: String,
    pub uri: String,
    pub suite: String,
    pub components: String,
    pub enabled: bool,
    /// Ligne d'origine (format ligne) : permet de vérifier qu'elle n'a pas bougé avant d'agir.
    pub raw: String,
}

/// Analyse une ligne `deb [options] uri suite composants…`, éventuellement
/// commentée (source désactivée).
pub fn parse_source_line(file: &str, number: usize, line: &str) -> Option<AptSource> {
    let trimmed = line.trim();
    let (enabled, body) = match trimmed.strip_prefix('#') {
        Some(rest) => (false, rest.trim_start()),
        None => (true, trimmed),
    };
    let mut tokens = body.split_whitespace();
    let kind = tokens.next()?;
    if kind != "deb" && kind != "deb-src" {
        return None;
    }
    let mut rest: Vec<&str> = tokens.collect();
    if rest.first().is_some_and(|token| token.starts_with('[')) {
        let mut end = 0;
        for (index, token) in rest.iter().enumerate() {
            if token.ends_with(']') {
                end = index + 1;
                break;
            }
        }
        rest.drain(..end);
    }
    if rest.len() < 2 || !(rest[0].contains("://") || rest[0].starts_with("cdrom:")) {
        return None;
    }
    Some(AptSource {
        file: file.to_string(),
        line: number,
        kind: kind.to_string(),
        uri: rest[0].to_string(),
        suite: rest[1].to_string(),
        components: rest[2..].join(" "),
        enabled,
        raw: line.to_string(),
    })
}

/// Analyse un fichier deb822 (`*.sources`).
pub fn parse_deb822(file: &str, text: &str) -> Vec<AptSource> {
    let mut sources = Vec::new();
    for stanza in text.split("\n\n") {
        let mut fields = std::collections::HashMap::new();
        for line in stanza.lines() {
            if line.starts_with('#') || line.starts_with(' ') {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                fields.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let Some(uris) = fields.get("uris") else {
            continue;
        };
        sources.push(AptSource {
            file: file.to_string(),
            line: 0,
            kind: fields.get("types").cloned().unwrap_or_else(|| "deb".to_string()),
            uri: uris.clone(),
            suite: fields.get("suites").cloned().unwrap_or_default(),
            components: fields.get("components").cloned().unwrap_or_default(),
            enabled: fields
                .get("enabled")
                .map(|value| !value.eq_ignore_ascii_case("no"))
                .unwrap_or(true),
            raw: String::new(),
        });
    }
    sources
}

/// Chemin sous `/etc/apt`, sans traversée.
pub fn apt_path_ok(path: &str) -> bool {
    safe_abs_path(path)
        && path.starts_with("/etc/apt/")
        && (path.ends_with(".list") || path.ends_with(".sources") || path.ends_with(".sources.disabled"))
}

pub fn apt_sources() -> Vec<AptSource> {
    let mut files = vec!["/etc/apt/sources.list".to_string()];
    if let Ok(entries) = std::fs::read_dir("/etc/apt/sources.list.d") {
        let mut found: Vec<String> = entries
            .flatten()
            .map(|entry| entry.path().to_string_lossy().to_string())
            .filter(|path| apt_path_ok(path))
            .collect();
        found.sort();
        files.extend(found);
    }
    let mut sources = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if file.ends_with(".sources") || file.ends_with(".sources.disabled") {
            let mut parsed = parse_deb822(&file, &text);
            if file.ends_with(".disabled") {
                for source in &mut parsed {
                    source.enabled = false;
                }
            }
            sources.extend(parsed);
        } else {
            for (index, line) in text.lines().enumerate() {
                if let Some(source) = parse_source_line(&file, index + 1, line) {
                    sources.push(source);
                }
            }
        }
    }
    sources
}

/// Commandes de bascule/suppression d'une source.
pub fn source_actions(source: &AptSource) -> Vec<Cmd> {
    if !apt_path_ok(&source.file) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if source.line > 0 {
        let line = source.line.to_string();
        let base = vec![source.file.clone(), line, source.raw.clone()];
        let with = |op: &str| {
            let mut args = base.clone();
            args.push(op.to_string());
            args
        };
        actions.push(Cmd::local(
            if source.enabled { "Désactiver" } else { "Activer" },
            edit_source_line,
            with(if source.enabled { "disable" } else { "enable" }),
        ));
        actions.push(
            Cmd::local("Supprimer", edit_source_line, with("delete"))
                .confirm(&format!("Supprimer la ligne {} de {} ?", source.line, source.file)),
        );
    } else if source.file.ends_with(".sources") {
        if source.enabled {
            actions.push(
                Cmd::new("Désactiver", "mv", &["--", &source.file, &format!("{}.disabled", source.file)])
                    .privileged(),
            );
        } else {
            actions.push(
                Cmd::new(
                    "Activer",
                    "sed",
                    &["-i", "s/^Enabled:[[:space:]]*[Nn][Oo]/Enabled: yes/", &source.file],
                )
                .privileged(),
            );
        }
    } else if let Some(original) = source.file.strip_suffix(".disabled") {
        actions.push(Cmd::new("Activer", "mv", &["--", &source.file, original]).privileged());
    }
    actions
}

/// Modifie une ligne d'un fichier de sources après avoir vérifié qu'elle a
/// toujours le contenu affiché (le fichier a pu changer depuis la lecture).
fn edit_source_line(args: &[String]) -> Result<String, String> {
    let [file, number, expected, op] = args else {
        return Err("arguments invalides".to_string());
    };
    if !apt_path_ok(file) {
        return Err(format!("fichier refusé : {file}"));
    }
    let index: usize = number.parse().map_err(|_| "numéro de ligne invalide".to_string())?;
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    let current = text.lines().nth(index.wrapping_sub(1)).unwrap_or("");
    if index == 0 || current != expected {
        return Err("le fichier a changé depuis la lecture : actualisez l'écran".to_string());
    }
    let expression = match op.as_str() {
        "disable" => format!("{index}s/^/# /"),
        "enable" => format!("{index}s/^#[[:space:]]*//"),
        "delete" => format!("{index}d"),
        _ => return Err(format!("opération inconnue : {op}")),
    };
    Cmd::new("Sources APT", "sed", &["-i", &expression, file]).privileged().run()?;
    Ok(format!("ligne {index} : {op}"))
}

pub fn apt_repos() -> Panel {
    let mut panel = Panel::default();
    let sources = apt_sources();
    panel.metric("Sources", sources.len().to_string(), Level::Info);
    panel.metric(
        "Actives",
        sources.iter().filter(|source| source.enabled).count().to_string(),
        Level::Ok,
    );
    if !exec::exists("apt-get") {
        panel.error("apt-get absent : cette rubrique concerne les systèmes Debian/Ubuntu.");
    }
    let rows = sources
        .iter()
        .map(|source| {
            let mut row = Row::owned(vec![
                if source.enabled { "actif" } else { "désactivé" }.to_string(),
                source.kind.clone(),
                util::ellipsis(&source.uri, 52),
                format!("{} {}", source.suite, source.components).trim().to_string(),
                source.file.rsplit('/').next().unwrap_or("").to_string(),
            ]);
            row.bad = !source.enabled;
            row.actions = source_actions(source);
            row
        })
        .collect();
    panel.section(
        Section::new("Sources de paquets")
            .columns(&["État", "Type", "Dépôt", "Suite / composants", "Fichier"])
            .rows(rows)
            .action(
                Cmd::new("Mettre à jour les index", "apt-get", &["update"])
                    .privileged()
                    .confirm("Lancer apt-get update ?"),
            ),
    );
    panel
}

// ---------------------------------------------------------------------------
// Paquets avancés
// ---------------------------------------------------------------------------

/// Une opération du journal dpkg.
#[derive(Debug, Clone, PartialEq)]
pub struct DpkgEvent {
    pub when: String,
    pub action: String,
    pub package: String,
    pub detail: String,
}

/// Analyse `/var/log/dpkg.log` : installations, mises à jour et suppressions.
pub fn parse_dpkg_log(text: &str) -> Vec<DpkgEvent> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 {
                return None;
            }
            let action = fields[2];
            if !matches!(action, "install" | "upgrade" | "remove" | "purge") {
                return None;
            }
            let detail = match action {
                "upgrade" => format!("{} → {}", fields[4], fields.get(5).unwrap_or(&"")),
                _ => fields.get(5).unwrap_or(&fields[4]).to_string(),
            };
            Some(DpkgEvent {
                when: format!("{} {}", fields[0], fields[1]),
                action: action.to_string(),
                package: fields[3].to_string(),
                detail,
            })
        })
        .collect()
}

/// Version Debian valide.
pub fn valid_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.chars().next().is_some_and(|ch| ch.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ".+~:-".contains(ch))
}

fn build_hold(values: &[String]) -> Result<Cmd, String> {
    let name = values.first().map(|value| value.trim()).unwrap_or("");
    if !util::valid_package_name(name) {
        return Err(format!("nom de paquet refusé : {name}"));
    }
    Ok(Cmd::new("Verrouillage", "apt-mark", &["hold", name]).privileged())
}

fn build_downgrade(values: &[String]) -> Result<Cmd, String> {
    let [name, version] = values else {
        return Err("paquet et version attendus".to_string());
    };
    let (name, version) = (name.trim(), version.trim());
    if !util::valid_package_name(name) {
        return Err(format!("nom de paquet refusé : {name}"));
    }
    if !valid_version(version) {
        return Err(format!("version refusée : {version}"));
    }
    Ok(Cmd::new(
        "Rétrogradation",
        "apt-get",
        &["install", "-y", "--allow-downgrades", &format!("{name}={version}")],
    )
    .privileged()
    .confirm(&format!("Installer {name} en version {version} ?")))
}

pub fn adv_packages() -> Panel {
    let mut panel = Panel::default();
    if !exec::exists("apt-mark") {
        panel.error("Verrouillage, rétrogradation et historique dpkg reposent sur APT : cette rubrique est réservée aux systèmes Debian/Ubuntu.");
        return panel;
    }
    let held = run_lossy("apt-mark", &["showhold"]);
    let held: Vec<&str> = held.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    panel.metric("Paquets verrouillés", held.len().to_string(), Level::Info);
    let rows = held
        .iter()
        .filter(|name| util::valid_package_name(name))
        .map(|name| {
            Row::new(&[name]).action(Cmd::new("Déverrouiller", "apt-mark", &["unhold", name]).privileged())
        })
        .collect();
    panel.section(
        Section::new("Verrouillage de version (apt-mark hold)")
            .columns(&["Paquet"])
            .rows(rows)
            .form(Form {
                fields: vec![Field::new("Paquet", "nom exact")],
                submit: "Verrouiller".to_string(),
                build: build_hold,
            }),
    );
    panel.section(
        Section::new("Rétrograder un paquet")
            .note("Utilisez `apt-cache policy <paquet>` pour connaître les versions disponibles.")
            .form(Form {
                fields: vec![Field::new("Paquet", "nom exact"), Field::new("Version", "ex. 1.2.3-1")],
                submit: "Rétrograder".to_string(),
                build: build_downgrade,
            }),
    );
    let log = read("/var/log/dpkg.log");
    if log.is_empty() {
        panel.error("/var/log/dpkg.log illisible ou vide.");
    }
    let events = parse_dpkg_log(&log);
    let rows = events
        .iter()
        .rev()
        .take(60)
        .map(|event| {
            Row::owned(vec![
                event.when.clone(),
                event.action.clone(),
                event.package.clone(),
                event.detail.clone(),
            ])
        })
        .collect();
    panel.section(
        Section::new("Historique dpkg (60 derniers événements)")
            .columns(&["Date", "Action", "Paquet", "Version"])
            .rows(rows),
    );
    panel
}

// ---------------------------------------------------------------------------
// Runtimes et langages
// ---------------------------------------------------------------------------

pub const RUNTIME_TOOLS: [(&str, &str, &[&str]); 8] = [
    ("Python", "python3", &["--version"]),
    ("GCC", "gcc", &["--version"]),
    ("Java", "java", &["-version"]),
    ("Node.js", "node", &["--version"]),
    ("Go", "go", &["version"]),
    ("Rust", "rustc", &["--version"]),
    ("Ruby", "ruby", &["--version"]),
    ("PHP", "php", &["--version"]),
];

/// Première ligne utile d'une sortie de version.
pub fn first_version_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Analyse `update-alternatives --get-selections` : `(nom, mode, chemin)`.
pub fn parse_alternatives(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 {
                return None;
            }
            Some((fields[0].to_string(), fields[1].to_string(), fields[2..].join(" ")))
        })
        .collect()
}

pub fn runtimes() -> Panel {
    let mut panel = Panel::default();
    let mut rows = Vec::new();
    let mut found = 0;
    for (label, program, args) in RUNTIME_TOOLS {
        if !exec::exists(program) {
            rows.push(Row::new(&[label, "non installé", ""]).bad(false));
            continue;
        }
        found += 1;
        let version = first_version_line(&run_lossy(program, args));
        rows.push(Row::owned(vec![
            label.to_string(),
            util::ellipsis(&version, 70),
            exec::which(program).map(|p| p.display().to_string()).unwrap_or_default(),
        ]));
    }
    panel.metric("Runtimes détectés", format!("{found} / {}", RUNTIME_TOOLS.len()), Level::Info);
    panel.section(
        Section::new("Langages")
            .columns(&["Langage", "Version", "Chemin"])
            .rows(rows),
    );
    let alternatives = parse_alternatives(&run_lossy("update-alternatives", &["--get-selections"]));
    let rows = alternatives
        .iter()
        .map(|(name, mode, path)| Row::new(&[name, mode, path]))
        .collect();
    panel.section(
        Section::new("Alternatives système (update-alternatives)")
            .columns(&["Nom", "Mode", "Choix actuel"])
            .rows(rows)
            .form(Form {
                fields: vec![
                    Field::new("Alternative", "java, editor, cc"),
                    Field::new("Chemin du binaire", "/usr/bin/…"),
                ],
                submit: "Définir l'alternative".to_string(),
                build: build_alternative,
            }),
    );
    panel
}

/// Formulaire « définir une alternative » : `update-alternatives --set`.
fn build_alternative(values: &[String]) -> Result<Cmd, String> {
    let name = values.first().map(|value| value.trim()).unwrap_or("");
    let path = values.get(1).map(|value| value.trim()).unwrap_or("");
    if !safe_token(name) {
        return Err("nom d'alternative invalide (ex. java, editor, cc)".to_string());
    }
    if !safe_abs_path(path) {
        return Err("chemin absolu attendu (ex. /usr/bin/python3)".to_string());
    }
    Ok(
        Cmd::new("Définir l'alternative", "update-alternatives", &["--set", name, path])
            .privileged()
            .confirm(&format!("Définir l'alternative {name} sur {path} ?")),
    )
}

// ---------------------------------------------------------------------------
// Conteneurs
// ---------------------------------------------------------------------------

/// Moteur de conteneurs disponible.
pub fn engine() -> Option<&'static str> {
    ["docker", "podman"].into_iter().find(|name| exec::exists(name))
}

/// Un identifiant ou un nom de conteneur/image/volume accepté.
pub fn valid_container_ref(text: &str) -> bool {
    safe_token(text) && !text.contains("..")
}

pub fn containers() -> Panel {
    let mut panel = Panel::default();
    match engine() {
        None => panel.error("Ni Docker ni Podman n'est installé."),
        Some(engine) => {
            let text = run_text(
                engine,
                &["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.State}}\t{{.Status}}"],
            );
            match text {
                Err(error) => panel.error(format!("{engine} : {error}")),
                Ok(text) => {
                    let list = tsv(&text);
                    let running = list.iter().filter(|c| c.get(3).is_some_and(|s| s == "running")).count();
                    panel.metric("Moteur", engine, Level::Info);
                    panel.metric("Conteneurs", list.len().to_string(), Level::Info);
                    panel.metric("En cours", running.to_string(), Level::Ok);
                    let rows = list
                        .into_iter()
                        .filter(|cells| cells.len() >= 5 && valid_container_ref(&cells[0]))
                        .map(|cells| {
                            let id = cells[0].clone();
                            let up = cells[3] == "running";
                            let mut row = Row::owned(vec![
                                cells[1].clone(),
                                cells[2].clone(),
                                cells[3].clone(),
                                cells[4].clone(),
                            ]);
                            if up {
                                row = row
                                    .action(Cmd::new("Arrêter", engine, &["stop", &id]))
                                    .action(Cmd::new("Redémarrer", engine, &["restart", &id]));
                            } else {
                                row = row.action(Cmd::new("Démarrer", engine, &["start", &id]));
                            }
                            row.action(
                                Cmd::new("Supprimer", engine, &["rm", &id])
                                    .confirm(&format!("Supprimer le conteneur {} ?", cells[1])),
                            )
                        })
                        .collect();
                    panel.section(
                        Section::new("Conteneurs")
                            .columns(&["Nom", "Image", "État", "Détail"])
                            .rows(rows),
                    );
                }
            }
        }
    }
    if exec::exists("lxc") {
        let text = run_lossy("lxc", &["list", "--format", "csv", "-c", "ns4"]);
        let rows = text
            .lines()
            .filter_map(|line| {
                let cells: Vec<&str> = line.split(',').collect();
                (cells.len() >= 2).then(|| Row::new(&cells[..cells.len().min(3)]))
            })
            .collect();
        panel.section(Section::new("Instances LXD").columns(&["Nom", "État", "IPv4"]).rows(rows));
    }
    panel
}

pub fn containers_advanced() -> Panel {
    let mut panel = Panel::default();
    let Some(engine) = engine() else {
        panel.error("Ni Docker ni Podman n'est installé.");
        return panel;
    };
    panel.metric("Moteur", engine, Level::Info);

    let images = tsv(&run_lossy(
        engine,
        &["images", "--format", "{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Size}}"],
    ));
    let rows = images
        .into_iter()
        .filter(|cells| cells.len() >= 3 && valid_container_ref(&cells[0]))
        .map(|cells| {
            let id = cells[0].clone();
            Row::owned(vec![cells[1].clone(), cells[2].clone(), id.clone()]).action(
                Cmd::new("Supprimer", engine, &["rmi", &id])
                    .confirm(&format!("Supprimer l'image {} ?", cells[1])),
            )
        })
        .collect::<Vec<_>>();
    panel.metric("Images", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Images")
            .columns(&["Image", "Taille", "Identifiant"])
            .rows(rows)
            .action(
                Cmd::new("Purger les images orphelines", engine, &["image", "prune", "-f"])
                    .confirm("Supprimer les images sans étiquette ?"),
            ),
    );

    let volumes = tsv(&run_lossy(engine, &["volume", "ls", "--format", "{{.Name}}\t{{.Driver}}"]));
    let rows = volumes
        .into_iter()
        .filter(|cells| cells.len() >= 2 && valid_container_ref(&cells[0]))
        .map(|cells| {
            let name = cells[0].clone();
            Row::owned(vec![cells[0].clone(), cells[1].clone()]).action(
                Cmd::new("Supprimer", engine, &["volume", "rm", &name])
                    .confirm(&format!("Supprimer le volume {name} et ses données ?")),
            )
        })
        .collect::<Vec<_>>();
    panel.metric("Volumes", rows.len().to_string(), Level::Info);
    panel.section(
        Section::new("Volumes")
            .columns(&["Nom", "Pilote"])
            .rows(rows)
            .action(
                Cmd::new("Purger les volumes inutilisés", engine, &["volume", "prune", "-f"])
                    .confirm("Supprimer les volumes non utilisés (données perdues) ?"),
            ),
    );

    let networks = tsv(&run_lossy(engine, &["network", "ls", "--format", "{{.ID}}\t{{.Name}}\t{{.Driver}}"]));
    let rows = networks
        .into_iter()
        .filter(|cells| cells.len() >= 3 && valid_container_ref(&cells[0]))
        .map(|cells| {
            let builtin = matches!(cells[1].as_str(), "bridge" | "host" | "none" | "podman");
            let mut row = Row::owned(vec![cells[1].clone(), cells[2].clone(), cells[0].clone()]);
            if !builtin {
                row = row.action(
                    Cmd::new("Supprimer", engine, &["network", "rm", &cells[0]])
                        .confirm(&format!("Supprimer le réseau {} ?", cells[1])),
                );
            }
            row
        })
        .collect();
    panel.section(
        Section::new("Réseaux")
            .columns(&["Nom", "Pilote", "Identifiant"])
            .rows(rows)
            .action(
                Cmd::new("Nettoyage complet", engine, &["system", "prune", "-f"])
                    .confirm("Supprimer conteneurs arrêtés, réseaux et images orphelines ?"),
            ),
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_source_lines() {
        let line = "deb [arch=amd64 signed-by=/k.gpg] http://archive.ubuntu.com/ubuntu noble main universe";
        let source = parse_source_line("/etc/apt/sources.list", 3, line).unwrap();
        assert!(source.enabled);
        assert_eq!(source.uri, "http://archive.ubuntu.com/ubuntu");
        assert_eq!(source.suite, "noble");
        assert_eq!(source.components, "main universe");
        assert_eq!(source.line, 3);

        let disabled = parse_source_line("f", 1, "# deb http://x.org/apt stable main").unwrap();
        assert!(!disabled.enabled);
        assert!(parse_source_line("f", 1, "# simple commentaire").is_none());
        assert!(parse_source_line("f", 1, "").is_none());
        assert!(parse_source_line("f", 1, "deb nonsense").is_none());
    }

    #[test]
    fn parses_deb822() {
        let text = "Types: deb\nURIs: http://fr.archive.ubuntu.com/ubuntu\nSuites: noble noble-updates\nComponents: main\n\nTypes: deb\nURIs: https://ppa.example/x\nSuites: noble\nComponents: main\nEnabled: no\n";
        let sources = parse_deb822("/etc/apt/sources.list.d/a.sources", text);
        assert_eq!(sources.len(), 2);
        assert!(sources[0].enabled);
        assert!(!sources[1].enabled);
        assert_eq!(sources[0].suite, "noble noble-updates");
    }

    #[test]
    fn source_actions_are_limited_to_apt_directory() {
        let mut source = parse_source_line("/etc/apt/sources.list", 2, "deb http://x.org/apt stable main").unwrap();
        let actions = source_actions(&source);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|cmd| cmd.local.is_some()));
        assert_eq!(actions[0].args[..3], ["/etc/apt/sources.list", "2", "deb http://x.org/apt stable main"]);
        source.file = "/etc/passwd".to_string();
        assert!(source_actions(&source).is_empty());
        source.file = "/etc/apt/../shadow.list".to_string();
        assert!(source_actions(&source).is_empty());
        assert!(edit_source_line(&["/etc/passwd".into(), "1".into(), "x".into(), "delete".into()]).is_err());
    }

    #[test]
    fn refuses_to_edit_a_line_that_moved() {
        let dir = std::env::temp_dir().join(format!("um-apt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.list");
        std::fs::write(&file, "deb http://a/ s main\ndeb http://b/ s main\n").unwrap();
        let path = file.to_string_lossy().to_string();
        // Chemin hors /etc/apt : refusé avant toute lecture.
        let error = edit_source_line(&[path, "1".into(), "deb http://a/ s main".into(), "delete".into()]).unwrap_err();
        assert!(error.contains("refusé"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deb822_toggle_depends_on_state() {
        let text = "Types: deb\nURIs: http://x.org/apt\nSuites: stable\nComponents: main\nEnabled: no\n";
        let off = parse_deb822("/etc/apt/sources.list.d/a.sources", text);
        let actions = source_actions(&off[0]);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Activer");
        assert_eq!(actions[0].program, "sed");
        let on = parse_deb822("/etc/apt/sources.list.d/a.sources", &text.replace("Enabled: no\n", ""));
        assert_eq!(source_actions(&on[0])[0].label, "Désactiver");
        let mut moved = on[0].clone();
        moved.file = "/etc/apt/sources.list.d/a.sources.disabled".to_string();
        moved.enabled = false;
        assert_eq!(source_actions(&moved)[0].program, "mv");
    }

    #[test]
    fn parses_dpkg_log() {
        let text = "2026-11-02 10:00:00 install foo:amd64 <none> 1.0\n2026-11-02 10:01:00 upgrade bar:amd64 1.0 1.1\n2026-11-02 10:02:00 status installed foo:amd64 1.0\n2026-11-02 10:03:00 remove baz:amd64 2.0 2.0\n";
        let events = parse_dpkg_log(text);
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].detail, "1.0 → 1.1");
        assert_eq!(events[2].action, "remove");
    }

    #[test]
    fn validates_versions_and_downgrade_forms() {
        assert!(valid_version("1:2.3.4-1ubuntu1~22.04"));
        assert!(!valid_version("1.0; rm"));
        assert!(!valid_version("-1"));
        assert!(build_downgrade(&["curl".into(), "7.81.0-1".into()]).is_ok());
        assert!(build_downgrade(&["curl;x".into(), "1.0".into()]).is_err());
        assert!(build_downgrade(&["curl".into(), "1 0".into()]).is_err());
        assert!(build_hold(&["Bad Name".into()]).is_err());
        assert!(build_hold(&["nginx".into()]).is_ok());
    }

    #[test]
    fn parses_alternatives_and_versions() {
        let alts = parse_alternatives("editor auto /usr/bin/nano\npager manual /usr/bin/less\n");
        assert_eq!(alts.len(), 2);
        assert_eq!(alts[1].1, "manual");
        assert_eq!(first_version_line("\n\nPython 3.12.3\nautre"), "Python 3.12.3");
    }

    #[test]
    fn alternative_form_requires_safe_name_and_absolute_path() {
        assert!(build_alternative(&["java".into(), "/usr/bin/java".into()]).is_ok());
        assert!(build_alternative(&["editor".into(), "/usr/bin/vim".into()]).is_ok());
        assert!(build_alternative(&["java".into(), "relatif".into()]).is_err());
        assert!(build_alternative(&["a;b".into(), "/usr/bin/java".into()]).is_err());
        assert!(build_alternative(&[]).is_err());
    }

    #[test]
    fn container_refs_are_validated() {
        assert!(valid_container_ref("a1b2c3d4e5f6"));
        assert!(valid_container_ref("nginx:latest"));
        assert!(!valid_container_ref("--all"));
        assert!(!valid_container_ref("a;b"));
        assert_eq!(tsv("a\tb\n\nc\td\n").len(), 2);
    }
}
