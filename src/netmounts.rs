//! Montages réseau : NFS et CIFS déclarés dans `fstab`, état des montages
//! actifs, montage/démontage et test du serveur.

use crate::exec;
use crate::netadv;
use serde::Serialize;

/// Un montage réseau déclaré dans `/etc/fstab`.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteMount {
    pub server: String,
    pub export: String,
    pub mount: String,
    pub fstype: String,
    pub options: String,
    pub auto_mount: bool,
    pub mounted: bool,
}

/// Rapport des montages réseau.
#[derive(Debug, Clone, Default)]
pub struct NetMountReport {
    pub entries: Vec<RemoteMount>,
    pub errors: Vec<String>,
    pub has_showmount: bool,
}

impl NetMountReport {
    pub fn mounted(&self) -> usize {
        self.entries.iter().filter(|entry| entry.mounted).count()
    }
}

/// Types de systèmes de fichiers réseau pris en charge.
pub fn is_network_fs(fstype: &str) -> bool {
    matches!(fstype, "nfs" | "nfs4" | "cifs" | "smbfs")
}

/// Sépare une source en (serveur, export/partage).
/// CIFS utilise `//serveur/partage` (pas de deux-points), NFS `serveur:/export`.
pub fn split_source(source: &str, fstype: &str) -> (String, String) {
    if fstype.starts_with("cifs") || fstype.starts_with("smb") {
        if let Some(rest) = source.strip_prefix("//") {
            return match rest.split_once('/') {
                Some((host, share)) => (format!("//{host}"), share.to_string()),
                None => (source.to_string(), String::new()),
            };
        }
    }
    match source.split_once(':') {
        Some((server, export)) => (server.to_string(), export.to_string()),
        None => (source.to_string(), String::new()),
    }
}

/// Analyse `/etc/fstab` pour les montages réseau.
pub fn parse_fstab(text: &str) -> Vec<RemoteMount> {
    let mut mounts: Vec<RemoteMount> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || !is_network_fs(fields[2]) {
            continue;
        }
        let (server, export) = split_source(fields[0], fields[2]);
        let options = fields[3].to_string();
        let option_list: Vec<&str> = options.split(',').collect();
        mounts.push(RemoteMount {
            server,
            export,
            mount: fields[1].to_string(),
            fstype: fields[2].to_string(),
            auto_mount: !option_list.contains(&"noauto") && !option_list.contains(&"comment=systemd.automount"),
            mounted: false,
            options,
        });
    }
    mounts
}

/// Marque les entrées effectivement montées d'après `/proc/mounts`.
pub fn mark_mounted(mounts: &mut [RemoteMount], proc_mounts: &str) {
    for mount in mounts.iter_mut() {
        mount.mounted = proc_mounts.lines().any(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            fields.len() >= 3
                && fields[1].replace("\\040", " ") == mount.mount
                && is_network_fs(fields[2])
        });
    }
}

/// Liste les montages réseau actifs absents de `fstab`.
pub fn active_mounts(proc_mounts: &str) -> Vec<(String, String, String)> {
    proc_mounts
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 || !is_network_fs(fields[2]) {
                return None;
            }
            Some((
                fields[0].to_string(),
                fields[1].replace("\\040", " "),
                fields[2].to_string(),
            ))
        })
        .collect()
}

/// Point de montage acceptable.
pub fn valid_mountpoint(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && path != "/"
        && !path.contains("..")
        && !path.ends_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '@'))
}

/// Monte un point déclaré dans `fstab`.
pub fn mount_path(path: &str) -> Result<String, String> {
    if !valid_mountpoint(path) {
        return Err(format!("point de montage refusé : {path}"));
    }
    exec::run_privileged("mount", &[path.to_string()])?;
    Ok(format!("monté : {path}"))
}

/// Démonte un point réseau.
pub fn unmount_path(path: &str) -> Result<String, String> {
    if !valid_mountpoint(path) {
        return Err(format!("point de montage refusé : {path}"));
    }
    exec::run_privileged("umount", &[path.to_string()])?;
    Ok(format!("démonté : {path}"))
}

/// Teste l'accessibilité du serveur (exports NFS, partages SMB).
pub fn probe(server: &str, fstype: &str) -> Result<String, String> {
    if !netadv::valid_host(server) {
        return Err(format!("serveur refusé : {server}"));
    }
    if fstype.starts_with("nfs") {
        let captured = exec::run_with_timeout(
            "showmount",
            &["-e", server],
            std::time::Duration::from_secs(20),
        )
        .map_err(|error| format!("showmount : {}", error.0))?;
        if captured.stdout.trim().is_empty() {
            return Err(captured.error_text());
        }
        return Ok(captured.text());
    }
    let captured = exec::run_with_timeout(
        "smbclient",
        &["-L", server, "-N"],
        std::time::Duration::from_secs(20),
    )
    .map_err(|error| format!("smbclient : {}", error.0))?;
    if captured.stdout.trim().is_empty() && !captured.ok {
        return Err(captured.error_text());
    }
    Ok(format!("{}{}", captured.stdout, captured.stderr))
}

/// Collecte l'état des montages réseau.
pub fn report() -> NetMountReport {
    let mut report = NetMountReport {
        has_showmount: exec::exists("showmount"),
        ..NetMountReport::default()
    };
    let fstab = std::fs::read_to_string("/etc/fstab").unwrap_or_default();
    let proc_mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut entries = parse_fstab(&fstab);
    mark_mounted(&mut entries, &proc_mounts);
    let known: Vec<String> = entries.iter().map(|entry| entry.mount.clone()).collect();
    for (source, target, fstype) in active_mounts(&proc_mounts) {
        if known.iter().any(|mount| mount == &target) {
            continue;
        }
        let (server, export) = split_source(&source, &fstype);
        entries.push(RemoteMount {
            server,
            export,
            mount: target,
            fstype,
            options: "monté hors fstab".to_string(),
            auto_mount: false,
            mounted: true,
        });
    }
    entries.sort_by(|a, b| a.mount.cmp(&b.mount));
    report.entries = entries;
    report
}

/// Caractères acceptés dans les options de montage.
pub fn valid_mount_options(text: &str) -> bool {
    let value = text.trim();
    value.len() <= 300
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ',' | '=' | '.' | '_' | '-' | ':' | '@' | '+')
        })
}

/// Déduit le type et la source normalisée depuis `//hôte/partage`,
/// `smb://hôte/partage` ou `hôte:/export`.
pub fn classify_remote(source: &str) -> Result<(&'static str, String), String> {
    let source = source.trim();
    if let Some(rest) = source
        .strip_prefix("smb://")
        .or_else(|| source.strip_prefix("//"))
    {
        let mut parts = rest.splitn(2, '/');
        let host = parts.next().unwrap_or("");
        let share = parts.next().unwrap_or("").trim_matches('/');
        if !netadv::valid_host(host) || share.is_empty() {
            return Err(format!("partage SMB refusé : {source}"));
        }
        return Ok(("cifs", format!("//{host}/{share}")));
    }
    let Some((host, export)) = source.split_once(':') else {
        return Err(format!(
            "source refusée : {source} (attendu //serveur/partage ou serveur:/export)"
        ));
    };
    if !netadv::valid_host(host) || !export.starts_with('/') || export.contains("..") {
        return Err(format!("export NFS refusé : {source}"));
    }
    Ok(("nfs", format!("{host}:{export}")))
}

/// Monte un partage SMB ou NFS dans un point de montage (créé si absent).
/// Le montage n'est pas ajouté à `fstab` : il ne survit pas au redémarrage.
pub fn mount_remote(source: &str, mountpoint: &str, options: &str) -> Result<String, String> {
    let (fstype, source) = classify_remote(source)?;
    if !valid_mountpoint(mountpoint) {
        return Err(format!("point de montage refusé : {mountpoint}"));
    }
    if !valid_mount_options(options) {
        return Err("options de montage refusées".to_string());
    }
    exec::run_privileged(
        "install",
        &[
            "-d".to_string(),
            "-m".to_string(),
            "0755".to_string(),
            mountpoint.to_string(),
        ],
    )
    .map_err(|error| format!("création de {mountpoint} : {error}"))?;
    let mut args = vec!["-t".to_string(), fstype.to_string()];
    let options = options.trim();
    if !options.is_empty() {
        args.push("-o".to_string());
        args.push(options.to_string());
    }
    args.push(source.clone());
    args.push(mountpoint.to_string());
    exec::run_privileged("mount", &args)?;
    Ok(format!("{source} monté sur {mountpoint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nfs_and_cifs_entries() {
        let text = "# commentaire\nserveur:/export /mnt/nfs nfs4 defaults,_netdev 0 0\n//nas/partage /mnt/nas cifs credentials=/etc/cifs,noauto 0 0\n/dev/sda1 / ext4 defaults 0 1\n";
        let mounts = parse_fstab(text);
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].server, "serveur");
        assert_eq!(mounts[0].export, "/export");
        assert!(mounts[0].auto_mount);
        assert_eq!(mounts[1].server, "//nas");
        assert_eq!(mounts[1].export, "partage");
        assert!(!mounts[1].auto_mount);
    }

    #[test]
    fn splits_nfs_and_unc_sources() {
        assert_eq!(
            split_source("srv:/data", "nfs4"),
            ("srv".to_string(), "/data".to_string())
        );
        assert_eq!(
            split_source("//nas/Documents", "cifs"),
            ("//nas".to_string(), "Documents".to_string())
        );
        assert_eq!(
            split_source("//nas", "cifs"),
            ("//nas".to_string(), String::new())
        );
    }

    #[test]
    fn marks_entries_present_in_proc_mounts() {
        let mut mounts = parse_fstab("serveur:/export /mnt/nfs nfs4 defaults 0 0\n");
        mark_mounted(
            &mut mounts,
            "serveur:/export /mnt/nfs nfs4 rw,relatime 0 0\n",
        );
        assert!(mounts[0].mounted);
    }

    #[test]
    fn lists_active_network_mounts() {
        let text = "//nas/partage /mnt/nas cifs rw 0 0\n/dev/sda1 / ext4 rw 0 0\nsrv:/data /mnt/donnees\\040partagees nfs4 rw 0 0\n";
        let active = active_mounts(text);
        assert_eq!(active.len(), 2);
        assert_eq!(active[1].1, "/mnt/donnees partagees");
    }

    #[test]
    fn validates_mount_points() {
        assert!(valid_mountpoint("/mnt/nas"));
        assert!(!valid_mountpoint("/"));
        assert!(!valid_mountpoint("mnt/nas"));
        assert!(!valid_mountpoint("/mnt/../etc"));
        assert!(!valid_mountpoint("/mnt/nas; reboot"));
        assert!(mount_path("/mnt/../etc").is_err());
        assert!(unmount_path("relatif").is_err());
    }

    #[test]
    fn classifies_remote_sources() {
        assert_eq!(
            classify_remote("//nas/docs").unwrap(),
            ("cifs", "//nas/docs".to_string())
        );
        assert_eq!(
            classify_remote("smb://nas/docs").unwrap(),
            ("cifs", "//nas/docs".to_string())
        );
        assert_eq!(
            classify_remote("srv:/export").unwrap(),
            ("nfs", "srv:/export".to_string())
        );
        assert!(classify_remote("nas/docs").is_err());
        assert!(classify_remote("//nas").is_err());
        assert!(classify_remote("srv:export").is_err());
        assert!(classify_remote("srv:/../etc").is_err());
    }

    #[test]
    fn validates_mount_options() {
        assert!(valid_mount_options("username=u,vers=3.0"));
        assert!(valid_mount_options(""));
        assert!(!valid_mount_options("a b"));
        assert!(!valid_mount_options("x;reboot"));
    }

    #[test]
    fn refuses_remote_mounts_before_running() {
        assert!(mount_remote("//nas/docs", "/mnt/../etc", "").is_err());
        assert!(mount_remote("//nas/docs", "relatif", "").is_err());
        assert!(mount_remote("nas/docs", "/mnt/x", "").is_err());
        assert!(mount_remote("//nas/docs", "/mnt/x", "a b").is_err());
    }

    #[test]
    fn probe_rejects_bad_servers() {
        assert!(probe("serveur; reboot", "nfs").is_err());
        assert!(probe("", "cifs").is_err());
    }
}
