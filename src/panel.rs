//! Modèle générique d'un écran : indicateurs, sections tabulaires, formulaires
//! et actions. Chaque rubrique décrit son contenu avec ces types ; l'interface
//! les dessine avec un seul rendu.
//!
//! Toute action est une commande préparée par le code du module (arguments déjà
//! validés), exécutée sans shell et, si besoin, via Polkit.

use crate::exec;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Action interne exécutée sans processus ; reçoit la charge utile `args`.
pub type LocalAction = fn(&[String]) -> Result<String, String>;

/// Une commande prête à être lancée.
#[derive(Debug, Clone)]
pub struct Cmd {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub privileged: bool,
    /// Texte de confirmation ; `None` pour une action sans risque.
    pub confirm: Option<String>,
    /// Action interne (sans processus) : reçoit `args` comme charge utile.
    pub local: Option<LocalAction>,
}

impl Cmd {
    pub fn new(label: &str, program: &str, args: &[&str]) -> Cmd {
        Cmd {
            label: label.to_string(),
            program: program.to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            privileged: false,
            confirm: None,
            local: None,
        }
    }

    pub fn owned(label: &str, program: &str, args: Vec<String>) -> Cmd {
        Cmd {
            label: label.to_string(),
            program: program.to_string(),
            args,
            privileged: false,
            confirm: None,
            local: None,
        }
    }

    /// Action interne exécutée sans processus.
    pub fn local(
        label: &str,
        action: LocalAction,
        args: Vec<String>,
    ) -> Cmd {
        Cmd {
            label: label.to_string(),
            program: String::new(),
            args,
            privileged: false,
            confirm: None,
            local: Some(action),
        }
    }

    pub fn privileged(mut self) -> Cmd {
        self.privileged = true;
        self
    }

    pub fn confirm(mut self, text: &str) -> Cmd {
        self.confirm = Some(text.to_string());
        self
    }

    /// Lance la commande et renvoie un message lisible.
    pub fn run(&self) -> Result<String, String> {
        if let Some(action) = self.local {
            return action(&self.args);
        }
        if self.privileged {
            return exec::run_privileged(&self.program, &self.args)
                .map(|text| non_empty(text, &self.label));
        }
        let refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        let captured = exec::run_with_timeout(&self.program, &refs, Duration::from_secs(300))
            .map_err(|error| error.0)?;
        if captured.ok {
            Ok(non_empty(captured.text(), &self.label))
        } else {
            Err(captured.error_text())
        }
    }
}

/// Texte complet de la sortie, ou le libellé si elle est vide.
fn non_empty(text: String, label: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        label.to_string()
    } else {
        text.to_string()
    }
}

/// Un champ de saisie.
#[derive(Debug, Clone)]
pub struct Field {
    pub label: String,
    pub hint: String,
    pub default: String,
}

impl Field {
    pub fn new(label: &str, hint: &str) -> Field {
        Field {
            label: label.to_string(),
            hint: hint.to_string(),
            default: String::new(),
        }
    }

    pub fn with_default(mut self, default: &str) -> Field {
        self.default = default.to_string();
        self
    }
}

/// Un formulaire : les valeurs saisies sont transformées en commande, ou
/// refusées avec un message.
#[derive(Clone)]
pub struct Form {
    pub fields: Vec<Field>,
    pub submit: String,
    pub build: fn(&[String]) -> Result<Cmd, String>,
}

/// Niveau d'un indicateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Bad,
    Info,
}

/// Un indicateur chiffré en tête d'écran.
#[derive(Debug, Clone)]
pub struct Metric {
    pub label: String,
    pub value: String,
    pub level: Level,
}

/// Une ligne de tableau.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub cells: Vec<String>,
    pub bad: bool,
    pub actions: Vec<Cmd>,
}

impl Row {
    pub fn new(cells: &[&str]) -> Row {
        Row {
            cells: cells.iter().map(|cell| cell.to_string()).collect(),
            ..Row::default()
        }
    }

    pub fn owned(cells: Vec<String>) -> Row {
        Row {
            cells,
            ..Row::default()
        }
    }

    pub fn bad(mut self, bad: bool) -> Row {
        self.bad = bad;
        self
    }

    pub fn action(mut self, cmd: Cmd) -> Row {
        self.actions.push(cmd);
        self
    }
}

/// Type de graphique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    /// Courbes dans le temps (une valeur par échantillon).
    Lines,
    /// Barres horizontales, une par catégorie.
    Bars,
}

/// Une série de valeurs.
#[derive(Debug, Clone, Default)]
pub struct Series {
    pub label: String,
    pub values: Vec<f32>,
}

impl Series {
    pub fn new(label: &str, values: Vec<f32>) -> Series {
        Series {
            label: label.to_string(),
            values,
        }
    }
}

/// Un graphique : courbes (plusieurs séries) ou barres (une valeur par catégorie).
#[derive(Debug, Clone)]
pub struct Chart {
    pub title: String,
    pub kind: ChartKind,
    pub series: Vec<Series>,
    /// Catégories (barres) : même longueur que les valeurs de la première série.
    pub categories: Vec<String>,
    /// Repères par catégorie (barres), par exemple un seuil d'alerte.
    pub marks: Vec<f32>,
    /// Maximum de l'axe ; `None` : calculé sur les données.
    pub y_max: Option<f32>,
    /// Unité : `%`, `B/s`, `°C`… (`B/s` et `B` sont affichés en octets lisibles).
    pub unit: String,
}

impl Chart {
    pub fn lines(title: &str, unit: &str, series: Vec<Series>) -> Chart {
        Chart {
            title: title.to_string(),
            kind: ChartKind::Lines,
            series,
            categories: Vec::new(),
            marks: Vec::new(),
            y_max: None,
            unit: unit.to_string(),
        }
    }

    pub fn bars(title: &str, unit: &str, categories: Vec<String>, values: Vec<f32>) -> Chart {
        Chart {
            title: title.to_string(),
            kind: ChartKind::Bars,
            series: vec![Series::new(title, values)],
            categories,
            marks: Vec::new(),
            y_max: None,
            unit: unit.to_string(),
        }
    }

    pub fn max_y(mut self, value: f32) -> Chart {
        self.y_max = Some(value);
        self
    }

    pub fn marks(mut self, marks: Vec<f32>) -> Chart {
        self.marks = marks;
        self
    }

    /// Plus grande valeur des données (marques comprises).
    pub fn data_max(&self) -> f32 {
        self.series
            .iter()
            .flat_map(|series| series.values.iter().copied())
            .chain(self.marks.iter().copied())
            .fold(0.0, f32::max)
    }

    /// Maximum de l'axe : celui demandé, sinon la donnée arrondie au-dessus
    /// (1, 2, 5 × 10ⁿ) avec une petite marge.
    pub fn axis_max(&self) -> f32 {
        if let Some(value) = self.y_max {
            return value.max(f32::EPSILON);
        }
        nice_ceiling(self.data_max())
    }
}

/// Arrondit au-dessus à 1, 2 ou 5 × 10ⁿ (axe lisible) ; 1 pour une donnée nulle.
pub fn nice_ceiling(value: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let magnitude = 10f32.powf(value.log10().floor());
    let normalized = value / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

/// Une section : tableau, texte brut, actions globales et formulaire.
#[derive(Clone, Default)]
pub struct Section {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub text: Option<String>,
    pub note: Option<String>,
    pub actions: Vec<Cmd>,
    pub form: Option<Form>,
    pub charts: Vec<Chart>,
}

impl Section {
    pub fn new(title: &str) -> Section {
        Section {
            title: title.to_string(),
            ..Section::default()
        }
    }

    pub fn columns(mut self, columns: &[&str]) -> Section {
        self.columns = columns.iter().map(|column| column.to_string()).collect();
        self
    }

    pub fn rows(mut self, rows: Vec<Row>) -> Section {
        self.rows = rows;
        self
    }

    pub fn text(mut self, text: String) -> Section {
        self.text = Some(text);
        self
    }

    pub fn note(mut self, note: &str) -> Section {
        self.note = Some(note.to_string());
        self
    }

    pub fn action(mut self, cmd: Cmd) -> Section {
        self.actions.push(cmd);
        self
    }

    pub fn chart(mut self, chart: Chart) -> Section {
        self.charts.push(chart);
        self
    }

    pub fn form(mut self, form: Form) -> Section {
        self.form = Some(form);
        self
    }
}

/// Le contenu complet d'un écran.
#[derive(Clone, Default)]
pub struct Panel {
    pub metrics: Vec<Metric>,
    pub sections: Vec<Section>,
    pub errors: Vec<String>,
}

impl Panel {
    pub fn metric(&mut self, label: &str, value: impl Into<String>, level: Level) {
        self.metrics.push(Metric {
            label: label.to_string(),
            value: value.into(),
            level,
        });
    }

    pub fn section(&mut self, section: Section) {
        self.sections.push(section);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }

    /// Rendu texte brut (mode ligne de commande et tests).
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        for metric in &self.metrics {
            out.push_str(&strip_controls(&format!("[{}] {}\n", metric.label, metric.value)));
        }
        for error in &self.errors {
            out.push_str(&strip_controls(&format!("! {error}\n")));
        }
        for section in &self.sections {
            out.push_str(&format!("\n== {} ==\n", section.title));
            for chart in &section.charts {
                out.push_str(&strip_controls(&format!("~ {} :", chart.title)));
                for series in &chart.series {
                    let last = series.values.last().copied().unwrap_or(0.0);
                    out.push_str(&strip_controls(&format!(" {}={:.1}{}", series.label, last, chart.unit)));
                }
                out.push_str(&format!(" ({} point(s))\n", chart.series.first().map_or(0, |s| s.values.len())));
            }
            if let Some(text) = &section.text {
                out.push_str(text);
                out.push('\n');
            }
            if !section.columns.is_empty() {
                out.push_str(&section.columns.join(" | "));
                out.push('\n');
            }
            for row in &section.rows {
                out.push_str(&strip_controls(&row.cells.join(" | ")));
                if !row.actions.is_empty() {
                    let labels: Vec<&str> = row.actions.iter().map(|a| a.label.as_str()).collect();
                    out.push_str(&format!("   <{}>", labels.join(", ")));
                }
                out.push('\n');
            }
            for action in &section.actions {
                out.push_str(&format!("  > {}\n", action.label));
            }
            if section.form.is_some() {
                out.push_str("  (formulaire)\n");
            }
        }
        out
    }

    /// Nombre total de lignes (utile aux tests).
    pub fn row_count(&self) -> usize {
        self.sections.iter().map(|section| section.rows.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Aides partagées
// ---------------------------------------------------------------------------

/// Sortie standard d'une commande, ou message d'erreur.
pub fn run_text(program: &str, args: &[&str]) -> Result<String, String> {
    let captured = exec::run_with_timeout(program, args, Duration::from_secs(30))
        .map_err(|error| error.0)?;
    if captured.ok || !captured.stdout.trim().is_empty() {
        Ok(captured.stdout)
    } else {
        Err(captured.error_text())
    }
}

/// Sortie standard (et erreur) d'une commande, vide en cas d'échec.
pub fn run_lossy(program: &str, args: &[&str]) -> String {
    exec::run_with_timeout(program, args, Duration::from_secs(30))
        .map(|captured| {
            if captured.stdout.trim().is_empty() {
                captured.stderr
            } else {
                captured.stdout
            }
        })
        .unwrap_or_default()
}

pub fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Dossier de données de l'application (`~/.local/share/orbisys`).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("orbisys")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Jeton simple : lettres, chiffres et `._:/@+-`, commence par un alphanumérique.
pub fn safe_token(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    text.len() <= 128
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "._:/@+-".contains(ch))
}

/// Chemin absolu sans caractère de contrôle ni composant `..`.
pub fn safe_abs_path(text: &str) -> bool {
    let path = Path::new(text);
    path.is_absolute()
        && text.len() <= 4096
        && !text.chars().any(|ch| ch.is_control())
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

pub fn valid_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "._%+-".contains(ch))
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ".-".contains(ch))
}

/// URL http(s) sans espace ni caractère de contrôle.
pub fn valid_http_url(text: &str) -> bool {
    (text.starts_with("https://") || text.starts_with("http://"))
        && text.len() > 8
        && text.len() <= 2048
        && !text.chars().any(|ch| ch.is_whitespace() || ch.is_control())
}

pub fn valid_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.len() <= 3 && part.parse::<u8>().is_ok())
}

/// Nom d'utilisateur Unix.
pub fn valid_username(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first == '_' => {}
        _ => return false,
    }
    text.len() <= 32
        && text
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || "_-".contains(ch))
}

/// Niveau selon un pourcentage d'utilisation.
pub fn level_for_percent(percent: f32, warn: f32, bad: f32) -> Level {
    if percent >= bad {
        Level::Bad
    } else if percent >= warn {
        Level::Warn
    } else {
        Level::Ok
    }
}

/// Lit un petit fichier JSON de configuration.
pub fn load_json<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Écrit un petit fichier JSON de configuration.
pub fn save_json<T: serde::Serialize>(name: &str, value: &T) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let path = dir.join(name);
    // Fichier de configuration pouvant contenir un secret (URL de webhook) : 0600.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| error.to_string())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes()).map_err(|error| error.to_string())
}

/// Remplace les caractères de contrôle (séquences d'échappement de terminal
/// venues de noms de matériel, de fichiers ou de réseaux) par `?`.
pub fn strip_controls(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() && ch != '\n' { '?' } else { ch })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_validated() {
        assert!(safe_token("remote:path/sub"));
        assert!(!safe_token("-rf"));
        assert!(!safe_token("a b"));
        assert!(!safe_token("a;b"));
        assert!(!safe_token(""));
    }

    #[test]
    fn paths_must_be_absolute_and_clean() {
        assert!(safe_abs_path("/home/user/file.iso"));
        assert!(!safe_abs_path("relatif/chemin"));
        assert!(!safe_abs_path("/home/../etc/shadow"));
        assert!(!safe_abs_path("/tmp/a\nb"));
    }

    #[test]
    fn emails_and_urls() {
        assert!(valid_email("nom@exemple.fr"));
        assert!(!valid_email("nom@exemple"));
        assert!(!valid_email("a b@exemple.fr"));
        assert!(valid_http_url("https://hooks.example.com/x"));
        assert!(!valid_http_url("ftp://exemple.fr"));
        assert!(!valid_http_url("https://a b"));
    }

    #[test]
    fn ipv4_and_usernames() {
        assert!(valid_ipv4("192.168.1.10"));
        assert!(!valid_ipv4("300.1.1.1"));
        assert!(!valid_ipv4("1.2.3"));
        assert!(valid_username("amine"));
        assert!(!valid_username("Root"));
        assert!(!valid_username("a;b"));
    }

    #[test]
    fn levels_follow_thresholds() {
        assert_eq!(level_for_percent(10.0, 70.0, 90.0), Level::Ok);
        assert_eq!(level_for_percent(75.0, 70.0, 90.0), Level::Warn);
        assert_eq!(level_for_percent(95.0, 70.0, 90.0), Level::Bad);
    }

    #[test]
    fn terminal_escapes_are_neutralised() {
        assert_eq!(strip_controls("a\x1b[31mb\u{7}c\nd"), "a?[31mb?c\nd");
        let mut panel = Panel::default();
        panel.sections.push(Section::new("t").columns(&["x"]).rows(vec![Row::new(&["\x1b]0;pwn\u{7}"])]));
        assert!(!panel.render_text().contains('\x1b'));
    }

    #[test]
    fn chart_axis_is_readable() {
        assert_eq!(nice_ceiling(0.0), 1.0);
        assert_eq!(nice_ceiling(0.7), 1.0);
        assert_eq!(nice_ceiling(1.3), 2.0);
        assert_eq!(nice_ceiling(3.0), 5.0);
        assert_eq!(nice_ceiling(7.0), 10.0);
        assert_eq!(nice_ceiling(42.0), 50.0);
        assert_eq!(nice_ceiling(f32::NAN), 1.0);
        let chart = Chart::lines("t", "%", vec![Series::new("a", vec![10.0, 85.0])]);
        assert_eq!(chart.axis_max(), 100.0);
        assert_eq!(chart.clone().max_y(100.0).axis_max(), 100.0);
        let bars = Chart::bars("b", "%", vec!["x".into()], vec![40.0]).marks(vec![90.0]);
        assert_eq!(bars.data_max(), 90.0);
    }

    #[test]
    fn charts_appear_in_text_rendering() {
        let mut panel = Panel::default();
        panel.section(Section::new("s").chart(Chart::lines("CPU", "%", vec![Series::new("cpu", vec![10.0, 20.0])])));
        let text = panel.render_text();
        assert!(text.contains("~ CPU : cpu=20.0% (2 point(s))"), "{text}");
    }

    #[test]
    fn command_runs_without_shell() {
        let cmd = Cmd::new("écho", "echo", &["a;b"]);
        assert!(cmd.run().unwrap().contains("a;b"));
        let failing = Cmd::new("échec", "false", &[]);
        assert!(failing.run().is_err());
    }
}
