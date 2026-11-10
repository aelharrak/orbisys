//! Interface graphique (egui) : écrans dédiés et rubriques génériques
//! (`panel`), thème clair/sombre, accent,
//! recherche globale Ctrl+K et actions confirmées via Polkit.

use orbisys::backup::{self, BackupReport, RsyncOptions};
use orbisys::kernel::{self, KernelReport};
use orbisys::lvm::{self, VolumesReport};
use orbisys::automation::{self, AutomationReport};
use orbisys::desktop::{self, DesktopReport};
use orbisys::i18n::{self, Lang};
use orbisys::i18n_ui::{fill as tf, ts, tx};
use orbisys::locale::{self, LocaleReport};
use orbisys::optimize::{self, OptimizeReport};
use orbisys::logsearch::{self, LogQuery, RecurringError};
use orbisys::netadv::{self, NetAdvData, Diagnostics as NetDiagnostics};
use orbisys::btrfs;
use orbisys::cis;
use orbisys::cups;
use orbisys::fonts;
use orbisys::orphans;
use orbisys::klogs;
use orbisys::perf;
use orbisys::netmounts;
use orbisys::sessions;
use orbisys::timers;
use orbisys::quota;
use orbisys::units::{self, UnitFile};
use orbisys::snapshots::{self, SnapshotsReport};
use orbisys::peripherals::{self, PeripheralsReport};
use orbisys::cleanup::{self, CleanupItem};
use orbisys::disk;
use orbisys::distro::{self, Capabilities};
use orbisys::hardware::{self, HardwareReport};
use orbisys::inventory::Inventory;
use orbisys::journal;
use orbisys::logs;
use orbisys::manual;
use orbisys::pkg::{self, PackageManager, RemovalPreview, Source, Update};
use orbisys::processes::{ProcAction, ProcInfo, Sampler, Snapshot};
use orbisys::report::Report;
use orbisys::security::{self, Audit};
use orbisys::firewall::{self, FirewallAction, FirewallReport};
use orbisys::network::{self, NetworkReport};
use orbisys::services::{self, Service, ServiceAction};
use orbisys::storage::{self, DuplicateScan, StorageReport};
use orbisys::util;
use orbisys::panel::{self as generic, Cmd};
use orbisys::panels::PanelId;

mod icons;
mod ui_fonts;
mod panel_view;
mod elastic;

use eframe::egui;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Page {
    Dashboard,
    Software,
    Updates,
    Disk,
    Storage,
    Quotas,
    Duplicates,
    Cleanup,
    Backup,
    Kernel,
    Volumes,
    Peripherals,
    Printers,
    Desktop,
    Automation,
    Locale,
    Fonts,
    Optimize,
    Snapshots,
    Btrfs,
    LogAdvanced,
    CustomUnits,
    Processes,
    Services,
    Logs,
    Hardware,
    Network,
    NetAdvanced,
    Firewall,
    Security,
    Cis,
    Orphans,
    Perf,
    KernelLogs,
    Sessions,
    NetMounts,
    Timers,
    Reports,
    About,
    /// Rubrique décrite par le modèle générique.
    Panel(PanelId),
}

impl Page {
    fn key(&self) -> &'static str {
        match self {
            Page::Dashboard => "page.dashboard",
            Page::Software => "page.software",
            Page::Updates => "page.updates",
            Page::Disk => "page.disk",
            Page::Storage => "page.storage",
            Page::Quotas => "page.quotas",
            Page::Duplicates => "page.duplicates",
            Page::Cleanup => "page.cleanup",
            Page::Backup => "page.backup",
            Page::Kernel => "page.kernel",
            Page::Volumes => "page.volumes",
            Page::Peripherals => "page.peripherals",
            Page::Printers => "page.printers",
            Page::Desktop => "page.desktop",
            Page::Automation => "page.automation",
            Page::Locale => "page.locale",
            Page::Fonts => "page.fonts",
            Page::Optimize => "page.optimize",
            Page::Snapshots => "page.snapshots",
            Page::Btrfs => "page.btrfs",
            Page::LogAdvanced => "page.logadvanced",
            Page::CustomUnits => "page.customunits",
            Page::Processes => "page.processes",
            Page::Services => "page.services",
            Page::Logs => "page.logs",
            Page::Hardware => "page.hardware",
            Page::Network => "page.network",
            Page::NetAdvanced => "page.netadvanced",
            Page::Firewall => "page.firewall",
            Page::Security => "page.security",
            Page::Cis => "page.cis",
            Page::Orphans => "page.orphans",
            Page::Perf => "page.perf",
            Page::KernelLogs => "page.klogs",
            Page::Sessions => "page.sessions",
            Page::NetMounts => "page.netmounts",
            Page::Timers => "page.timers",
            Page::Reports => "page.reports",
            Page::About => "page.about",
            Page::Panel(id) => id.key(),
        }
    }

    fn key_hint(&self) -> &'static str {
        match self {
            Page::Dashboard => "hint.dashboard",
            Page::Software => "hint.software",
            Page::Updates => "hint.updates",
            Page::Disk => "hint.disk",
            Page::Storage => "hint.storage",
            Page::Quotas => "hint.quotas",
            Page::Duplicates => "hint.duplicates",
            Page::Cleanup => "hint.cleanup",
            Page::Backup => "hint.backup",
            Page::Kernel => "hint.kernel",
            Page::Volumes => "hint.volumes",
            Page::Peripherals => "hint.peripherals",
            Page::Printers => "hint.printers",
            Page::Desktop => "hint.desktop",
            Page::Automation => "hint.automation",
            Page::Locale => "hint.locale",
            Page::Fonts => "hint.fonts",
            Page::Optimize => "hint.optimize",
            Page::Snapshots => "hint.snapshots",
            Page::Btrfs => "hint.btrfs",
            Page::LogAdvanced => "hint.logadvanced",
            Page::CustomUnits => "hint.customunits",
            Page::Processes => "hint.processes",
            Page::Services => "hint.services",
            Page::Logs => "hint.logs",
            Page::Hardware => "hint.hardware",
            Page::Network => "hint.network",
            Page::NetAdvanced => "hint.netadvanced",
            Page::Firewall => "hint.firewall",
            Page::Security => "hint.security",
            Page::Cis => "hint.cis",
            Page::Orphans => "hint.orphans",
            Page::Perf => "hint.perf",
            Page::KernelLogs => "hint.klogs",
            Page::Sessions => "hint.sessions",
            Page::NetMounts => "hint.netmounts",
            Page::Timers => "hint.timers",
            Page::Reports => "hint.reports",
            Page::About => "hint.about",
            Page::Panel(id) => id.hint_key(),
        }
    }

    /// Titre de l'écran lui-même (onglet).
    fn own_title(&self, lang: Lang) -> &'static str {
        i18n::t(lang, self.key())
    }

    /// Titre affiché dans le menu et l'en-tête : pour une fenêtre à onglets, le nom du groupe.
    fn title(&self, lang: Lang) -> &'static str {
        if let Some((group, list, 0)) = merge_of(*self) {
            if list.len() > 1 {
                if let Some(key) = group_title_key(group) {
                    return i18n::t(lang, key);
                }
            }
        }
        i18n::t(lang, self.key())
    }

    fn hint(&self, lang: Lang) -> &'static str {
        i18n::t(lang, self.key_hint())
    }
}

/// Pastille d'état colorée, alignée sur la ligne de texte.
fn status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

/// Interpolation de Catmull-Rom : courbes lissées entre les relevés (maquette).
fn smooth(points: &[egui::Pos2], floor: f32, ceil: f32) -> Vec<egui::Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 6);
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(points.len() - 1)];
        for step in 0..6 {
            let t = step as f32 / 6.0;
            let (t2, t3) = (t * t, t * t * t);
            let spline = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push(egui::pos2(
                spline(p0.x, p1.x, p2.x, p3.x),
                spline(p0.y, p1.y, p2.y, p3.y).clamp(ceil, floor),
            ));
        }
    }
    out.push(points[points.len() - 1]);
    out
}

/// Quatre tons orange → ambre des courbes superposées (maquette), quel que soit l'accent choisi.
fn chart_tones(dark: bool) -> [egui::Color32; 4] {
    if dark {
        [
            egui::Color32::from_rgb(249, 115, 22),
            egui::Color32::from_rgb(251, 146, 60),
            egui::Color32::from_rgb(245, 158, 11),
            egui::Color32::from_rgb(251, 191, 36),
        ]
    } else {
        [
            egui::Color32::from_rgb(194, 65, 12),
            egui::Color32::from_rgb(234, 88, 12),
            egui::Color32::from_rgb(217, 119, 6),
            egui::Color32::from_rgb(245, 158, 11),
        ]
    }
}

/// Graphique à courbes superposées (maquette « System Monitor ») : quadrillage fin
/// horizontal et vertical, étiquettes de l'axe Y et de l'axe du temps, courbes lissées
/// et aire en dégradé sous chaque courbe (plus marquée pour la première série).
#[allow(clippy::too_many_arguments)]
fn series_chart(
    ui: &mut egui::Ui,
    height: f32,
    series: &[(&[f32], egui::Color32)],
    max_y: Option<f32>,
    format: &dyn Fn(f32) -> String,
    time_labels: Option<[String; 5]>,
    muted: egui::Color32,
    grid: egui::Color32,
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let peak = max_y.unwrap_or_else(|| {
        let top = series.iter().flat_map(|(values, _)| values.iter().copied()).fold(0.0_f32, f32::max);
        (top * 1.15).max(1024.0)
    });
    // Largeur de l'axe Y selon la plus longue étiquette (« 100 % » ou « 20.1 Kio/s »).
    let axis_w = (format(peak).chars().count() as f32 * 6.2 + 18.0).clamp(52.0, 110.0);
    let bottom_margin = if time_labels.is_some() { 24.0 } else { 8.0 };
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + axis_w, rect.top() + 8.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - bottom_margin),
    );
    let faint = grid.gamma_multiply(0.8);
    for step in 0..=4 {
        let ratio = step as f32 / 4.0;
        let y = plot.bottom() - ratio * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, faint),
        );
        painter.text(
            egui::pos2(plot.left() - 10.0, y),
            egui::Align2::RIGHT_CENTER,
            format(ratio * peak),
            egui::FontId::proportional(11.0),
            muted,
        );
        // Quadrillage vertical aligné sur les étiquettes de temps.
        let x = plot.left() + ratio * plot.width();
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(1.0_f32, faint.gamma_multiply(0.55)),
        );
        if let Some(labels) = &time_labels {
            painter.text(
                egui::pos2(x, plot.bottom() + 6.0),
                match step {
                    0 => egui::Align2::LEFT_TOP,
                    4 => egui::Align2::RIGHT_TOP,
                    _ => egui::Align2::CENTER_TOP,
                },
                &labels[step],
                egui::FontId::proportional(11.0),
                muted,
            );
        }
    }
    // Aires d'abord (dégradé vertical), traits ensuite.
    let curves: Vec<(Vec<egui::Pos2>, egui::Color32)> = series
        .iter()
        .filter(|(values, _)| values.len() >= 2)
        .map(|(values, color)| {
            let points: Vec<egui::Pos2> = values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    let x = plot.left() + index as f32 / (values.len() - 1) as f32 * plot.width();
                    let y = plot.bottom() - (value / peak).clamp(0.0, 1.0) * plot.height();
                    egui::pos2(x, y)
                })
                .collect();
            (smooth(&points, plot.bottom(), plot.top()), *color)
        })
        .collect();
    for (order, (points, color)) in curves.iter().enumerate() {
        // Beaucoup de séries (un cœur chacune) : aires plus discrètes pour que l'ensemble reste lisible.
        let top_alpha: u8 = if order == 0 { if curves.len() > 6 { 40 } else { 96 } } else { (200 / curves.len().max(1)).clamp(6, 40) as u8 };
        let mut mesh = egui::Mesh::default();
        for point in points {
            let [r, g, b, _] = color.to_array();
            mesh.colored_vertex(*point, egui::Color32::from_rgba_unmultiplied(r, g, b, top_alpha));
            mesh.colored_vertex(egui::pos2(point.x, plot.bottom()), egui::Color32::from_rgba_unmultiplied(r, g, b, 0));
        }
        for i in 0..points.len().saturating_sub(1) as u32 {
            let k = i * 2;
            mesh.add_triangle(k, k + 1, k + 2);
            mesh.add_triangle(k + 1, k + 2, k + 3);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    for (points, color) in &curves {
        painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(1.6_f32, *color)));
    }
}

/// Étiquettes de l'axe du temps pour une fenêtre de `seconds` secondes.
fn time_axis(seconds: usize) -> [String; 5] {
    let ago = |value: usize| -> String {
        if value >= 60 && value % 60 == 0 {
            tf("il y a {} min", &[&(value / 60)])
        } else {
            tf("il y a {} s", &[&value])
        }
    };
    [
        ago(seconds),
        ago(seconds * 3 / 4),
        ago(seconds / 2),
        ago(seconds / 4),
        ts("Maintenant").to_string(),
    ]
}

/// Entrée de légende (maquette) : pastille, libellé, valeur en dessous. Une seule zone
/// réservée (pas de `horizontal` imbriqué, qui cassait le retour à la ligne).
fn legend_entry(ui: &mut egui::Ui, color: egui::Color32, label: &str, value: &str, muted: egui::Color32) {
    let text = ui.visuals().text_color();
    let label_galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), text);
    let value_galley = ui.painter().layout_no_wrap(value.to_string(), egui::FontId::proportional(12.0), muted);
    let width = 18.0 + label_galley.size().x.max(value_galley.size().x) + 14.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(egui::pos2(rect.left() + 5.0, rect.top() + 8.0), 5.0, color);
    painter.galley(egui::pos2(rect.left() + 18.0, rect.top()), label_galley, text);
    painter.galley(egui::pos2(rect.left() + 18.0, rect.top() + 17.0), value_galley, muted);
}

/// Cellule de largeur fixe, contenu aligné à gauche et centré verticalement
/// (`add_sized` centre le texte, ce qui désaligne les colonnes).
fn left_cell(ui: &mut egui::Ui, size: [f32; 2], widget: impl egui::Widget) -> egui::Response {
    // La cellule est réservée une seule fois (`allocate_exact_size`) ; le widget est
    // placé dans une interface enfant qui, elle, n'avance pas le curseur du parent.
    // (`allocate_ui_with_layout` ne gardait que la place utilisée et `allocate_new_ui`
    // avançait deux fois : dans les deux cas les colonnes se décalaient.)
    // La largeur est bornée par la place restante : en colonne étroite, une
    // cellule trop large rognait le reste de la ligne et débordait du panneau.
    let width = size[0].min(ui.available_width().max(24.0));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, size[1]), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.add(widget)
}

/// Fenêtres à onglets : la première page est celle du menu, les suivantes sont des onglets.
/// Les anciennes entrées restent trouvables (Ctrl+K) et ouvrent l'onglet voulu.
const MERGES: &[(&str, &[Page])] = &[
    ("processes", &[Page::Processes, Page::Panel(PanelId::Monitor)]),
    ("reports", &[Page::Reports, Page::Panel(PanelId::History), Page::Panel(PanelId::Schedule), Page::Panel(PanelId::Delivery)]),
    ("containers", &[Page::Panel(PanelId::Containers), Page::Panel(PanelId::ContainersAdv)]),
    ("disks", &[Page::Disk, Page::Storage, Page::Quotas, Page::Panel(PanelId::Mounts)]),
    ("cleanup", &[Page::Cleanup, Page::Panel(PanelId::AdvCleanup), Page::Duplicates]),
    ("backups", &[Page::Backup, Page::Panel(PanelId::CloudBackup), Page::Panel(PanelId::Dotfiles), Page::Panel(PanelId::Archives)]),
    ("logs", &[Page::Logs, Page::LogAdvanced, Page::KernelLogs, Page::Panel(PanelId::ActionLog)]),
    ("services", &[Page::Services, Page::CustomUnits, Page::Panel(PanelId::Startup), Page::Timers]),
        ("maintenance", &[Page::Panel(PanelId::Maintenance), Page::Panel(PanelId::Troubleshoot), Page::Orphans]),
    ("network", &[Page::Network, Page::NetAdvanced, Page::Panel(PanelId::WifiVpn), Page::Panel(PanelId::Lan), Page::Panel(PanelId::Traffic), Page::NetMounts]),
    ("audit", &[Page::Security, Page::Cis, Page::Panel(PanelId::Integrity)]),
    ("keys", &[Page::Panel(PanelId::SshGpg), Page::Panel(PanelId::Certs)]),
    ("protections", &[Page::Panel(PanelId::SecureBoot), Page::Panel(PanelId::AppArmor), Page::Panel(PanelId::Acl)]),
    ("packages", &[Page::Updates, Page::Panel(PanelId::AptRepos), Page::Panel(PanelId::AdvPackages)]),
    ("hardware", &[Page::Hardware, Page::Panel(PanelId::Power), Page::Panel(PanelId::Fans), Page::Panel(PanelId::Firmware)]),
    ("peripherals", &[Page::Peripherals, Page::Printers, Page::Panel(PanelId::Bluetooth)]),
];

/// Groupe, liste et rang de `page` s'il appartient à une fenêtre à onglets.
/// Clé de traduction du nom d'un groupe (absente : le titre du premier onglet sert de nom).
fn group_title_key(group: &str) -> Option<&'static str> {
    Some(match group {
        "processes" => "group.processes",
        "reports" => "group.reports",
        "disks" => "group.disks",
        "backups" => "group.backups",
        "audit" => "group.audit",
        "keys" => "group.keys",
        "protections" => "group.protections",
        "packages" => "group.packages",
        "peripherals" => "group.peripherals",
        _ => return None,
    })
}

fn merge_of(page: Page) -> Option<(&'static str, &'static [Page], usize)> {
    MERGES.iter().find_map(|(group, list)| list.iter().position(|candidate| *candidate == page).map(|index| (*group, *list, index)))
}

const NAV: &[(&str, &[Page])] = &[
    (
        "nav.monitoring",
        &[
            Page::Dashboard,
            Page::Processes,
            Page::Panel(PanelId::Alerts),
            Page::Panel(PanelId::Health),
            Page::Reports,
        ],
    ),
    (
        "nav.software",
        &[
            Page::Software,
            Page::Updates,
            Page::Panel(PanelId::Runtimes),
            Page::Panel(PanelId::Containers),
        ],
    ),
    (
        "nav.storage",
        &[
            Page::Disk,
            Page::Cleanup,
            Page::Panel(PanelId::UsbBoot),
            Page::Backup,
        ],
    ),
    ("nav.kernel", &[Page::Kernel]),
    ("nav.volumes", &[Page::Volumes]),
    (
        "nav.peripherals",
        &[
            Page::Peripherals,
        ],
    ),
    ("nav.desktop", &[Page::Desktop]),
    ("nav.automation", &[Page::Automation, Page::Panel(PanelId::Profiles)]),
    ("nav.locale", &[Page::Locale, Page::Fonts]),
    ("nav.optimize", &[Page::Optimize, Page::Perf, Page::Panel(PanelId::Benchmark)]),
    ("nav.snapshots", &[Page::Snapshots, Page::Btrfs, Page::Panel(PanelId::BtrfsMaint)]),
    (
        "nav.system",
        &[
            Page::Services,
            Page::Logs,
            Page::Hardware,
            Page::Sessions,
            Page::Panel(PanelId::Maintenance),
        ],
    ),
    (
        "nav.network",
        &[
            Page::Network,
        ],
    ),
    (
        "nav.security",
        &[
            Page::Firewall,
            Page::Security,
            Page::Panel(PanelId::SshGpg),
            Page::Panel(PanelId::SecureBoot),
            Page::Panel(PanelId::Scan),
        ],
    ),
    ("nav.app", &[Page::About]),
];

/// Pages dédiées ; les rubriques génériques sont listées par `PanelId::ALL`.
const ALL_PAGES: &[Page] = &[
    Page::Dashboard,
    Page::Software,
    Page::Updates,
    Page::Disk,
    Page::Storage,
    Page::Quotas,
    Page::Duplicates,
    Page::Cleanup,
    Page::Backup,
    Page::Kernel,
    Page::Volumes,
    Page::Peripherals,
    Page::Printers,
    Page::Desktop,
    Page::Automation,
    Page::Locale,
    Page::Fonts,
    Page::Optimize,
    Page::Snapshots,
    Page::Btrfs,
    Page::LogAdvanced,
    Page::CustomUnits,
    Page::Processes,
    Page::Services,
    Page::Hardware,
    Page::Network,
    Page::NetAdvanced,
    Page::Firewall,
    Page::Logs,
    Page::Security,
    Page::Cis,
    Page::Orphans,
    Page::Perf,
    Page::KernelLogs,
    Page::Sessions,
    Page::NetMounts,
    Page::Timers,
    Page::Reports,
    Page::About,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Accent {
    Bleu,
    Violet,
    Vert,
    Orange,
    Rose,
}

impl Accent {
    const ALL: [Accent; 5] = [
        Accent::Bleu,
        Accent::Violet,
        Accent::Vert,
        Accent::Orange,
        Accent::Rose,
    ];

    fn label(&self) -> &'static str {
        match self {
            Accent::Bleu => ts("Bleu"),
            Accent::Violet => ts("Violet"),
            Accent::Vert => ts("Vert"),
            Accent::Orange => ts("Orange"),
            Accent::Rose => ts("Rose"),
        }
    }

    fn color(&self, dark: bool) -> egui::Color32 {
        match (self, dark) {
            (Accent::Bleu, true) => egui::Color32::from_rgb(59, 147, 255),
            (Accent::Bleu, false) => egui::Color32::from_rgb(31, 111, 235),
            (Accent::Violet, true) => egui::Color32::from_rgb(168, 107, 255),
            (Accent::Violet, false) => egui::Color32::from_rgb(123, 63, 228),
            (Accent::Vert, true) => egui::Color32::from_rgb(43, 209, 138),
            (Accent::Vert, false) => egui::Color32::from_rgb(15, 125, 87),
            (Accent::Orange, true) => egui::Color32::from_rgb(249, 115, 22),
            (Accent::Orange, false) => egui::Color32::from_rgb(194, 65, 12),
            (Accent::Rose, true) => egui::Color32::from_rgb(255, 111, 156),
            (Accent::Rose, false) => egui::Color32::from_rgb(199, 45, 99),
        }
    }
}

// ---------------------------------------------------------------------------
// Messages des tâches de fond
// ---------------------------------------------------------------------------

enum Msg {
    Inventory(Inventory),
    Updates(Vec<Update>),
    UpdatesApplied(String, bool, Vec<Update>),
    Disks(Vec<disk::Mount>),
    Services(Vec<Service>),
    Logs(String),
    Audit(Audit),
    Cleanup(Vec<CleanupItem>),
    Storage(StorageReport),
    Duplicates(DuplicateScan),
    Hardware(HardwareReport),
    Network(NetworkReport),
    Firewall(FirewallReport),
    Backup(BackupReport),
    Kernel(KernelReport),
    Volumes(VolumesReport),
    Peripherals(PeripheralsReport),
    Desktop(DesktopReport),
    Automation(AutomationReport),
    Locale(LocaleReport),
    Optimize(OptimizeReport),
    Snapshots(SnapshotsReport),
    LogResults(String),
    RecurringErrors(Vec<RecurringError>),
    CustomUnits(Vec<UnitFile>),
    NetAdv(Box<NetAdvData>),
    NetAdvDiag(Box<NetDiagnostics>),
    CupsPrinters(Vec<cups::Printer>),
    CupsJobs(Vec<cups::Job>),
    CupsSharing(Box<cups::Sharing>),
    Fonts(Vec<fonts::Family>),
    FontCache(Box<fonts::CacheInfo>),
    FontMatch(Box<fonts::Match>),
    Quotas(Box<quota::QuotaReport>),
    Btrfs(Box<btrfs::BtrfsReport>),
    Cis(Box<cis::AuditReport>),
    Orphans(Box<orphans::OrphanReport>),
    Perf(Box<perf::PerfReport>),
    KernelLogs(Box<klogs::KernelReport>),
    Sessions(Box<sessions::SessionsReport>),
    NetMounts(Box<netmounts::NetMountReport>),
    Timers(Box<timers::TimersReport>),
    Panel(PanelId, Box<generic::Panel>),
    Breaches(Vec<(String, String)>),
    RuleAlerts(Vec<String>),
    PanelDone {
        id: PanelId,
        label: String,
        ok: bool,
        text: String,
    },
    UnitText(String),
    UnitStatus(String),
    Preview(RemovalPreview),
    Action(String, bool),
    Error(String),
}

/// Une action différée, appliquée après le rendu de la page.
#[derive(Clone)]
enum Pending {
    PreviewNative(Vec<String>),
    Remove {
        names: Vec<String>,
        source: Source,
    },
    RemoveManual(std::path::PathBuf),
    Service(String, ServiceAction),
    Clean(CleanupItem),
    Firewall(FirewallAction),
    ExportList,
    ImportList(std::path::PathBuf),
    Rsync(RsyncOptions),
    RegenerateGrub,
    ApplySysctl(Vec<(String, String)>),
    RemoveKernels(Vec<String>),
    InstallDriver(String),
    CreateVolume {
        group: String,
        name: String,
        size: String,
    },
    RemoveVolume(String),
    CloseCrypt(String),
    MountDevice {
        device: String,
        mount: bool,
    },
    SetPrinterEnabled {
        name: String,
        enabled: bool,
    },
    RemovePrinter(String),
    SetDefaultPrinter(String),
    SetDefaultSink(String),
    ToggleMute(String),
    AdjustVolume {
        name: String,
        delta: i32,
    },
    SetGsetting {
        schema: String,
        key: String,
        value: String,
    },
    ToggleExtension {
        id: String,
        enabled: bool,
    },
    AddCron {
        schedule: String,
        command: String,
    },
    RemoveCron {
        schedule: String,
        command: String,
    },
    SetTimer {
        unit: String,
        enabled: bool,
    },
    SetLocale(String),
    SetTimezone(String),
    RunTrim,
    SetGovernor(String),
    RemoteCheck,
    RemoteBackup {
        source: String,
        target: String,
        dry_run: bool,
    },
    VerifyArchive(String),
    SearchLogs(Box<LogQuery>),
    AnalyzeErrors,
    ExportLogs,
    UnitsReload,
    LoadUnit(String),
    SaveUnit {
        name: String,
        text: String,
    },
    RemoveUnit(String),
    UnitAction(String, String),
    UnitStatus(String),
    NetAdvReload,
    NetAdvPing(String),
    NetAdvResolve(String),
    NetAdvDiagnose,
    CupsReload,
    CupsCancelJob(String),
    CupsClearQueue(String),
    CupsSetEnabled {
        name: String,
        enabled: bool,
    },
    CupsRemove(String),
    FontsReload,
    FontMatchQuery(String),
    FontsInstall(String),
    FontsRemove(String),
    FontsRefresh,
    QuotasReload,
    QuotasEnable(String),
    SetQuota {
        user: String,
        mount: String,
        soft: u64,
        hard: u64,
    },
    BtrfsReload,
    BtrfsCreate {
        source: String,
        destination: String,
    },
    BtrfsDelete(String),
    CisReload,
    OrphansReload,
    OrphansAutoremove,
    OrphansPurge(Vec<String>),
    PerfReload,
    PerfGovernor(String),
    PerfSwappiness(u32),
    PerfThp(String),
    PerfProfile(String),
    KernelLogsReload,
    SessionsReload,
    SessionLock {
        user: String,
        locked: bool,
    },
    SessionTerminate(String),
    NetMountsReload,
    NetMountToggle {
        path: String,
        mount: bool,
    },
    NetMountProbe {
        server: String,
        fstype: String,
    },
    TimersReload,
    PanelLoad(PanelId),
    ApplyUpdates,
    ProcessAction {
        pid: u32,
        name: String,
        action: ProcAction,
    },
    DeleteDuplicates(storage::DuplicateGroup),
    ExportDuplicatesCsv,
    PanelRun(Cmd, PanelId),
    TimerAction {
        unit: String,
        action: String,
    },
    Refresh(Page),
    Export(&'static str),
    ApplyDns(Vec<String>),
    AddHost {
        address: String,
        name: String,
    },
    RemoveHost {
        address: String,
        name: String,
    },
    SetZram(bool),
    AddRule {
        metric: String,
        threshold: u8,
        action: String,
    },
    RemoveRule(usize),
    TestRuleNotification,
    NetMountRemote {
        source: String,
        point: String,
        options: String,
    },
}

struct Confirmation {
    title: String,
    detail: String,
    pending: Pending,
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

pub struct App {
    page: Page,
    dark: bool,
    accent: Accent,
    lang: Lang,
    os: distro::OsRelease,
    capabilities: Capabilities,

    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    busy: usize,
    toasts: Vec<(String, bool, f64)>,

    inventory: Option<Inventory>,
    updates: Vec<Update>,
    mounts: Vec<disk::Mount>,
    processes: Vec<ProcInfo>,
    snapshot: Snapshot,
    sampler: Sampler,
    services: Vec<Service>,
    audit: Option<Audit>,
    cleanup_items: Vec<CleanupItem>,
    storage: Option<StorageReport>,
    duplicates: Option<DuplicateScan>,
    duplicate_root: String,
    duplicate_min_size: u64,
    hardware: Option<HardwareReport>,
    network: Option<NetworkReport>,
    firewall: Option<FirewallReport>,
    firewall_port: String,
    backup: Option<BackupReport>,
    list_path: String,
    rsync_source: String,
    rsync_destination: String,
    rsync_mirror: bool,
    rsync_delete: bool,
    rsync_output: String,
    kernel: Option<KernelReport>,
    kernel_values: Vec<(String, String)>,
    volumes: Option<VolumesReport>,
    lv_group: String,
    lv_name: String,
    lv_size: String,
    peripherals: Option<PeripheralsReport>,
    desktop: Option<DesktopReport>,
    theme_input: String,
    icon_input: String,
    font_input: String,
    scheme_input: String,
    automation: Option<AutomationReport>,
    cron_schedule: String,
    cron_command: String,
    locale: Option<LocaleReport>,
    locale_input: String,
    timezone_input: String,
    optimize: Option<OptimizeReport>,
    snapshots: Option<SnapshotsReport>,
    remote_source: String,
    remote_target: String,
    snapshot_output: String,
    log_units: String,
    log_since: String,
    log_until: String,
    log_level: String,
    log_grep: String,
    log_limit: usize,
    log_results: String,
    log_errors: Vec<RecurringError>,
    custom_units: Vec<UnitFile>,
    unit_filter: String,
    unit_name: String,
    unit_description: String,
    unit_exec: String,
    unit_user: String,
    unit_editor: String,
    unit_status: String,
    netadv: Option<Box<NetAdvData>>,
    netadv_host: String,
    netadv_output: String,
    netadv_diag: Option<Box<NetDiagnostics>>,
    cups_printers: Vec<cups::Printer>,
    cups_jobs: Vec<cups::Job>,
    cups_sharing: Option<Box<cups::Sharing>>,
    cups_filter: String,
    fonts: Vec<fonts::Family>,
    font_cache: Option<fonts::CacheInfo>,
    font_filter: String,
    font_query: String,
    font_match: String,
    font_path: String,
    quota_report: Option<Box<quota::QuotaReport>>,
    quota_user: String,
    quota_mount: String,
    quota_soft: String,
    quota_hard: String,
    quota_filter: String,
    btrfs_report: Option<Box<btrfs::BtrfsReport>>,
    btrfs_source: String,
    btrfs_dest: String,
    btrfs_filter: String,
    cis_report: Option<Box<cis::AuditReport>>,
    cis_filter: String,
    orphan_report: Option<Box<orphans::OrphanReport>>,
    orphan_filter: String,
    perf_report: Option<Box<perf::PerfReport>>,
    perf_swappiness: String,
    perf_governor: String,
    perf_thp: String,
    klogs: Option<Box<klogs::KernelReport>>,
    klogs_filter: String,
    klogs_errors_only: bool,
    sessions_report: Option<Box<sessions::SessionsReport>>,
    sessions_filter: String,
    netmounts_report: Option<Box<netmounts::NetMountReport>>,
    netmounts_filter: String,
    netmounts_output: String,
    timers_report: Option<Box<timers::TimersReport>>,
    timers_filter: String,
    archive_path: String,
    panels: std::collections::HashMap<PanelId, Box<generic::Panel>>,
    panel_loading: HashSet<PanelId>,
    panel_loaded_at: std::collections::HashMap<PanelId, Instant>,
    panel_filter: String,
    panel_page: usize,
    panel_page_for: Option<PanelId>,
    panel_inputs: std::collections::HashMap<(PanelId, usize), Vec<String>>,
    panel_output: Option<(String, String)>,
    selected_process: Option<(u32, String)>,
    visited_pages: HashSet<Page>,
    fonts_lang: Option<Lang>,
    font_missing: bool,
    cpu_history: std::collections::VecDeque<f32>,
    mem_history: std::collections::VecDeque<f32>,
    core_history: Vec<std::collections::VecDeque<f32>>,
    core_prev: Vec<(u64, u64)>,
    sys_host: String,
    merged_tab: std::collections::HashMap<&'static str, usize>,
    sys_kernel: String,
    cpu_mhz: f32,
    rx_history: std::collections::VecDeque<f32>,
    tx_history: std::collections::VecDeque<f32>,
    net_last: Option<(Instant, f64, f64)>,
    selected_history: std::collections::VecDeque<(f32, f32)>,
    process_nice: i32,
    last_alert_check: Instant,
    alert_cooldown: std::collections::HashMap<String, Instant>,
    last_rule_check: Instant,
    dns_server_input: String,
    hosts_entries: Vec<netadv::HostEntry>,
    host_address: String,
    host_name: String,
    automation_rules: Vec<automation::Rule>,
    rule_metric: usize,
    rule_threshold: u8,
    rule_action: usize,
    netmount_source: String,
    netmount_point: String,
    netmount_options: String,
    software_page: usize,

    search: String,
    source_filter: Option<Source>,
    selected: HashSet<String>,
    preview: Option<RemovalPreview>,
    confirm: Option<Confirmation>,
    palette: bool,
    palette_query: String,

    service_filter: String,
    log_unit: String,
    log_priority: logs::Priority,
    logs_text: String,

    last_processes: Instant,
    last_export: Option<String>,
}

fn prefs_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("orbisys").join("prefs.json"))
}

fn load_prefs() -> (Option<bool>, Accent, Lang) {
    let Some(path) = prefs_path() else {
        return (None, Accent::Orange, Lang::Fr);
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return (None, Accent::Orange, Lang::Fr);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (None, Accent::Orange, Lang::Fr);
    };
    let dark = value.get("dark").and_then(|value| value.as_bool());
    let accent = match value.get("accent").and_then(|value| value.as_str()) {
        Some("bleu") => Accent::Bleu,
        Some("violet") => Accent::Violet,
        Some("vert") => Accent::Vert,
        Some("rose") => Accent::Rose,
        _ => Accent::Orange,
    };
    let lang = Lang::from_code(
        value
            .get("lang")
            .and_then(|value| value.as_str())
            .unwrap_or("fr"),
    );
    (dark, accent, lang)
}

/// Intitulé d'une action du pare-feu dans la langue courante (le journal garde `label()`).
fn firewall_label(action: &FirewallAction) -> String {
    match action {
        FirewallAction::Enable => ts("Activer le pare-feu").to_string(),
        FirewallAction::Disable => ts("Désactiver le pare-feu").to_string(),
        FirewallAction::Allow(spec) => tf("Autoriser {}", &[spec]),
        FirewallAction::Deny(spec) => tf("Refuser {}", &[spec]),
        FirewallAction::Delete(number, _) => tf("Supprimer la règle n°{}", &[number]),
    }
}

/// Intitulé d'une action sur un processus dans la langue courante.
fn proc_label(action: ProcAction) -> String {
    match action {
        ProcAction::Terminate => ts("terminer (SIGTERM)").to_string(),
        ProcAction::Kill => ts("forcer l'arrêt (SIGKILL)").to_string(),
        ProcAction::Suspend => ts("suspendre (SIGSTOP)").to_string(),
        ProcAction::Resume => ts("reprendre (SIGCONT)").to_string(),
        ProcAction::Nice(value) => tf("priorité {}", &[&value]),
    }
}

fn save_prefs(dark: bool, accent: Accent, lang: Lang) {
    let Some(path) = prefs_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let accent_name = match accent {
        Accent::Bleu => "bleu",
        Accent::Violet => "violet",
        Accent::Vert => "vert",
        Accent::Orange => "orange",
        Accent::Rose => "rose",
    };
    let payload = serde_json::json!({ "dark": dark, "accent": accent_name, "lang": lang.code() });
    let _ = std::fs::write(path, payload.to_string());
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (stored_dark, accent, lang) = load_prefs();
        // Surcharges pour les captures d'écran et les tests : thème et langue.
        let stored_dark = match std::env::var("ORBISYS_THEME").as_deref() {
            Ok("dark") => Some(true),
            Ok("light") => Some(false),
            _ => stored_dark,
        };
        let lang = std::env::var("ORBISYS_LANG").ok().map(|code| Lang::from_code(&code)).unwrap_or(lang);
        orbisys::i18n_ui::set_lang(lang);
        // Les polices (dont la famille d'icônes) doivent exister dès la première image.
        cc.egui_ctx.set_fonts(ui_fonts::definitions_for(lang).0);
        let os = distro::current();
        let capabilities = Capabilities::probe();
        let manager = PackageManager::detect();
        let (tx, rx) = channel();
        let mut app = App {
            // `ORBISYS_PAGE=<nom>` ouvre directement un écran (tests, démonstrations).
            page: std::env::var("ORBISYS_PAGE")
                .ok()
                .and_then(|name| {
                    // Écrans dédiés d'abord : `processes` désigne la page Processus.
                    ALL_PAGES
                        .iter()
                        .copied()
                        .find(|page| page.key() == format!("page.{name}"))
                        .or_else(|| PanelId::from_name(&name).map(Page::Panel))
                })
                .unwrap_or(Page::Dashboard),
            dark: stored_dark.unwrap_or(true),
            accent,
            lang,
            os,
            capabilities,
            tx,
            rx,
            busy: 0,
            toasts: Vec::new(),
            inventory: None,
            updates: Vec::new(),
            mounts: Vec::new(),
            processes: Vec::new(),
            snapshot: Snapshot::default(),
            sampler: Sampler::new(),
            services: Vec::new(),
            audit: None,
            cleanup_items: Vec::new(),
            storage: None,
            duplicates: None,
            duplicate_root: dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .display()
                .to_string(),
            duplicate_min_size: 1024,
            hardware: None,
            network: None,
            firewall: None,
            firewall_port: "22/tcp".to_string(),
            backup: None,
            list_path: {
                let base = dirs::document_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                base.join("orbisys-logiciels.json")
                    .display()
                    .to_string()
            },
            rsync_source: dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .display()
                .to_string(),
            rsync_destination: {
                let base = dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                base.join(ts("Sauvegardes")).display().to_string()
            },
            rsync_mirror: false,
            rsync_delete: false,
            rsync_output: String::new(),
            kernel: None,
            kernel_values: Vec::new(),
            volumes: None,
            lv_group: String::new(),
            lv_name: String::new(),
            lv_size: "10G".to_string(),
            peripherals: None,
            desktop: None,
            theme_input: String::new(),
            icon_input: String::new(),
            font_input: String::new(),
            scheme_input: String::new(),
            automation: None,
            cron_schedule: "0 3 * * *".to_string(),
            cron_command: String::new(),
            locale: None,
            locale_input: String::new(),
            timezone_input: String::new(),
            optimize: None,
            snapshots: None,
            remote_source: dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .display()
                .to_string(),
            remote_target: String::new(),
            snapshot_output: String::new(),
            log_units: String::new(),
            log_since: String::new(),
            log_until: String::new(),
            log_level: "err".to_string(),
            log_grep: String::new(),
            log_limit: 200,
            log_results: String::new(),
            log_errors: Vec::new(),
            custom_units: Vec::new(),
            unit_filter: String::new(),
            unit_name: String::new(),
            unit_description: String::new(),
            unit_exec: String::new(),
            unit_user: String::new(),
            unit_editor: String::new(),
            unit_status: String::new(),
            netadv: None,
            netadv_host: "1.1.1.1".to_string(),
            netadv_output: String::new(),
            netadv_diag: None,
            cups_printers: Vec::new(),
            cups_jobs: Vec::new(),
            cups_sharing: None,
            cups_filter: String::new(),
            fonts: Vec::new(),
            font_cache: None,
            font_filter: String::new(),
            font_query: String::new(),
            font_match: String::new(),
            font_path: String::new(),
            quota_report: None,
            quota_user: String::new(),
            quota_mount: String::new(),
            quota_soft: String::new(),
            quota_hard: String::new(),
            quota_filter: String::new(),
            btrfs_report: None,
            btrfs_source: String::new(),
            btrfs_dest: String::new(),
            btrfs_filter: String::new(),
            cis_report: None,
            cis_filter: String::new(),
            orphan_report: None,
            orphan_filter: String::new(),
            perf_report: None,
            perf_swappiness: String::new(),
            perf_governor: String::new(),
            perf_thp: String::new(),
            klogs: None,
            klogs_filter: String::new(),
            klogs_errors_only: false,
            sessions_report: None,
            sessions_filter: String::new(),
            netmounts_report: None,
            netmounts_filter: String::new(),
            netmounts_output: String::new(),
            timers_report: None,
            timers_filter: String::new(),
            archive_path: String::new(),
            panels: std::collections::HashMap::new(),
            panel_loading: HashSet::new(),
            panel_loaded_at: std::collections::HashMap::new(),
            panel_filter: String::new(),
            panel_page: 0,
            panel_page_for: None,
            panel_inputs: std::collections::HashMap::new(),
            panel_output: None,
            selected_process: None,
            visited_pages: HashSet::new(),
            fonts_lang: None,
            font_missing: false,
            cpu_history: std::collections::VecDeque::new(),
            mem_history: std::collections::VecDeque::new(),
            core_history: Vec::new(),
            core_prev: Vec::new(),
            merged_tab: std::collections::HashMap::new(),
            sys_host: std::fs::read_to_string("/proc/sys/kernel/hostname").map(|h| h.trim().to_string()).unwrap_or_default(),
            sys_kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|k| k.trim().to_string()).unwrap_or_default(),
            cpu_mhz: 0.0,
            rx_history: std::collections::VecDeque::new(),
            tx_history: std::collections::VecDeque::new(),
            net_last: None,
            selected_history: std::collections::VecDeque::new(),
            process_nice: 10,
            last_alert_check: Instant::now(),
            alert_cooldown: std::collections::HashMap::new(),
            last_rule_check: Instant::now(),
            dns_server_input: String::new(),
            hosts_entries: Vec::new(),
            host_address: String::new(),
            host_name: String::new(),
            automation_rules: Vec::new(),
            rule_metric: 0,
            rule_threshold: 90,
            rule_action: 0,
            netmount_source: String::new(),
            netmount_point: "/mnt/partage".to_string(),
            netmount_options: String::new(),
            software_page: 0,
            search: String::new(),
            source_filter: None,
            selected: HashSet::new(),
            preview: None,
            confirm: None,
            palette: false,
            palette_query: String::new(),
            service_filter: String::new(),
            log_unit: String::new(),
            log_priority: logs::Priority::All,
            logs_text: String::new(),
            last_processes: Instant::now(),
            last_export: None,
        };
        let _ = manager;
        app.apply_theme(&cc.egui_ctx);
        app.refresh_all();
        app
    }

    fn toast(&mut self, message: String, ok: bool) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        // Messages des outils : traduits s'ils sont connus (en entier, ou de part et d'autre de « : »).
        let translated = tx(&message);
        let message = match message.split_once(" : ") {
            Some((head, tail)) if translated == message => format!("{} : {}", tx(head), tx(tail)),
            _ => translated,
        };
        self.toasts.push((message, ok, now + 6.0));
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn spawn<F>(&mut self, job: F)
    where
        F: FnOnce() -> Msg + Send + 'static,
    {
        self.busy += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let message = job();
            let _ = tx.send(message);
        });
    }

    fn refresh_all(&mut self) {
        self.spawn(|| {
            let capabilities = Capabilities::probe();
            Msg::Inventory(Inventory::load(&capabilities))
        });
        self.spawn(|| match pkg::all_updates() {
            Ok(updates) => Msg::Updates(updates),
            Err(error) => Msg::Error(error),
        });
        self.spawn(|| Msg::Disks(disk::mounts()));
        if self.capabilities.systemctl {
            self.spawn(|| match services::list() {
                Ok(services) => Msg::Services(services),
                Err(error) => Msg::Error(error),
            });
        }
        self.spawn(|| {
            let capabilities = Capabilities::probe();
            Msg::Audit(security::audit(&capabilities))
        });
        self.spawn(|| {
            let capabilities = Capabilities::probe();
            Msg::Cleanup(cleanup::scan(&capabilities))
        });
        self.spawn(|| Msg::Storage(storage::report()));
        self.spawn(|| Msg::Hardware(hardware::report()));
        self.spawn(|| Msg::Network(network::report()));
        self.spawn(|| Msg::Backup(backup::report()));
        self.spawn(|| Msg::Kernel(kernel::report()));
        self.spawn(|| Msg::Volumes(lvm::report()));
        self.spawn(|| Msg::Peripherals(peripherals::report()));
        self.spawn(|| Msg::Desktop(desktop::report()));
        self.spawn(|| Msg::Automation(automation::report()));
        self.spawn(|| Msg::Locale(locale::report()));
        self.spawn(|| Msg::Optimize(optimize::report()));
        self.spawn(|| Msg::Snapshots(snapshots::report()));
        self.spawn(|| Msg::Firewall(firewall::report()));
        self.reload_logs();
    }

    fn reload_logs(&mut self) {
        let unit = self.log_unit.trim().to_string();
        let priority = self.log_priority;
        self.spawn(move || {
            let unit = if unit.is_empty() { None } else { Some(unit.as_str()) };
            match logs::read(400, unit, priority) {
                Ok(text) => Msg::Logs(text),
                Err(error) => Msg::Error(error),
            }
        });
    }

    fn poll(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = self.busy.saturating_sub(1);
            match message {
                Msg::Inventory(inventory) => self.inventory = Some(inventory),
                Msg::Updates(updates) => self.updates = updates,
                Msg::UpdatesApplied(message, ok, updates) => {
                    self.updates = updates;
                    journal::record("mises à jour", "", ok, &message);
                    self.toast(message.lines().collect::<Vec<_>>().join(" · "), ok);
                }
                Msg::Disks(mounts) => self.mounts = mounts,
                Msg::Services(services) => self.services = services,
                Msg::Logs(text) => self.logs_text = text,
                Msg::Audit(audit) => self.audit = Some(audit),
                Msg::Cleanup(items) => self.cleanup_items = items,
                Msg::Storage(report) => self.storage = Some(report),
                Msg::Duplicates(scan) => self.duplicates = Some(scan),
                Msg::Hardware(report) => self.hardware = Some(report),
                Msg::Network(report) => self.network = Some(report),
                Msg::Firewall(report) => self.firewall = Some(report),
                Msg::Backup(report) => self.backup = Some(report),
                Msg::Kernel(report) => {
                    self.kernel_values = kernel::presets(&report);
                    self.kernel = Some(report);
                }
                Msg::Peripherals(report) => self.peripherals = Some(report),
                Msg::Automation(report) => self.automation = Some(report),
                Msg::Optimize(report) => self.optimize = Some(report),
                Msg::Snapshots(report) => self.snapshots = Some(report),
                Msg::LogResults(text) => self.log_results = text,
                Msg::RecurringErrors(errors) => self.log_errors = errors,
                Msg::CustomUnits(list) => self.custom_units = list,
                Msg::UnitText(text) => self.unit_editor = text,
                Msg::UnitStatus(text) => self.unit_status = text,
                Msg::NetAdv(data) => self.netadv = Some(data),
                Msg::NetAdvDiag(diag) => self.netadv_diag = Some(diag),
                Msg::CupsPrinters(list) => self.cups_printers = list,
                Msg::CupsJobs(list) => self.cups_jobs = list,
                Msg::CupsSharing(sharing) => self.cups_sharing = Some(sharing),
                Msg::Fonts(list) => self.fonts = list,
                Msg::Quotas(report) => self.quota_report = Some(report),
                Msg::Btrfs(report) => self.btrfs_report = Some(report),
                Msg::Cis(report) => self.cis_report = Some(report),
                Msg::Orphans(report) => self.orphan_report = Some(report),
                Msg::KernelLogs(report) => self.klogs = Some(report),
                Msg::Sessions(report) => self.sessions_report = Some(report),
                Msg::NetMounts(report) => self.netmounts_report = Some(report),
                Msg::Timers(report) => self.timers_report = Some(report),
                Msg::Panel(id, panel) => {
                    self.panel_loading.remove(&id);
                    self.panel_loaded_at.insert(id, Instant::now());
                    self.panels.insert(id, panel);
                }
                Msg::Breaches(list) => {
                    let due: Vec<(String, String)> = list
                        .into_iter()
                        .filter(|(key, _)| {
                            self.alert_cooldown
                                .get(key)
                                .map_or(true, |at| at.elapsed() >= Duration::from_secs(600))
                        })
                        .collect();
                    for (key, _) in &due {
                        self.alert_cooldown.insert(key.clone(), Instant::now());
                    }
                    if !due.is_empty() {
                        self.spawn(move || {
                            let body = due
                                .iter()
                                .map(|(_, message)| message.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let _ = Cmd::new("Alerte", "notify-send", &["OrbiSys", &body]).run();
                            Msg::Action(tf("Alerte : {}", &[&body]), false)
                        });
                    }
                }
                Msg::RuleAlerts(messages) => {
                    if let Some(first) = messages.first() {
                        let message = format!("Seuil : {first}");
                        self.toast(message, false);
                    }
                }
                Msg::PanelDone { id, label, ok, text } => {
                    journal::record(&label, "", ok, &text);
                    let first = text.lines().next().unwrap_or("").to_string();
                    self.toast(
                        if text == label { label.clone() } else { format!("{label} : {}", util::ellipsis(&first, 110)) },
                        ok,
                    );
                    if text.lines().count() > 1 {
                        self.panel_output = Some((label, text));
                    }
                    self.apply_pending(Pending::PanelLoad(id));
                }
                Msg::Perf(report) => {
                    if self.perf_swappiness.is_empty() {
                        self.perf_swappiness = report
                            .swappiness
                            .clone()
                            .unwrap_or_else(|| "60".to_string());
                    }
                    if self.perf_governor.is_empty() {
                        self.perf_governor = report.dominant_governor();
                    }
                    if self.perf_thp.is_empty() {
                        self.perf_thp = report
                            .thp
                            .clone()
                            .unwrap_or_else(|| "madvise".to_string());
                    }
                    self.perf_report = Some(report);
                }
                Msg::FontCache(cache) => self.font_cache = Some(*cache),
                Msg::FontMatch(found) => {
                    self.font_match = if found.substituted {
                        tf("« {} » absente → remplacée par « {} »", &[&found.requested, &found.resolved])
                    } else {
                        tf("« {} » installée", &[&found.requested])
                    }
                }
                Msg::Locale(report) => {
                    if self.locale_input.is_empty() {
                        self.locale_input = if report.locale.system_lang.is_empty() {
                            report.locale.session_lang.clone()
                        } else {
                            report.locale.system_lang.clone()
                        };
                        self.timezone_input = report.locale.timezone.clone();
                    }
                    self.locale = Some(report);
                }
                Msg::Desktop(report) => {
                    if self.theme_input.is_empty() {
                        self.theme_input = report.appearance.gtk_theme.clone();
                        self.icon_input = report.appearance.icon_theme.clone();
                        self.font_input = report.appearance.font.clone();
                        self.scheme_input = report.appearance.color_scheme.clone();
                    }
                    self.desktop = Some(report);
                }
                Msg::Volumes(report) => {
                    if self.lv_group.is_empty() {
                        if let Some(group) = report.groups.first() {
                            self.lv_group = group.name.clone();
                        }
                    }
                    self.volumes = Some(report);
                }
                Msg::Preview(preview) => self.preview = Some(preview),
                Msg::Action(message, ok) => {
                    journal::record("action", "", ok, &message);
                    if message.contains("rsync") {
                        self.rsync_output = message.clone();
                    }
                    if message.contains("rsync") || message.contains("archive") {
                        self.snapshot_output = message.clone();
                    }
                    if message.contains("ping") || message.contains("getent") || message.contains("icmp") {
                        self.netadv_output = message.clone();
                    }
                    if message.contains("Export list") || message.contains("Sharename") || message.contains("monté :") || message.contains("démonté :") {
                        self.netmounts_output = message.clone();
                    }
                    self.toast(message, ok);
                }
                Msg::Error(error) => self.toast(error, false),
            }
        }
    }

    fn apply_pending(&mut self, pending: Pending) {
        match pending {
            Pending::PreviewNative(names) => {
                let Some(manager) = self.inventory.as_ref().and_then(|inventory| inventory.manager)
                else {
                    self.toast(ts("Aucun gestionnaire natif détecté").to_string(), false);
                    return;
                };
                self.spawn(move || match manager.removal_preview(&names) {
                    Ok(preview) => Msg::Preview(preview),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::Remove { names, source } => {
                let manager = self.inventory.as_ref().and_then(|inventory| inventory.manager);
                match source {
                    Source::Native => {
                        let Some(manager) = manager else { return };
                        self.spawn(move || match manager.remove(&names) {
                            Ok(message) => Msg::Action(message, true),
                            Err(error) => Msg::Error(error),
                        });
                    }
                    Source::Snap => {
                        self.spawn(move || {
                            let mut args = vec!["remove".to_string()];
                            args.extend(names.iter().cloned());
                            match orbisys::exec::run_privileged("snap", &args) {
                                Ok(message) => Msg::Action(message, true),
                                Err(error) => Msg::Error(error),
                            }
                        });
                    }
                    Source::Flatpak => {
                        self.spawn(move || {
                            let mut args = vec!["uninstall".to_string(), "-y".to_string()];
                            args.extend(names.iter().cloned());
                            match orbisys::exec::run_privileged("flatpak", &args) {
                                Ok(message) => Msg::Action(message, true),
                                Err(error) => Msg::Error(error),
                            }
                        });
                    }
                    Source::Manual => {}
                }
                self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Inventory(Inventory::load(&capabilities))
                });
            }
            Pending::RemoveManual(path) => {
                let app = self
                    .inventory
                    .as_ref()
                    .and_then(|inventory| inventory.manual.iter().find(|app| app.path == path).cloned());
                let Some(app) = app else {
                    self.toast(ts("logiciel introuvable dans l'inventaire").to_string(), false);
                    return;
                };
                self.spawn(move || match manual::remove(&app) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(error),
                });
                self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Inventory(Inventory::load(&capabilities))
                });
            }
            Pending::Service(unit, action) => {
                self.spawn(move || match services::action(&unit, action) {
                    Ok(message) => Msg::Action(
                        format!(
                            "{} : {}",
                            ts(action.label()),
                            if message.is_empty() {
                                ts("fait").to_string()
                            } else {
                                message
                            }
                        ),
                        true,
                    ),
                    Err(error) => Msg::Error(error),
                });
                self.spawn(|| match services::list() {
                    Ok(services) => Msg::Services(services),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::Clean(item) => {
                let label = item.label.clone();
                self.spawn(move || match cleanup::run(&item) {
                    Ok(message) => Msg::Action(format!("{label} : {message}"), true),
                    Err(error) => Msg::Error(format!("{label} : {error}")),
                });
                self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Cleanup(cleanup::scan(&capabilities))
                });
                self.spawn(|| Msg::Disks(disk::mounts()));
            }
            Pending::Refresh(page) => match page {
                Page::Updates => self.spawn(|| match pkg::all_updates() {
                    Ok(updates) => Msg::Updates(updates),
                    Err(error) => Msg::Error(error),
                }),
                Page::Disk => self.spawn(|| Msg::Disks(disk::mounts())),
                Page::Cleanup => self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Cleanup(cleanup::scan(&capabilities))
                }),
                Page::Services => self.spawn(|| match services::list() {
                    Ok(services) => Msg::Services(services),
                    Err(error) => Msg::Error(error),
                }),
                Page::Cis => self.spawn(|| Msg::Cis(Box::new(cis::report()))),
                Page::Orphans => self.spawn(|| Msg::Orphans(Box::new(orphans::report()))),
                Page::Perf => self.spawn(|| Msg::Perf(Box::new(perf::report()))),
                Page::KernelLogs => self.spawn(|| Msg::KernelLogs(Box::new(klogs::report()))),
                Page::Sessions => self.spawn(|| Msg::Sessions(Box::new(sessions::report()))),
                Page::NetMounts => self.spawn(|| Msg::NetMounts(Box::new(netmounts::report()))),
                Page::Timers => self.spawn(|| Msg::Timers(Box::new(timers::report()))),
                Page::Panel(id) => self.apply_pending(Pending::PanelLoad(id)),
                Page::Security => self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Audit(security::audit(&capabilities))
                }),
                Page::Storage => self.spawn(|| Msg::Storage(storage::report())),
                Page::Quotas => self.spawn(|| Msg::Quotas(Box::new(quota::report()))),
                Page::Hardware => self.spawn(|| Msg::Hardware(hardware::report())),
                Page::Network => {
                    self.hosts_entries = netadv::hosts();
                    self.spawn(|| Msg::Network(network::report()));
                }
                Page::NetAdvanced => self.spawn(|| Msg::NetAdv(Box::new(netadv::collect()))),
                Page::Backup => self.spawn(|| Msg::Backup(backup::report())),
                Page::Kernel => self.spawn(|| Msg::Kernel(kernel::report())),
                Page::Volumes => self.spawn(|| Msg::Volumes(lvm::report())),
                Page::Peripherals => self.spawn(|| Msg::Peripherals(peripherals::report())),
                Page::Printers => {
                    self.spawn(|| Msg::CupsPrinters(cups::printers().unwrap_or_default()));
                    self.spawn(|| Msg::CupsJobs(cups::jobs()));
                    self.spawn(|| Msg::CupsSharing(Box::new(cups::sharing())));
                }
                Page::Desktop => self.spawn(|| Msg::Desktop(desktop::report())),
                Page::Automation => {
                    self.automation_rules = automation::load_rules();
                    self.spawn(|| Msg::Automation(automation::report()));
                }
                Page::Locale => self.spawn(|| Msg::Locale(locale::report())),
                Page::Fonts => {
                    self.spawn(|| match fonts::families() {
                        Ok(list) => Msg::Fonts(list),
                        Err(error) => Msg::Error(tf("polices : {}", &[&error])),
                    });
                    self.spawn(|| Msg::FontCache(Box::new(fonts::cache_info())));
                }
                Page::Optimize => self.spawn(|| Msg::Optimize(optimize::report())),
                Page::Snapshots => self.spawn(|| Msg::Snapshots(snapshots::report())),
                Page::Btrfs => self.spawn(|| Msg::Btrfs(Box::new(btrfs::report()))),
                Page::LogAdvanced => {}
                Page::CustomUnits => self.spawn(|| match units::list_units() {
                    Ok(list) => Msg::CustomUnits(list),
                    Err(error) => Msg::Error(tf("unités : {}", &[&error])),
                }),
                Page::Firewall => self.spawn(|| Msg::Firewall(firewall::report())),
                Page::Duplicates => {
                    let root = std::path::PathBuf::from(self.duplicate_root.clone());
                    let min_size = self.duplicate_min_size;
                    self.spawn(move || {
                        Msg::Duplicates(storage::find_duplicates(&root, min_size, 200_000))
                    });
                }
                Page::Logs => self.reload_logs(),
                _ => self.refresh_all(),
            },
            Pending::ExportList => {
                let inventory = self.inventory.clone();
                let path = std::path::PathBuf::from(self.list_path.clone());
                self.spawn(move || {
                    let Some(inventory) = inventory else {
                        return Msg::Error(ts("inventaire non chargé").to_string());
                    };
                    let list = backup::software_list(&inventory);
                    match backup::write_list(&list, &path) {
                        Ok(()) => Msg::Action(
                            tf("liste exportée : {} ({} logiciels)", &[&path.display(), &list.packages.len()]),
                            true,
                        ),
                        Err(error) => Msg::Error(tf("export impossible : {}", &[&error])),
                    }
                });
            }
            Pending::ImportList(path) => {
                self.spawn(move || {
                    match backup::read_list(&path).and_then(|list| backup::install(&list)) {
                        Ok(message) => Msg::Action(message, true),
                        Err(error) => Msg::Error(error),
                    }
                });
                self.spawn(|| {
                    let capabilities = Capabilities::probe();
                    Msg::Inventory(Inventory::load(&capabilities))
                });
            }
            Pending::Rsync(options) => {
                let dry_run = options.dry_run;
                self.spawn(move || match backup::rsync(&options) {
                    Ok(output) => Msg::Action(
                        tf("{}rsync terminé\n{}", &[&(if dry_run { ts("simulation : ") } else { "" }), &output]),
                        true,
                    ),
                    Err(error) => Msg::Error(tf("rsync : {}", &[&error])),
                });
            }
            Pending::RegenerateGrub => {
                self.spawn(|| match backup::regenerate_grub() {
                    Ok(message) => Msg::Action(
                        if message.trim().is_empty() {
                            ts("GRUB régénéré").to_string()
                        } else {
                            format!("GRUB : {message}")
                        },
                        true,
                    ),
                    Err(error) => Msg::Error(format!("GRUB : {error}")),
                });
            }
            Pending::ApplySysctl(entries) => {
                self.spawn(move || match kernel::apply_sysctl(&entries) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("sysctl : {error}")),
                });
            }
            Pending::RemoveKernels(packages) => {
                self.spawn(move || match kernel::remove_kernels(&packages) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("noyaux : {}", &[&error])),
                });
                self.spawn(|| Msg::Kernel(kernel::report()));
            }
            Pending::InstallDriver(driver) => {
                self.spawn(move || match kernel::install_driver(&driver) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("pilote : {}", &[&error])),
                });
            }
            Pending::CreateVolume { group, name, size } => {
                self.spawn(move || match lvm::create_volume(&group, &name, &size) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("LVM : {error}")),
                });
                self.spawn(|| Msg::Volumes(lvm::report()));
            }
            Pending::RemoveVolume(reference) => {
                self.spawn(move || match lvm::remove_volume(&reference) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("LVM : {error}")),
                });
                self.spawn(|| Msg::Volumes(lvm::report()));
            }
            Pending::CloseCrypt(name) => {
                self.spawn(move || match lvm::close_crypt(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("LUKS : {error}")),
                });
                self.spawn(|| Msg::Volumes(lvm::report()));
            }
            Pending::MountDevice { device, mount } => {
                self.spawn(move || match lvm::mount_device(&device, mount) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("montage : {}", &[&error])),
                });
                self.spawn(|| Msg::Volumes(lvm::report()));
            }
            Pending::SetPrinterEnabled { name, enabled } => {
                self.spawn(move || match peripherals::set_printer_enabled(&name, enabled) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("imprimante : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::RemovePrinter(name) => {
                self.spawn(move || match peripherals::remove_printer(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("imprimante : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::SetDefaultPrinter(name) => {
                self.spawn(move || match peripherals::set_default_printer(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("imprimante : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::SetDefaultSink(name) => {
                self.spawn(move || match peripherals::set_default_sink(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("audio : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::ToggleMute(name) => {
                self.spawn(move || match peripherals::toggle_mute(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("audio : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::AdjustVolume { name, delta } => {
                self.spawn(move || match peripherals::adjust_volume(&name, delta) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("audio : {}", &[&error])),
                });
                self.spawn(|| Msg::Peripherals(peripherals::report()));
            }
            Pending::SetGsetting { schema, key, value } => {
                self.spawn(move || match desktop::set_setting(&schema, &key, &value) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("gsettings : {error}")),
                });
                self.spawn(|| Msg::Desktop(desktop::report()));
            }
            Pending::ToggleExtension { id, enabled } => {
                self.spawn(move || match desktop::set_extension(&id, enabled) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("extension : {}", &[&error])),
                });
                self.spawn(|| Msg::Desktop(desktop::report()));
            }
            Pending::AddCron { schedule, command } => {
                self.spawn(move || match automation::add_job(&schedule, &command) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("cron : {error}")),
                });
                self.spawn(|| Msg::Automation(automation::report()));
            }
            Pending::RemoveCron { schedule, command } => {
                self.spawn(move || match automation::remove_job(&schedule, &command) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("cron : {error}")),
                });
                self.spawn(|| Msg::Automation(automation::report()));
            }
            Pending::SetTimer { unit, enabled } => {
                self.spawn(move || match automation::set_timer(&unit, enabled) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("minuteur : {}", &[&error])),
                });
                self.spawn(|| Msg::Automation(automation::report()));
            }
            Pending::SetLocale(code) => {
                self.spawn(move || match locale::set_system_locale(&code) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("locale : {}", &[&error])),
                });
                self.spawn(|| Msg::Locale(locale::report()));
            }
            Pending::SetTimezone(zone) => {
                self.spawn(move || match locale::set_timezone(&zone) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("fuseau : {}", &[&error])),
                });
                self.spawn(|| Msg::Locale(locale::report()));
            }
            Pending::ProcessAction { pid, name, action } => {
                self.spawn(move || match orbisys::processes::apply(pid, &name, action) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("processus : {}", &[&error])),
                });
            }
            Pending::DeleteDuplicates(group) => {
                let root = std::path::PathBuf::from(self.duplicate_root.clone());
                if let Some(scan) = self.duplicates.as_mut() {
                    scan.groups.retain(|other| other.fingerprint != group.fingerprint);
                }
                self.spawn(move || match storage::delete_duplicates(&group, &root) {
                    Ok((count, bytes)) => Msg::Action(
                        tf("{} copie(s) supprimée(s), {} libérés", &[&count, &util::human_bytes(bytes)]),
                        true,
                    ),
                    Err(error) => Msg::Error(tf("doublons : {}", &[&error])),
                });
            }
            Pending::ExportDuplicatesCsv => {
                let Some(scan) = self.duplicates.clone() else {
                    self.toast(ts("Aucune analyse à exporter").to_string(), false);
                    return;
                };
                let path = dirs::document_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("doublons.csv");
                self.spawn(move || match std::fs::write(&path, storage::duplicates_csv(&scan)) {
                    Ok(()) => Msg::Action(tf("Exporté : {}", &[&path.display()]), true),
                    Err(error) => Msg::Error(tf("export : {}", &[&error])),
                });
            }
            Pending::ApplyUpdates => {
                let manager = self.inventory.as_ref().and_then(|inventory| inventory.manager);
                let pending = self.updates.clone();
                self.spawn(move || {
                    let result = pkg::apply_updates(manager, &pending);
                    let remaining = pkg::all_updates().unwrap_or_default();
                    match result {
                        Ok(message) => Msg::UpdatesApplied(message, true, remaining),
                        Err(error) => Msg::UpdatesApplied(error, false, remaining),
                    }
                });
            }
            Pending::PanelLoad(id) => {
                if self.panel_loading.insert(id) {
                    self.spawn(move || Msg::Panel(id, Box::new(id.collect())));
                }
            }
            Pending::PanelRun(cmd, id) => {
                self.spawn(move || {
                    let label = cmd.label.clone();
                    match cmd.run() {
                        Ok(text) => Msg::PanelDone { id, label, ok: true, text },
                        Err(text) => Msg::PanelDone { id, label, ok: false, text },
                    }
                });
            }
            Pending::TimersReload => {
                self.spawn(|| Msg::Timers(Box::new(timers::report())));
            }
            Pending::TimerAction { unit, action } => {
                self.spawn(move || match timers::control(&unit, &action) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("timer : {}", &[&error])),
                });
            }
            Pending::NetMountsReload => {
                self.spawn(|| Msg::NetMounts(Box::new(netmounts::report())));
            }
            Pending::NetMountToggle { path, mount } => {
                self.spawn(move || {
                    let result = if mount {
                        netmounts::mount_path(&path)
                    } else {
                        netmounts::unmount_path(&path)
                    };
                    match result {
                        Ok(message) => Msg::Action(message, true),
                        Err(error) => Msg::Error(tf("montage : {}", &[&error])),
                    }
                });
            }
            Pending::NetMountProbe { server, fstype } => {
                self.spawn(move || match netmounts::probe(&server, &fstype) {
                    Ok(output) => Msg::Action(output, true),
                    Err(error) => Msg::Error(tf("test serveur : {}", &[&error])),
                });
            }
            Pending::SessionsReload => {
                self.spawn(|| Msg::Sessions(Box::new(sessions::report())));
            }
            Pending::SessionLock { user, locked } => {
                self.spawn(move || match sessions::set_locked(&user, locked) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("compte : {}", &[&error])),
                });
            }
            Pending::SessionTerminate(id) => {
                self.spawn(move || match sessions::terminate(&id) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("session : {}", &[&error])),
                });
            }
            Pending::KernelLogsReload => {
                self.spawn(|| Msg::KernelLogs(Box::new(klogs::report())));
            }
            Pending::PerfReload => {
                self.spawn(|| Msg::Perf(Box::new(perf::report())));
            }
            Pending::PerfGovernor(governor) => {
                self.spawn(move || match perf::set_governor(&governor) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("gouverneur : {}", &[&error])),
                });
            }
            Pending::PerfSwappiness(value) => {
                self.spawn(move || match perf::set_swappiness(value) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("swappiness : {error}")),
                });
            }
            Pending::PerfThp(mode) => {
                self.spawn(move || match perf::set_thp(&mode) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("THP : {error}")),
                });
            }
            Pending::PerfProfile(name) => {
                self.spawn(move || match perf::apply_profile(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("profil : {}", &[&error])),
                });
            }
            Pending::OrphansReload => {
                self.spawn(|| Msg::Orphans(Box::new(orphans::report())));
            }
            Pending::OrphansAutoremove => {
                self.spawn(|| match orphans::autoremove() {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("autoremove : {error}")),
                });
            }
            Pending::OrphansPurge(names) => {
                self.spawn(move || match orphans::purge(&names) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("purge : {}", &[&error])),
                });
            }
            Pending::CisReload => {
                self.spawn(|| Msg::Cis(Box::new(cis::report())));
            }
            Pending::BtrfsReload => {
                self.spawn(|| Msg::Btrfs(Box::new(btrfs::report())));
            }
            Pending::BtrfsCreate {
                source,
                destination,
            } => {
                self.spawn(move || match btrfs::create_snapshot(&source, &destination) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("instantané : {}", &[&error])),
                });
            }
            Pending::BtrfsDelete(path) => {
                self.spawn(move || match btrfs::delete_subvolume(&path) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("suppression : {}", &[&error])),
                });
            }
            Pending::QuotasReload => {
                self.spawn(|| Msg::Quotas(Box::new(quota::report())));
            }
            Pending::QuotasEnable(mount) => {
                self.spawn(move || match quota::enable(&mount) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("quotaon : {error}")),
                });
            }
            Pending::SetQuota {
                user,
                mount,
                soft,
                hard,
            } => {
                self.spawn(move || match quota::set_quota(&user, &mount, soft, hard) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("setquota : {error}")),
                });
            }
            Pending::FontsReload => {
                self.spawn(|| match fonts::families() {
                    Ok(list) => Msg::Fonts(list),
                    Err(error) => Msg::Error(tf("polices : {}", &[&error])),
                });
                self.spawn(|| Msg::FontCache(Box::new(fonts::cache_info())));
            }
            Pending::FontMatchQuery(name) => {
                self.spawn(move || match fonts::match_family(&name) {
                    Ok(found) => Msg::FontMatch(Box::new(found)),
                    Err(error) => Msg::Error(tf("substitution : {}", &[&error])),
                });
            }
            Pending::FontsInstall(path) => {
                self.spawn(move || {
                    let path = std::path::PathBuf::from(path);
                    match fonts::install(&path) {
                        Ok(message) => Msg::Action(message, true),
                        Err(error) => Msg::Error(tf("installation : {}", &[&error])),
                    }
                });
            }
            Pending::FontsRemove(path) => {
                self.spawn(move || {
                    let path = std::path::PathBuf::from(path);
                    match fonts::remove(&path) {
                        Ok(message) => Msg::Action(message, true),
                        Err(error) => Msg::Error(tf("suppression : {}", &[&error])),
                    }
                });
            }
            Pending::FontsRefresh => {
                self.spawn(|| match fonts::refresh_cache() {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("cache : {}", &[&error])),
                });
            }
            Pending::CupsReload => {
                self.spawn(|| Msg::CupsPrinters(cups::printers().unwrap_or_default()));
                self.spawn(|| Msg::CupsJobs(cups::jobs()));
                self.spawn(|| Msg::CupsSharing(Box::new(cups::sharing())));
            }
            Pending::CupsCancelJob(id) => {
                self.spawn(move || match cups::cancel_job(&id) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("annulation : {}", &[&error])),
                });
            }
            Pending::CupsClearQueue(name) => {
                self.spawn(move || match cups::cancel_all(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("file : {}", &[&error])),
                });
            }
            Pending::CupsSetEnabled { name, enabled } => {
                self.spawn(move || match cups::set_enabled(&name, enabled) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("imprimante : {}", &[&error])),
                });
            }
            Pending::CupsRemove(name) => {
                self.spawn(move || match cups::remove(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("suppression : {}", &[&error])),
                });
            }
            Pending::NetAdvReload => {
                self.spawn(|| Msg::NetAdv(Box::new(netadv::collect())));
            }
            Pending::NetAdvPing(host) => {
                self.spawn(move || match netadv::ping(&host) {
                    Ok(output) => Msg::Action(output, true),
                    Err(error) => Msg::Error(tf("ping : {}", &[&error])),
                });
            }
            Pending::NetAdvResolve(host) => {
                self.spawn(move || match netadv::resolve(&host) {
                    Ok(output) => Msg::Action(output, true),
                    Err(error) => Msg::Error(tf("résolution : {}", &[&error])),
                });
            }
            Pending::NetAdvDiagnose => {
                self.spawn(|| Msg::NetAdvDiag(Box::new(netadv::diagnose())));
            }
            Pending::UnitsReload => {
                self.spawn(|| match units::list_units() {
                    Ok(list) => Msg::CustomUnits(list),
                    Err(error) => Msg::Error(tf("unités : {}", &[&error])),
                });
            }
            Pending::LoadUnit(name) => {
                self.spawn(move || match units::read_unit(&name) {
                    Ok(text) => Msg::UnitText(text),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::SaveUnit { name, text } => {
                self.spawn(move || match units::install(&name, &text) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("installation : {}", &[&error])),
                });
            }
            Pending::RemoveUnit(name) => {
                self.spawn(move || match units::remove(&name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(tf("suppression : {}", &[&error])),
                });
            }
            Pending::UnitAction(action, name) => {
                self.spawn(move || match units::control(&action, &name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("systemctl : {error}")),
                });
            }
            Pending::UnitStatus(name) => {
                self.spawn(move || match units::status(&name) {
                    Ok(text) => Msg::UnitStatus(text),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::SearchLogs(query) => {
                self.spawn(move || match logsearch::search(&query) {
                    Ok(text) => Msg::LogResults(text),
                    Err(error) => Msg::Error(tf("journaux : {}", &[&error])),
                });
            }
            Pending::AnalyzeErrors => {
                self.spawn(|| match logsearch::recurring_errors(20) {
                    Ok(errors) => Msg::RecurringErrors(errors),
                    Err(error) => Msg::Error(tf("analyse : {}", &[&error])),
                });
            }
            Pending::ExportLogs => {
                let text = self.log_results.clone();
                self.spawn(move || {
                    let directory = dirs::document_dir()
                        .or_else(|| dirs::home_dir().map(|home| home.join("Documents")))
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                    let path = directory.join(format!("orbisys-journal-{stamp}.txt"));
                    match logsearch::export(&text, &path) {
                        Ok(()) => Msg::Action(tf("Journal exporté : {}", &[&path.display()]), true),
                        Err(error) => Msg::Error(tf("export : {}", &[&error])),
                    }
                });
            }
            Pending::RemoteCheck => {
                let target = self.remote_target.clone();
                self.spawn(move || match snapshots::check_remote(&target) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::RemoteBackup {
                source,
                target,
                dry_run,
            } => {
                self.spawn(move || match snapshots::remote_backup(&source, &target, dry_run) {
                    Ok(output) => Msg::Action(
                        tf("{}rsync distant\n{}", &[&(if dry_run { ts("simulation : ") } else { "" }), &output]),
                        true,
                    ),
                    Err(error) => Msg::Error(tf("rsync distant : {}", &[&error])),
                });
            }
            Pending::VerifyArchive(path) => {
                let path = std::path::PathBuf::from(path);
                self.spawn(move || match snapshots::verify_archive(&path) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(error),
                });
            }
            Pending::RunTrim => {
                self.spawn(|| match optimize::run_trim() {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("TRIM : {error}")),
                });
                self.spawn(|| Msg::Optimize(optimize::report()));
            }
            Pending::SetGovernor(governor) => {
                self.spawn(move || match optimize::set_governor(&governor) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("CPU : {error}")),
                });
                self.spawn(|| Msg::Optimize(optimize::report()));
            }
            Pending::Firewall(action) => {
                let label = firewall_label(&action);
                self.spawn(move || match firewall::apply(&action) {
                    Ok(message) => Msg::Action(
                        format!(
                            "{label} : {}",
                            if message.is_empty() { ts("fait") } else { message.as_str() }
                        ),
                        true,
                    ),
                    Err(error) => Msg::Error(format!("{label} : {error}")),
                });
                self.spawn(|| Msg::Firewall(firewall::report()));
            }
            Pending::ApplyDns(servers) => {
                self.spawn(move || match netadv::apply_dns(&servers) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("DNS : {error}")),
                });
                self.apply_pending(Pending::Refresh(Page::Network));
            }
            Pending::AddHost { address, name } => {
                self.spawn(move || match netadv::add_host(&address, &name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("/etc/hosts : {error}")),
                });
                self.host_address.clear();
                self.host_name.clear();
                self.apply_pending(Pending::Refresh(Page::Network));
            }
            Pending::RemoveHost { address, name } => {
                self.spawn(move || match netadv::remove_host(&address, &name) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("/etc/hosts : {error}")),
                });
                self.apply_pending(Pending::Refresh(Page::Network));
            }
            Pending::SetZram(enabled) => {
                self.spawn(move || match kernel::set_zram(enabled) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("zram : {error}")),
                });
                self.apply_pending(Pending::Refresh(Page::Kernel));
            }
            Pending::AddRule {
                metric,
                threshold,
                action,
            } => match automation::add_rule(automation::Rule {
                metric,
                threshold,
                action,
            }) {
                Ok(message) => {
                    self.automation_rules = automation::load_rules();
                    self.toast(message, true);
                }
                Err(error) => self.toast(format!("règle : {error}"), false),
            },
            Pending::RemoveRule(index) => match automation::remove_rule(index) {
                Ok(message) => {
                    self.automation_rules = automation::load_rules();
                    self.toast(message, true);
                }
                Err(error) => self.toast(error, false),
            },
            Pending::TestRuleNotification => {
                self.spawn(|| match automation::test_notification() {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("notification : {error}")),
                });
            }
            Pending::NetMountRemote {
                source,
                point,
                options,
            } => {
                self.spawn(move || match netmounts::mount_remote(&source, &point, &options) {
                    Ok(message) => Msg::Action(message, true),
                    Err(error) => Msg::Error(format!("montage : {error}")),
                });
                self.apply_pending(Pending::Refresh(Page::NetMounts));
            }
            Pending::Export(format) => {
                self.spawn(move || {
                    let report = Report::collect();
                    let directory = dirs::document_dir()
                        .or_else(|| dirs::home_dir().map(|home| home.join("Documents")))
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                    let path = directory.join(format!("orbisys-{stamp}.{format}"));
                    let result = match format {
                        "txt" => report.write_txt(&path),
                        "csv" => report.write_csv(&path),
                        "json" => report.write_json(&path),
                        _ => report.write_pdf(&path),
                    };
                    match result {
                        Ok(()) => Msg::Action(
                            tf("Rapport écrit : {}", &[&path.display()]),
                            true,
                        ),
                        Err(error) => Msg::Error(tf("export impossible : {}", &[&error])),
                    }
                });
            }
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let accent = self.accent.color(self.dark);
        let (text, muted, panel, card, border, field) = if self.dark {
            (
                egui::Color32::from_rgb(245, 237, 230),
                egui::Color32::from_rgb(181, 166, 152),
                egui::Color32::from_rgb(15, 11, 8),
                egui::Color32::from_rgb(26, 20, 16),
                egui::Color32::from_rgb(44, 34, 27),
                egui::Color32::from_rgb(33, 26, 20),
            )
        } else {
            (
                egui::Color32::from_rgb(31, 22, 16),
                egui::Color32::from_rgb(94, 77, 64),
                egui::Color32::from_rgb(250, 245, 238),
                egui::Color32::from_rgb(255, 250, 243),
                egui::Color32::from_rgb(230, 217, 200),
                egui::Color32::from_rgb(246, 238, 227),
            )
        };

        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.override_text_color = Some(text);
        visuals.panel_fill = panel;
        visuals.window_fill = card;
        visuals.extreme_bg_color = field;
        visuals.faint_bg_color = card;
        visuals.code_bg_color = field;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
        visuals.hyperlink_color = accent;
        visuals.widgets.noninteractive.bg_fill = card;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, muted);
        visuals.widgets.inactive.weak_bg_fill = card;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
        visuals.widgets.hovered.weak_bg_fill = field;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
        visuals.widgets.active.weak_bg_fill = field;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent);
        visuals.window_rounding = 18.0_f32.into();
        visuals.menu_rounding = 12.0_f32.into();
        visuals.widgets.noninteractive.rounding = 12.0_f32.into();
        visuals.widgets.inactive.rounding = 12.0_f32.into();
        visuals.widgets.hovered.rounding = 12.0_f32.into();
        visuals.widgets.active.rounding = 12.0_f32.into();
        visuals.widgets.open.rounding = 12.0_f32.into();
        ctx.set_visuals(visuals);
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(14.0, 8.0);
            style.spacing.interact_size.y = 22.0;
            style.visuals.window_shadow = egui::epaint::Shadow::NONE;
        });
    }

    fn tr(&self, key: &str) -> &'static str {
        i18n::t(self.lang, key)
    }

    fn muted(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(181, 166, 152)
        } else {
            egui::Color32::from_rgb(94, 77, 64)
        }
    }

    fn ok_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(52, 211, 153)
        } else {
            egui::Color32::from_rgb(4, 120, 87)
        }
    }

    fn warn_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(251, 191, 36)
        } else {
            egui::Color32::from_rgb(180, 83, 9)
        }
    }

    fn danger_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(248, 113, 113)
        } else {
            egui::Color32::from_rgb(185, 28, 28)
        }
    }

    fn accent_color(&self) -> egui::Color32 {
        self.accent.color(self.dark)
    }

    fn card_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(26, 20, 16)
        } else {
            egui::Color32::from_rgb(255, 250, 243)
        }
    }

    fn border_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(44, 34, 27)
        } else {
            egui::Color32::from_rgb(230, 217, 200)
        }
    }

    fn sidebar_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(21, 16, 12)
        } else {
            egui::Color32::from_rgb(244, 236, 225)
        }
    }

    fn hover_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(33, 26, 20)
        } else {
            egui::Color32::from_rgb(237, 227, 213)
        }
    }

    fn track_color(&self) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgb(47, 38, 31)
        } else {
            egui::Color32::from_rgb(233, 222, 208)
        }
    }

    /// Carte : fond, bordure et arrondi identiques partout.
    fn card_frame(&self) -> egui::Frame {
        egui::Frame::none()
            .fill(self.card_color())
            .stroke(egui::Stroke::new(1.0_f32, self.border_color()))
            .rounding(12.0_f32)
            .inner_margin(egui::Margin {
                left: 16.0,
                right: 16.0,
                top: 14.0,
                bottom: 14.0,
            })
    }

    /// Entrée de menu : pastille orange pour la page affichée.
    fn nav_item(&self, ui: &mut egui::Ui, page: Page) -> bool {
        let selected = self.page == page;
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            if selected {
                // Fond « actif » teinté orange et fine bordure (maquette).
                let (fill, line) = if self.dark {
                    (egui::Color32::from_rgb(58, 31, 13), egui::Color32::from_rgb(124, 58, 18))
                } else {
                    (egui::Color32::from_rgb(251, 227, 207), egui::Color32::from_rgb(240, 184, 138))
                };
                painter.rect(rect, 8.0, fill, egui::Stroke::new(1.0_f32, line));
            } else if response.hovered() {
                painter.rect_filled(rect, 8.0, self.hover_color());
            }
            let color = if selected {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            };
            painter.text(
                egui::pos2(rect.left() + 12.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                icons::for_page_key(page.key()).to_string(),
                icons::font(16.0),
                self.accent_color(),
            );
            painter.text(
                egui::pos2(rect.left() + 38.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                util::ellipsis(page.title(self.lang), 27),
                egui::FontId::proportional(13.0),
                color,
            );
        }
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    }

    /// Jauge circulaire : anneau de fond, arc d'accent et pourcentage au centre.
    fn page_header(&self, ui: &mut egui::Ui, page: Page) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(page.title(self.lang))
                        .size(21.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(page.hint(self.lang))
                        .color(self.muted())
                        .size(12.5),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if page == Page::Dashboard {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            ts("Démarrage"),
                            // La durée est écrite avec des unités françaises (j, h, min) : « d » ailleurs.
                            if self.lang == Lang::Fr {
                                util::human_duration(self.snapshot.uptime)
                            } else {
                                util::human_duration(self.snapshot.uptime).replace(" j ", " d ")
                            }
                        ))
                        .size(11.5)
                        .color(self.muted()),
                    );
                }
            });
        });
        ui.add_space(12.0);
    }

    // -----------------------------------------------------------------------
    // Sidebar et barre supérieure
    // -----------------------------------------------------------------------

    fn sidebar(&mut self, ctx: &egui::Context) {
        let fill = self.sidebar_color();
        let border = self.border_color();
        egui::SidePanel::left("nav")
            .exact_width(232.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(fill)
                    .inner_margin(egui::Margin {
                        left: 12.0,
                        right: 12.0,
                        top: 14.0,
                        bottom: 10.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 13.0, self.accent_color());
                    ui.painter().circle_stroke(
                        rect.center(),
                        7.5,
                        egui::Stroke::new(2.5_f32, egui::Color32::WHITE),
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("OrbiSys").strong().size(15.0));
                        ui.label(
                            egui::RichText::new(self.os.display_name())
                                .size(10.5)
                                .color(self.muted()),
                        );
                    });
                });
                ui.add_space(14.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 142.0).max(120.0))
                    .show(ui, |ui| {
                        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
                        for (group, pages) in NAV {
                            ui.label(
                                egui::RichText::new(i18n::t(self.lang, group).to_uppercase())
                                    .size(9.5)
                                    .color(self.muted())
                                    .strong(),
                            );
                            ui.add_space(2.0);
                            for page in pages.iter() {
                                if self.nav_item(ui, *page) {
                                    self.page = *page;
                                }
                            }
                            ui.add_space(10.0);
                        }
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!("v{} · MIT", orbisys::APP_VERSION))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(self.tr("app.search_hint"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    ui.add_space(8.0);
                    let (color, title, detail) = self.health_summary();
                    ui.label(egui::RichText::new(detail).size(10.5).color(self.muted()));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        status_dot(ui, color);
                        ui.label(egui::RichText::new(title).size(12.0).strong().color(color));
                    });
                });
                let full = ui.max_rect();
                ui.ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Middle,
                        egui::Id::new("nav-border"),
                    ))
                    .vline(
                        full.right() + 11.5,
                        full.y_range(),
                        egui::Stroke::new(1.0_f32, border),
                    );
            });
    }

    /// Contrôle segmenté des fenêtres fusionnées ; renvoie l'onglet choisi.
    fn merged_tabs(&mut self, ui: &mut egui::Ui, group: &'static str, labels: &[String]) -> usize {
        let current = self.merged_tab.get(group).copied().unwrap_or(0).min(labels.len().saturating_sub(1));
        let mut chosen = current;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (index, label) in labels.iter().enumerate() {
                let selected = index == current;
                let (fill, line) = if selected {
                    if self.dark {
                        (egui::Color32::from_rgb(58, 31, 13), egui::Color32::from_rgb(124, 58, 18))
                    } else {
                        (egui::Color32::from_rgb(251, 227, 207), egui::Color32::from_rgb(240, 184, 138))
                    }
                } else {
                    (egui::Color32::TRANSPARENT, self.border_color())
                };
                let text = egui::RichText::new(label).size(12.5);
                let text = if selected { text.strong() } else { text.color(self.muted()) };
                if ui
                    .add(egui::Button::new(text).fill(fill).stroke(egui::Stroke::new(1.0_f32, line)).rounding(14.0_f32).min_size(egui::vec2(0.0, 28.0)))
                    .clicked()
                {
                    chosen = index;
                }
            }
        });
        ui.add_space(8.0);
        self.merged_tab.insert(group, chosen);
        chosen
    }

    /// Les écrans fusionnés gardent leurs anciens points d'entrée (recherche, liens) :
    /// on les redirige vers la fenêtre commune et l'onglet voulu.
    fn redirect_merged(&mut self) {
        // « Comptes locaux » doublonnait la section COMPTES de Sessions & utilisateurs (qui permet en plus de verrouiller).
        if self.page == Page::Panel(PanelId::Users) {
            self.page = Page::Sessions;
        }
        if let Some((group, list, index)) = merge_of(self.page) {
            if index > 0 {
                self.page = list[0];
                self.merged_tab.insert(group, index);
            }
        }
    }

    /// État global affiché en haut et dans le menu : (couleur, titre, détail).
    fn health_summary(&self) -> (egui::Color32, &'static str, String) {
        let failed = self.services.iter().filter(|service| service.failed()).count();
        let disk = self.mounts.iter().find(|mount| mount.target == "/").map(|mount| mount.percent).unwrap_or(0.0);
        let hot = self.snapshot.cpu_percent >= 90.0 || self.snapshot.memory.percent() >= 92.0 || disk >= 90.0;
        if failed > 0 {
            (self.warn_color(), ts("Attention requise"), tf("{} services en échec", &[&failed]))
        } else if hot {
            (self.warn_color(), ts("Attention requise"), ts("Seuils dépassés").to_string())
        } else {
            (self.ok_color(), ts("Système sain"), ts("Tous les services sont actifs").to_string())
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        self.toasts.retain(|(_, _, expiry)| *expiry > now);
        egui::TopBottomPanel::top("top")
            .exact_height(52.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(4.0);
                    if ui.button(self.tr("app.search")).on_hover_text("Ctrl+K").clicked() {
                        self.palette = true;
                    }
                    if self.busy > 0 {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(
                            egui::RichText::new(self.tr("app.busy"))
                                .size(11.0)
                                .color(self.muted()),
                        );
                    }
                    if let Some((message, ok, _)) = self.toasts.last() {
                        ui.add_space(6.0);
                        let color = if *ok {
                            egui::Color32::from_rgb(43, 209, 138)
                        } else {
                            egui::Color32::from_rgb(224, 92, 92)
                        };
                        ui.label(egui::RichText::new(util::ellipsis(message, 90)).color(color).size(12.0));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        // Menu Réglages (engrenage) : thème, actualisation et langue.
                        let gear = ui.add(
                            egui::Button::new(
                                egui::RichText::new(icons::SETTINGS.to_string()).font(icons::font(17.0)).color(self.accent_color()),
                            )
                            .min_size(egui::vec2(34.0, 30.0)),
                        );
                        let popup_id = egui::Id::new("settings-popup");
                        if gear.clicked() {
                            ui.memory_mut(|memory| memory.toggle_popup(popup_id));
                        }
                        let mut chosen: Option<Lang> = None;
                        let (mut toggle_theme, mut refresh) = (false, false);
                        egui::popup_below_widget(
                            ui,
                            popup_id,
                            &gear,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(190.0);
                                if ui.add(egui::Button::new(self.tr("app.theme")).min_size(egui::vec2(180.0, 28.0))).clicked() {
                                    toggle_theme = true;
                                }
                                if ui.add(egui::Button::new(self.tr("app.refresh")).min_size(egui::vec2(180.0, 28.0))).clicked() {
                                    refresh = true;
                                }
                                ui.separator();
                                ui.label(egui::RichText::new(self.tr("about.language").to_uppercase()).size(9.5).color(self.muted()));
                                for lang in Lang::ALL {
                                    if ui.selectable_label(self.lang == lang, lang.label()).clicked() {
                                        chosen = Some(lang);
                                    }
                                }
                            },
                        );
                        if toggle_theme {
                            self.dark = !self.dark;
                            save_prefs(self.dark, self.accent, self.lang);
                        }
                        if refresh {
                            self.apply_pending(Pending::Refresh(self.page));
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                        if let Some(lang) = chosen {
                            self.lang = lang;
                            save_prefs(self.dark, self.accent, self.lang);
                        }
                        // Machine, état global et noyau (maquette « System Monitor »).
                        ui.add_space(14.0);
                        if !self.sys_kernel.is_empty() {
                            ui.label(egui::RichText::new(format!("Linux {}", self.sys_kernel)).size(11.5).color(self.muted()));
                            ui.label(egui::RichText::new("|").size(11.5).color(self.border_color()));
                        }
                        let (color, title, _) = self.health_summary();
                        ui.label(egui::RichText::new(title).size(11.5).color(color));
                        status_dot(ui, color);
                        if !self.sys_host.is_empty() {
                            ui.label(egui::RichText::new("|").size(11.5).color(self.border_color()));
                            ui.label(egui::RichText::new(&self.sys_host).size(11.5).color(self.muted()));
                        }
                    });
                });
            });
    }

    // -----------------------------------------------------------------------
    // Pages
    // -----------------------------------------------------------------------

    /// Échantillonne les processus et alimente les historiques (système et
    /// processus sélectionné). `force` ignore le délai minimal de 1,5 s.
    fn sample_processes(&mut self, force: bool) {
        let now = Instant::now();
        if !force
            && !self.processes.is_empty()
            && now.duration_since(self.last_processes) <= Duration::from_millis(1500)
        {
            return;
        }
        let (processes, snapshot) = self.sampler.refresh();
        self.processes = processes;
        self.snapshot = snapshot;
        self.last_processes = now;
        for (history, value) in [
            (&mut self.cpu_history, self.snapshot.cpu_percent),
            (&mut self.mem_history, self.snapshot.memory.percent()),
        ] {
            history.push_back(value);
            while history.len() > 120 {
                history.pop_front();
            }
        }
        // Fréquence moyenne des cœurs (MHz) d'après /proc/cpuinfo.
        if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
            let values: Vec<f32> = text
                .lines()
                .filter(|line| line.starts_with("cpu MHz"))
                .filter_map(|line| line.split(':').nth(1)?.trim().parse().ok())
                .collect();
            if !values.is_empty() {
                self.cpu_mhz = values.iter().sum::<f32>() / values.len() as f32;
            }
        }
        // Charge par cœur d'après /proc/stat : variation (total − inactif) entre deux relevés.
        if let Ok(text) = std::fs::read_to_string("/proc/stat") {
            let counters: Vec<(u64, u64)> = text
                .lines()
                .filter(|line| line.starts_with("cpu") && line[3..].starts_with(|c: char| c.is_ascii_digit()))
                .map(|line| {
                    let values: Vec<u64> = line
                        .split_whitespace()
                        .skip(1)
                        .take(8)
                        .filter_map(|value| value.parse().ok())
                        .collect();
                    let total: u64 = values.iter().sum();
                    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
                    (idle, total)
                })
                .collect();
            // Intervalle trop court (< 0,2 s) : la mesure serait du bruit, on garde le relevé précédent.
            let too_short = self.core_prev.len() == counters.len()
                && !counters.is_empty()
                && counters[0].1.saturating_sub(self.core_prev[0].1) < 20;
            if self.core_prev.len() == counters.len() && !counters.is_empty() && !too_short {
                if self.core_history.len() != counters.len() {
                    self.core_history = vec![std::collections::VecDeque::new(); counters.len()];
                }
                for (index, (idle, total)) in counters.iter().enumerate() {
                    let (idle0, total0) = self.core_prev[index];
                    let span = total.saturating_sub(total0) as f32;
                    let busy = span - idle.saturating_sub(idle0) as f32;
                    let history = &mut self.core_history[index];
                    history.push_back(if span > 0.0 { (busy * 100.0 / span).clamp(0.0, 100.0) } else { 0.0 });
                    while history.len() > 120 {
                        history.pop_front();
                    }
                }
            }
            if !too_short {
                self.core_prev = counters;
            }
        }
        // Débit réseau (octets/s) d'après les compteurs de /proc/net/dev, loopback exclu.
        if let Ok(text) = std::fs::read_to_string("/proc/net/dev") {
            let (mut rx, mut tx) = (0.0_f64, 0.0_f64);
            for (name, received, sent) in orbisys::p_monitor::parse_net_dev(&text) {
                if name != "lo" {
                    rx += received as f64;
                    tx += sent as f64;
                }
            }
            if let Some((then, rx0, tx0)) = self.net_last {
                let seconds = now.duration_since(then).as_secs_f64().max(0.001);
                for (history, rate) in [
                    (&mut self.rx_history, ((rx - rx0) / seconds).max(0.0) as f32),
                    (&mut self.tx_history, ((tx - tx0) / seconds).max(0.0) as f32),
                ] {
                    history.push_back(rate);
                    while history.len() > 120 {
                        history.pop_front();
                    }
                }
            }
            self.net_last = Some((now, rx, tx));
        }
        if let Some((pid, _)) = &self.selected_process {
            let total = self.snapshot.memory.total.max(1) as f32;
            let point = self
                .processes
                .iter()
                .find(|process| process.pid == *pid)
                .map(|process| (process.cpu, process.memory as f32 * 100.0 / total));
            match point {
                Some(point) => {
                    self.selected_history.push_back(point);
                    while self.selected_history.len() > 120 {
                        self.selected_history.pop_front();
                    }
                }
                // Le processus a disparu : la sélection n'a plus d'objet.
                None => {
                    self.selected_process = None;
                    self.selected_history.clear();
                }
            }
        }
    }

    fn page_dashboard(&mut self, ui: &mut egui::Ui) {
        // Le tableau de bord échantillonne lui-même : sans cela, processeur,
        // mémoire et processus restaient à zéro tant que l'écran « Processus »
        // n'avait pas été ouvert.
        self.sample_processes(false);
        ui.ctx().request_repaint_after(Duration::from_millis(1500));
        // La carte des services n'était alimentée qu'en visitant la rubrique :
        // on la charge dès l'affichage du tableau de bord.
        if self.services.is_empty() && self.visited_pages.insert(Page::Services) {
            self.apply_pending(Pending::Refresh(Page::Services));
        }
        // Plus de défilement : les hauteurs sont réparties sur la fenêtre et la
        // carte des services s'efface quand la place manque (voir `Elastic`).
        self.dashboard_body(ui);
    }

    fn dashboard_body(&mut self, ui: &mut egui::Ui) {
        let el = elastic::Elastic::new(ui);
        elastic::density(ui, &el);

        let cpu = self.snapshot.cpu_percent;
        let memory = self.snapshot.memory;
        let swap_percent = if memory.swap_total > 0 {
            memory.swap_used() as f32 * 100.0 / memory.swap_total as f32
        } else {
            0.0
        };
        let root = self.mounts.iter().find(|mount| mount.target == "/").cloned();

        let gap = el.gap();
        let tile_h = if el.compact { 98.0 } else { 118.0 };
        let available = ui.available_width();
        // 4 tuiles : 4, 2 ou 1 colonne (jamais 3, qui laisserait une tuile seule sur une ligne).
        let fit = ((available + gap) / (218.0 + gap)).floor() as usize;
        let columns = if fit >= 4 { 4 } else if fit >= 2 { 2 } else { 1 };
        let width = (available - gap * (columns as f32 - 1.0)) / columns as f32;

        let disk_line = root
            .as_ref()
            .map(|mount| util::human_bytes(mount.used))
            .unwrap_or_else(|| "—".to_string());
        let disk_hint = root
            .as_ref()
            .map(|mount| format!("/ {}", util::human_bytes(mount.total)))
            .unwrap_or_else(|| ts("aucun montage /").to_string());
        let cores = self.core_history.len().max(1);
        let ghz = if self.cpu_mhz > 0.0 { format!(" · {:.1} GHz", self.cpu_mhz / 1000.0) } else { String::new() };
        let rate = |history: &std::collections::VecDeque<f32>| history.back().copied().unwrap_or(0.0);
        let (rx_now, tx_now) = (rate(&self.rx_history), rate(&self.tx_history));
        let cards: [(char, &str, f32, String, String); 4] = [
            (
                icons::CPU,
                ts("Processeur"),
                cpu,
                format!("{cpu:.0} %"),
                format!("{}{ghz}", tf("{} cœurs", &[&cores])),
            ),
            (
                icons::DATABASE,
                ts("Mémoire"),
                memory.percent(),
                format!("{:.0} %", memory.percent()),
                format!("{} / {}", util::human_bytes(memory.used()), util::human_bytes(memory.total)),
            ),
            (
                icons::HARD_DRIVE,
                ts("Disque /"),
                root.as_ref().map(|mount| mount.percent).unwrap_or(0.0),
                format!("{:.0} %", root.as_ref().map(|mount| mount.percent).unwrap_or(0.0)),
                format!("{} {}", disk_line, disk_hint),
            ),
            (
                icons::NETWORK,
                ts("Réseau"),
                0.0,
                format!("{}/s", util::human_bytes((rx_now + tx_now) as u64)),
                format!("↑ {}/s  ↓ {}/s", util::human_bytes(tx_now as u64), util::human_bytes(rx_now as u64)),
            ),
        ];
        let _ = swap_percent;
        for row in cards.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (icon, title, percent, big, detail) in row {
                    self.stat_card(ui, width, tile_h, *icon, title, *percent, big, detail);
                }
            });
            ui.add_space(gap);
        }

        // Grille 2 × 2 (maquette) : processeur, mémoire, réseau, processus les plus gourmands.
        let mut window = ui
            .ctx()
            .data(|data| data.get_temp::<usize>(egui::Id::new("dash-window")))
            .unwrap_or(60);
        let mut chosen = window;
        let samples = ((window as f32 / 1.5) as usize).max(2);
        let tail = |history: &std::collections::VecDeque<f32>| -> Vec<f32> {
            let all: Vec<f32> = history.iter().copied().collect();
            all[all.len().saturating_sub(samples)..].to_vec()
        };
        let (cpu_series, mem_series) = (tail(&self.cpu_history), tail(&self.mem_history));
        let core_series: Vec<Vec<f32>> = self.core_history.iter().map(|history| tail(history)).collect();
        let (rx_series, tx_series) = (tail(&self.rx_history), tail(&self.tx_history));
        let tones = chart_tones(self.dark);
        let (accent, amber, muted, grid) = (tones[0], tones[3], self.muted(), self.track_color());
        let icon_tint = self.accent_color();
        let time_labels = time_axis(window);
        let two_columns = ui.available_width() >= 620.0;
        let card_w = if two_columns { (ui.available_width() - gap) / 2.0 } else { ui.available_width() };
        // La légende liste chaque cœur : la carte grandit quand il y a beaucoup de cœurs.
        let per_line = (((card_w - 32.0) / 104.0) as usize).max(1);
        let legend_lines = core_series.len().max(1).div_ceil(per_line);
        // Hauteur partagée : les graphiques prennent la place restante. En mode
        // compact la carte des services est retirée pour la leur laisser.
        let chart_rows = if two_columns { 2.0 } else { 4.0 };
        let used = (ui.cursor().top() - ui.min_rect().top()).max(0.0);
        let left = (el.height - used - gap - 8.0).max(0.0);
        // La carte des services reste toujours visible : en mode compact elle se
        // réduit à un résumé, mais ne disparaît plus de l'écran.
        // 76 px couvrent le bandeau compact (marges, libellé, bouton) avec une
        // petite réserve : à 56 px le bas de la carte était rogné.
        let reserved = if el.compact { 76.0 } else { 220.0 };
        let extra = (legend_lines.saturating_sub(1)) as f32 * 18.0;
        let card_h = ((left - reserved - (chart_rows - 1.0) * gap) / chart_rows).max(0.0);
        let plot_h = (card_h - 96.0 - extra).clamp(40.0, 214.0);
        let percent_fmt = |value: f32| format!("{value:.0} %");
        let bytes_fmt = |value: f32| format!("{}/s", util::human_bytes(value as u64));
        let mut draw_card = |ui: &mut egui::Ui, id: u8| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(card_w, card_h), egui::Sense::hover());
            ui.painter().rect(rect, 12.0_f32, self.card_color(), egui::Stroke::new(1.0_f32, self.border_color()));
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink2(egui::vec2(16.0, 14.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            inner.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
            let ui = &mut inner;
            let head = |ui: &mut egui::Ui, icon: char, title: &str, value: String| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon.to_string()).font(icons::font(18.0)).color(icon_tint));
                    ui.label(egui::RichText::new(title).size(15.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(value).size(22.0).strong());
                    });
                });
                ui.add_space(8.0);
            };
            let last = |values: &[f32]| values.last().copied().unwrap_or(0.0);
            match id {
                0 => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(icons::CPU.to_string()).font(icons::font(18.0)).color(icon_tint));
                        ui.label(egui::RichText::new(ts("Utilisation du processeur")).size(15.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt("dash-window")
                                .selected_text(format!("{window} s"))
                                .width(86.0)
                                .show_ui(ui, |ui| {
                                    for seconds in [60usize, 120, 180] {
                                        ui.selectable_value(&mut chosen, seconds, format!("{seconds} s"));
                                    }
                                });
                            ui.label(egui::RichText::new(percent_fmt(last(&cpu_series))).size(22.0).strong());
                        });
                    });
                    ui.add_space(8.0);
                    // Une courbe par cœur, dans quatre tons orange → ambre (maquette).
                    let layers: Vec<(&[f32], egui::Color32)> = if core_series.is_empty() {
                        vec![(&cpu_series, accent)]
                    } else {
                        core_series.iter().enumerate().map(|(i, v)| (v.as_slice(), tones[i % 4])).collect()
                    };
                    series_chart(ui, plot_h, &layers, Some(100.0), &percent_fmt, Some(time_labels.clone()), muted, grid);
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 4.0);
                        if core_series.is_empty() {
                            legend_entry(ui, accent, ts("Processeur"), &percent_fmt(last(&cpu_series)), muted);
                        }
                        for (index, values) in core_series.iter().enumerate() {
                            legend_entry(ui, tones[index % 4], &format!("CPU {}", index + 1), &percent_fmt(last(values)), muted);
                        }
                    });
                }
                1 => {
                    head(ui, icons::DATABASE, ts("Utilisation de la mémoire"), percent_fmt(self.snapshot.memory.percent()));
                    series_chart(ui, plot_h, &[(&mem_series, tones[0])], Some(100.0), &percent_fmt, Some(time_labels.clone()), muted, grid);
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        let memory = self.snapshot.memory;
                        legend_entry(ui, accent, ts("Utilisée"), &util::human_bytes(memory.used()), muted);
                        legend_entry(ui, tones[2], ts("Disponible"), &util::human_bytes(memory.total.saturating_sub(memory.used())), muted);
                        legend_entry(ui, muted, ts("Total"), &util::human_bytes(memory.total), muted);
                        legend_entry(ui, tones[3], "Swap", &format!("{} / {}", util::human_bytes(memory.swap_used()), util::human_bytes(memory.swap_total)), muted);
                    });
                }
                2 => {
                    head(ui, icons::NETWORK, ts("Réseau"), bytes_fmt(last(&rx_series) + last(&tx_series)));
                    series_chart(ui, plot_h, &[(&rx_series, tones[0]), (&tx_series, tones[3])], None, &bytes_fmt, Some(time_labels.clone()), muted, grid);
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        legend_entry(ui, accent, ts("Réception"), &bytes_fmt(last(&rx_series)), muted);
                        legend_entry(ui, amber, ts("Émission"), &bytes_fmt(last(&tx_series)), muted);
                    });
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(icons::LIST.to_string()).font(icons::font(18.0)).color(icon_tint));
                        ui.label(egui::RichText::new(ts("Processus principaux")).size(15.0).strong());
                    });
                    // En-têtes de colonnes, alignés sur les lignes ci-dessous.
                    let (heads, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::hover());
                    for (x, align, text) in [
                        (heads.left() + heads.width() * 0.34, egui::Align2::LEFT_CENTER, "CPU"),
                        (heads.right() - 58.0, egui::Align2::RIGHT_CENTER, ts("Mémoire")),
                        (heads.right(), egui::Align2::RIGHT_CENTER, "PID"),
                    ] {
                        ui.painter().text(egui::pos2(x, heads.center().y), align, text.to_uppercase(), egui::FontId::monospace(9.5), muted);
                    }
                    let mut ranked: Vec<&ProcInfo> = self.processes.iter().collect();
                    ranked.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
                    let width = ui.available_width();
                    let shown = (((card_h - 86.0) / 30.0) as usize).clamp(2, 6);
                    for process in ranked.into_iter().take(shown) {
                        let (row, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
                        let painter = ui.painter_at(row);
                        let cy = row.center().y;
                        // Pastille à l'initiale du programme (comme les icônes d'application de la maquette).
                        painter.circle_filled(egui::pos2(row.left() + 9.0, cy), 9.0, grid);
                        painter.text(
                            egui::pos2(row.left() + 9.0, cy),
                            egui::Align2::CENTER_CENTER,
                            process.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default(),
                            egui::FontId::proportional(10.5),
                            accent,
                        );
                        painter.text(
                            egui::pos2(row.left() + 26.0, cy),
                            egui::Align2::LEFT_CENTER,
                            util::ellipsis(&process.name, 14),
                            egui::FontId::proportional(12.5),
                            ui.visuals().text_color(),
                        );
                        let bar_x = row.left() + width * 0.34;
                        let bar = egui::Rect::from_min_size(egui::pos2(bar_x, cy - 3.0), egui::vec2((width * 0.18).max(36.0), 6.0));
                        painter.rect_filled(bar, 3.0, grid);
                        let mut fill = bar;
                        fill.set_width((bar.width() * (process.cpu / 100.0).clamp(0.0, 1.0)).max(4.0));
                        painter.rect_filled(fill, 3.0, if process.cpu > 20.0 { amber } else { accent });
                        painter.text(
                            egui::pos2(bar.right() + 8.0, cy),
                            egui::Align2::LEFT_CENTER,
                            format!("{:.1} %", process.cpu),
                            egui::FontId::monospace(11.0),
                            muted,
                        );
                        painter.text(
                            egui::pos2(row.right() - 58.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            util::human_bytes(process.memory),
                            egui::FontId::monospace(11.0),
                            muted,
                        );
                        painter.text(
                            egui::pos2(row.right(), cy),
                            egui::Align2::RIGHT_CENTER,
                            process.pid.to_string(),
                            egui::FontId::monospace(11.0),
                            muted,
                        );
                    }
                }
            }
        };
        for pair in [[0u8, 1], [2, 3]] {
            if two_columns {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    draw_card(ui, pair[0]);
                    draw_card(ui, pair[1]);
                });
            } else {
                draw_card(ui, pair[0]);
                ui.add_space(gap);
                draw_card(ui, pair[1]);
            }
            ui.add_space(gap);
        }
        if chosen != window {
            window = chosen;
            ui.ctx()
                .data_mut(|data| data.insert_temp(egui::Id::new("dash-window"), window));
        }

        let mut goto: Option<Page> = None;
        ui.add_space(if el.compact { 6.0 } else { 12.0 });
        if el.compact {
            // Fenêtre courte : la carte des services reste accessible, réduite à
            // un résumé d'une ligne (elle disparaissait complètement auparavant).
            self.card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(i18n::t(self.lang, "page.services"))
                            .size(12.5)
                            .strong(),
                    );
                    // Compteur sans libellé : aucun texte à traduire en plus.
                    let running = self.services.iter().filter(|service| service.running()).count();
                    ui.label(
                        egui::RichText::new(format!("{running}/{}", self.services.len()))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("→").size(13.0))
                            .on_hover_text(i18n::t(self.lang, "page.services"))
                            .clicked()
                        {
                            goto = Some(Page::Services);
                        }
                    });
                });
            });
        } else {
        self.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(i18n::t(self.lang, "page.services"))
                        .size(13.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new("→").size(13.0))
                        .on_hover_text(i18n::t(self.lang, "page.services"))
                        .clicked()
                    {
                        goto = Some(Page::Services);
                    }
                });
            });
            ui.add_space(8.0);
            if self.services.is_empty() {
                ui.label(
                    egui::RichText::new(ts("Lecture en cours…"))
                        .size(11.5)
                        .color(self.muted()),
                );
                return;
            }
            // Colonnes à largeur fixe : une grille comptait chaque pastille comme
            // une cellule, ce qui décalait toute la ligne d'une colonne.
            let muted = self.muted();
            let total = ui.available_width();
            let name_w = (total * 0.30).clamp(110.0, 320.0);
            let state_w = (total * 0.16).clamp(70.0, 180.0);
            ui.horizontal(|ui| {
                left_cell(ui, [name_w + 16.0, 16.0], egui::Label::new(egui::RichText::new(ts("Nom").to_uppercase()).size(10.0).strong().color(muted)));
                left_cell(ui, [state_w + 16.0, 16.0], egui::Label::new(egui::RichText::new(ts("Statut").to_uppercase()).size(10.0).strong().color(muted)));
                let rest = (ui.available_width() - 52.0).max(80.0);
                left_cell(ui, [rest, 16.0], egui::Label::new(egui::RichText::new(ts("Description").to_uppercase()).size(10.0).strong().color(muted)));
                left_cell(ui, [46.0, 16.0], egui::Label::new(egui::RichText::new(ts("Action").to_uppercase()).size(10.0).strong().color(muted)));
            });
            ui.add_space(2.0);
            for service in self.services.iter().take(6) {
                let running = service.running();
                let color = if service.failed() {
                    egui::Color32::from_rgb(224, 92, 92)
                } else if running {
                    egui::Color32::from_rgb(43, 209, 138)
                } else {
                    egui::Color32::from_rgb(232, 161, 60)
                };
                let etat = if running {
                    ts("En cours").to_string()
                } else {
                    service.sub.clone()
                };
                ui.horizontal(|ui| {
                    status_dot(ui, color);
                    left_cell(
                        ui,
                        [name_w, 18.0],
                        egui::Label::new(egui::RichText::new(util::ellipsis(&service.unit, 60)).size(12.0)).truncate(),
                    );
                    status_dot(ui, color);
                    left_cell(
                        ui,
                        [state_w, 18.0],
                        egui::Label::new(egui::RichText::new(etat).size(11.5).color(color)).truncate(),
                    );
                    let rest = (ui.available_width() - 52.0).max(80.0);
                    left_cell(
                        ui,
                        [rest, 18.0],
                        egui::Label::new(
                            egui::RichText::new(util::ellipsis(&service.description, 60))
                                .size(11.5)
                                .color(muted),
                        )
                        .truncate(),
                    );
                    if ui.small_button("⋯").clicked() {
                        goto = Some(Page::Services);
                    }
                });
            }
        });
        }
        if let Some(page) = goto {
            self.page = page;
        }
    }

    /// Tuile du tableau de bord (maquette « System Monitor ») : anneau à gauche ; libellé,
    /// grande valeur, sous-texte et pastille d'état à droite. La tuile occupe exactement
    /// `width` (rectangle réservé puis dessiné à la main) : un `Frame` s'élargissait à son
    /// contenu, ce qui faisait déborder la rangée.
    fn stat_card(
        &self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        icon: char,
        title: &str,
        percent: f32,
        big: &str,
        detail: &str,
    ) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        ui.painter().rect(
            rect,
            12.0_f32,
            self.card_color(),
            egui::Stroke::new(1.0_f32, self.border_color()),
        );
        // Icône linéaire orange à gauche (maquette « System Monitor »).
        ui.painter().text(
            egui::pos2(rect.left() + 18.0, rect.top() + 36.0),
            egui::Align2::LEFT_CENTER,
            icon.to_string(),
            icons::font(30.0),
            self.accent_color(),
        );
        let mut inner = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 64.0, rect.top() + 14.0),
                    egui::pos2(rect.right() - 14.0, rect.bottom() - 12.0),
                ))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        inner.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
        let ui = &mut inner;
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add(egui::Label::new(egui::RichText::new(title).size(12.5).color(self.muted())).truncate());
        ui.add(egui::Label::new(egui::RichText::new(big).size(26.0).strong()).truncate());
        ui.add(egui::Label::new(egui::RichText::new(detail).size(11.0).color(self.muted())).truncate());
        ui.add_space(2.0);
        // Pastille d'état (Normal / Attention / Critique) d'après le taux d'utilisation.
        let (label, color) = if percent >= 90.0 {
            (ts("Critique"), self.danger_color())
        } else if percent >= 75.0 {
            (ts("Attention"), self.warn_color())
        } else {
            (ts("Normal"), self.ok_color())
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            status_dot(ui, color);
            ui.label(egui::RichText::new(label).size(11.5).color(color));
        });
    }

    fn page_software(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Pending> = Vec::new();
        let mut selected = std::mem::take(&mut self.selected);

        ui.horizontal(|ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(ts("Filtrer par nom, description ou section…"))
                    .desired_width(320.0),
            );
            if search.changed() {
                self.software_page = 0;
            }
            egui::ComboBox::from_id_salt("source")
                .selected_text(match self.source_filter {
                    Some(source) => ts(source.label()),
                    None => ts("Toutes sources"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.source_filter, None, ts("Toutes sources"));
                    for source in [Source::Native, Source::Snap, Source::Flatpak] {
                        ui.selectable_value(&mut self.source_filter, Some(source), ts(source.label()));
                    }
                });
            if ui.button(ts("Tout décocher")).clicked() {
                selected.clear();
            }
        });

        let counts = self.inventory.as_ref().map(|inventory| inventory.counts());
        if let Some((native, snap, flatpak, manual_count)) = counts {
            ui.label(
                egui::RichText::new(tf("{} paquets · {} snap · {} flatpak · {} hors dépôts", &[&native, &snap, &flatpak, &manual_count]))
                .size(11.5)
                .color(self.muted()),
            );
        }
        if let Some(inventory) = &self.inventory {
            if !inventory.errors.is_empty() {
                ui.label(
                    egui::RichText::new(inventory.errors.join(" | "))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(224, 92, 92)),
                );
            }
        }
        ui.add_space(6.0);

        let query = self.search.to_lowercase();
        let source_filter = self.source_filter;
        let packages = self.inventory.as_ref().map(|inventory| &inventory.packages);
        let indices: Vec<usize> = match packages {
            Some(packages) => packages
                .iter()
                .enumerate()
                .filter(|(_, package)| {
                    source_filter
                        .map(|source| source == package.source)
                        .unwrap_or(true)
                })
                .filter(|(_, package)| {
                    query.is_empty()
                        || package.name.to_lowercase().contains(&query)
                        || package.description.to_lowercase().contains(&query)
                        || package.section.to_lowercase().contains(&query)
                })
                .map(|(index, _)| index)
                .collect(),
            None => Vec::new(),
        };

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tf("{} résultat(s)", &[&indices.len()]))
                    .size(11.5)
                    .color(self.muted()),
            );
            if !selected.is_empty() {
                ui.label(
                    egui::RichText::new(tf("{} sélectionné(s)", &[&selected.len()]))
                        .size(11.5)
                        .color(self.accent_color()),
                );
            }
        });
        ui.add_space(2.0);

        let row_height = 21.0;
        let per_page = elastic::Elastic::new(ui)
            .fit_rows(elastic::remaining(ui) - 130.0, row_height + 4.0, 5, 60);
        let (first, last) = elastic::page_bounds(self.software_page, per_page, indices.len());
        {
                if let Some(packages) = packages {
                    for index in first..last {
                        let package = &packages[indices[index]];
                        ui.horizontal(|ui| {
                            let mut checked = selected.contains(&package.name);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    selected.insert(package.name.clone());
                                } else {
                                    selected.remove(&package.name);
                                }
                            }
                            left_cell(ui, 
                                [270.0, 18.0],
                                egui::Label::new(util::ellipsis(&package.name, 34)).truncate(),
                            );
                            left_cell(ui, 
                                [150.0, 18.0],
                                egui::Label::new(util::ellipsis(&package.version, 18))
                                    .truncate()
                                    .sense(egui::Sense::hover()),
                            );
                            left_cell(ui, 
                                [76.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(ts(package.source.label()))
                                        .size(10.5)
                                        .color(self.muted()),
                                ),
                            );
                            left_cell(ui, 
                                [86.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(util::human_bytes(package.size_bytes))
                                        .size(11.0)
                                        .monospace()
                                        .color(self.muted()),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(util::ellipsis(&package.description, 60))
                                    .size(11.0)
                                    .color(self.muted()),
                            );
                        });
                    }
                }
        }
        ui.add_space(2.0);
        elastic::pager_bar(ui, &mut self.software_page, per_page, indices.len());

        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            let has_selection = !selected.is_empty();
            let preview = ui.add_enabled(
                has_selection,
                egui::Button::new(ts("Aperçu de la suppression")),
            );
            if preview.clicked() {
                let names: Vec<String> = selected.iter().cloned().collect();
                pending.push(Pending::PreviewNative(names));
            }
            let remove = ui.add_enabled(has_selection, egui::Button::new(ts("Désinstaller…")));
            if remove.clicked() {
                let mut native = Vec::new();
                let mut snap = Vec::new();
                let mut flatpak = Vec::new();
                if let Some(inventory) = &self.inventory {
                    for package in &inventory.packages {
                        if selected.contains(&package.name) {
                            match package.source {
                                Source::Native => native.push(package.name.clone()),
                                Source::Snap => snap.push(package.name.clone()),
                                Source::Flatpak => flatpak.push(package.name.clone()),
                                Source::Manual => {}
                            }
                        }
                    }
                }
                for (names, source) in [
                    (native, Source::Native),
                    (snap, Source::Snap),
                    (flatpak, Source::Flatpak),
                ] {
                    if names.is_empty() {
                        continue;
                    }
                    pending.push(Pending::Remove {
                        names: names.clone(),
                        source,
                    });
                }
                selected.clear();
            }
            ui.label(
                egui::RichText::new(ts("suppression via le gestionnaire natif, confirmée par Polkit"))
                    .size(10.5)
                    .color(self.muted()),
            );
        });

        // Logiciels hors dépôts.
        ui.add_space(10.0);
        let manual_apps = self
            .inventory
            .as_ref()
            .map(|inventory| inventory.filtered_manual(&self.search))
            .unwrap_or_default();
        if !manual_apps.is_empty() {
            ui.label(egui::RichText::new(ts("Installés hors dépôts")).strong());
            ui.add_space(2.0);
            for app in manual_apps.iter().take(40) {
                ui.horizontal(|ui| {
                    left_cell(ui, 
                        [270.0, 18.0],
                        egui::Label::new(util::ellipsis(&app.name, 40)).truncate(),
                    );
                    left_cell(ui, 
                        [120.0, 18.0],
                        egui::Label::new(
                            egui::RichText::new(ts(app.kind.label()))
                                .size(10.5)
                                .color(self.muted()),
                        ),
                    );
                    left_cell(ui, 
                        [330.0, 18.0],
                        egui::Label::new(
                            egui::RichText::new(util::ellipsis(&app.path.display().to_string(), 60))
                                .size(10.5)
                                .color(self.muted()),
                        )
                        .truncate(),
                    );
                    if ui.small_button(ts("Supprimer…")).clicked() {
                        pending.push(Pending::RemoveManual(app.path.clone()));
                    }
                });
            }
        }

        self.selected = selected;
        for item in pending {
            self.apply_pending(item);
        }
    }

    fn page_updates(&mut self, ui: &mut egui::Ui) {
        let manager = self.inventory.as_ref().and_then(|inventory| inventory.manager);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tf("{} mise(s) à jour disponible(s)", &[&self.updates.len()]))
                    .strong(),
            );
            if let Some(manager) = manager {
                ui.label(
                    egui::RichText::new(tf("via {}", &[&manager.label()]))
                        .size(11.5)
                        .color(self.muted()),
                );
            }
            if ui.button(ts("Relire")).clicked() {
                self.apply_pending(Pending::Refresh(Page::Updates));
            }
        });
        if !self.updates.is_empty() {
            ui.horizontal(|ui| {
                if ui.button(ts("Tout mettre à jour…")).clicked() {
                    let detail = tf("{} mise(s) à jour seront installées ({}). Une authentification administrateur est demandée.", &[&self.updates.len(), &manager.map(|manager| manager.label()).unwrap_or(ts("Snap / Flatpak"))]);
                    self.confirm = Some(Confirmation {
                        title: ts("Appliquer les mises à jour").to_string(),
                        detail,
                        pending: Pending::ApplyUpdates,
                    });
                }
                if let Some(manager) = manager {
                    ui.label(
                        egui::RichText::new(tf("ou en terminal : sudo {}", &[&manager.update_command()]))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
            });
        }
        ui.add_space(8.0);
        if self.updates.is_empty() {
            ui.label(egui::RichText::new(ts("Aucune mise à jour en attente.")).color(self.muted()));
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 22.0, self.updates.len(), |ui, range| {
                for index in range {
                    let update = &self.updates[index];
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [320.0, 18.0],
                            egui::Label::new(util::ellipsis(&update.name, 40)).truncate(),
                        );
                        left_cell(ui, 
                            [150.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(if update.current.is_empty() {
                                    "—"
                                } else {
                                    update.current.as_str()
                                })
                                .size(11.0)
                                .monospace()
                                .color(self.muted()),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(format!("→ {}", update.available))
                                .size(11.0)
                                .monospace()
                                .color(self.accent_color()),
                        );
                    });
                }
            });
    }

    fn page_disk(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tf("{} système(s) de fichiers", &[&self.mounts.len()]))
                    .strong(),
            );
            if ui.button(ts("Relire")).clicked() {
                self.apply_pending(Pending::Refresh(Page::Disk));
            }
            let memory = disk::memory();
            ui.label(
                egui::RichText::new(format!(
                    "RAM {} / {} · swap {} / {}",
                    util::human_bytes(memory.used()),
                    util::human_bytes(memory.total),
                    util::human_bytes(memory.swap_used()),
                    util::human_bytes(memory.swap_total)
                ))
                .size(11.0)
                .color(self.muted()),
            );
        });
        ui.add_space(8.0);
        for mount in self.mounts.clone() {
            ui.horizontal(|ui| {
                left_cell(ui, 
                    [220.0, 18.0],
                    egui::Label::new(util::ellipsis(&mount.target, 30)).truncate(),
                );
                left_cell(ui, 
                    [90.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(util::ellipsis(&mount.fstype, 10))
                            .size(11.0)
                            .color(self.muted()),
                    ),
                );
                ui.add(
                    egui::ProgressBar::new((mount.percent / 100.0).clamp(0.0, 1.0))
                        .desired_width(260.0)
                        .text(format!("{:.0} %", mount.percent)),
                );
                ui.label(
                    egui::RichText::new(tf("{} utilisés · {} libres sur {}", &[&util::human_bytes(mount.used), &util::human_bytes(mount.available), &util::human_bytes(mount.total)]))
                    .size(11.0)
                    .color(if mount.critical() {
                        egui::Color32::from_rgb(224, 92, 92)
                    } else {
                        self.muted()
                    }),
                );
            });
        }
    }

    fn page_btrfs(&mut self, ui: &mut egui::Ui) {
        let report = self.btrfs_report.clone();
        let filter = self.btrfs_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Sous-volumes et instantanés Btrfs")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.btrfs_filter)
                    .desired_width(160.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::BtrfsReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} montage(s) · {} sous-volume(s) · {} instantané(s) · outil {}", &[&report.mounts.len(), &report.subvolumes.len(), &report.snapshots(), &(if report.tool_available { ts("présent") } else { ts("absent") })]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des sous-volumes…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                if report.mounts.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun système de fichiers Btrfs monté sur cette machine."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }
                for mount in &report.mounts {
                    ui.label(
                        egui::RichText::new(format!("{} ({})", mount.target, mount.source))
                            .size(10.5)
                            .strong(),
                    );
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(ts("SOUS-VOLUMES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for volume in &report.subvolumes {
                    if !filter.is_empty() && !volume.path.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [60.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!("#{}", volume.id))
                                    .monospace()
                                    .size(9.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [300.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&volume.path).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        if volume.is_snapshot() {
                            ui.label(
                                egui::RichText::new(ts("instantané"))
                                    .size(9.5)
                                    .color(egui::Color32::from_rgb(120, 170, 240)),
                            );
                            if ui.small_button(ts("Supprimer…")).clicked() {
                                let path = if volume.path.starts_with('/') {
                                    volume.path.clone()
                                } else {
                                    let mount = report
                                        .mounts
                                        .first()
                                        .map(|item| item.target.clone())
                                        .unwrap_or_else(|| "/".to_string());
                                    format!("{}/{}", mount.trim_end_matches('/'), volume.path)
                                };
                                confirm = Some(Confirmation {
                                    title: tf("Supprimer {}", &[&volume.path]),
                                    detail: ts("Le sous-volume sera définitivement supprimé (btrfs subvolume delete). Les données qu'il contient seront perdues.")
                                        .to_string(),
                                    pending: Pending::BtrfsDelete(path),
                                });
                            }
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("CRÉER UN INSTANTANÉ"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Source"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.btrfs_source)
                            .desired_width(280.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("/home"),
                    );
                    ui.label(ts("Destination"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.btrfs_dest)
                            .desired_width(320.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("/home/snapshots/home-2026-11-02"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button(ts("Créer…")).clicked() {
                        confirm = Some(Confirmation {
                            title: ts("Créer un instantané").to_string(),
                            detail: tf("btrfs subvolume snapshot -r {} {} (lecture seule, polkit).", &[&self.btrfs_source, &self.btrfs_dest]),
                            pending: Pending::BtrfsCreate {
                                source: self.btrfs_source.clone(),
                                destination: self.btrfs_dest.clone(),
                            },
                        });
                    }
                    ui.label(
                        egui::RichText::new(ts("Chemins absolus, sans `..` ; la destination ne doit pas exister."))
                            .size(9.5)
                            .color(self.muted()),
                    );
                });
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_snapshots(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.snapshots.clone();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut actions: Vec<Pending> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Instantanés et archives")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} instantané(s) · Timeshift {}", &[&report.timeshift.snapshots.len(), &(if report.timeshift.available {
                            ts("disponible")
                        } else {
                            ts("absent")
                        })]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des instantanés…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };

                ui.label(
                    egui::RichText::new("TIMESHIFT")
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                egui::Grid::new("timeshift-grid")
                    .num_columns(2)
                    .spacing([18.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                ts("Planification"),
                                if report.timeshift.schedule.is_empty() {
                                    ts("désactivée").to_string()
                                } else {
                                    report.timeshift.schedule.join(", ")
                                },
                            ),
                            (
                                ts("Conservation"),
                                if report.timeshift.keep.is_empty() {
                                    "—".to_string()
                                } else {
                                    tf("{} instantané(s)", &[&report.timeshift.keep])
                                },
                            ),
                            (
                                ts("Emplacement"),
                                if report.timeshift.location.is_empty() {
                                    "—".to_string()
                                } else {
                                    report.timeshift.location.clone()
                                },
                            ),
                        ] {
                            ui.label(
                                egui::RichText::new(label).size(11.0).color(self.muted()),
                            );
                            ui.label(egui::RichText::new(value).size(11.5));
                            ui.end_row();
                        }
                    });
                if report.timeshift.snapshots.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            ts("Aucun instantané lisible (support non monté ou Timeshift non configuré)."),
                        )
                        .size(10.5)
                        .color(self.muted()),
                    );
                }
                for entry in &report.timeshift.snapshots {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [220.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&entry.name).monospace().size(10.5),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(util::human_bytes(entry.size))
                                .size(10.0)
                                .color(self.muted()),
                        );
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("SAUVEGARDE DISTANTE (RSYNC/SSH)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(
                        ts("Connexion sans mot de passe interactif (BatchMode) : une clé SSH doit être en place."),
                    )
                    .size(10.0)
                    .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Source"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_source)
                            .desired_width(320.0)
                            .font(egui::TextStyle::Monospace),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Cible"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_target)
                            .desired_width(320.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("user@hôte:/srv/sauvegarde")),
                    );
                    if ui.button(ts("Tester la connexion")).clicked() {
                        actions.push(Pending::RemoteCheck);
                    }
                    if ui.button(ts("Simuler")).clicked() {
                        actions.push(Pending::RemoteBackup {
                            source: self.remote_source.clone(),
                            target: self.remote_target.clone(),
                            dry_run: true,
                        });
                    }
                    if ui.button(ts("Sauvegarder…")).clicked() {
                        confirmations.push(Confirmation {
                            title: ts("Sauvegarde distante").to_string(),
                            detail: tf("rsync -a {} → {}\nLes fichiers absents de la source ne sont pas supprimés sur la cible.", &[&self.remote_source, &self.remote_target]),
                            pending: Pending::RemoteBackup {
                                source: self.remote_source.clone(),
                                target: self.remote_target.clone(),
                                dry_run: false,
                            },
                        });
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("VÉRIFICATION D'ARCHIVE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.archive_path)
                            .desired_width(420.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("/home/amine/sauvegarde.tar.gz")),
                    );
                    if ui.button(ts("Vérifier")).clicked() {
                        actions.push(Pending::VerifyArchive(self.archive_path.clone()));
                    }
                });
                if !self.snapshot_output.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(self.snapshot_output.clone());
                        });
                }
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Snapshots));
        }
    }

    fn page_optimize(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.optimize.clone();
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("TRIM, processeur et mémoire")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} disque(s) · gouvernance {} · TRIM {}", &[&report.disks.len(), &if report.cpu.governor.is_empty() {
                            "—"
                        } else {
                            report.cpu.governor.as_str()
                        }, &if report.trim_timer.is_empty() {
                            ts("inconnu")
                        } else {
                            report.trim_timer.as_str()
                        }]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture de l'état…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };

                ui.label(
                    egui::RichText::new(ts("DISQUES ET TRIM"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for disk in &report.disks {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [130.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&disk.device).monospace().size(10.5),
                            ),
                        );
                        left_cell(ui, 
                            [220.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&disk.model, 34)).size(10.5),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(if optimize::is_solid_state(disk) {
                                "SSD"
                            } else {
                                ts("disque rotatif")
                            })
                            .size(10.0)
                            .color(self.muted()),
                        );
                        ui.label(
                            egui::RichText::new(if disk.discard {
                                ts("TRIM pris en charge")
                            } else {
                                ts("TRIM non exposé")
                            })
                            .size(10.0)
                            .color(self.muted()),
                        );
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tf("Minuteur fstrim.timer : {}", &[&(if report.trim_timer.is_empty() {
                                ts("inconnu")
                            } else {
                                report.trim_timer.as_str()
                            })]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                    if ui.button(ts("Lancer un TRIM maintenant…")).clicked() {
                        confirmations.push(Confirmation {
                            title: ts("Lancer un TRIM").to_string(),
                            detail: ts("fstrim -av parcourt tous les systèmes de fichiers montés et signale aux SSD les blocs libres. L'opération peut durer plusieurs minutes et n'efface aucune donnée.").to_string(),
                            pending: Pending::RunTrim,
                        });
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("PROCESSEUR"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tf("Gouvernance : {} · pilote : {}", &[&(if report.cpu.governor.is_empty() {
                                "—"
                            } else {
                                report.cpu.governor.as_str()
                            }), &(if report.cpu.driver.is_empty() {
                                "—"
                            } else {
                                report.cpu.driver.as_str()
                            })]))
                        .size(10.5)
                        .color(self.muted()),
                    );
                });
                ui.horizontal(|ui| {
                    for governor in &report.cpu.available {
                        let selected = governor == &report.cpu.governor;
                        if ui
                            .add(egui::Button::new(governor).min_size(egui::vec2(96.0, 24.0)))
                            .clicked()
                        {
                            actions.push(Pending::SetGovernor(governor.clone()));
                        }
                        if selected {
                            ui.label(
                                egui::RichText::new(ts("actif"))
                                    .size(10.0)
                                    .color(self.muted()),
                            );
                        }
                    }
                });
                if !report.cpupower {
                    ui.label(
                        egui::RichText::new(
                            ts("cpupower absent : la gouvernance peut être lue mais pas modifiée."),
                        )
                        .size(10.0)
                        .color(self.muted()),
                    );
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("MÉMOIRE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                egui::Grid::new("memory-grid")
                    .num_columns(2)
                    .spacing([18.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                ts("RAM totale"),
                                util::human_bytes(report.memory.total),
                            ),
                            (
                                ts("RAM disponible"),
                                util::human_bytes(report.memory.available),
                            ),
                            (
                                ts("Swap utilisé"),
                                format!(
                                    "{} / {}",
                                    util::human_bytes(report.memory.swap_used),
                                    util::human_bytes(report.memory.swap_total)
                                ),
                            ),
                            (ts("Pages sales"), util::human_bytes(report.memory.dirty)),
                            ("swappiness", report.memory.swappiness.clone()),
                            (
                                "vfs_cache_pressure",
                                report.memory.cache_pressure.clone(),
                            ),
                        ] {
                            ui.label(
                                egui::RichText::new(label).size(10.5).color(self.muted()),
                            );
                            ui.label(egui::RichText::new(value).size(11.0));
                            ui.end_row();
                        }
                    });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("SERVICES LES PLUS LENTS AU DÉMARRAGE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.boot.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Mesure indisponible (systemd-analyze absent)."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for service in &report.boot {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [90.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!("{:.2} s", service.seconds))
                                    .monospace()
                                    .size(10.0),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&service.unit, 60))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    });
                }
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Optimize));
        }
    }

    fn page_fonts(&mut self, ui: &mut egui::Ui) {
        let families = self.fonts.clone();
        let cache = self.font_cache.clone();
        let filter = self.font_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Polices installées")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.font_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer une famille")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::FontsReload);
            }
            if ui.button(ts("Reconstruire le cache")).clicked() {
                actions.push(Pending::FontsRefresh);
            }
            ui.label(
                egui::RichText::new(tf("{} famille(s) installée(s)", &[&families.len()]))
                    .size(10.5)
                    .color(self.muted()),
            );
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(cache) = &cache {
                    ui.label(
                        egui::RichText::new(tf("Cache : {} fichier(s), {} — {}", &[&cache.files, &util::human_bytes(cache.bytes), &cache.path]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                    ui.add_space(6.0);
                }

                let visible: Vec<&fonts::Family> = families
                    .iter()
                    .filter(|family| {
                        filter.is_empty() || family.name.to_lowercase().contains(&filter)
                    })
                    .collect();
                if visible.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune famille ne correspond (fontconfig absent ?)."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for family in visible.iter().take(400) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [240.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&family.name).size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [220.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(tf("{} style(s) : {}", &[&family.styles.len(), &util::ellipsis(&family.styles.join(", "), 40)]))
                                .size(9.5)
                                .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Tester")).clicked() {
                            actions.push(Pending::FontMatchQuery(family.name.clone()));
                        }
                    });
                }
                if visible.len() > 400 {
                    ui.label(
                        egui::RichText::new(tf("… {} familles supplémentaires masquées, affinez le filtre.", &[&(visible.len() - 400)]))
                        .size(9.5)
                        .color(self.muted()),
                    );
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("SUBSTITUTION FONTCONFIG"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.font_query)
                            .desired_width(260.0)
                            .hint_text(ts("nom de famille demandé")),
                    );
                    if ui.button(ts("Vérifier")).clicked() {
                        actions.push(Pending::FontMatchQuery(self.font_query.clone()));
                    }
                });
                if !self.font_match.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.font_match)
                            .size(10.5)
                            .color(self.muted()),
                    );
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("INSTALLER UNE POLICE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(tf("Formats acceptés : {} · destination : {}", &[&fonts::FONT_EXTENSIONS.join(", "), &fonts::FONT_DIR]))
                    .size(9.5)
                    .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.font_path)
                            .desired_width(420.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("/home/amine/Téléchargements/ma-police.ttf")),
                    );
                    if ui.button(ts("Installer…")).clicked() {
                        confirm = Some(Confirmation {
                            title: ts("Installer une police").to_string(),
                            detail: tf("Le fichier sera copié dans {} puis le cache fontconfig sera reconstruit (polkit).", &[&fonts::FONT_DIR]),
                            pending: Pending::FontsInstall(self.font_path.clone()),
                        });
                    }
                    if ui.button(ts("Supprimer…")).clicked() {
                        confirm = Some(Confirmation {
                            title: ts("Supprimer une police").to_string(),
                            detail: tf("Seules les polices de {} peuvent être supprimées ; les polices des paquets sont protégées.", &[&fonts::FONT_DIR]),
                            pending: Pending::FontsRemove(self.font_path.clone()),
                        });
                    }
                });
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_locale(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.locale.clone();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Langue, heure et polices")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} famille(s) de polices · {} locale(s) disponible(s)", &[&report.font_total, &report.locale.available_locales]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des locales…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };

                egui::Grid::new("locale-grid")
                    .num_columns(2)
                    .spacing([18.0, 6.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (ts("Langue de la session"), report.locale.session_lang.clone()),
                            (
                                ts("Langue du système"),
                                if report.locale.system_lang.is_empty() {
                                    "—".to_string()
                                } else {
                                    report.locale.system_lang.clone()
                                },
                            ),
                            (
                                ts("Clavier (console)"),
                                if report.locale.keymap.is_empty() {
                                    "—".to_string()
                                } else {
                                    report.locale.keymap.clone()
                                },
                            ),
                            (
                                ts("Fuseau horaire"),
                                if report.locale.timezone.is_empty() {
                                    "—".to_string()
                                } else {
                                    report.locale.timezone.clone()
                                },
                            ),
                            (
                                ts("Horloge synchronisée"),
                                if report.locale.synchronized { ts("oui") } else { ts("non") }.to_string(),
                            ),
                            (
                                ts("Service NTP"),
                                if report.locale.ntp { ts("actif") } else { ts("inactif") }.to_string(),
                            ),
                        ] {
                            ui.label(
                                egui::RichText::new(label).size(11.0).color(self.muted()),
                            );
                            ui.label(egui::RichText::new(value).size(11.5));
                            ui.end_row();
                        }
                    });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("LANGUE ET FUSEAU DU SYSTÈME"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Locale"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.locale_input)
                            .desired_width(180.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("fr_FR.UTF-8"),
                    );
                    if ui.button(ts("Appliquer…")).clicked() {
                        let code = self.locale_input.clone();
                        confirmations.push(Confirmation {
                            title: ts("Changer la locale du système").to_string(),
                            detail: tf("localectl set-locale LANG={} (Polkit).\nLa session en cours n'est pas modifiée ; la locale s'applique aux nouvelles sessions.", &[&code]),
                            pending: Pending::SetLocale(code),
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Fuseau"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.timezone_input)
                            .desired_width(220.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("Europe/Paris"),
                    );
                    if ui.button(ts("Appliquer…")).clicked() {
                        let zone = self.timezone_input.clone();
                        confirmations.push(Confirmation {
                            title: ts("Changer le fuseau horaire").to_string(),
                            detail: tf("timedatectl set-timezone {} (Polkit). Les horodatages des journaux existants restent inchangés.", &[&zone]),
                            pending: Pending::SetTimezone(zone),
                        });
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("POLICES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tf("Sans-serif : {} · monospace : {}", &[&report.default_sans, &report.default_mono]))
                        .size(10.5)
                        .color(self.muted()),
                    );
                });
                if report.fonts.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune police détectée (fontconfig absent ?)."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for font in report.fonts.iter().take(60) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [340.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&font.family, 52)).size(10.5),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(tf("{} style(s)", &[&font.styles]))
                                .size(10.0)
                                .color(self.muted()),
                        );
                    });
                }
                if report.fonts.len() > 60 {
                    ui.label(
                        egui::RichText::new(tf("… et {} autre(s) famille(s)", &[&(report.fonts.len() - 60)]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                }
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Locale));
        }
    }

    fn page_automation(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.automation.clone();
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Tâches planifiées")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} tâche(s) cron · {} minuteur(s)", &[&report.jobs.len(), &report.timers.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        let el = elastic::Elastic::new(ui);
        elastic::density(ui, &el);
        let columns = if el.two_columns() { 2 } else { 1 };
        let right = if columns == 2 { 1usize } else { 0usize };
        ui.columns(columns, |cols| {
            {
                let ui = &mut cols[0];
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des tâches…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };

                ui.label(
                    egui::RichText::new(ts("CRONTAB UTILISATEUR"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if !report.crontab_available {
                    ui.label(
                        egui::RichText::new(ts("crontab n'est pas installé."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                if report.jobs.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune tâche planifiée."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for job in &report.jobs {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [130.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&job.schedule).monospace().size(10.0),
                            ),
                        );
                        left_cell(ui, 
                            [420.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&job.command, 64)).size(10.5),
                            ),
                        );
                        if ui.button(ts("Supprimer…")).clicked() {
                            confirmations.push(Confirmation {
                                title: ts("Supprimer une tâche cron").to_string(),
                                detail: tf("{} {}\nLa tâche sera retirée du crontab utilisateur.", &[&job.schedule, &job.command]),
                                pending: Pending::RemoveCron {
                                    schedule: job.schedule.clone(),
                                    command: job.command.clone(),
                                },
                            });
                        }
                    });
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(ts("Nouvelle tâche"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cron_schedule)
                            .desired_width(130.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("0 3 * * *"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cron_command)
                            .desired_width(380.0)
                            .hint_text(ts("/usr/local/bin/sauvegarde.sh")),
                    );
                    if ui.button(ts("Ajouter")).clicked() {
                        actions.push(Pending::AddCron {
                            schedule: self.cron_schedule.clone(),
                            command: self.cron_command.clone(),
                        });
                    }
                });
                ui.label(
                    egui::RichText::new(
                        ts("Format : minute heure jour mois jour-semaine, ou @daily / @weekly / @reboot. La commande est exécutée par le shell de cron."),
                    )
                    .size(10.0)
                    .color(self.muted()),
                );

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("RÈGLES DE SEUIL"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if self.automation_rules.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune règle de seuil."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for (index, rule) in self.automation_rules.iter().enumerate() {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [180.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(tx(automation::metric_label(&rule.metric)))
                                    .size(10.5),
                            ),
                        );
                        left_cell(ui, 
                            [120.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "≥ {} {}",
                                    rule.threshold,
                                    automation::metric_unit(&rule.metric)
                                ))
                                .monospace()
                                .size(10.5),
                            ),
                        );
                        left_cell(ui, 
                            [260.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(tx(automation::action_label(&rule.action)))
                                    .size(10.5)
                                    .color(self.muted()),
                            ),
                        );
                        if ui.small_button(ts("Supprimer…")).clicked() {
                            actions.push(Pending::RemoveRule(index));
                        }
                    });
                }
                {
                    let metric_index =
                        self.rule_metric.min(automation::METRICS.len().saturating_sub(1));
                    let action_index =
                        self.rule_action.min(automation::ACTIONS.len().saturating_sub(1));
                    let metric = automation::METRICS[metric_index];
                    let action = automation::ACTIONS[action_index];
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("rule-metric")
                            .selected_text(tx(metric.1))
                            .show_ui(ui, |ui| {
                                for (index, (_, label, _)) in automation::METRICS.iter().enumerate()
                                {
                                    ui.selectable_value(&mut self.rule_metric, index, tx(label));
                                }
                            });
                        ui.add(
                            egui::Slider::new(&mut self.rule_threshold, 1..=120).text(metric.2),
                        );
                        egui::ComboBox::from_id_salt("rule-action")
                            .selected_text(tx(action.1))
                            .show_ui(ui, |ui| {
                                for (index, (_, label)) in automation::ACTIONS.iter().enumerate() {
                                    ui.selectable_value(&mut self.rule_action, index, tx(label));
                                }
                            });
                        if ui.button(ts("Ajouter la règle")).clicked() {
                            actions.push(Pending::AddRule {
                                metric: metric.0.to_string(),
                                threshold: self.rule_threshold,
                                action: action.0.to_string(),
                            });
                        }
                        if ui.button(ts("Tester la notification")).clicked() {
                            actions.push(Pending::TestRuleNotification);
                        }
                    });
                }
                ui.label(
                    egui::RichText::new(ts("Les règles sont vérifiées toutes les 30 secondes tant que l'application est ouverte. Une même règle ne notifie qu'une fois par dix minutes."))
                        .size(10.0)
                        .color(self.muted()),
                );

            }
            {
                let ui = &mut cols[right];
                // Le binding de la colonne de gauche ne vit que dans son bloc :
                // on relit l'instantané pour les minuteurs.
                let Some(report) = &snapshot else { return; };
                ui.label(
                    egui::RichText::new(ts("MINUTEURS SYSTEMD"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.timers.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun minuteur systemd détecté."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                let timer_rows = el.fit_rows(elastic::remaining(ui) - 30.0, 22.0, 2, 30);
                for timer in report.timers.iter().take(timer_rows) {
                    let enabled = timer.state == "enabled";
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [400.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&timer.unit).monospace().size(10.5),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(&timer.state)
                                .size(10.0)
                                .color(if enabled {
                                    egui::Color32::from_rgb(0x3f, 0xa9, 0x6a)
                                } else {
                                    self.muted()
                                }),
                        );
                        if ui
                            .button(if enabled { ts("Désactiver…") } else { ts("Activer") })
                            .clicked()
                        {
                            let pending = Pending::SetTimer {
                                unit: timer.unit.clone(),
                                enabled: !enabled,
                            };
                            if enabled {
                                confirmations.push(Confirmation {
                                    title: ts("Désactiver un minuteur").to_string(),
                                    detail: tf("systemctl disable --now {} : les tâches planifiées de ce minuteur ne seront plus exécutées.", &[&timer.unit]),
                                    pending,
                                });
                            } else {
                                actions.push(pending);
                            }
                        }
                    });
                }
            }
        });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Automation));
        }
    }

    fn page_desktop(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.desktop.clone();
        let mut actions: Vec<Pending> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Apparence et extensions GNOME")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} extension(s) · {} raccourci(s)", &[&report.extensions.len(), &report.shortcuts.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des réglages…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                if !report.available {
                    ui.label(
                        egui::RichText::new(
                            ts("gsettings est absent : GNOME n'est pas le bureau actif."),
                        )
                        .size(10.5)
                        .color(self.muted()),
                    );
                    return;
                }

                ui.label(
                    egui::RichText::new(ts("APPARENCE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                let rows: [(&str, &str, &str, &mut String); 4] = [
                    (
                        ts("Thème GTK"),
                        "org.gnome.desktop.interface",
                        "gtk-theme",
                        &mut self.theme_input,
                    ),
                    (
                        ts("Thème d'icônes"),
                        "org.gnome.desktop.interface",
                        "icon-theme",
                        &mut self.icon_input,
                    ),
                    (
                        ts("Police"),
                        "org.gnome.desktop.interface",
                        "font-name",
                        &mut self.font_input,
                    ),
                    (
                        ts("Schéma clair/sombre"),
                        "org.gnome.desktop.interface",
                        "color-scheme",
                        &mut self.scheme_input,
                    ),
                ];
                for (label, schema, key, value) in rows {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [150.0, 18.0],
                            egui::Label::new(egui::RichText::new(label).size(10.5)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .desired_width(240.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        if ui.button(ts("Appliquer")).clicked() {
                            actions.push(Pending::SetGsetting {
                                schema: schema.to_string(),
                                key: key.to_string(),
                                value: value.clone(),
                            });
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tf("Fond d'écran : {}", &[&util::ellipsis(&report.appearance.background, 70)]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tf("Boutons de fenêtre : {} · {} thème(s) · {} jeu(x) d'icônes · {} police(s)", &[&report.appearance.button_layout, &report.appearance.themes.len(), &report.appearance.icons.len(), &report.fonts.len()]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("EXTENSIONS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.extensions.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune extension détectée."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for extension in &report.extensions {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [300.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&extension.id, 48))
                                    .monospace()
                                    .size(10.0),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(if extension.enabled {
                                ts("activée")
                            } else {
                                ts("désactivée")
                            })
                            .size(10.0)
                            .color(if extension.enabled {
                                egui::Color32::from_rgb(0x3f, 0xa9, 0x6a)
                            } else {
                                self.muted()
                            }),
                        );
                        if ui
                            .button(if extension.enabled {
                                ts("Désactiver")
                            } else {
                                ts("Activer")
                            })
                            .clicked()
                        {
                            actions.push(Pending::ToggleExtension {
                                id: extension.id.clone(),
                                enabled: !extension.enabled,
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("RACCOURCIS CLAVIER"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.shortcuts.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun raccourci personnalisé."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for shortcut in report.shortcuts.iter().take(40) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [240.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&shortcut.key).monospace().size(10.0),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(desktop::shortcut_label(&shortcut.value))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    });
                }
            });

        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Desktop));
        }
    }

    fn page_printers(&mut self, ui: &mut egui::Ui) {
        let printers = self.cups_printers.clone();
        let jobs = self.cups_jobs.clone();
        let sharing = self.cups_sharing.clone();
        let filter = self.cups_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Imprimantes et files CUPS")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.cups_filter)
                    .desired_width(160.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::CupsReload);
            }
            ui.label(
                egui::RichText::new(tf("{} imprimante(s) · {} travail(x) en file", &[&printers.len(), &jobs.len()]))
                .size(10.5)
                .color(self.muted()),
            );
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(ts("IMPRIMANTES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if printers.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune imprimante CUPS détectée (lpstat indisponible ou liste vide)."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for printer in &printers {
                    if !filter.is_empty() && !printer.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [170.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&printer.name).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        let color = if !printer.enabled {
                            egui::Color32::from_rgb(224, 92, 92)
                        } else if printer.state == "printing" {
                            egui::Color32::from_rgb(240, 180, 60)
                        } else {
                            egui::Color32::from_rgb(43, 209, 138)
                        };
                        left_cell(ui, 
                            [80.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&printer.state).size(10.0).color(color),
                            ),
                        );
                        left_cell(ui, 
                            [260.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&printer.make_model, 42))
                                    .size(10.0)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Activer")).clicked() {
                            actions.push(Pending::CupsSetEnabled {
                                name: printer.name.clone(),
                                enabled: true,
                            });
                        }
                        if ui.small_button(ts("Désactiver")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Désactiver {}", &[&printer.name]),
                                detail: ts("L'imprimante restera configurée mais n'acceptera plus de travaux.")
                                    .to_string(),
                                pending: Pending::CupsSetEnabled {
                                    name: printer.name.clone(),
                                    enabled: false,
                                },
                            });
                        }
                        if ui.small_button(ts("Vider la file")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Vider la file de {}", &[&printer.name]),
                                detail: ts("Tous les travaux en attente pour cette imprimante seront annulés.")
                                    .to_string(),
                                pending: Pending::CupsClearQueue(printer.name.clone()),
                            });
                        }
                        if ui.small_button(ts("Supprimer…")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Supprimer {}", &[&printer.name]),
                                detail: ts("L'imprimante sera retirée de CUPS (lpadmin -x). Cette action est irréversible.")
                                    .to_string(),
                                pending: Pending::CupsRemove(printer.name.clone()),
                            });
                        }
                    });
                    if !printer.location.is_empty() || !printer.device.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "    {} {}",
                                if printer.location.is_empty() { "".to_string() } else { format!("@{} ", printer.location) },
                                printer.device
                            ))
                            .monospace()
                            .size(9.5)
                            .color(self.muted()),
                        );
                    }
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("TRAVAUX"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if jobs.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun travail en attente."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for job in &jobs {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [120.0, 18.0],
                            egui::Label::new(egui::RichText::new(&job.id).monospace().size(10.0)),
                        );
                        left_cell(ui, 
                            [90.0, 18.0],
                            egui::Label::new(egui::RichText::new(&job.owner).size(10.0)),
                        );
                        left_cell(ui, 
                            [70.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&job.size).size(10.0).color(self.muted()),
                            ),
                        );
                        if job.held {
                            ui.label(
                                egui::RichText::new(ts("bloqué"))
                                    .size(9.5)
                                    .color(egui::Color32::from_rgb(224, 92, 92)),
                            );
                        }
                        if ui.small_button(ts("Annuler")).clicked() {
                            actions.push(Pending::CupsCancelJob(job.id.clone()));
                        }
                        if ui.small_button(ts("Vider la file")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Vider la file de {}", &[&job.printer]),
                                detail: ts("Tous les travaux de cette imprimante seront annulés.").to_string(),
                                pending: Pending::CupsClearQueue(job.printer.clone()),
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("PARTAGE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if let Some(sharing) = &sharing {
                    let exposed = sharing.exposed();
                    ui.horizontal(|ui| {
                        ui.label("CUPS");
                        ui.label(
                            egui::RichText::new(if !sharing.service_installed {
                                ts("absent")
                            } else if sharing.service_enabled {
                                ts("actif au démarrage")
                            } else {
                                ts("non activé au démarrage")
                            })
                            .size(10.5)
                            .color(if sharing.service_enabled {
                                egui::Color32::from_rgb(43, 209, 138)
                            } else {
                                self.muted()
                            }),
                        );
                        ui.label(
                            egui::RichText::new(if exposed {
                                ts("partage exposé sur le réseau")
                            } else {
                                ts("partage non exposé")
                            })
                            .size(10.5)
                            .color(if exposed {
                                egui::Color32::from_rgb(240, 180, 60)
                            } else {
                                self.muted()
                            }),
                        );
                    });
                    ui.label(
                        egui::RichText::new(tf("Port 631 : {} · Browsing : {} · écoute globale : {}", &[&(if sharing.port_631 { ts("oui") } else { ts("non") }), &(if sharing.browsing { ts("oui") } else { ts("non") }), &(if sharing.listen_all { ts("oui") } else { ts("non") })]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_peripherals(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.peripherals.clone();
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Imprimantes et audio")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &snapshot {
                ui.label(
                    egui::RichText::new(tf("{} imprimante(s) · {} sortie(s) · {} entrée(s)", &[&report.printers.len(), &report.outputs.len(), &report.inputs.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &snapshot else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des périphériques…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };

                ui.label(
                    egui::RichText::new(ts("IMPRIMANTES (CUPS)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if !report.cups_available {
                    ui.label(
                        egui::RichText::new(ts("CUPS n'est pas installé : aucun pilote d'impression actif."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                if report.printers.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune imprimante configurée."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for printer in &report.printers {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [220.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&printer.name).monospace().size(10.5),
                            ),
                        );
                        left_cell(ui, 
                            [130.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(tx(&printer.state))
                                    .size(10.5)
                                    .color(if printer.enabled {
                                        egui::Color32::from_rgb(0x3f, 0xa9, 0x6a)
                                    } else {
                                        egui::Color32::from_rgb(0xc0, 0x5a, 0x4a)
                                    }),
                            ),
                        );
                        if printer.default {
                            ui.label(
                                egui::RichText::new(ts("par défaut"))
                                    .size(10.0)
                                    .color(self.muted()),
                            );
                        }
                        ui.label(
                            egui::RichText::new(util::ellipsis(&printer.device, 46))
                                .size(10.0)
                                .color(self.muted()),
                        );
                        if ui
                            .button(if printer.enabled {
                                ts("Désactiver…")
                            } else {
                                ts("Activer")
                            })
                            .clicked()
                        {
                            let enabled = !printer.enabled;
                            let pending = Pending::SetPrinterEnabled {
                                name: printer.name.clone(),
                                enabled,
                            };
                            if enabled {
                                actions.push(pending);
                            } else {
                                confirmations.push(Confirmation {
                                    title: ts("Désactiver une imprimante").to_string(),
                                    detail: tf("cupsdisable {} : les impressions en attente resteront bloquées.", &[&printer.name]),
                                    pending,
                                });
                            }
                        }
                        if !printer.default && ui.button(ts("Par défaut")).clicked() {
                            actions.push(Pending::SetDefaultPrinter(printer.name.clone()));
                        }
                        if ui.button(ts("Supprimer…")).clicked() {
                            confirmations.push(Confirmation {
                                title: ts("Supprimer une imprimante").to_string(),
                                detail: tf("lpadmin -x {} : la file et ses réglages seront supprimés.", &[&printer.name]),
                                pending: Pending::RemovePrinter(printer.name.clone()),
                            });
                        }
                    });
                }

                for (title, devices) in [
                    (ts("SORTIES AUDIO"), &report.outputs),
                    (ts("ENTRÉES AUDIO"), &report.inputs),
                ] {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(title)
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    if !report.audio_available {
                        ui.label(
                            egui::RichText::new(ts("pactl est absent : serveur audio non détecté."))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    }
                    for device in devices.iter() {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [280.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{} · {} %{}",
                                        util::ellipsis(&device.description, 34),
                                        device.volume,
                                        if device.muted { ts(" · muet") } else { "" }
                                    ))
                                    .size(10.5),
                                ),
                            );
                            if device.default {
                                ui.label(
                                    egui::RichText::new(ts("par défaut"))
                                        .size(10.0)
                                        .color(self.muted()),
                                );
                            }
                            if ui.button("−10 %").clicked() {
                                actions.push(Pending::AdjustVolume {
                                    name: device.name.clone(),
                                    delta: -10,
                                });
                            }
                            if ui.button("+10 %").clicked() {
                                actions.push(Pending::AdjustVolume {
                                    name: device.name.clone(),
                                    delta: 10,
                                });
                            }
                            if ui
                                .button(if device.muted {
                                    ts("Rétablir le son")
                                } else {
                                    ts("Muet")
                                })
                                .clicked()
                            {
                                actions.push(Pending::ToggleMute(device.name.clone()));
                            }
                            if !device.default && ui.button(ts("Par défaut")).clicked() {
                                actions.push(Pending::SetDefaultSink(device.name.clone()));
                            }
                        });
                    }
                }
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Peripherals));
        }
    }

    fn page_volumes(&mut self, ui: &mut egui::Ui) {
        let volumes_snapshot = self.volumes.clone();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut actions: Vec<Pending> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("LVM et chiffrement LUKS")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &volumes_snapshot {
                ui.label(
                    egui::RichText::new(tf("{} groupe(s) · {} volume(s) · {} périphérique(s) chiffré(s)", &[&report.groups.len(), &report.volumes.len(), &report.crypts.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(report) = &volumes_snapshot {
                    if !report.available {
                        ui.label(
                            egui::RichText::new(
                                ts("LVM n'est pas installé : la page reste informative."),
                            )
                            .size(10.5)
                            .color(self.muted()),
                        );
                        ui.add_space(6.0);
                    }

                    ui.label(
                        egui::RichText::new(ts("GROUPES DE VOLUMES"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    if report.groups.is_empty() {
                        ui.label(
                            egui::RichText::new(ts("Aucun groupe de volumes LVM."))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    }
                    for group in &report.groups {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [180.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&group.name).monospace().size(11.0),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(tf("{} · libre {} · {} PV / {} LV · {} % utilisé", &[&util::human_bytes(group.size), &util::human_bytes(group.free), &group.physical, &group.logical, &format!("{:.0}", lvm::group_usage(group))]))
                                .size(10.5)
                                .color(self.muted()),
                            );
                        });
                    }

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(ts("VOLUMES LOGIQUES"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    if report.volumes.is_empty() {
                        ui.label(
                            egui::RichText::new(ts("Aucun volume logique."))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    }
                    for volume in &report.volumes {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [280.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{}/{}",
                                        volume.group, volume.name
                                    ))
                                    .monospace()
                                    .size(11.0),
                                ),
                            );
                            left_cell(ui, 
                                [90.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(util::human_bytes(volume.size))
                                        .size(10.5)
                                        .color(self.muted()),
                                ),
                            );
                            left_cell(ui, 
                                [100.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&volume.attributes)
                                        .monospace()
                                        .size(10.0)
                                        .color(self.muted()),
                                ),
                            );
                            if ui.button(ts("Supprimer…")).clicked() {
                                confirmations.push(Confirmation {
                                    title: ts("Supprimer un volume logique").to_string(),
                                    detail: tf("lvremove -f {}/{} : toutes les données de ce volume seront perdues.", &[&volume.group, &volume.name]),
                                    pending: Pending::RemoveVolume(format!(
                                        "{}/{}",
                                        volume.group, volume.name
                                    )),
                                });
                            }
                        });
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(ts("Nouveau volume"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.lv_group)
                                .desired_width(140.0)
                                .hint_text(ts("groupe")),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.lv_name)
                                .desired_width(140.0)
                                .hint_text(ts("nom")),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.lv_size)
                                .desired_width(80.0)
                                .hint_text("10G"),
                        );
                        if ui.button(ts("Créer…")).clicked() {
                            confirmations.push(Confirmation {
                                title: ts("Créer un volume logique").to_string(),
                                detail: format!(
                                    "lvcreate -L {} -n {} {} (Polkit)",
                                    self.lv_size, self.lv_name, self.lv_group
                                ),
                                pending: Pending::CreateVolume {
                                    group: self.lv_group.clone(),
                                    name: self.lv_name.clone(),
                                    size: self.lv_size.clone(),
                                },
                            });
                        }
                    });

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(ts("PÉRIPHÉRIQUES CHIFFRÉS (LUKS)"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    if report.crypts.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                ts("Aucun volume chiffré ouvert. Les partitions LUKS fermées n'apparaissent pas ici."),
                            )
                            .size(10.5)
                            .color(self.muted()),
                        );
                    }
                    for crypt in &report.crypts {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [240.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&crypt.device).monospace().size(10.5),
                                ),
                            );
                            left_cell(ui, 
                                [90.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(if crypt.fstype.is_empty() {
                                        "—"
                                    } else {
                                        crypt.fstype.as_str()
                                    })
                                    .size(10.5)
                                    .color(self.muted()),
                                ),
                            );
                            if ui
                                .button(if crypt.mounted {
                                    ts("Démonter")
                                } else {
                                    ts("Monter")
                                })
                                .clicked()
                            {
                                let device = crypt.device.clone();
                                let mount = !crypt.mounted;
                                let pending = Pending::MountDevice { device, mount };
                                if mount {
                                    actions.push(pending);
                                } else {
                                    confirmations.push(Confirmation {
                                        title: ts("Démonter un périphérique").to_string(),
                                        detail: tf("udisksctl unmount -b {} : arrêter les applications qui l'utilisent.", &[&crypt.device]),
                                        pending,
                                    });
                                }
                            }
                            if ui.button(ts("Fermer…")).clicked() {
                                confirmations.push(Confirmation {
                                    title: ts("Fermer un volume chiffré").to_string(),
                                    detail: tf("cryptsetup close {} : le volume sera verrouillé et devra être réouvert avec sa phrase secrète.", &[&crypt.name]),
                                    pending: Pending::CloseCrypt(crypt.name.clone()),
                                });
                            }
                        });
                    }
                }
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Volumes));
        }
    }

    fn page_kernel(&mut self, ui: &mut egui::Ui) {
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Noyau, swap et paramètres système")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
        });
        ui.add_space(8.0);

        let el = elastic::Elastic::new(ui);
        elastic::density(ui, &el);
        let columns = if el.two_columns() { 2 } else { 1 };
        let right = if columns == 2 { 1usize } else { 0usize };
        ui.columns(columns, |cols| {
            {
                let ui = &mut cols[0];
                if let Some(report) = &self.kernel {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tf("Noyau en cours : {}", &[&report.running]))
                                .monospace()
                                .size(11.0),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "swap : {} · swappiness {} · zram {}",
                                report.swap.len(),
                                report.swappiness,
                                report.zram.len()
                            ))
                            .size(10.5)
                            .color(self.muted()),
                        );
                    });
                }

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(ts("PARAMÈTRES NOYAU (PERSISTANTS)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(
                        ts("Valeurs écrites dans /etc/sysctl.d/99-orbisys.conf puis appliquées par sysctl -p."),
                    )
                    .size(10.5)
                    .color(self.muted()),
                );
                egui::Grid::new("sysctl-grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        for (key, value) in &mut self.kernel_values {
                            ui.label(egui::RichText::new(&*key).monospace().size(11.0));
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .desired_width(200.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            ui.end_row();
                        }
                    });
                if ui.button(ts("Appliquer et persister…")).clicked() {
                    let entries = self.kernel_values.clone();
                    let detail = entries
                        .iter()
                        .map(|(key, value)| format!("{key} = {value}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    confirmations.push(Confirmation {
                        title: ts("Appliquer des paramètres noyau").to_string(),
                        detail: tf("Ces valeurs seront écrites dans /etc/sysctl.d/99-orbisys.conf et appliquées immédiatement (Polkit) :\n{}", &[&detail]),
                        pending: Pending::ApplySysctl(entries),
                    });
                }

            }
            {
                let ui = &mut cols[right];
                ui.label(
                    egui::RichText::new(ts("ÉCHANGE (SWAP)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if let Some(report) = &self.kernel {
                    if report.swap.is_empty() {
                        ui.label(
                            egui::RichText::new(ts("Aucune zone d'échange active."))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    }
                    for entry in &report.swap {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [260.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(util::ellipsis(&entry.device, 40))
                                        .monospace()
                                        .size(10.5),
                                ),
                            );
                            left_cell(ui, 
                                [90.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&entry.kind).size(10.5).color(self.muted()),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(tf("{} / {} · priorité {}", &[&util::human_bytes(entry.used), &util::human_bytes(entry.size), &entry.priority.as_str()]))
                                .size(10.5)
                                .color(self.muted()),
                            );
                        });
                    }
                    for zram in &report.zram {
                        ui.label(
                            egui::RichText::new(zram).size(10.5).color(self.muted()),
                        );
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(ts("Échange compressé (zram)"))
                                .size(10.5)
                                .strong(),
                        );
                        let active = !report.zram.is_empty();
                        ui.label(
                            egui::RichText::new(if active {
                                ts("actif")
                            } else {
                                ts("inactif")
                            })
                            .size(10.0)
                            .color(if active {
                                egui::Color32::from_rgb(0x3f, 0xa9, 0x6a)
                            } else {
                                self.muted()
                            }),
                        );
                        if active {
                            if ui.button(ts("Désactiver zram…")).clicked() {
                                confirmations.push(Confirmation {
                                    title: ts("Désactiver zram").to_string(),
                                    detail: ts("Le périphérique /dev/zram0 sera retiré et le module déchargé. La mémoire compressée qu'il contient est perdue.").to_string(),
                                    pending: Pending::SetZram(false),
                                });
                            }
                        } else if ui.button(ts("Activer zram…")).clicked() {
                            confirmations.push(Confirmation {
                                title: ts("Activer zram").to_string(),
                                detail: ts("Le module zram est chargé et systemd-zram-setup@zram0.service démarré : une zone d'échange compressée en RAM sera créée.").to_string(),
                                pending: Pending::SetZram(true),
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("NOYAUX INSTALLÉS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if let Some(report) = &self.kernel {
                    for entry in &report.kernels {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&entry.version).monospace().size(11.0),
                            );
                            if entry.running {
                                ui.label(
                                    egui::RichText::new(ts("en cours"))
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(0x3f, 0xa9, 0x6a)),
                                );
                            }
                        });
                    }
                    let candidates = kernel::removal_candidates(&report.kernels);
                    if candidates.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                ts("Rien à supprimer : seuls le noyau en cours et le plus récent sont conservés."),
                            )
                            .size(10.5)
                            .color(self.muted()),
                        );
                    } else if ui
                        .button(tf("Supprimer {} ancien(s) noyau(x)…", &[&candidates.len()]))
                        .clicked()
                    {
                        confirmations.push(Confirmation {
                            title: ts("Supprimer des noyaux").to_string(),
                            detail: tf("Paquets à purger :\n{}\nLe noyau en cours et le plus récent sont toujours conservés.", &[&candidates.join("\n")]),
                            pending: Pending::RemoveKernels(candidates),
                        });
                    }
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("PILOTES RECOMMANDÉS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if let Some(report) = &self.kernel {
                    if report.drivers.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                ts("Aucune recommandation (ubuntu-drivers absent ou matériel entièrement libre)."),
                            )
                            .size(10.5)
                            .color(self.muted()),
                        );
                    }
                    for driver in &report.drivers {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} → {}{}",
                                    util::ellipsis(&driver.device, 44),
                                    driver.driver,
                                    if driver.free { ts(" (libre)") } else { "" }
                                ))
                                .size(10.5),
                            );
                            if ui.button(ts("Installer…")).clicked() {
                                confirmations.push(Confirmation {
                                    title: ts("Installer un pilote").to_string(),
                                    detail: tf("Le pilote {} sera installé pour {}. Un redémarrage peut être nécessaire.", &[&driver.driver, &driver.device]),
                                    pending: Pending::InstallDriver(driver.driver.clone()),
                                });
                            }
                        });
                    }
                }
            }
        });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Kernel));
        }
    }

    fn page_backup(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(ts("Liste des logiciels, copie, dotfiles et GRUB")).strong(),
            );
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &self.backup {
                ui.label(
                    egui::RichText::new(tf("{} élément(s) de configuration · {} noyau(x)", &[&report.dotfiles.len(), &report.grub.kernels.len()]))
                    .size(11.0)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(ts("LISTE DES LOGICIELS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(
                        ts("Export au format JSON (nom, version, source) puis réinstallation via APT, Snap et Flatpak."),
                    )
                    .size(10.5)
                    .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.list_path)
                            .desired_width(430.0),
                    );
                    if ui.button(ts("Exporter")).clicked() {
                        actions.push(Pending::ExportList);
                    }
                    if ui.button(ts("Réinstaller…")).clicked() {
                        let path = std::path::PathBuf::from(self.list_path.clone());
                        confirmations.push(Confirmation {
                            title: ts("Réinstaller des logiciels").to_string(),
                            detail: tf("Les logiciels listés dans {} seront installés (Polkit).\nVérifiez le fichier avant de continuer.", &[&path.display()]),
                            pending: Pending::ImportList(path),
                        });
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("COPIE RSYNC"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(
                        ts("La simulation utilise --dry-run : rien n'est écrit, rsync affiche ce qu'il ferait."),
                    )
                    .size(10.5)
                    .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Source"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rsync_source)
                            .desired_width(320.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Destination"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rsync_destination)
                            .desired_width(320.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.rsync_mirror, ts("mode miroir (--delete)"));
                    if ui.button(ts("Simuler")).clicked() {
                        actions.push(Pending::Rsync(RsyncOptions {
                            source: std::path::PathBuf::from(self.rsync_source.clone()),
                            destination: std::path::PathBuf::from(
                                self.rsync_destination.clone(),
                            ),
                            mirror: self.rsync_mirror,
                            delete: self.rsync_delete,
                            dry_run: true,
                        }));
                    }
                    if ui.button(ts("Copier…")).clicked() {
                        let options = RsyncOptions {
                            source: std::path::PathBuf::from(self.rsync_source.clone()),
                            destination: std::path::PathBuf::from(
                                self.rsync_destination.clone(),
                            ),
                            mirror: self.rsync_mirror,
                            delete: self.rsync_delete,
                            dry_run: false,
                        };
                        if options.mirror {
                            confirmations.push(Confirmation {
                                title: ts("Copie en mode miroir").to_string(),
                                detail: ts("Les fichiers absents de la source seront supprimés de la destination.").to_string(),
                                pending: Pending::Rsync(options),
                            });
                        } else {
                            actions.push(Pending::Rsync(options));
                        }
                    }
                });
                if !self.rsync_output.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(self.rsync_output.clone());
                        });
                }


                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("GRUB")
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if let Some(report) = &self.backup {
                    egui::Grid::new("grub-grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (label, value) in [
                                (ts("Entrée par défaut"), report.grub.default.clone()),
                                (ts("Délai d'attente"), format!("{} s", report.grub.timeout)),
                                (ts("Ligne de commande"), report.grub.cmdline.clone()),
                                (
                                    ts("Noyaux"),
                                    if report.grub.kernels.is_empty() {
                                        "—".to_string()
                                    } else {
                                        report.grub.kernels.join(", ")
                                    },
                                ),
                            ] {
                                ui.label(
                                    egui::RichText::new(label).size(11.0).color(self.muted()),
                                );
                                ui.label(egui::RichText::new(util::ellipsis(&value, 80)).size(11.5));
                                ui.end_row();
                            }
                        });
                }
                ui.horizontal(|ui| {
                    if ui.button(ts("Régénérer la configuration…")).clicked() {
                        confirmations.push(Confirmation {
                            title: ts("Régénérer GRUB").to_string(),
                            detail:
                                ts("update-grub réécrit /boot/grub/grub.cfg. Une entrée manquante peut empêcher le démarrage : vérifiez les noyaux listés ci-dessus.")
                                    .to_string(),
                            pending: Pending::RegenerateGrub,
                        });
                    }
                });
            });

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Backup));
        }
    }

    fn page_netadvanced(&mut self, ui: &mut egui::Ui) {
        let data = self.netadv.clone();
        let diag = self.netadv_diag.clone();
        let (gateway, device) = netadv::default_route();
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Réseau étendu")).strong());
            ui.label(
                egui::RichText::new(tf("route {} via {} · {} interface(s)", &[&if device.is_empty() { "—" } else { &device }, &if gateway.is_empty() { "—" } else { &gateway }, &data.as_ref().map(|data| data.interfaces.len()).unwrap_or(0)]))
                .size(10.5)
                .color(self.muted()),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::NetAdvReload);
            }
            if ui.button(ts("Diagnostic complet")).clicked() {
                actions.push(Pending::NetAdvDiagnose);
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(_) = &data else {
                    ui.label(
                        egui::RichText::new(ts("Lecture du réseau…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };


                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("DIAGNOSTIC"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netadv_host)
                            .desired_width(260.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("hôte ou adresse")),
                    );
                    if ui.button("Ping").clicked() {
                        actions.push(Pending::NetAdvPing(self.netadv_host.clone()));
                    }
                    if ui.button(ts("Résoudre")).clicked() {
                        actions.push(Pending::NetAdvResolve(self.netadv_host.clone()));
                    }
                });
                if let Some(diag) = &diag {
                    ui.add_space(4.0);
                    for (label, text) in [
                        (ts("Passerelle"), &diag.ping_output),
                        ("DNS", &diag.dns_output),
                        ("Internet", &diag.internet_output),
                    ] {
                        ui.label(
                            egui::RichText::new(label)
                                .size(9.5)
                                .strong()
                                .color(self.muted()),
                        );
                        for line in text.lines().take(4) {
                            ui.label(egui::RichText::new(line).monospace().size(9.5));
                        }
                    }
                }
                if !self.netadv_output.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("netadv-output")
                        .max_height(140.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(self.netadv_output.clone());
                        });
                }
            });

        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_network(&mut self, ui: &mut egui::Ui) {
        let mut reload = false;
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmation: Option<Confirmation> = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Interfaces, DNS, SSH, partages et connexions")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &self.network {
                ui.label(
                    egui::RichText::new(tf("{} interface(s) · {} connexion(s)", &[&report.interfaces.len(), &report.connections.len()]))
                    .size(11.0)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(6.0);

        let Some(report) = self.network.clone() else {
            ui.label(egui::RichText::new(ts("Lecture en cours…")).color(self.muted()));
            if reload {
                self.apply_pending(Pending::Refresh(Page::Network));
            }
            return;
        };

        let el = elastic::Elastic::new(ui);
        elastic::density(ui, &el);
        let columns = if el.two_columns() { 2 } else { 1 };
        let right = if columns == 2 { 1usize } else { 0usize };
        ui.columns(columns, |cols| {
            {
                let ui = &mut cols[0];
                ui.label(
                    egui::RichText::new(ts("INTERFACES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for interface in &report.interfaces {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [130.0, 18.0],
                            egui::Label::new(egui::RichText::new(&interface.name).strong().size(11.5)),
                        );
                        left_cell(ui, 
                            [80.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&interface.state)
                                    .size(10.5)
                                    .color(if interface.state == "UP" {
                                        egui::Color32::from_rgb(43, 209, 138)
                                    } else {
                                        self.muted()
                                    }),
                            ),
                        );
                        left_cell(ui, 
                            [300.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(if interface.addresses.is_empty() {
                                    "—".to_string()
                                } else {
                                    interface.addresses.join(", ")
                                })
                                .monospace()
                                .size(10.5),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · MTU {}",
                                if interface.mac.is_empty() {
                                    "—"
                                } else {
                                    interface.mac.as_str()
                                },
                                interface.mtu
                            ))
                            .size(10.0)
                            .color(self.muted()),
                        );
                    });
                }


                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("DNS")
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(tf("Serveurs : {}", &[&(if report.dns.nameservers.is_empty() {
                            "—".to_string()
                        } else {
                            report.dns.nameservers.join(", ")
                        })]))
                    .size(11.0),
                );
                ui.label(
                    egui::RichText::new(tf("Recherche : {} · {} entrée(s) /etc/hosts", &[&(if report.dns.search.is_empty() {
                            "—".to_string()
                        } else {
                            report.dns.search.join(" ")
                        }), &report.dns.hosts.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(ts("Résolveurs proposés"))
                            .size(10.0)
                            .color(self.muted()),
                    );
                    for (label, servers) in netadv::DNS_PRESETS {
                        if ui.small_button(label).clicked() {
                            confirmation = Some(Confirmation {
                                title: format!("DNS : {label}"),
                                detail: servers.join(", "),
                                pending: Pending::ApplyDns(
                                    servers.iter().map(|server| server.to_string()).collect(),
                                ),
                            });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Serveur DNS"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dns_server_input)
                            .desired_width(150.0)
                            .hint_text("1.1.1.1"),
                    );
                    let valid = netadv::valid_nameserver(&self.dns_server_input);
                    if ui
                        .add_enabled(valid, egui::Button::new(ts("Appliquer le DNS…")))
                        .clicked()
                    {
                        confirmation = Some(Confirmation {
                            title: ts("Serveur DNS").to_string(),
                            detail: self.dns_server_input.trim().to_string(),
                            pending: Pending::ApplyDns(vec![
                                self.dns_server_input.trim().to_string()
                            ]),
                        });
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(ts("Hôtes (/etc/hosts)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                let host_rows = el.fit_rows(elastic::remaining(ui) - 150.0, 22.0, 2, 12);
                for entry in self.hosts_entries.iter().take(host_rows) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [140.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&entry.address).monospace().size(10.5),
                            ),
                        );
                        left_cell(ui, 
                            [320.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(entry.names.join(" ")).size(10.5),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Supprimer…")).clicked() {
                            if let Some(name) = entry.names.first() {
                                confirmation = Some(Confirmation {
                                    title: ts("Retirer un hôte").to_string(),
                                    detail: format!(
                                        "{} {} : {}",
                                        entry.address,
                                        name,
                                        ts("l'entrée sera retirée de /etc/hosts")
                                    ),
                                    pending: Pending::RemoveHost {
                                        address: entry.address.clone(),
                                        name: name.clone(),
                                    },
                                });
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(ts("Adresse"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.host_address)
                            .desired_width(130.0)
                            .hint_text("192.168.1.10"),
                    );
                    ui.label(ts("Nom d'hôte"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.host_name)
                            .desired_width(160.0)
                            .hint_text("nas.local"),
                    );
                    if ui.button(ts("Ajouter")).clicked() {
                        actions.push(Pending::AddHost {
                            address: self.host_address.clone(),
                            name: self.host_name.clone(),
                        });
                    }
                });

            }
            {
                let ui = &mut cols[right];
                ui.label(
                    egui::RichText::new(ts("SERVEUR SSH"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(tf("{} · port {} · PermitRootLogin {} · PasswordAuthentication {}", &[&if !report.ssh.installed {
                            ts("non installé").to_string()
                        } else if report.ssh.active {
                            ts("actif").to_string()
                        } else {
                            ts("arrêté").to_string()
                        }, &report.ssh.port, &orbisys::i18n_ui::tr(self.lang, &report.ssh.root_login), &orbisys::i18n_ui::tr(self.lang, &report.ssh.password_auth)]))
                    .size(11.0),
                );

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(ts("PARTAGES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.shares.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun partage Samba ou NFS configuré."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for share in &report.shares {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [70.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(share.kind)
                                    .size(10.5)
                                    .color(self.accent_color()),
                            ),
                        );
                        left_cell(ui, 
                            [160.0, 18.0],
                            egui::Label::new(egui::RichText::new(&share.name).size(11.0)),
                        );
                        left_cell(ui, 
                            [280.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&share.path)
                                    .monospace()
                                    .size(10.5)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&share.detail, 40))
                                .size(10.0)
                                .color(self.muted()),
                        );
                    });
                }

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(tf("CONNEXIONS ({})", &[&report.connections.len()]))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                let connection_rows = el.fit_rows(elastic::remaining(ui) - 26.0, 22.0, 2, 80);
                for connection in report.connections.iter().take(connection_rows) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [50.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&connection.protocol)
                                    .size(10.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [230.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&connection.local)
                                    .monospace()
                                    .size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [230.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&connection.peer)
                                    .monospace()
                                    .size(10.5)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&connection.process, 44))
                                .size(10.0)
                                .color(self.muted()),
                        );
                    });
                }

                for error in &report.errors {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(10.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }
            }
        });

        if let Some(confirmation) = confirmation {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Network));
        }
    }

    fn page_firewall(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Pending> = Vec::new();
        let mut confirmations: Vec<Confirmation> = Vec::new();
        let mut reload = false;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Pare-feu UFW et tunnels VPN")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
        });
        ui.add_space(6.0);

        let Some(report) = self.firewall.clone() else {
            ui.label(egui::RichText::new(ts("Lecture en cours…")).color(self.muted()));
            if reload {
                self.apply_pending(Pending::Refresh(Page::Firewall));
            }
            return;
        };

        ui.horizontal(|ui| {
            let active = report.status.active;
            ui.label(
                egui::RichText::new(if active { ts("Actif") } else { ts("Inactif") })
                    .size(16.0)
                    .strong()
                    .color(if active {
                        egui::Color32::from_rgb(43, 209, 138)
                    } else {
                        egui::Color32::from_rgb(224, 92, 92)
                    }),
            );
            if !report.status.incoming.is_empty() {
                ui.label(
                    egui::RichText::new(tf("entrant {} · sortant {}", &[&report.status.incoming, &report.status.outgoing]))
                    .size(11.0)
                    .color(self.muted()),
                );
            }
            if ui.button(ts("Activer")).clicked() {
                actions.push(Pending::Firewall(FirewallAction::Enable));
            }
            if ui.button(ts("Désactiver…")).clicked() {
                let action = FirewallAction::Disable;
                confirmations.push(Confirmation {
                    title: firewall_label(&action),
                    detail: ts("Toutes les règles seront ignorées et les ports seront exposés.")
                        .to_string(),
                    pending: Pending::Firewall(action),
                });
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(ts("Port à autoriser ou refuser"));
            ui.add(
                egui::TextEdit::singleline(&mut self.firewall_port)
                    .hint_text("22/tcp")
                    .desired_width(110.0),
            );
            let valid = firewall::valid_port_spec(self.firewall_port.trim());
            if ui.add_enabled(valid, egui::Button::new(ts("Autoriser"))).clicked() {
                actions.push(Pending::Firewall(FirewallAction::Allow(
                    self.firewall_port.trim().to_string(),
                )));
            }
            if ui.add_enabled(valid, egui::Button::new(ts("Refuser…"))).clicked() {
                let action = FirewallAction::Deny(self.firewall_port.trim().to_string());
                confirmations.push(Confirmation {
                    title: firewall_label(&action),
                    detail: ts("Le trafic correspondant sera bloqué.").to_string(),
                    pending: Pending::Firewall(action),
                });
            }
            if !valid {
                ui.label(
                    egui::RichText::new(ts("format : 22, 22/tcp ou 8080/udp"))
                        .size(10.0)
                        .color(self.muted()),
                );
            }
        });

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(ts("RÈGLES"))
                .size(10.0)
                .strong()
                .color(self.muted()),
        );
        if report.status.rules.is_empty() {
            ui.label(
                egui::RichText::new(ts("Aucune règle détaillée disponible (pare-feu inactif ou privilèges insuffisants)."))
                    .size(10.5)
                    .color(self.muted()),
            );
        }
        for rule in &report.status.rules {
            ui.horizontal(|ui| {
                left_cell(ui, 
                    [40.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(format!("#{} ", rule.number))
                            .monospace()
                            .size(10.5)
                            .color(self.muted()),
                    ),
                );
                left_cell(ui, 
                    [200.0, 18.0],
                    egui::Label::new(util::ellipsis(&rule.target, 28)).truncate(),
                );
                left_cell(ui, 
                    [100.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(&rule.action)
                            .size(10.5)
                            .color(self.accent_color()),
                    ),
                );
                left_cell(ui, 
                    [240.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(util::ellipsis(&rule.source, 34))
                            .size(10.5)
                            .color(self.muted()),
                    )
                    .truncate(),
                );
                if ui.small_button(ts("Supprimer…")).clicked() {
                    let action = FirewallAction::Delete(rule.number, rule.fingerprint());
                    confirmations.push(Confirmation {
                        title: firewall_label(&action),
                        detail: ts("La règle sera retirée immédiatement.").to_string(),
                        pending: Pending::Firewall(action),
                    });
                }
            });
        }

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("VPN")
                .size(10.0)
                .strong()
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(tf("interfaces : {} · wireguard : {} · connexions actives : {}", &[&(if report.vpn.interfaces.is_empty() {
                    "—".to_string()
                } else {
                    report.vpn.interfaces.join(", ")
                }), &(if report.vpn.wireguard.is_empty() {
                    "—".to_string()
                } else {
                    report.vpn.wireguard.join(", ")
                }), &(if report.vpn.active_connections.is_empty() {
                    "—".to_string()
                } else {
                    report.vpn.active_connections.join(", ")
                })]))
            .size(11.0),
        );

        for error in &report.errors {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(tx(error))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(224, 92, 92)),
            );
        }

        if let Some(confirmation) = confirmations.into_iter().next() {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
        if reload {
            self.apply_pending(Pending::Refresh(Page::Firewall));
        }
    }

    fn page_quotas(&mut self, ui: &mut egui::Ui) {
        let report = self.quota_report.clone();
        let filter = self.quota_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Quotas de disque")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.quota_filter)
                    .desired_width(160.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::QuotasReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} entrée(s) · {} montage(s) avec quotas · outil {}", &[&report.entries.len(), &report.mounts.len(), &(if report.tool_available { ts("présent") } else { ts("absent") })]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des quotas…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.label(
                    egui::RichText::new(ts("MONTAGES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.mounts.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun montage ne déclare usrquota/grpquota/prjquota dans /etc/fstab."))
                            .size(10.0)
                            .color(self.muted()),
                    );
                }
                for mount in &report.mounts {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [160.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&mount.mount).monospace().size(10.5),
                            ),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{}{}{}",
                                if mount.user_quota { "u" } else { "-" },
                                if mount.group_quota { "g" } else { "-" },
                                if mount.project_quota { "p" } else { "-" }
                            ))
                            .monospace()
                            .size(10.0)
                            .color(self.muted()),
                        );
                        if ui.small_button(ts("Activer")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Activer les quotas sur {}", &[&mount.mount]),
                                detail: ts("quotaon activera les quotas déjà configurés sur ce montage.")
                                    .to_string(),
                                pending: Pending::QuotasEnable(mount.mount.clone()),
                            });
                        }
                    });
                }

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(ts("CONSOMMATION"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.entries.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune entrée de quota (quotas non activés ou outil absent)."))
                            .size(10.0)
                            .color(self.muted()),
                    );
                }
                let entries: Vec<&quota::QuotaEntry> = report
                    .entries
                    .iter()
                    .filter(|entry| {
                        filter.is_empty()
                            || entry.name.to_lowercase().contains(&filter)
                            || entry.mount.to_lowercase().contains(&filter)
                    })
                    .collect();
                for entry in entries.iter().take(300) {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [140.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&entry.name).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [60.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(entry.kind).size(9.5).color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [170.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} / {}",
                                    util::human_bytes(entry.used),
                                    if entry.hard > 0 {
                                        util::human_bytes(entry.hard)
                                    } else if entry.soft > 0 {
                                        util::human_bytes(entry.soft)
                                    } else {
                                        ts("illimité").to_string()
                                    }
                                ))
                                .size(10.0),
                            ),
                        );
                        let percent = entry.usage_percent();
                        left_cell(ui, 
                            [70.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(if percent > 0.0 {
                                    format!("{percent:.0} %")
                                } else {
                                    "—".to_string()
                                })
                                .size(10.0)
                                .color(if entry.exceeded {
                                    egui::Color32::from_rgb(224, 92, 92)
                                } else if entry.alert() {
                                    egui::Color32::from_rgb(240, 180, 60)
                                } else {
                                    self.muted()
                                }),
                            ),
                        );
                        if entry.exceeded {
                            ui.label(
                                egui::RichText::new(ts("dépassé"))
                                    .size(9.5)
                                    .color(egui::Color32::from_rgb(224, 92, 92)),
                            );
                        }
                    });
                }
                if entries.len() > 300 {
                    ui.label(
                        egui::RichText::new(tf("… {} entrée(s) masquée(s), affinez le filtre.", &[&(entries.len() - 300)]))
                        .size(9.5)
                        .color(self.muted()),
                    );
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("APPLIQUER UN QUOTA"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Utilisateur"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quota_user)
                            .desired_width(130.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.label(ts("Montage"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quota_mount)
                            .desired_width(130.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("/home"),
                    );
                    ui.label(ts("Souple"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quota_soft)
                            .desired_width(90.0)
                            .hint_text("2G"),
                    );
                    ui.label(ts("Dure"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quota_hard)
                            .desired_width(90.0)
                            .hint_text("3G"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button(ts("Appliquer…")).clicked() {
                        let soft = quota::parse_size(&self.quota_soft);
                        let hard = quota::parse_size(&self.quota_hard);
                        if let Err(error) = quota::validate_setquota(
                            &self.quota_user,
                            &self.quota_mount,
                            soft,
                            hard,
                        ) {
                            self.toast(tf("quota : {}", &[&error]), false);
                        } else {
                            confirm = Some(Confirmation {
                                title: tf("Quota pour {}", &[&self.quota_user]),
                                detail: tf("{} souple / {} dure sur {} (setquota, polkit).", &[&util::human_bytes(soft), &util::human_bytes(hard), &self.quota_mount]),
                                pending: Pending::SetQuota {
                                    user: self.quota_user.clone(),
                                    mount: self.quota_mount.clone(),
                                    soft,
                                    hard,
                                },
                            });
                        }
                    }
                });
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_storage(&mut self, ui: &mut egui::Ui) {
        let mut reload = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(ts("Appareils, montages, Btrfs et instantanés")).strong(),
            );
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
            if let Some(report) = &self.storage {
                ui.label(
                    egui::RichText::new(tf("{} appareil(s) · {} montage(s) · {} instantané(s)", &[&report.devices.len(), &report.mounts.len(), &report.snapshots.len()]))
                    .size(11.0)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(6.0);

        let Some(report) = self.storage.clone() else {
            ui.label(egui::RichText::new(ts("Lecture en cours…")).color(self.muted()));
            if reload {
                self.apply_pending(Pending::Refresh(Page::Storage));
            }
            return;
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(ts("APPAREILS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for (depth, device) in &report.devices {
                    ui.horizontal(|ui| {
                        ui.add_space(*depth as f32 * 14.0);
                        left_cell(ui, 
                            [190.0, 18.0],
                            egui::Label::new(util::ellipsis(&device.name, 26)).truncate(),
                        );
                        left_cell(ui, 
                            [90.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::human_bytes(device.size))
                                    .monospace()
                                    .size(11.0)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [80.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&device.kind).size(10.5).color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [70.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&device.fstype)
                                    .size(10.5)
                                    .color(self.accent_color()),
                            ),
                        );
                        left_cell(ui, 
                            [170.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(if device.mountpoint.is_empty() {
                                    "—".to_string()
                                } else {
                                    device.mountpoint.clone()
                                })
                                .size(10.5),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&device.model, 34))
                                .size(10.5)
                                .color(self.muted()),
                        );
                    });
                }


                if !report.fstab.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("FSTAB")
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    for entry in &report.fstab {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} → {} ({}, {})",
                                entry.source, entry.target, entry.fstype, entry.options
                            ))
                            .size(10.5)
                            .color(self.muted()),
                        );
                    }
                }

                if !report.btrfs.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("BTRFS")
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    for line in &report.btrfs {
                        ui.label(egui::RichText::new(line).monospace().size(10.5));
                    }
                }

                if !report.snapshots.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(ts("INSTANTANÉS TIMESHIFT"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    for snapshot in &report.snapshots {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [220.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&snapshot.created).size(11.0),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(&snapshot.tags)
                                    .size(10.5)
                                    .color(self.accent_color()),
                            );
                        });
                    }
                }

                for error in &report.errors {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(10.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }
            });

        if reload {
            self.apply_pending(Pending::Refresh(Page::Storage));
        }
    }

    fn page_duplicates(&mut self, ui: &mut egui::Ui) {
        let mut scan_requested = false;
        let mut export_requested = false;
        let mut delete_group: Option<storage::DuplicateGroup> = None;
        ui.horizontal(|ui| {
            ui.label(ts("Dossier"));
            ui.add(egui::TextEdit::singleline(&mut self.duplicate_root).desired_width(320.0));
            ui.label(ts("taille minimale (Kio)"));
            let mut kilobytes = (self.duplicate_min_size / 1024).max(1);
            if ui
                .add(egui::DragValue::new(&mut kilobytes).range(1..=1_048_576))
                .changed()
            {
                self.duplicate_min_size = kilobytes * 1024;
            }
            if ui.button(ts("Analyser")).clicked() {
                scan_requested = true;
            }
            if self.duplicates.as_ref().is_some_and(|scan| !scan.groups.is_empty())
                && ui.button(ts("Exporter en CSV")).clicked()
            {
                export_requested = true;
            }
        });
        ui.label(
            egui::RichText::new(
                ts("Comparaison par taille puis empreinte des 64 premiers Kio. Dossiers techniques ignorés (.git, node_modules, target, .cache, snap)."),
            )
            .size(10.5)
            .color(self.muted()),
        );
        ui.add_space(6.0);

        if let Some(scan) = self.duplicates.clone() {
            ui.label(
                egui::RichText::new(tf("{} fichiers analysés · {} groupe(s) · {} récupérables{}", &[&scan.files_scanned, &scan.groups.len(), &util::human_bytes(scan.bytes_duplicated), &(if scan.truncated { ts(" (limite atteinte)") } else { "" })]))
                .strong(),
            );
            ui.add_space(6.0);
            let groups = scan.groups.clone();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 20.0, groups.len(), |ui, range| {
                    for index in range {
                        let group = &groups[index];
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [400.0, 17.0],
                                egui::Label::new(
                                    util::ellipsis(&group.paths[0].display().to_string(), 62),
                                )
                                .truncate(),
                            );
                            left_cell(ui, 
                                [80.0, 17.0],
                                egui::Label::new(
                                    egui::RichText::new(util::human_bytes(group.size))
                                        .monospace()
                                        .size(11.0)
                                        .color(self.accent_color()),
                                ),
                            );
                            left_cell(ui, 
                                [50.0, 17.0],
                                egui::Label::new(
                                    egui::RichText::new(format!("× {}", group.paths.len()))
                                        .size(11.0),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(util::ellipsis(
                                    &group.paths[1].display().to_string(),
                                    48,
                                ))
                                .size(10.5)
                                .color(self.muted()),
                            );
                            if ui.small_button(ts("Supprimer les copies…")).clicked() {
                                delete_group = Some(group.clone());
                            }
                        });
                    }
                });
        } else {
            ui.label(egui::RichText::new(ts("Aucune analyse lancée.")).color(self.muted()));
        }

        if scan_requested {
            self.apply_pending(Pending::Refresh(Page::Duplicates));
        }
        if export_requested {
            self.apply_pending(Pending::ExportDuplicatesCsv);
        }
        if let Some(group) = delete_group {
            let copies = group.paths.len().saturating_sub(1);
            self.confirm = Some(Confirmation {
                title: ts("Supprimer les copies").to_string(),
                detail: tf("Garder {} et supprimer {} copie(s) identique(s) ({} libérés) ?", &[&group.paths[0].display(), &copies, &util::human_bytes(group.size * copies as u64)]),
                pending: Pending::DeleteDuplicates(group),
            });
        }
    }

    fn page_hardware(&mut self, ui: &mut egui::Ui) {
        let mut reload = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Composants et capteurs, en lecture seule")).strong());
            if ui.button(ts("Relire")).clicked() {
                reload = true;
            }
        });
        ui.add_space(6.0);

        let Some(report) = self.hardware.clone() else {
            ui.label(egui::RichText::new(ts("Lecture en cours…")).color(self.muted()));
            if reload {
                self.apply_pending(Pending::Refresh(Page::Hardware));
            }
            return;
        };

        egui::Grid::new("hardware-grid")
            .num_columns(2)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                let rows = vec![
                    (ts("Processeur"), report.cpu_model.clone()),
                    (
                        ts("Cœurs"),
                        tf("{} physiques · {} logiques · {}", &[&report.physical_cores, &report.logical_cpus, &report.architecture]),
                    ),
                    (
                        ts("Mémoire"),
                        format!(
                            "{} RAM · {} swap",
                            util::human_bytes(report.memory_total),
                            util::human_bytes(report.swap_total)
                        ),
                    ),
                    (
                        ts("Châssis"),
                        format!("{} {}", report.vendor, report.product).trim().to_string(),
                    ),
                    ("BIOS", report.bios.clone()),
                    (
                        ts("Cartes graphiques"),
                        if report.gpus.is_empty() {
                            ts("non détectées").to_string()
                        } else {
                            report.gpus.join(" · ")
                        },
                    ),
                    (
                        "Bus",
                        tf("{} périphériques USB · {} PCI", &[&report.usb_devices, &report.pci_devices]),
                    ),
                ];
                for (label, value) in rows {
                    ui.label(egui::RichText::new(label).size(11.5).color(self.muted()));
                    ui.label(egui::RichText::new(util::ellipsis(&value, 96)).size(12.0));
                    ui.end_row();
                }
            });


        if reload {
            self.apply_pending(Pending::Refresh(Page::Hardware));
        }
    }

    fn page_cleanup(&mut self, ui: &mut egui::Ui) {
        let mut pending = Vec::new();
        ui.horizontal(|ui| {
            let total: u64 = self.cleanup_items.iter().map(|item| item.size).sum();
            ui.label(
                egui::RichText::new(tf("Total mesuré : {}", &[&util::human_bytes(total)])).strong(),
            );
            if ui.button(ts("Remesurer")).clicked() {
                pending.push(Pending::Refresh(Page::Cleanup));
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                ts("Chaque cible est mesurée avant action ; les cibles système demandent Polkit."),
            )
            .size(11.0)
            .color(self.muted()),
        );
        ui.add_space(8.0);
        for item in self.cleanup_items.clone() {
            ui.horizontal(|ui| {
                left_cell(ui, 
                    [300.0, 18.0],
                    egui::Label::new(util::ellipsis(&tx(&item.label), 40)).truncate(),
                );
                left_cell(ui, 
                    [100.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(if item.size > 0 {
                            util::human_bytes(item.size)
                        } else {
                            ts("à traiter").to_string()
                        })
                        .monospace()
                        .size(11.0)
                        .color(self.accent_color()),
                    ),
                );
                left_cell(ui, 
                    [520.0, 18.0],
                    egui::Label::new(
                        egui::RichText::new(util::ellipsis(&tx(&item.description), 80))
                            .size(11.0)
                            .color(self.muted()),
                    )
                    .truncate(),
                );
                if ui.small_button(ts("Nettoyer…")).clicked() {
                    pending.push(Pending::Clean(item.clone()));
                }
            });
        }
        for item in pending {
            self.apply_pending(item);
        }
    }

    fn page_processes(&mut self, ui: &mut egui::Ui) {
        self.sample_processes(false);
        ui.ctx().request_repaint_after(Duration::from_millis(1500));
        let mut refresh_now = false;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tf("{} processus · CPU {} % · charge {} / {} / {}", &[&self.snapshot.process_count, &format!("{:.0}", self.snapshot.cpu_percent), &format!("{:.2}", self.snapshot.load.0), &format!("{:.2}", self.snapshot.load.1), &format!("{:.2}", self.snapshot.load.2)]))
                .strong(),
            );
            if ui.button(ts("Actualiser")).clicked() {
                refresh_now = true;
            }
        });
        ui.add_space(4.0);
        // Graphiques : système, classements et processus sélectionné.
        {
            use orbisys::panel::{Chart, Series};
            let (lang, accent, muted, dark) = (self.lang, self.accent_color(), self.muted(), self.dark);
            let total = self.snapshot.memory.total.max(1) as f32;
            let top = |key: fn(&ProcInfo) -> f32| -> (Vec<String>, Vec<f32>) {
                let mut ranked: Vec<&ProcInfo> = self.processes.iter().collect();
                ranked.sort_by(|a, b| key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal));
                ranked
                    .into_iter()
                    .take(6)
                    .map(|process| (format!("{} ({})", process.name, process.pid), key(process)))
                    .unzip()
            };
            let (cpu_names, cpu_values) = top(|process| process.cpu);
            let (mem_names, mem_values) = top(|process| process.memory as f32);
            let cpu_top = Chart::bars(ts("Processus les plus gourmands (processeur)"), "%", cpu_names, cpu_values);
            let mem_top = Chart::bars(ts("Processus les plus gourmands (mémoire)"), "B", mem_names, mem_values);
            let selected = self.selected_process.clone();
            let selected_chart = selected.as_ref().map(|_| {
                Chart::lines(
                    ts("Processus sélectionné"),
                    "%",
                    vec![
                        Series::new(ts("Processeur"), self.selected_history.iter().map(|(cpu, _)| *cpu).collect()),
                        Series::new(ts("Mémoire (% de la RAM)"), self.selected_history.iter().map(|(_, mem)| *mem).collect()),
                    ],
                )
                .max_y(100.0)
            });
            let _ = total;
            egui::CollapsingHeader::new(egui::RichText::new(i18n::t(lang, "ui.charts")).strong())
                .default_open(true)
                .show(ui, |ui| {
                    // Le graphique « processeur et mémoire » du système est dans l'onglet Surveillance.
                    ui.columns(2, |columns| {
                        panel_view::draw_chart(&mut columns[0], &cpu_top, lang, accent, muted, dark);
                        panel_view::draw_chart(&mut columns[1], &mem_top, lang, accent, muted, dark);
                    });
                    match &selected_chart {
                        Some(chart) => panel_view::draw_chart(ui, chart, lang, accent, muted, dark),
                        None => {
                            ui.label(
                                egui::RichText::new(i18n::t(lang, "ui.select_process_hint"))
                                    .size(10.5)
                                    .color(muted),
                            );
                        }
                    }
                });
        }
        ui.add_space(4.0);

        let selected = self.selected_process.clone();
        let mut chosen: Option<(u32, String)> = None;
        let mut requested: Option<ProcAction> = None;
        let danger = self.danger_color();
        ui.horizontal(|ui| match &selected {
            Some((pid, name)) => {
                ui.label(egui::RichText::new(tf("Sélection : {} ({})", &[&name, &pid])).strong());
                // Actions destructrices en rouge (spec) ; les autres en boutons standard.
                let danger_button = |text: &str| {
                    egui::Button::new(egui::RichText::new(text).color(danger))
                        .fill(danger.gamma_multiply(0.10))
                        .stroke(egui::Stroke::new(1.0_f32, danger.gamma_multiply(0.45)))
                };
                if ui.add(danger_button(ts("Terminer"))).clicked() {
                    requested = Some(ProcAction::Terminate);
                }
                if ui.add(danger_button(ts("Forcer…"))).clicked() {
                    requested = Some(ProcAction::Kill);
                }
                if ui.button(ts("Suspendre")).clicked() {
                    requested = Some(ProcAction::Suspend);
                }
                if ui.button(ts("Reprendre")).clicked() {
                    requested = Some(ProcAction::Resume);
                }
                ui.add(egui::DragValue::new(&mut self.process_nice).range(-20..=19).prefix("nice "));
                if ui.button(ts("Appliquer la priorité")).clicked() {
                    requested = Some(ProcAction::Nice(self.process_nice));
                }
            }
            None => {
                ui.label(
                    egui::RichText::new(ts("Cliquez sur le nom d'un processus pour le sélectionner."))
                        .size(10.5)
                        .color(self.muted()),
                );
            }
        });
        ui.add_space(6.0);

        // Tableau : colonnes proportionnelles, barre CPU, ligne sélectionnée teintée.
        let muted = self.muted();
        let accent = self.accent_color();
        let border = self.border_color();
        let hover = self.hover_color();
        let track = self.track_color();
        let dark = self.dark;
        let total_w = ui.available_width();
        let name_w = (total_w * 0.20).clamp(130.0, 260.0);
        let (pid_w, user_w, cpu_w, mem_w) = (64.0_f32, 92.0_f32, 170.0_f32, 92.0_f32);
        let gap = 8.0_f32;
        let columns_x = [
            0.0,
            pid_w + gap,
            pid_w + name_w + 2.0 * gap,
            pid_w + name_w + user_w + 3.0 * gap,
            pid_w + name_w + user_w + cpu_w + 4.0 * gap,
            pid_w + name_w + user_w + cpu_w + mem_w + 5.0 * gap,
        ];
        let heads = ["PID", ts("NOM"), ts("UTILISATEUR"), "CPU", ts("MÉMOIRE"), ts("ORIGINE")];
        let (head_rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 28.0), egui::Sense::hover());
        for (index, head) in heads.iter().enumerate() {
            ui.painter().text(
                egui::pos2(head_rect.left() + 12.0 + columns_x[index], head_rect.center().y),
                egui::Align2::LEFT_CENTER,
                head.to_uppercase(),
                egui::FontId::monospace(10.0),
                muted,
            );
        }
        ui.painter().hline(head_rect.x_range(), head_rect.bottom(), egui::Stroke::new(1.0_f32, border));
        let row_h = 32.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, self.processes.len(), |ui, range| {
                for index in range {
                    let process = &self.processes[index];
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
                    let is_selected = selected.as_ref().is_some_and(|(pid, _)| *pid == process.pid);
                    let painter = ui.painter();
                    if is_selected {
                        let (fill, _) = if dark {
                            (egui::Color32::from_rgb(58, 31, 13), ())
                        } else {
                            (egui::Color32::from_rgb(251, 227, 207), ())
                        };
                        painter.rect_filled(rect, 0.0, fill);
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                            0.0,
                            accent,
                        );
                    } else if response.hovered() {
                        painter.rect_filled(rect, 0.0, hover);
                    }
                    painter.hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0_f32, border));
                    let cy = rect.center().y;
                    let x0 = rect.left() + 12.0;
                    let text_color = ui.visuals().text_color();
                    let clipped = |x: f32, w: f32| painter.with_clip_rect(egui::Rect::from_min_size(
                        egui::pos2(x, rect.top()),
                        egui::vec2(w, rect.height()),
                    ));
                    clipped(x0 + columns_x[0], pid_w).text(
                        egui::pos2(x0 + columns_x[0], cy),
                        egui::Align2::LEFT_CENTER,
                        process.pid.to_string(),
                        egui::FontId::monospace(11.5),
                        muted,
                    );
                    clipped(x0 + columns_x[1], name_w).text(
                        egui::pos2(x0 + columns_x[1], cy),
                        egui::Align2::LEFT_CENTER,
                        &process.name,
                        egui::FontId::monospace(12.0),
                        text_color,
                    );
                    clipped(x0 + columns_x[2], user_w).text(
                        egui::pos2(x0 + columns_x[2], cy),
                        egui::Align2::LEFT_CENTER,
                        &process.user,
                        egui::FontId::proportional(11.5),
                        muted,
                    );
                    // Barre CPU : piste 100 × 6, remplissage orange (ambre si > 20 %).
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(x0 + columns_x[3], cy - 3.0),
                        egui::vec2(100.0, 6.0),
                    );
                    painter.rect_filled(bar, 3.0, track);
                    let ratio = (process.cpu / 100.0).clamp(0.0, 1.0);
                    if ratio > 0.0 {
                        let mut fill = bar;
                        fill.set_width((bar.width() * ratio).max(4.0));
                        painter.rect_filled(fill, 3.0, if process.cpu > 20.0 { self.warn_color() } else { accent });
                    }
                    painter.text(
                        egui::pos2(bar.right() + 8.0, cy),
                        egui::Align2::LEFT_CENTER,
                        format!("{:.1} %", process.cpu),
                        egui::FontId::monospace(11.0),
                        muted,
                    );
                    clipped(x0 + columns_x[4], mem_w).text(
                        egui::pos2(x0 + columns_x[4], cy),
                        egui::Align2::LEFT_CENTER,
                        util::human_bytes(process.memory),
                        egui::FontId::monospace(11.0),
                        muted,
                    );
                    let origin = if process.origin.is_empty() { &process.cmdline } else { &process.origin };
                    clipped(x0 + columns_x[5], (rect.right() - x0 - columns_x[5]).max(40.0)).text(
                        egui::pos2(x0 + columns_x[5], cy),
                        egui::Align2::LEFT_CENTER,
                        origin,
                        egui::FontId::proportional(11.0),
                        muted,
                    );
                    if response.clicked() {
                        chosen = Some((process.pid, process.name.clone()));
                    }
                }
            });

        if refresh_now {
            self.sample_processes(true);
        }
        if let Some(choice) = chosen {
            if self.selected_process.as_ref().map(|(pid, _)| *pid) != Some(choice.0) {
                self.selected_history.clear();
            }
            self.selected_process = Some(choice);
        }
        if let (Some(action), Some((pid, name))) = (requested, self.selected_process.clone()) {
            let pending = Pending::ProcessAction { pid, name: name.clone(), action };
            match action {
                ProcAction::Terminate | ProcAction::Kill => {
                    self.confirm = Some(Confirmation {
                        title: format!("{} {name}", if action == ProcAction::Kill { ts("Forcer l'arrêt de") } else { ts("Terminer") }),
                        detail: tf("Processus {} (PID {}) : {}.\nLes données non enregistrées de ce programme peuvent être perdues.", &[&name, &pid, &proc_label(action)]),
                        pending,
                    });
                }
                _ => self.apply_pending(pending),
            }
        }
    }

    fn page_services(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Pending> = Vec::new();
        let mut confirm: Option<Confirmation> = None;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.service_filter)
                    .hint_text(ts("Filtrer les services…"))
                    .desired_width(280.0),
            );
            ui.label(
                egui::RichText::new(tf("{} unités", &[&self.services.len()]))
                    .size(11.0)
                    .color(self.muted()),
            );
            if ui.button(ts("Relire")).clicked() {
                pending.push(Pending::Refresh(Page::Services));
            }
        });
        ui.add_space(6.0);

        let filter = self.service_filter.to_lowercase();
        let services = &self.services;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 22.0, services.len(), |ui, range| {
                for index in range {
                    let service = &self.services[index];
                    if !filter.is_empty()
                        && !service.unit.to_lowercase().contains(&filter)
                        && !service.description.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        let color = if service.failed() {
                            egui::Color32::from_rgb(224, 92, 92)
                        } else if service.running() {
                            egui::Color32::from_rgb(43, 209, 138)
                        } else {
                            self.muted()
                        };
                        left_cell(ui, 
                            [230.0, 18.0],
                            egui::Label::new(util::ellipsis(&service.unit, 32)).truncate(),
                        );
                        left_cell(ui, 
                            [120.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} / {}",
                                    service.active, service.sub
                                ))
                                .size(10.5)
                                .color(color),
                            ),
                        );
                        left_cell(ui, 
                            [90.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&service.enabled, 12))
                                    .size(10.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [320.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(&service.description, 55))
                                    .size(10.5)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Démarrer")).clicked() {
                            pending.push(Pending::Service(
                                service.unit.clone(),
                                ServiceAction::Start,
                            ));
                        }
                        if ui.small_button(ts("Arrêter")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Arrêter {}", &[&service.unit]),
                                detail: ts("Le service sera arrêté immédiatement. Les services qui en dépendent peuvent s'arrêter aussi.").to_string(),
                                pending: Pending::Service(service.unit.clone(), ServiceAction::Stop),
                            });
                        }
                        if ui.small_button(ts("Redémarrer")).clicked() {
                            pending.push(Pending::Service(
                                service.unit.clone(),
                                ServiceAction::Restart,
                            ));
                        }
                        if ui.small_button(ts("Activer")).clicked() {
                            pending.push(Pending::Service(
                                service.unit.clone(),
                                ServiceAction::Enable,
                            ));
                        }
                        if ui.small_button(ts("Désactiver")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Désactiver {}", &[&service.unit]),
                                detail: ts("Le service ne démarrera plus automatiquement au prochain démarrage.").to_string(),
                                pending: Pending::Service(
                                    service.unit.clone(),
                                    ServiceAction::Disable,
                                ),
                            });
                        }
                    });
                }
            });
        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for item in pending {
            self.apply_pending(item);
        }
    }

    fn page_customunits(&mut self, ui: &mut egui::Ui) {
        let list = self.custom_units.clone();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();
        let filter = self.unit_filter.trim().to_lowercase();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Unités systemd personnalisées")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.unit_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::UnitsReload);
            }
            if ui.button(ts("Nouveau modèle")).clicked() {
                if self.unit_name.trim().is_empty() {
                    self.unit_name = "mon-service.service".to_string();
                }
                self.unit_editor = units::unit_template(
                    &self.unit_name,
                    &self.unit_description,
                    &self.unit_exec,
                    &self.unit_user,
                );
            }
            let custom = list.iter().filter(|unit| unit.custom).count();
            ui.label(
                egui::RichText::new(tf("{} personnalisée(s) · {} service(s)", &[&custom, &list.len()]))
                    .size(10.5)
                    .color(self.muted()),
            );
        });
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("unit-list")
                    .max_height(180.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for unit in &list {
                            if !filter.is_empty() && !unit.name.to_lowercase().contains(&filter) {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                left_cell(ui, 
                                    [240.0, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(if unit.custom {
                                            format!("★ {}", unit.name)
                                        } else {
                                            format!("  {}", unit.name)
                                        })
                                        .monospace()
                                        .size(10.5),
                                    )
                                    .truncate(),
                                );
                                left_cell(ui, 
                                    [110.0, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(&unit.state).size(10.0).color(self.muted()),
                                    ),
                                );
                                if ui.small_button(ts("Charger")).clicked() {
                                    actions.push(Pending::LoadUnit(unit.name.clone()));
                                }
                                if ui.small_button(ts("Statut")).clicked() {
                                    actions.push(Pending::UnitStatus(unit.name.clone()));
                                }
                                if ui.small_button(ts("Démarrer")).clicked() {
                                    actions.push(Pending::UnitAction(
                                        "start".to_string(),
                                        unit.name.clone(),
                                    ));
                                }
                                if ui.small_button(ts("Redémarrer")).clicked() {
                                    actions.push(Pending::UnitAction(
                                        "restart".to_string(),
                                        unit.name.clone(),
                                    ));
                                }
                                if ui.small_button(ts("Arrêter")).clicked() {
                                    confirm = Some(Confirmation {
                                        title: tf("Arrêter {}", &[&unit.name]),
                                        detail: ts("Le service sera arrêté immédiatement.").to_string(),
                                        pending: Pending::UnitAction(
                                            "stop".to_string(),
                                            unit.name.clone(),
                                        ),
                                    });
                                }
                            });
                        }
                    });

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(ts("ÉDITEUR D'UNITÉ"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Nom"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.unit_name)
                            .desired_width(220.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("mon-service.service")),
                    );
                    ui.label(ts("Description"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.unit_description)
                            .desired_width(220.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("ExecStart");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.unit_exec)
                            .desired_width(320.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("/usr/bin/mon-script")),
                    );
                    ui.label("User");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.unit_user)
                            .desired_width(120.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(ts("facultatif")),
                    );
                });
                egui::ScrollArea::vertical()
                    .id_salt("unit-editor")
                    .max_height(220.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.unit_editor)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(14)
                                .code_editor(),
                        );
                    });

                ui.horizontal(|ui| {
                    if ui.button(ts("Installer…")).clicked() {
                        confirm = Some(Confirmation {
                            title: tf("Installer {}", &[&self.unit_name]),
                            detail: tf("Le fichier sera écrit dans {}/{} puis systemd sera rechargé (polkit).", &[&units::SYSTEM_DIR, &self.unit_name]),
                            pending: Pending::SaveUnit {
                                name: self.unit_name.clone(),
                                text: self.unit_editor.clone(),
                            },
                        });
                    }
                    if ui.button(ts("Supprimer…")).clicked() {
                        confirm = Some(Confirmation {
                            title: tf("Supprimer {}", &[&self.unit_name]),
                            detail: ts("L'unité sera arrêtée, désactivée puis effacée du disque. Cette action est irréversible.").to_string(),
                            pending: Pending::RemoveUnit(self.unit_name.clone()),
                        });
                    }
                    if ui.button(ts("Activer au démarrage")).clicked() {
                        actions.push(Pending::UnitAction(
                            "enable".to_string(),
                            self.unit_name.clone(),
                        ));
                    }
                    if ui.button(ts("Désactiver")).clicked() {
                        actions.push(Pending::UnitAction(
                            "disable".to_string(),
                            self.unit_name.clone(),
                        ));
                    }
                });

                if !self.unit_status.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("unit-status")
                        .max_height(140.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(self.unit_status.clone());
                        });
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_logadvanced(&mut self, ui: &mut egui::Ui) {
        let results = self.log_results.clone();
        let errors = self.log_errors.clone();
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Recherche dans le journal systemd")).strong());
            if ui.button(ts("Exporter…")).clicked() {
                actions.push(Pending::ExportLogs);
            }
            ui.label(
                egui::RichText::new(tf("{} ligne(s) · {} message(s) récurrent(s)", &[&results.lines().filter(|line| !line.trim().is_empty()).count(), &errors.len()]))
                .size(10.5)
                .color(self.muted()),
            );
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(ts("Unités"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_units)
                            .desired_width(300.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("nginx.service, sshd.service"),
                    );
                    ui.label(ts("Lignes"));
                    let mut limit_text = self.log_limit.to_string();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut limit_text)
                                .desired_width(70.0)
                                .font(egui::TextStyle::Monospace),
                        )
                        .changed()
                    {
                        if let Ok(value) = limit_text.trim().parse() {
                            self.log_limit = value;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Depuis"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_since)
                            .desired_width(140.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("2026-11-01"),
                    );
                    ui.label(ts("Jusqu'à"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_until)
                            .desired_width(140.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("2026-11-02 18:00"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Motif"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_grep)
                            .desired_width(260.0)
                            .hint_text(ts("timeout, refusé…")),
                    );
                    if ui.button(ts("Rechercher")).clicked() {
                        actions.push(Pending::SearchLogs(Box::new(LogQuery {
                            units: logsearch::parse_units(&self.log_units),
                            since: self.log_since.clone(),
                            until: self.log_until.clone(),
                            priority: self.log_level.clone(),
                            grep: self.log_grep.clone(),
                            limit: self.log_limit,
                        })));
                    }
                    if ui.button(ts("Analyser les erreurs (24 h)")).clicked() {
                        actions.push(Pending::AnalyzeErrors);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Priorité"));
                    for (value, label) in [
                        ("", ts("Tout")),
                        ("err", ts("Erreurs")),
                        ("warning", ts("Avertissements")),
                        ("notice", ts("Notice")),
                        ("info", ts("Infos")),
                    ] {
                        let selected = self.log_level == value;
                        if ui.selectable_label(selected, label).clicked() {
                            self.log_level = value.to_string();
                        }
                    }
                });

                if !errors.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(ts("ERREURS RÉCURRENTES"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    for entry in &errors {
                        ui.horizontal(|ui| {
                            left_cell(ui, 
                                [46.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(format!("×{}", entry.count))
                                        .monospace()
                                        .size(10.0),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(util::ellipsis(&entry.message, 96))
                                    .size(10.5)
                                    .color(self.muted()),
                            );
                        });
                    }
                }

                if !results.is_empty() {
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .id_salt("log-results")
                        .max_height(320.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(results.clone());
                        });
                }
            });

        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_logs(&mut self, ui: &mut egui::Ui) {
        let mut reload = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.log_unit)
                    .hint_text(ts("unité (ex. nginx.service)"))
                    .desired_width(240.0),
            );
            egui::ComboBox::from_id_salt("priority")
                .selected_text(ts(self.log_priority.label()))
                .show_ui(ui, |ui| {
                    for priority in [
                        logs::Priority::All,
                        logs::Priority::Errors,
                        logs::Priority::Warnings,
                        logs::Priority::Info,
                    ] {
                        ui.selectable_value(&mut self.log_priority, priority, ts(priority.label()));
                    }
                });
            if ui.button(ts("Lire")).clicked() {
                reload = true;
            }
            ui.label(
                egui::RichText::new(tf("{} ligne(s)", &[&logs::line_count(&self.logs_text)]))
                .size(11.0)
                .color(self.muted()),
            );
        });
        ui.add_space(6.0);
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.logs_text.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune entrée : filtre “Erreurs” sur un système calme ?"))
                            .color(self.muted()),
                    );
                } else {
                    ui.monospace(self.logs_text.clone());
                }
            });
        if reload {
            self.reload_logs();
        }
    }

    fn page_timers(&mut self, ui: &mut egui::Ui) {
        let report = self.timers_report.clone();
        let filter = self.timers_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Timers et unités de sauvegarde")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.timers_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::TimersReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} timer(s) · {} unité(s) de sauvegarde · {} en échec", &[&report.timers.len(), &report.units.len(), &report.failed()]))
                    .size(10.5)
                    .color(if report.failed() > 0 {
                        egui::Color32::from_rgb(224, 92, 92)
                    } else {
                        self.muted()
                    }),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des timers…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.label(
                    egui::RichText::new("TIMERS")
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.timers.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun timer systemd."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for timer in &report.timers {
                    if !filter.is_empty()
                        && !timer.unit.to_lowercase().contains(&filter)
                        && !timer.activates.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [230.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&timer.unit).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [110.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(if timer.left.is_empty() {
                                    "—"
                                } else {
                                    timer.left.as_str()
                                })
                                .size(9.5)
                                .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [260.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(util::ellipsis(
                                    &format!("{} → {}", timer.next, timer.activates),
                                    48,
                                ))
                                .size(9.5)
                                .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Démarrer")).clicked() {
                            actions.push(Pending::TimerAction {
                                unit: timer.unit.clone(),
                                action: "start".to_string(),
                            });
                        }
                        if ui.small_button(ts("Arrêter…")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Arrêter {}", &[&timer.unit]),
                                detail: ts("Le timer ne déclenchera plus son unité jusqu'au prochain démarrage ou réactivation.")
                                    .to_string(),
                                pending: Pending::TimerAction {
                                    unit: timer.unit.clone(),
                                    action: "stop".to_string(),
                                },
                            });
                        }
                        if ui.small_button(ts("Activer au démarrage")).clicked() {
                            actions.push(Pending::TimerAction {
                                unit: timer.unit.clone(),
                                action: "enable".to_string(),
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("UNITÉS DE SAUVEGARDE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.units.is_empty() {
                    ui.label(
                        egui::RichText::new(tf("Aucune unité contenant {}.", &[&timers::BACKUP_KEYWORDS.join(", ")]))
                        .size(10.0)
                        .color(self.muted()),
                    );
                }
                for unit in &report.units {
                    if !filter.is_empty() && !unit.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [240.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&unit.name).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [120.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!("{} / {}", unit.active, unit.sub))
                                    .size(9.5)
                                    .color(if unit.failed() {
                                        egui::Color32::from_rgb(224, 92, 92)
                                    } else {
                                        self.muted()
                                    }),
                            ),
                        );
                        if let Some(timer) = report.timer_for(&unit.name) {
                            ui.label(
                                egui::RichText::new(tf("timer {}", &[&timer.unit]))
                                    .size(9.0)
                                    .color(self.muted()),
                            );
                        }
                        if ui.small_button(ts("Lancer")).clicked() {
                            actions.push(Pending::TimerAction {
                                unit: unit.name.clone(),
                                action: "start".to_string(),
                            });
                        }
                    });
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_netmounts(&mut self, ui: &mut egui::Ui) {
        let report = self.netmounts_report.clone();
        let filter = self.netmounts_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Montages réseau")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.netmounts_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer un point de montage")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::NetMountsReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} entrée(s) · {} montée(s) · showmount {}", &[&report.entries.len(), &report.mounted(), &(if report.has_showmount { ts("présent") } else { ts("absent") })]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        let el = elastic::Elastic::new(ui);
        elastic::density(ui, &el);
        let columns = if el.two_columns() { 2 } else { 1 };
        let right = if columns == 2 { 1usize } else { 0usize };
        ui.columns(columns, |cols| {
            {
                let ui = &mut cols[0];
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des montages…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }
                if report.entries.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun montage NFS/CIFS déclaré ni actif."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                let entry_rows = el.fit_rows(elastic::remaining(ui) - 40.0, 22.0, 2, 20);
                for entry in report.entries.iter().take(entry_rows) {
                    if !filter.is_empty()
                        && !entry.mount.to_lowercase().contains(&filter)
                        && !entry.server.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [220.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&entry.mount).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [70.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&entry.fstype)
                                    .size(9.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [260.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{}:{}",
                                    entry.server, entry.export
                                ))
                                .size(9.5)
                                .color(self.muted()),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(if entry.mounted {
                                ts("monté")
                            } else if entry.auto_mount {
                                "auto"
                            } else {
                                ts("manuel")
                            })
                            .size(9.5)
                            .color(if entry.mounted {
                                egui::Color32::from_rgb(43, 209, 138)
                            } else {
                                self.muted()
                            }),
                        );
                        if entry.mounted {
                            if ui.small_button(ts("Démonter…")).clicked() {
                                confirm = Some(Confirmation {
                                    title: tf("Démonter {}", &[&entry.mount]),
                                    detail: ts("Les programmes utilisant ce montage risquent de bloquer. Le système peut attendre la fin des accès avant de rendre la main.")
                                        .to_string(),
                                    pending: Pending::NetMountToggle {
                                        path: entry.mount.clone(),
                                        mount: false,
                                    },
                                });
                            }
                        } else if ui.small_button(ts("Monter…")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Monter {}", &[&entry.mount]),
                                detail: tf("mount {} ({}:{}, polkit). Le serveur doit être joignable.", &[&entry.mount, &entry.server, &entry.export]),
                                pending: Pending::NetMountToggle {
                                    path: entry.mount.clone(),
                                    mount: true,
                                },
                            });
                        }
                        if ui.small_button(ts("Tester")).clicked() {
                            actions.push(Pending::NetMountProbe {
                                server: entry.server.clone(),
                                fstype: entry.fstype.clone(),
                            });
                        }
                    });
                    if !entry.options.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("    {}", util::ellipsis(&entry.options, 90)))
                                .size(9.0)
                                .color(self.muted()),
                        );
                    }
                }
            }
            {
                let ui = &mut cols[right];
                ui.label(
                    egui::RichText::new(ts("MONTER UN PARTAGE À LA DEMANDE"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Partage"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netmount_source)
                            .desired_width(280.0)
                            .hint_text("//nas/partage ou serveur:/export"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Point de montage"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netmount_point)
                            .desired_width(180.0)
                            .hint_text("/mnt/partage"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(ts("Options"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netmount_options)
                            .desired_width(220.0)
                            .hint_text("username=…,vers=3.0"),
                    );
                });
                let remote_valid = netmounts::classify_remote(&self.netmount_source).is_ok()
                    && netmounts::valid_mountpoint(&self.netmount_point)
                    && netmounts::valid_mount_options(&self.netmount_options);
                if ui
                    .add_enabled(remote_valid, egui::Button::new(ts("Monter…")))
                    .clicked()
                {
                    confirm = Some(Confirmation {
                        title: ts("Monter un partage réseau").to_string(),
                        detail: format!(
                            "{} → {} ({})",
                            self.netmount_source.trim(),
                            self.netmount_point.trim(),
                            ts("via Polkit")
                        ),
                        pending: Pending::NetMountRemote {
                            source: self.netmount_source.clone(),
                            point: self.netmount_point.clone(),
                            options: self.netmount_options.clone(),
                        },
                    });
                }
                ui.label(
                    egui::RichText::new(ts("Le point de montage est créé si nécessaire. Le montage n'est pas ajouté à /etc/fstab."))
                        .size(10.0)
                        .color(self.muted()),
                );

                if !self.netmounts_output.is_empty() {
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .id_salt("netmounts-output")
                        .max_height(160.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.monospace(self.netmounts_output.clone());
                        });
                }
            }
        });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_sessions(&mut self, ui: &mut egui::Ui) {
        let report = self.sessions_report.clone();
        let filter = self.sessions_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Sessions et comptes")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.sessions_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer un utilisateur")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::SessionsReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} session(s) · {} connexion(s) · {} compte(s) vérifié(s)", &[&report.sessions.len(), &report.logins.len(), &report.locks.len()]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des sessions…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.label(
                    egui::RichText::new(ts("SESSIONS ACTIVES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.sessions.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune session active lisible."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for session in &report.sessions {
                    if !filter.is_empty() && !session.user.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [50.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&session.id)
                                    .monospace()
                                    .size(9.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [140.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&session.user).size(10.5),
                            )
                            .truncate(),
                        );
                        left_cell(ui, 
                            [120.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    session.tty, session.seat
                                ))
                                .size(9.5)
                                .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.small_button(ts("Fermer…")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Fermer la session {}", &[&session.id]),
                                detail: tf("La session de {} sur {} sera terminée immédiatement (loginctl, polkit).", &[&session.user, &session.tty]),
                                pending: Pending::SessionTerminate(session.id.clone()),
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("COMPTES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.locks.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("État des comptes indisponible (passwd restreint)."))
                            .size(10.0)
                            .color(self.muted()),
                    );
                }
                for state in &report.locks {
                    if !filter.is_empty() && !state.user.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [140.0, 18.0],
                            egui::Label::new(egui::RichText::new(&state.user).size(10.5)),
                        );
                        left_cell(ui, 
                            [150.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(tx(&state.detail)).size(9.5).color(
                                    if state.locked {
                                        egui::Color32::from_rgb(240, 180, 60)
                                    } else {
                                        self.muted()
                                    },
                                ),
                            ),
                        );
                        let (label, locked) = if state.locked {
                            (ts("Déverrouiller…"), false)
                        } else {
                            (ts("Verrouiller…"), true)
                        };
                        if ui.small_button(label).clicked() {
                            confirm = Some(Confirmation {
                                title: format!("{} {}", label, state.user),
                                detail: if locked {
                                    ts("usermod -L empêche toute connexion par mot de passe de ce compte.")
                                        .to_string()
                                } else {
                                    ts("usermod -U rétablit la connexion par mot de passe.").to_string()
                                },
                                pending: Pending::SessionLock {
                                    user: state.user.clone(),
                                    locked,
                                },
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("DERNIÈRES CONNEXIONS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for login in report.logins.iter().take(25) {
                    if !filter.is_empty() && !login.user.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [130.0, 16.0],
                            egui::Label::new(egui::RichText::new(&login.user).size(10.0)),
                        );
                        left_cell(ui, 
                            [90.0, 16.0],
                            egui::Label::new(
                                egui::RichText::new(&login.tty)
                                    .monospace()
                                    .size(9.5)
                                    .color(self.muted()),
                            ),
                        );
                        left_cell(ui, 
                            [140.0, 16.0],
                            egui::Label::new(
                                egui::RichText::new(&login.host)
                                    .size(9.5)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&login.when, 46))
                                .size(9.0)
                                .color(if login.active {
                                    egui::Color32::from_rgb(43, 209, 138)
                                } else {
                                    self.muted()
                                }),
                        );
                    });
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_kernellogs(&mut self, ui: &mut egui::Ui) {
        let report = self.klogs.clone();
        let filter = self.klogs_filter.trim().to_lowercase();
        let errors_only = self.klogs_errors_only;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Journaux du noyau")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.klogs_filter)
                    .desired_width(200.0)
                    .hint_text(ts("filtrer un message")),
            );
            ui.checkbox(&mut self.klogs_errors_only, ts("notables seulement"));
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::KernelLogsReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} · {} erreur(s) · {} avertissement(s)", &[&report.version, &report.count(klogs::Level::Error), &report.count(klogs::Level::Warning)]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture du journal noyau…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.horizontal_wrapped(|ui| {
                    for category in [
                        klogs::Category::OutOfMemory,
                        klogs::Category::Hardware,
                        klogs::Category::Storage,
                        klogs::Category::Thermal,
                        klogs::Category::Network,
                        klogs::Category::Graphics,
                    ] {
                        let count = report.count_category(category);
                        if count == 0 {
                            continue;
                        }
                        ui.label(
                            egui::RichText::new(format!("{} {count}", ts(category.label())))
                                .size(9.5)
                                .color(self.muted()),
                        );
                    }
                });

                if !report.boots.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(ts("DÉMARRAGES"))
                            .size(10.0)
                            .strong()
                            .color(self.muted()),
                    );
                    for boot in report.boots.iter().take(6) {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {} · {}",
                                boot.index, boot.id, boot.range
                            ))
                            .size(9.0)
                            .color(self.muted()),
                        );
                    }
                }

                ui.add_space(8.0);
                let visible: Vec<&klogs::Event> = report
                    .events
                    .iter()
                    .filter(|event| !errors_only || event.notable())
                    .filter(|event| {
                        filter.is_empty() || event.line.to_lowercase().contains(&filter)
                    })
                    .collect();
                ui.label(
                    egui::RichText::new(tf("{} ligne(s) affichée(s)", &[&visible.len()]))
                    .size(9.5)
                    .color(self.muted()),
                );
                egui::ScrollArea::vertical()
                    .id_salt("klogs-lines")
                    .max_height(420.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for event in visible.iter().take(1_500) {
                            let color = match event.level {
                                klogs::Level::Error => egui::Color32::from_rgb(224, 92, 92),
                                klogs::Level::Warning => egui::Color32::from_rgb(240, 180, 60),
                                klogs::Level::Info => self.muted(),
                            };
                            ui.horizontal(|ui| {
                                left_cell(ui, 
                                    [80.0, 16.0],
                                    egui::Label::new(
                                        egui::RichText::new(ts(event.level.label()))
                                            .size(9.0)
                                            .color(color),
                                    ),
                                );
                                left_cell(ui, 
                                    [70.0, 16.0],
                                    egui::Label::new(
                                        egui::RichText::new(ts(event.category.label()))
                                            .size(9.0)
                                            .color(self.muted()),
                                    ),
                                );
                                ui.label(
                                    egui::RichText::new(util::ellipsis(&event.line, 150))
                                        .monospace()
                                        .size(9.0)
                                        .color(color),
                                );
                            });
                        }
                    });
            });

        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_perf(&mut self, ui: &mut egui::Ui) {
        let report = self.perf_report.clone();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Profils de performance")).strong());
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::PerfReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} cœur(s) · gouverneur {} · swap {}", &[&report.per_cpu.len(), &report.dominant_governor(), &(if report.swaps.is_empty() {
                            ts("absent").to_string()
                        } else {
                            util::human_bytes(report.swap_bytes())
                        })]))
                    .size(10.5)
                    .color(self.muted()),
                );
                if !report.cpupower {
                    ui.label(
                        egui::RichText::new(ts("cpupower absent : application du gouverneur impossible"))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(240, 180, 60)),
                    );
                }
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Lecture des réglages…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.label(
                    egui::RichText::new(ts("PROFILS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                for profile in perf::PROFILES {
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [110.0, 18.0],
                            egui::Label::new(egui::RichText::new(ts(profile.name)).strong().size(10.5)),
                        );
                        left_cell(ui, 
                            [420.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(ts(profile.description))
                                    .size(9.5)
                                    .color(self.muted()),
                            )
                            .truncate(),
                        );
                        if ui.button(ts("Appliquer…")).clicked() {
                            confirm = Some(Confirmation {
                                title: tf("Appliquer le profil {}", &[&ts(profile.name)]),
                                detail: tf("Gouverneur {}, vm.swappiness={}, THP={} (polkit).", &[&profile.governor, &profile.swappiness, &profile.thp]),
                                pending: Pending::PerfProfile(profile.name.to_string()),
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("RÉGLAGES INDIVIDUELS"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.horizontal(|ui| {
                    ui.label(ts("Gouverneur"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.perf_governor)
                            .desired_width(140.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui.button(ts("Appliquer")).clicked() {
                        confirm = Some(Confirmation {
                            title: ts("Changer le gouverneur CPU").to_string(),
                            detail: tf("cpupower frequency-set -g {} (polkit).", &[&self.perf_governor]),
                            pending: Pending::PerfGovernor(self.perf_governor.clone()),
                        });
                    }
                    ui.label(
                        egui::RichText::new(tf("disponibles : {}", &[&if report.governors.is_empty() {
                                "—".to_string()
                            } else {
                                report.governors.join(", ")
                            }]))
                        .size(9.5)
                        .color(self.muted()),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("vm.swappiness");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.perf_swappiness)
                            .desired_width(60.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui.button(ts("Appliquer")).clicked() {
                        match self.perf_swappiness.trim().parse::<u32>() {
                            Ok(value) if value <= 100 => {
                                actions.push(Pending::PerfSwappiness(value));
                            }
                            _ => self.toast(
                                ts("swappiness : valeur entre 0 et 100 attendue").to_string(),
                                false,
                            ),
                        }
                    }
                    ui.label("THP");
                    for mode in ["always", "madvise", "never"] {
                        if ui
                            .selectable_label(self.perf_thp == mode, mode)
                            .clicked()
                        {
                            self.perf_thp = mode.to_string();
                        }
                    }
                    if ui.button(ts("Appliquer THP")).clicked() {
                        actions.push(Pending::PerfThp(self.perf_thp.clone()));
                    }
                });

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("ÉTAT"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "vm.swappiness = {} · THP = {} · turbo no_turbo = {}",
                        report.swappiness.as_deref().unwrap_or("—"),
                        report.thp.as_deref().unwrap_or("—"),
                        report.turbo.as_deref().unwrap_or("—")
                    ))
                    .size(10.0)
                    .color(self.muted()),
                );
                for swap in &report.swaps {
                    ui.label(
                        egui::RichText::new(tf("{} ({}) — {} utilisés sur {}", &[&swap.path, &swap.kind, &util::human_bytes(swap.used_kb * 1024), &util::human_bytes(swap.size_kb * 1024)]))
                        .size(9.5)
                        .color(self.muted()),
                    );
                }
                if report.swaps.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun espace d'échange actif."))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(240, 180, 60)),
                    );
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_orphans(&mut self, ui: &mut egui::Ui) {
        let report = self.orphan_report.clone();
        let filter = self.orphan_filter.trim().to_lowercase();
        let mut confirm: Option<Confirmation> = None;
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Paquets orphelins")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.orphan_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer")),
            );
            if ui.button(ts("Relire")).clicked() {
                actions.push(Pending::OrphansReload);
            }
            if let Some(report) = &report {
                ui.label(
                    egui::RichText::new(tf("{} supprimable(s) · {} configuration(s) résiduelle(s) · {}", &[&report.autoremovable.len(), &report.residual.len(), &util::human_bytes(report.residual_bytes())]))
                    .size(10.5)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Analyse des paquets…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                ui.label(
                    egui::RichText::new(ts("DÉPENDANCES INUTILES (AUTOREMOVE)"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.autoremovable.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucune dépendance à supprimer."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for orphan in &report.autoremovable {
                    if !filter.is_empty() && !orphan.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [300.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&orphan.name).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(&orphan.version)
                                .size(9.5)
                                .color(self.muted()),
                        );
                    });
                }
                if !report.autoremovable.is_empty() {
                    if ui
                        .button(tf("Supprimer {} dépendance(s)…", &[&report.autoremovable.len()]))
                        .clicked()
                    {
                        confirm = Some(Confirmation {
                            title: ts("Supprimer les dépendances inutiles").to_string(),
                            detail: tf("apt-get autoremove --purge supprimera {} paquet(s) : {}", &[&report.autoremovable.len(), &(report
                                    .autoremovable
                                    .iter()
                                    .take(12)
                                    .map(|orphan| orphan.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "))]),
                            pending: Pending::OrphansAutoremove,
                        });
                    }
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(ts("CONFIGURATIONS RÉSIDUELLES"))
                        .size(10.0)
                        .strong()
                        .color(self.muted()),
                );
                if report.residual.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("Aucun paquet en état `rc`."))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
                for residual in report.residual.iter().take(200) {
                    if !filter.is_empty() && !residual.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [320.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(&residual.name).monospace().size(10.5),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::human_bytes(residual.size_kb * 1024))
                                .size(9.5)
                                .color(self.muted()),
                        );
                    });
                }
                if !report.residual.is_empty() {
                    if ui
                        .button(tf("Purger {} configuration(s)…", &[&report.residual.len()]))
                        .clicked()
                    {
                        confirm = Some(Confirmation {
                            title: ts("Purger les configurations résiduelles").to_string(),
                            detail: tf("apt-get purge supprimera les fichiers de configuration de {} paquet(s) déjà désinstallés.", &[&report.residual.len()]),
                            pending: Pending::OrphansPurge(
                                report
                                    .residual
                                    .iter()
                                    .map(|item| item.name.clone())
                                    .collect(),
                            ),
                        });
                    }
                }
            });

        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_cis(&mut self, ui: &mut egui::Ui) {
        let report = self.cis_report.clone();
        let filter = self.cis_filter.trim().to_lowercase();
        let mut actions: Vec<Pending> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ts("Audit de durcissement")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.cis_filter)
                    .desired_width(180.0)
                    .hint_text(ts("filtrer un contrôle")),
            );
            if ui.button(ts("Relancer l'audit")).clicked() {
                actions.push(Pending::CisReload);
            }
            if let Some(report) = &report {
                let color = if report.score >= 80 {
                    egui::Color32::from_rgb(43, 209, 138)
                } else if report.score >= 50 {
                    egui::Color32::from_rgb(240, 180, 60)
                } else {
                    egui::Color32::from_rgb(224, 92, 92)
                };
                ui.label(
                    egui::RichText::new(tf("score {} %", &[&report.score]))
                        .size(11.0)
                        .strong()
                        .color(color),
                );
                ui.label(
                    egui::RichText::new(tf("{} conformes · {} partiels · {} non conformes · {} inconnus", &[&report.count(cis::Status::Pass), &report.count(cis::Status::Warn), &report.count(cis::Status::Fail), &report.count(cis::Status::Unknown)]))
                    .size(10.0)
                    .color(self.muted()),
                );
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(report) = &report else {
                    ui.label(
                        egui::RichText::new(ts("Audit en cours…"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                    return;
                };
                for error in &report.errors {
                    ui.label(
                        egui::RichText::new(tx(error))
                            .size(9.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    for (category, score) in &report.categories {
                        ui.label(
                            egui::RichText::new(format!("{} {score} %", tx(category)))
                                .size(9.5)
                                .color(if *score >= 80 {
                                    egui::Color32::from_rgb(43, 209, 138)
                                } else if *score >= 50 {
                                    egui::Color32::from_rgb(240, 180, 60)
                                } else {
                                    egui::Color32::from_rgb(224, 92, 92)
                                }),
                        );
                    }
                });
                ui.add_space(8.0);

                let mut current_category = "";
                for check in &report.checks {
                    if !filter.is_empty()
                        && !check.title.to_lowercase().contains(&filter)
                        && !check.category.to_lowercase().contains(&filter)
                        && !check.detail.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    if check.category != current_category {
                        current_category = check.category;
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(ts(check.category).to_uppercase())
                                .size(10.0)
                                .strong()
                                .color(self.muted()),
                        );
                    }
                    let color = match check.status {
                        cis::Status::Pass => egui::Color32::from_rgb(43, 209, 138),
                        cis::Status::Warn => egui::Color32::from_rgb(240, 180, 60),
                        cis::Status::Fail => egui::Color32::from_rgb(224, 92, 92),
                        cis::Status::Unknown => self.muted(),
                    };
                    ui.horizontal(|ui| {
                        left_cell(ui, 
                            [110.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(ts(check.status.label())).size(9.5).color(color),
                            ),
                        );
                        left_cell(ui, 
                            [420.0, 18.0],
                            egui::Label::new(
                                egui::RichText::new(ts(check.title)).size(10.5),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(util::ellipsis(&tx(&check.detail), 70))
                                .size(9.5)
                                .color(self.muted()),
                        );
                    });
                    if check.status != cis::Status::Pass {
                        ui.label(
                            egui::RichText::new(format!("    → {}", tx(check.remediation)))
                                .size(9.5)
                                .color(self.muted()),
                        );
                    }
                }
            });

        for action in actions {
            self.apply_pending(action);
        }
    }

    fn page_security(&mut self, ui: &mut egui::Ui) {
        let Some(audit) = self.audit.as_ref() else {
            ui.label(ts("Audit en cours…"));
            return;
        };
        let audit = audit.clone();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tf("Score {}/100", &[&audit.score()]))
                    .size(20.0)
                    .strong()
                    .color(if audit.score() >= 80 {
                        egui::Color32::from_rgb(43, 209, 138)
                    } else if audit.score() >= 60 {
                        egui::Color32::from_rgb(242, 161, 60)
                    } else {
                        egui::Color32::from_rgb(224, 92, 92)
                    }),
            );
            ui.label(
                egui::RichText::new(tf("{} point(s) à surveiller", &[&audit.problems()]))
                    .size(11.5)
                    .color(self.muted()),
            );
            if ui.button(ts("Relancer l'audit")).clicked() {
                self.apply_pending(Pending::Refresh(Page::Security));
            }
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(ts("Contrôles en lecture seule : rien n'est modifié par cet écran."))
                .size(11.0)
                .color(self.muted()),
        );
        ui.add_space(8.0);
        for finding in &audit.findings {
            let color = match finding.level {
                security::Level::Ok => egui::Color32::from_rgb(43, 209, 138),
                security::Level::Info => self.muted(),
                security::Level::Warning => egui::Color32::from_rgb(242, 161, 60),
                security::Level::Critical => egui::Color32::from_rgb(224, 92, 92),
            };
            ui.horizontal(|ui| {
                left_cell(ui, 
                    [80.0, 18.0],
                    egui::Label::new(egui::RichText::new(ts(finding.level.label())).size(10.5).color(color)),
                );
                left_cell(ui, 
                    [260.0, 18.0],
                    egui::Label::new(egui::RichText::new(&finding.title).strong().size(12.0)),
                );
                ui.label(
                    egui::RichText::new(util::ellipsis(&finding.detail, 110))
                        .size(11.0)
                        .color(self.muted()),
                );
            });
        }
    }

    fn page_reports(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(ts("Exporter un instantané complet de la machine")).strong(),
        );
        ui.label(
            egui::RichText::new(
                ts("Le rapport contient la distribution, les compteurs de logiciels, le stockage, la mémoire, le score de sécurité et les dernières actions."),
            )
            .size(11.0)
            .color(self.muted()),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for (format, label) in [
                ("txt", ts("Rapport texte")),
                ("csv", ts("Tableau CSV")),
                ("json", ts("Données JSON")),
                ("pdf", ts("Document PDF")),
            ] {
                if ui.button(label).clicked() {
                    self.apply_pending(Pending::Export(format));
                }
            }
        });
        if let Some(path) = &self.last_export {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(path).monospace().size(11.0));
        }
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new(ts("Journal des actions")).strong());
        ui.add_space(4.0);
        let entries = journal::recent(25);
        if entries.is_empty() {
            ui.label(
                egui::RichText::new(ts("Aucune action enregistrée. Le fichier est créé au premier nettoyage ou à la première suppression."))
                    .size(11.0)
                    .color(self.muted()),
            );
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for entry in entries {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&entry.timestamp)
                            .monospace()
                            .size(10.5)
                            .color(self.muted()),
                    );
                    ui.label(
                        egui::RichText::new(&entry.action)
                            .size(11.5)
                            .color(if entry.ok {
                                egui::Color32::from_rgb(43, 209, 138)
                            } else {
                                egui::Color32::from_rgb(224, 92, 92)
                            }),
                    );
                    ui.label(
                        egui::RichText::new(util::ellipsis(&entry.target, 60))
                            .size(11.0)
                            .color(self.muted()),
                    );
                });
            }
        });
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("OrbiSys").size(22.0).strong());
        ui.label(
            egui::RichText::new(tf("Version {} · licence MIT · Rust + egui", &[&orbisys::APP_VERSION]))
            .color(self.muted()),
        );
        ui.add_space(12.0);
        egui::Grid::new("about")
            .num_columns(2)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                for (label, value) in [
                    (self.tr("about.distribution"), self.os.display_name()),
                    (self.tr("about.kernel"), distro::kernel_release()),
                    (
                        self.tr("about.manager"),
                        PackageManager::detect()
                            .map(|manager| manager.label().to_string())
                            .unwrap_or_else(|| ts("inconnu").to_string()),
                    ),
                    (
                        self.tr("about.native"),
                        self.inventory
                            .as_ref()
                            .map(|inventory| inventory.counts().0.to_string())
                            .unwrap_or_else(|| "—".to_string()),
                    ),
                    ("Polkit", if self.capabilities.pkexec { ts("disponible") } else { ts("absent") }.to_string()),
                    ("systemd", if self.capabilities.systemctl { ts("disponible") } else { ts("absent") }.to_string()),
                ] {
                    ui.label(egui::RichText::new(label).color(self.muted()).size(11.5));
                    ui.label(egui::RichText::new(value).size(12.5));
                    ui.end_row();
                }
            });
        ui.add_space(14.0);
        ui.label(egui::RichText::new(self.tr("about.accent")).strong());
        ui.horizontal(|ui| {
            for accent in Accent::ALL {
                let selected = self.accent == accent;
                if ui
                    .add(
                        egui::Button::new(accent.label())
                            .fill(accent.color(self.dark))
                            .min_size(egui::vec2(74.0, 26.0)),
                    )
                    .clicked()
                {
                    self.accent = accent;
                    save_prefs(self.dark, self.accent, self.lang);
                }
                if selected {
                    ui.label(
                        egui::RichText::new(self.tr("about.active"))
                            .size(10.5)
                            .color(self.muted()),
                    );
                }
            }
        });
        ui.add_space(14.0);
        ui.label(egui::RichText::new(self.tr("about.language")).strong());
        let mut chosen: Option<Lang> = None;
        egui::ComboBox::from_id_salt("lang-about")
            .selected_text(self.lang.label())
            .show_ui(ui, |ui| {
                for lang in Lang::ALL {
                    if ui.selectable_label(self.lang == lang, lang.label()).clicked() {
                        chosen = Some(lang);
                    }
                }
            });
        if let Some(lang) = chosen {
            self.lang = lang;
            save_prefs(self.dark, self.accent, self.lang);
        }
        if self.font_missing {
            ui.label(
                egui::RichText::new(self.tr("about.font_missing"))
                    .size(10.5)
                    .color(egui::Color32::from_rgb(242, 161, 60)),
            );
        }
        ui.add_space(14.0);
        ui.label(egui::RichText::new(self.tr("about.missing")).strong());
        let missing = self.capabilities.missing();
        ui.label(
            egui::RichText::new(if missing.is_empty() {
                self.tr("about.all_present").to_string()
            } else {
                missing.join(", ")
            })
            .size(11.0)
            .color(self.muted()),
        );
        ui.add_space(14.0);
        ui.label(
            egui::RichText::new(self.tr("about.policy"))
            .size(11.0)
            .color(self.muted()),
        );
    }

    // -----------------------------------------------------------------------
    // Dialogues
    // -----------------------------------------------------------------------

    fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        egui::Window::new(ts("Aperçu de la suppression"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                if preview.is_empty() {
                    ui.label(
                        egui::RichText::new(ts("La simulation n'a rien renvoyé : la suppression peut être sans dépendance ou indisponible."))
                            .color(self.muted()),
                    );
                }
                if !preview.remove.is_empty() {
                    ui.label(
                        egui::RichText::new(tf("{} paquet(s) seraient retirés", &[&preview.remove.len()]))
                        .strong(),
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for name in &preview.remove {
                            ui.label(
                                egui::RichText::new(name).monospace().size(11.5),
                            );
                        }
                    });
                }
                if !preview.install.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tf("{} paquet(s) seraient installés", &[&preview.install.len()]))
                        .strong(),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(ts("Fermer")).clicked() {
                        confirm = false;
                    }
                    if ui.button(ts("Confirmer la suppression")).clicked() {
                        confirm = true;
                    }
                });
            });
        if confirm {
            let names = preview.requested.clone();
            self.apply_pending(Pending::Remove {
                names,
                source: Source::Native,
            });
            self.preview = None;
            self.selected.clear();
        } else if !open {
            self.preview = None;
        }
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(confirmation) = &self.confirm else {
            return;
        };
        let mut decision: Option<bool> = None;
        egui::Window::new(&confirmation.title)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.label(&confirmation.detail);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(ts("Annuler")).clicked() {
                        decision = Some(false);
                    }
                    if ui.button(ts("Confirmer")).clicked() {
                        decision = Some(true);
                    }
                });
            });
        match decision {
            Some(true) => {
                let pending = self.confirm.take().map(|confirmation| confirmation.pending);
                if let Some(pending) = pending {
                    self.apply_pending(pending);
                }
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.palette {
            return;
        }
        let mut open = true;
        let mut chosen_page: Option<Page> = None;
        egui::Window::new(ts("Recherche globale"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text(ts("Page, logiciel, service, processus…"))
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();
                let query = self.palette_query.to_lowercase();
                ui.add_space(6.0);
                for page in ALL_PAGES
                    .iter()
                    .copied()
                    .chain(PanelId::ALL.iter().map(|id| Page::Panel(*id)))
                {
                    if query.is_empty() || page.title(self.lang).to_lowercase().contains(&query) {
                        if ui.selectable_label(false, page.title(self.lang)).clicked() {
                            chosen_page = Some(page);
                        }
                    }
                }
                if let Some(inventory) = &self.inventory {
                    ui.add_space(4.0);
                    let mut shown = 0;
                    for package in inventory.packages.iter() {
                        if shown >= 8 {
                            break;
                        }
                        if !query.is_empty() && package.name.to_lowercase().contains(&query) {
                            shown += 1;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&package.name).size(12.0));
                                ui.label(
                                    egui::RichText::new(ts(package.source.label()))
                                        .size(10.0)
                                        .color(self.muted()),
                                );
                            });
                        }
                    }
                }
            });
        if let Some(page) = chosen_page {
            self.page = page;
            self.palette = false;
            self.palette_query.clear();
        }
        if !open {
            self.palette = false;
        }
    }
}

impl App {
    /// Corps de l'écran `page` (sans en-tête ni onglets).
    fn render_body(&mut self, ui: &mut egui::Ui, page: Page) {
        match page {
            Page::Dashboard => self.page_dashboard(ui),
            Page::Software => self.page_software(ui),
            Page::Updates => self.page_updates(ui),
            Page::Disk => self.page_disk(ui),
            Page::Storage => self.page_storage(ui),
            Page::Quotas => self.page_quotas(ui),
            Page::Duplicates => self.page_duplicates(ui),
            Page::Network => self.page_network(ui),
            Page::NetAdvanced => self.page_netadvanced(ui),
            Page::Firewall => self.page_firewall(ui),
            Page::Cleanup => self.page_cleanup(ui),
            Page::Backup => self.page_backup(ui),
            Page::Kernel => self.page_kernel(ui),
            Page::Volumes => self.page_volumes(ui),
            Page::Peripherals => self.page_peripherals(ui),
            Page::Printers => self.page_printers(ui),
            Page::Desktop => self.page_desktop(ui),
            Page::Automation => self.page_automation(ui),
            Page::Locale => self.page_locale(ui),
            Page::Fonts => self.page_fonts(ui),
            Page::Optimize => self.page_optimize(ui),
            Page::Snapshots => self.page_snapshots(ui),
            Page::Btrfs => self.page_btrfs(ui),
            Page::LogAdvanced => self.page_logadvanced(ui),
            Page::CustomUnits => self.page_customunits(ui),
            Page::Processes => self.page_processes(ui),
            Page::Services => self.page_services(ui),
            Page::Logs => self.page_logs(ui),
            Page::Hardware => self.page_hardware(ui),
            Page::Security => self.page_security(ui),
            Page::Cis => self.page_cis(ui),
            Page::Orphans => self.page_orphans(ui),
            Page::Perf => self.page_perf(ui),
            Page::KernelLogs => self.page_kernellogs(ui),
            Page::Sessions => self.page_sessions(ui),
            Page::NetMounts => self.page_netmounts(ui),
            Page::Timers => self.page_timers(ui),
            Page::Reports => self.page_reports(ui),
            Page::About => self.page_about(ui),
            Page::Panel(id) => self.page_panel(ui, id),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        orbisys::i18n_ui::set_lang(self.lang);
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::K)) {
            self.palette = true;
            self.palette_query.clear();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.palette = false;
            self.preview = None;
            self.confirm = None;
        }

        if self.fonts_lang != Some(self.lang) {
            // Japonais, coréen, chinois, arabe : police de secours du système.
            let (definitions, found) = ui_fonts::definitions_for(self.lang);
            ctx.set_fonts(definitions);
            self.font_missing = !found;
            self.fonts_lang = Some(self.lang);
        }
        self.poll();
        if self.last_alert_check.elapsed() >= Duration::from_secs(60) {
            self.last_alert_check = Instant::now();
            self.spawn(|| Msg::Breaches(orbisys::p_monitor::breaches_now()));
        }
        if self.last_rule_check.elapsed() >= Duration::from_secs(30) {
            self.last_rule_check = Instant::now();
            self.spawn(|| Msg::RuleAlerts(automation::check_now()));
        }
        ctx.request_repaint_after(Duration::from_secs(30));
        self.apply_theme(ctx);
        self.redirect_merged();
        self.sidebar(ctx);
        self.top_bar(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            let page = self.page;
            if let Page::Panel(id) = page {
                self.ensure_panel(ctx, id);
            } else if self.visited_pages.insert(page) {
                // Certains écrans dédiés n'étaient chargés que par « Rafraîchir » et
                // restaient sur « … en cours » : on les charge à la première visite.
                self.apply_pending(Pending::Refresh(page));
            }
            self.page_header(ui, page);
            // Filet de sécurité : les écrans répartissent leur contenu sur la
            // hauteur disponible, mais une fenêtre vraiment trop courte garde un
            // défilement plutôt que de rogner du contenu.
            egui::ScrollArea::vertical()
                .id_salt("page-body")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                match merge_of(page) {
                Some((group, list, _)) if list[0] == page => {
                    let labels: Vec<String> = list.iter().map(|sub| sub.own_title(self.lang).to_string()).collect();
                    let chosen = self.merged_tabs(ui, group, &labels);
                    let sub = list[chosen];
                    if sub != page {
                        if let Page::Panel(id) = sub {
                            self.ensure_panel(ctx, id);
                        } else if self.visited_pages.insert(sub) {
                            self.apply_pending(Pending::Refresh(sub));
                        }
                    }
                    self.render_body(ui, sub);
                }
                _ => self.render_body(ui, page),
                }
                });
        });

        self.preview_window(ctx);
        self.confirm_window(ctx);
        self.palette_window(ctx);

        if self.busy > 0 {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

/// Lance l'application graphique.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 790.0])
            .with_min_inner_size([940.0, 600.0])
            .with_title("OrbiSys"),
        ..Default::default()
    };
    eframe::run_native(
        "OrbiSys",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)) as Box<dyn eframe::App>)),
    )
}
