//! Rapports : collecte d'un instantané système, exports TXT, CSV, JSON et PDF.

use crate::cleanup;
use crate::disk::{self, Memory, Mount};
use crate::distro::{self, Capabilities};
use crate::inventory::Inventory;
use crate::journal::{self, Entry};
use crate::pkg::PackageManager;
use crate::security::{self, Audit};
use std::io::Write;
use std::path::Path;

/// Instantané complet utilisé par les exports.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub generated: String,
    pub distro: String,
    pub kernel: String,
    pub manager: String,
    pub packages: usize,
    pub snap: usize,
    pub flatpak: usize,
    pub manual: usize,
    pub updates: usize,
    pub mounts: Vec<Mount>,
    pub memory: Memory,
    pub audit_score: u32,
    pub findings: Vec<(String, String, String)>,
    pub recent: Vec<Entry>,
}

impl Report {
    /// Collecte un rapport complet (lecture seule).
    pub fn collect() -> Self {
        let capabilities = Capabilities::probe();
        let os = distro::current();
        let inventory = Inventory::load(&capabilities);
        let (native, snap, flatpak, manual) = inventory.counts();
        let updates = inventory
            .manager
            .and_then(|manager| manager.updates().ok())
            .map(|updates| updates.len())
            .unwrap_or(0);
        let audit = security::audit(&capabilities);
        let findings = audit
            .findings
            .iter()
            .map(|finding| {
                (
                    finding.level.label().to_string(),
                    finding.title.clone(),
                    finding.detail.clone(),
                )
            })
            .collect();
        Report {
            generated: crate::util::now_human(),
            distro: os.display_name(),
            kernel: distro::kernel_release(),
            manager: inventory
                .manager
                .map(|manager| manager.label().to_string())
                .unwrap_or_else(|| "inconnu".to_string()),
            packages: native,
            snap,
            flatpak,
            manual,
            updates,
            mounts: disk::mounts(),
            memory: disk::memory(),
            audit_score: audit.score(),
            findings,
            recent: journal::recent(20),
        }
    }

    /// Représentation texte, utilisée par l'export TXT et le PDF.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            "ORBISYS — RAPPORT SYSTÈME".to_string(),
            format!("Généré le {}", self.generated),
            String::new(),
            format!("Distribution : {}", self.distro),
            format!("Noyau        : {}", self.kernel),
            format!("Gestionnaire : {}", self.manager),
            String::new(),
            "LOGICIELS".to_string(),
            format!("  Paquets natifs      : {}", self.packages),
            format!("  Snap                : {}", self.snap),
            format!("  Flatpak             : {}", self.flatpak),
            format!("  Installés à la main : {}", self.manual),
            format!("  Mises à jour        : {}", self.updates),
            String::new(),
            "STOCKAGE".to_string(),
        ];
        for mount in &self.mounts {
            lines.push(format!(
                "  {:<24} {:>8} utilisés / {:>8} ({:.0} %)",
                mount.target,
                crate::util::human_bytes(mount.used),
                crate::util::human_bytes(mount.total),
                mount.percent
            ));
        }
        lines.push(String::new());
        lines.push("MÉMOIRE".to_string());
        lines.push(format!(
            "  RAM  : {} utilisés / {}",
            crate::util::human_bytes(self.memory.used()),
            crate::util::human_bytes(self.memory.total)
        ));
        lines.push(format!(
            "  Swap : {} utilisés / {}",
            crate::util::human_bytes(self.memory.swap_used()),
            crate::util::human_bytes(self.memory.swap_total)
        ));
        lines.push(String::new());
        lines.push(format!("SÉCURITÉ — score {}", self.audit_score));
        for (level, title, detail) in &self.findings {
            lines.push(format!("  [{level}] {title} : {detail}"));
        }
        lines.push(String::new());
        lines.push("DERNIÈRES ACTIONS".to_string());
        if self.recent.is_empty() {
            lines.push("  (aucune action enregistrée)".to_string());
        }
        for entry in &self.recent {
            lines.push(format!(
                "  {} {:<12} {:<24} {}",
                entry.timestamp,
                entry.action,
                crate::util::ellipsis(&entry.target, 24),
                if entry.ok { "ok" } else { "échec" }
            ));
        }
        lines
    }

    pub fn write_txt(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        for line in self.lines() {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    pub fn write_csv(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        writeln!(file, "categorie;cle;valeur")?;
        writeln!(file, "systeme;distribution;{}", self.distro)?;
        writeln!(file, "systeme;noyau;{}", self.kernel)?;
        writeln!(file, "logiciels;paquets;{}", self.packages)?;
        writeln!(file, "logiciels;snap;{}", self.snap)?;
        writeln!(file, "logiciels;flatpak;{}", self.flatpak)?;
        writeln!(file, "logiciels;manuel;{}", self.manual)?;
        writeln!(file, "logiciels;mises_a_jour;{}", self.updates)?;
        writeln!(file, "securite;score;{}", self.audit_score)?;
        for (level, title, detail) in &self.findings {
            writeln!(
                file,
                "securite;{};{}",
                title.replace(';', ","),
                format!("[{level}] {}", detail).replace(';', ",")
            )?;
        }
        for mount in &self.mounts {
            writeln!(
                file,
                "stockage;{};{:.0}% de {}",
                mount.target.replace(';', ","),
                mount.percent,
                crate::util::human_bytes(mount.total)
            )?;
        }
        Ok(())
    }

    pub fn write_json(&self, path: &Path) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "genere": self.generated,
            "distribution": self.distro,
            "noyau": self.kernel,
            "gestionnaire": self.manager,
            "logiciels": {
                "natifs": self.packages,
                "snap": self.snap,
                "flatpak": self.flatpak,
                "manuels": self.manual,
                "mises_a_jour": self.updates,
            },
            "memoire": {
                "total": self.memory.total,
                "utilisee": self.memory.used(),
                "swap_total": self.memory.swap_total,
                "swap_utilisee": self.memory.swap_used(),
            },
            "stockage": self.mounts.iter().map(|mount| serde_json::json!({
                "cible": mount.target,
                "source": mount.source,
                "type": mount.fstype,
                "total": mount.total,
                "utilise": mount.used,
                "disponible": mount.available,
                "pourcentage": mount.percent,
            })).collect::<Vec<_>>(),
            "securite": {
                "score": self.audit_score,
                "constatations": self.findings.iter().map(|(level, title, detail)| serde_json::json!({
                    "niveau": level,
                    "titre": title,
                    "detail": detail,
                })).collect::<Vec<_>>(),
            },
            "journal": self.recent,
        });
        std::fs::write(path, serde_json::to_string_pretty(&payload)?)
    }

    pub fn write_pdf(&self, path: &Path) -> std::io::Result<()> {
        write_pdf_text(&self.lines(), path)
    }
}

/// Écrit un PDF minimal (texte Helvetica, plusieurs pages, encodage WinAnsi).
pub fn write_pdf_text(lines: &[String], path: &Path) -> std::io::Result<()> {
    const LINES_PER_PAGE: usize = 52;
    let chunks: Vec<&[String]> = if lines.is_empty() {
        vec![&[]]
    } else {
        lines.chunks(LINES_PER_PAGE).collect()
    };
    let page_count = chunks.len();
    let total_objects = 3 + page_count * 2;
    let mut offsets: Vec<usize> = vec![0; total_objects + 1];
    let mut buffer: Vec<u8> = Vec::new();
    buffer.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let kids: String = (0..page_count)
        .map(|index| format!("{} 0 R", 4 + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    push_object(
        &mut buffer,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    push_object(
        &mut buffer,
        &mut offsets,
        2,
        format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>").as_bytes(),
    );
    push_object(
        &mut buffer,
        &mut offsets,
        3,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );

    for (index, chunk) in chunks.iter().enumerate() {
        let page_id = 4 + index * 2;
        let content_id = page_id + 1;
        let mut content: Vec<u8> = Vec::new();
        content.extend_from_slice(b"BT\n/F1 9.5 Tf\n13 TL\n50 800 Td\n");
        for line in chunk.iter() {
            push_escaped(&mut content, line);
            content.extend_from_slice(b" Tj\nT*\n");
        }
        content.extend_from_slice(b"ET\n");
        push_object(
            &mut buffer,
            &mut offsets,
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        let mut body: Vec<u8> = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        body.extend_from_slice(&content);
        body.extend_from_slice(b"endstream");
        push_object(&mut buffer, &mut offsets, content_id, &body);
    }

    let xref_offset = buffer.len();
    buffer.extend_from_slice(format!("xref\n0 {}\n", total_objects + 1).as_bytes());
    buffer.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        buffer.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buffer.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            total_objects + 1
        )
        .as_bytes(),
    );
    std::fs::write(path, buffer)
}

fn push_object(buffer: &mut Vec<u8>, offsets: &mut [usize], id: usize, body: &[u8]) {
    if id < offsets.len() {
        offsets[id] = buffer.len();
    }
    buffer.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    buffer.extend_from_slice(body);
    buffer.extend_from_slice(b"\nendobj\n");
}

fn push_escaped(buffer: &mut Vec<u8>, text: &str) {
    for character in text.chars() {
        match character {
            '(' => buffer.extend_from_slice(b"\\("),
            ')' => buffer.extend_from_slice(b"\\)"),
            '\\' => buffer.extend_from_slice(b"\\\\"),
            '—' | '–' => buffer.push(b'-'),
            '’' => buffer.push(b'\''),
            '«' | '»' => buffer.push(b'"'),
            '…' => buffer.extend_from_slice(b"..."),
            character if (character as u32) < 256 => buffer.push(character as u8),
            _ => buffer.push(b'?'),
        }
    }
}

/// Export de l'inventaire complet au format texte.
pub fn write_inventory_txt(inventory: &Inventory, path: &Path) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    let (native, snap, flatpak, manual) = inventory.counts();
    writeln!(
        file,
        "Inventaire du {} — {native} paquets, {snap} snap, {flatpak} flatpak, {manual} manuels",
        crate::util::now_human()
    )?;
    for package in &inventory.packages {
        writeln!(
            file,
            "{}\t{}\t{}\t{}",
            package.name,
            package.version,
            package.source.label(),
            crate::util::human_bytes(package.size_bytes)
        )?;
    }
    for app in &inventory.manual {
        writeln!(
            file,
            "{}\t—\t{}\t{}",
            app.name,
            app.kind.label(),
            app.path.display()
        )?;
    }
    Ok(())
}

/// Export de l'inventaire au format CSV.
pub fn write_inventory_csv(inventory: &Inventory, path: &Path) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "nom;version;source;taille_octets;description")?;
    for package in &inventory.packages {
        writeln!(
            file,
            "{};{};{};{};{}",
            package.name,
            package.version,
            package.source.label(),
            package.size_bytes,
            package.description.replace(';', ",")
        )?;
    }
    Ok(())
}

/// Export de l'inventaire au format JSON.
pub fn write_inventory_json(inventory: &Inventory, path: &Path) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "genere": crate::util::now_human(),
        "gestionnaire": inventory.manager.map(|manager| manager.label().to_string()),
        "paquets": inventory.packages,
        "manuels": inventory.manual.iter().map(|app| serde_json::json!({
            "nom": app.name,
            "type": app.kind.label(),
            "chemin": app.path.display().to_string(),
        })).collect::<Vec<_>>(),
        "erreurs": inventory.errors,
    });
    std::fs::write(path, serde_json::to_string_pretty(&payload)?)
}

/// Construit un rapport à partir des données déjà chargées par l'interface.
pub fn from_parts(
    capabilities: &Capabilities,
    inventory: &Inventory,
    updates: usize,
    audit: &Audit,
    cleanup_items: &[cleanup::CleanupItem],
    manager: Option<PackageManager>,
) -> Report {
    let (native, snap, flatpak, manual) = inventory.counts();
    let _ = cleanup_items;
    Report {
        generated: crate::util::now_human(),
        distro: distro::current().display_name(),
        kernel: distro::kernel_release(),
        manager: manager
            .map(|manager| manager.label().to_string())
            .unwrap_or_else(|| "inconnu".to_string()),
        packages: native,
        snap,
        flatpak,
        manual,
        updates,
        mounts: disk::mounts(),
        memory: disk::memory(),
        audit_score: audit.score(),
        findings: audit
            .findings
            .iter()
            .map(|finding| {
                (
                    finding.level.label().to_string(),
                    finding.title.clone(),
                    finding.detail.clone(),
                )
            })
            .collect(),
        recent: journal::recent(20),
    }
    .with_capabilities(capabilities)
}

impl Report {
    fn with_capabilities(self, _capabilities: &Capabilities) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_pdf_structure() {
        let path = std::env::temp_dir().join(format!("um-test-{}.pdf", std::process::id()));
        write_pdf_text(&["Rapport".to_string(), "Ligne (avec) parenthèses".to_string()], &path)
            .expect("écriture PDF");
        let bytes = std::fs::read(&path).expect("lecture PDF");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-"));
        assert!(text.contains("/Type /Page"));
        assert!(text.contains("xref"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("Ligne \\(avec\\) parenth"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pdf_handles_multiple_pages() {
        let lines: Vec<String> = (0..120).map(|index| format!("ligne {index}")).collect();
        let path = std::env::temp_dir().join(format!("um-test-multi-{}.pdf", std::process::id()));
        write_pdf_text(&lines, &path).expect("écriture PDF");
        let bytes = std::fs::read(&path).expect("lecture PDF");
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.contains("/Count 3"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn report_lines_contain_key_sections() {
        let report = Report {
            generated: "2026-01-01 10:00".to_string(),
            distro: "Ubuntu 24.04".to_string(),
            kernel: "6.8.0".to_string(),
            manager: "APT / dpkg".to_string(),
            packages: 10,
            ..Default::default()
        };
        let lines = report.lines();
        assert!(lines.iter().any(|line| line.contains("LOGICIELS")));
        assert!(lines.iter().any(|line| line.contains("Ubuntu 24.04")));
    }
}
