//! Paquets orphelins : dépendances devenues inutiles et configurations
//! résiduelles (`rc`) laissées par des paquets supprimés.

use crate::exec;
use crate::util;
use serde::Serialize;

/// Un paquet supprimable automatiquement.
#[derive(Debug, Clone, Serialize)]
pub struct Orphan {
    pub name: String,
    pub version: String,
}

/// Un paquet supprimé dont la configuration subsiste.
#[derive(Debug, Clone, Serialize)]
pub struct Residual {
    pub name: String,
    pub size_kb: u64,
}

/// Rapport des paquets orphelins.
#[derive(Debug, Clone, Default)]
pub struct OrphanReport {
    pub autoremovable: Vec<Orphan>,
    pub residual: Vec<Residual>,
    pub errors: Vec<String>,
}

impl OrphanReport {
    pub fn residual_bytes(&self) -> u64 {
        self.residual.iter().map(|item| item.size_kb).sum::<u64>() * 1024
    }
}

/// Analyse `apt-get -s autoremove`.
pub fn parse_autoremove(text: &str) -> Vec<Orphan> {
    let mut orphans: Vec<Orphan> = Vec::new();
    let mut in_removal_list = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("The following packages will be REMOVED") {
            in_removal_list = true;
            continue;
        }
        if in_removal_list {
            if trimmed.is_empty() || trimmed.starts_with("0 upgraded") {
                in_removal_list = false;
            } else {
                for name in trimmed.split_whitespace() {
                    if util::valid_package_name(name)
                        && !orphans.iter().any(|orphan| orphan.name == name)
                    {
                        orphans.push(Orphan {
                            name: name.to_string(),
                            version: String::new(),
                        });
                    }
                }
                continue;
            }
        }
        // `Remv nom [version]`
        let mut fields = trimmed.split_whitespace();
        if fields.next() != Some("Remv") {
            continue;
        }
        let Some(name) = fields.next() else {
            continue;
        };
        if !util::valid_package_name(name) {
            continue;
        }
        let version = fields
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        if let Some(orphan) = orphans.iter_mut().find(|orphan| orphan.name == name) {
            if orphan.version.is_empty() {
                orphan.version = version;
            }
        } else {
            orphans.push(Orphan {
                name: name.to_string(),
                version,
            });
        }
    }
    orphans.sort_by(|a, b| a.name.cmp(&b.name));
    orphans
}

/// Analyse `dpkg-query -W -f='${db:Status-Abbrev}\t${Package}\t${Installed-Size}\n'`.
pub fn parse_residual(text: &str) -> Vec<Residual> {
    let mut residual: Vec<Residual> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 2 {
            continue;
        }
        let status = fields[0].trim();
        let name = fields[1].trim();
        // `rc` = removed, config files restants.
        if status != "rc" || !util::valid_package_name(name) {
            continue;
        }
        let size_kb = fields
            .get(2)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        residual.push(Residual {
            name: name.to_string(),
            size_kb,
        });
    }
    residual.sort_by(|a, b| b.size_kb.cmp(&a.size_kb).then_with(|| a.name.cmp(&b.name)));
    residual
}

/// Liste les configurations résiduelles.
pub fn residual() -> Vec<Residual> {
    exec::run(
        "dpkg-query",
        &[
            "-W",
            "-f",
            "${db:Status-Abbrev}\t${Package}\t${Installed-Size}\n",
        ],
    )
    .map(|captured| parse_residual(&captured.stdout))
    .unwrap_or_default()
}

/// Simule `apt-get autoremove` pour lister les paquets concernés.
pub fn autoremovable() -> Result<Vec<Orphan>, String> {
    let captured = exec::run_with_timeout(
        "apt-get",
        &["-s", "autoremove"],
        std::time::Duration::from_secs(60),
    )
    .map_err(|error| error.0)?;
    if captured.stdout.trim().is_empty() && !captured.ok {
        return Err(captured.error_text());
    }
    Ok(parse_autoremove(&captured.stdout))
}

/// Supprime les dépendances devenues inutiles.
pub fn autoremove() -> Result<String, String> {
    exec::run_privileged(
        "apt-get",
        &[
            "autoremove".to_string(),
            "--purge".to_string(),
            "-y".to_string(),
        ],
    )?;
    Ok("dépendances inutiles supprimées".to_string())
}

/// Purge les configurations résiduelles des paquets indiqués.
pub fn purge(names: &[String]) -> Result<String, String> {
    if names.is_empty() {
        return Err("aucun paquet à purger".to_string());
    }
    if names.len() > 200 {
        return Err("trop de paquets en une fois (200 maximum)".to_string());
    }
    for name in names {
        if !util::valid_package_name(name) {
            return Err(format!("nom de paquet refusé : {name}"));
        }
    }
    let mut args = vec!["purge".to_string(), "-y".to_string()];
    args.extend(names.iter().cloned());
    exec::run_privileged("apt-get", &args)?;
    Ok(format!("{} configuration(s) purgée(s)", names.len()))
}

/// Collecte le rapport complet.
pub fn report() -> OrphanReport {
    let mut report = OrphanReport::default();
    match autoremovable() {
        Ok(orphans) => report.autoremovable = orphans,
        Err(error) => report.errors.push(format!("autoremove : {error}")),
    }
    report.residual = residual();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simulated_autoremove() {
        let text = "Reading package lists...\nThe following packages will be REMOVED:\n  libfoo libbar\n0 upgraded, 0 newly installed, 2 to remove and 0 not upgraded.\nRemv libfoo [1.2-1]\nRemv libbar [3.0]\n";
        let orphans = parse_autoremove(text);
        assert_eq!(orphans.len(), 2);
        assert_eq!(orphans[0].name, "libbar");
        assert_eq!(orphans[0].version, "3.0");
        assert_eq!(orphans[1].name, "libfoo");
        assert_eq!(orphans[1].version, "1.2-1");
    }

    #[test]
    fn handles_empty_autoremove() {
        assert!(parse_autoremove("0 upgraded, 0 newly installed, 0 to remove.\n").is_empty());
    }

    #[test]
    fn parses_residual_configs() {
        let text = "ii\tcurl\t100\nrc\told-tool\t64\nrc\tanother\t32\niU\tpartial\t10\n";
        let residual = parse_residual(text);
        assert_eq!(residual.len(), 2);
        assert_eq!(residual[0].name, "old-tool");
        assert_eq!(residual[0].size_kb, 64);
        assert_eq!(residual[1].name, "another");
    }

    #[test]
    fn rejects_invalid_purge_names() {
        assert!(purge(&[]).is_err());
        assert!(purge(&["ok;reboot".to_string()]).is_err());
        assert!(purge(&["../etc/passwd".to_string()]).is_err());
    }

    #[test]
    fn sums_residual_size() {
        let report = OrphanReport {
            residual: vec![
                Residual {
                    name: "a".to_string(),
                    size_kb: 1024,
                },
                Residual {
                    name: "b".to_string(),
                    size_kb: 512,
                },
            ],
            ..OrphanReport::default()
        };
        assert_eq!(report.residual_bytes(), 1536 * 1024);
    }
}
