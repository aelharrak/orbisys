//! Rendu générique des rubriques décrites par `panel::Panel`.

use super::*;
use orbisys::panel::{Chart, ChartKind, Cmd, Level, Panel};
use orbisys::i18n_ui::{tr, tx};
use orbisys::panels::PanelId;

/// Valeur formatée selon l'unité du graphique.
fn chart_value(value: f32, unit: &str) -> String {
    match unit {
        "B/s" => format!("{}/s", util::human_bytes(value.max(0.0) as u64)),
        "B" => util::human_bytes(value.max(0.0) as u64),
        "%" | "°C" => format!("{value:.0} {unit}"),
        "" => format!("{value:.1}"),
        _ => format!("{value:.1} {unit}"),
    }
}

/// Dessine un graphique (courbes ou barres) avec le moteur de rendu d'egui.
pub(super) fn draw_chart(
    ui: &mut egui::Ui,
    chart: &Chart,
    lang: Lang,
    accent: egui::Color32,
    muted: egui::Color32,
    dark: bool,
) {
    let palette = [
        accent,
        egui::Color32::from_rgb(63, 185, 120),
        egui::Color32::from_rgb(242, 161, 60),
        egui::Color32::from_rgb(168, 107, 255),
    ];
    let background = if dark {
        egui::Color32::from_rgb(24, 24, 24)
    } else {
        egui::Color32::from_rgb(250, 250, 250)
    };
    let grid = if dark {
        egui::Color32::from_rgb(42, 42, 42)
    } else {
        egui::Color32::from_rgb(232, 232, 232)
    };
    let max = chart.axis_max();
    ui.add_space(6.0);
    ui.label(egui::RichText::new(tr(lang, &chart.title)).strong().size(11.5));

    match chart.kind {
        ChartKind::Lines => {
            let width = ui.available_width().min(820.0);
            let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 150.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, background);
            painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0_f32, grid));
            let plot = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 58.0, rect.top() + 10.0),
                egui::pos2(rect.right() - 12.0, rect.bottom() - 10.0),
            );
            for step in 0..=4 {
                let ratio = step as f32 / 4.0;
                let y = plot.bottom() - ratio * plot.height();
                painter.line_segment(
                    [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                    egui::Stroke::new(1.0_f32, grid),
                );
                painter.text(
                    egui::pos2(plot.left() - 6.0, y),
                    egui::Align2::RIGHT_CENTER,
                    chart_value(max * ratio, &chart.unit),
                    egui::FontId::proportional(9.5),
                    muted,
                );
            }
            let points = chart.series.iter().map(|series| series.values.len()).max().unwrap_or(0);
            if points < 2 {
                painter.text(
                    plot.center(),
                    egui::Align2::CENTER_CENTER,
                    "…",
                    egui::FontId::proportional(14.0),
                    muted,
                );
            } else {
                for (index, series) in chart.series.iter().enumerate() {
                    let color = palette[index % palette.len()];
                    let count = series.values.len();
                    let line: Vec<egui::Pos2> = series
                        .values
                        .iter()
                        .enumerate()
                        .map(|(position, value)| {
                            // Les séries plus courtes sont alignées sur la droite (échantillon le plus récent).
                            let offset = points - count + position;
                            let x = plot.left() + offset as f32 / (points - 1) as f32 * plot.width();
                            let y = plot.bottom() - (value / max).clamp(0.0, 1.0) * plot.height();
                            egui::pos2(x, y)
                        })
                        .collect();
                    if index == 0 && line.len() >= 2 {
                        // Une bande par segment : chaque trapèze est convexe, contrairement
                        // au polygone d'ensemble dès que la courbe monte puis redescend.
                        for pair in line.windows(2) {
                            painter.add(egui::Shape::convex_polygon(
                                vec![
                                    pair[0],
                                    pair[1],
                                    egui::pos2(pair[1].x, plot.bottom()),
                                    egui::pos2(pair[0].x, plot.bottom()),
                                ],
                                color.gamma_multiply(0.12),
                                egui::Stroke::NONE,
                            ));
                        }
                    }
                    painter.add(egui::Shape::line(line, egui::Stroke::new(1.8_f32, color)));
                }
            }
            if let Some(pointer) = response.hover_pos() {
                if plot.contains(pointer) && points >= 2 {
                    let ratio = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                    let index = (ratio * (points - 1) as f32).round() as usize;
                    let mut text = String::new();
                    for series in &chart.series {
                        let count = series.values.len();
                        if index + count >= points {
                            if let Some(value) = series.values.get(index + count - points) {
                                text.push_str(&format!("{} : {}\n", tr(lang, &series.label), chart_value(*value, &chart.unit)));
                            }
                        }
                    }
                    if !text.is_empty() {
                        response.on_hover_text(text.trim_end());
                    }
                }
            }
            ui.horizontal_wrapped(|ui| {
                for (index, series) in chart.series.iter().enumerate() {
                    let color = palette[index % palette.len()];
                    let (swatch, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(swatch, 2.0, color);
                    let last = series.values.last().copied().unwrap_or(0.0);
                    ui.label(
                        egui::RichText::new(format!("{} {}", tr(lang, &series.label), chart_value(last, &chart.unit)))
                            .size(10.5)
                            .color(muted),
                    );
                    ui.add_space(8.0);
                }
            });
        }
        ChartKind::Bars => {
            let values = chart.series.first().map(|series| series.values.as_slice()).unwrap_or(&[]);
            let available = ui.available_width();
            let label_width = (available * 0.36).clamp(90.0, 170.0);
            let bar_width = (available - label_width - 90.0).clamp(60.0, 520.0);
            for (index, value) in values.iter().enumerate() {
                let label = chart.categories.get(index).cloned().unwrap_or_default();
                ui.horizontal(|ui| {
                    left_cell(
                        ui,
                        [label_width, 18.0],
                        egui::Label::new(egui::RichText::new(util::ellipsis(&tr(lang, &label), 26)).size(10.5)).truncate(),
                    );
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(bar_width, 14.0), egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 4.0, background);
                    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0_f32, grid));
                    let ratio = (value / max).clamp(0.0, 1.0);
                    let over = chart.marks.get(index).is_some_and(|mark| value >= mark);
                    let color = if over { egui::Color32::from_rgb(224, 92, 92) } else { accent };
                    let filled = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * ratio, rect.height()));
                    painter.rect_filled(filled, 4.0, color);
                    if let Some(mark) = chart.marks.get(index) {
                        let x = rect.left() + (mark / max).clamp(0.0, 1.0) * rect.width();
                        painter.line_segment(
                            [egui::pos2(x, rect.top() - 2.0), egui::pos2(x, rect.bottom() + 2.0)],
                            egui::Stroke::new(1.5_f32, muted),
                        );
                    }
                    if let Some(mark) = chart.marks.get(index) {
                        response.on_hover_text(format!(
                            "{} / {}",
                            chart_value(*value, &chart.unit),
                            chart_value(*mark, &chart.unit)
                        ));
                    }
                    ui.label(
                        egui::RichText::new(chart_value(*value, &chart.unit))
                            .monospace()
                            .size(10.5)
                            .color(if over { egui::Color32::from_rgb(224, 92, 92) } else { muted }),
                    );
                });
            }
        }
    }
    ui.add_space(6.0);
}

impl App {
    /// Demande le chargement d'une rubrique, ou son rafraîchissement automatique.
    pub(super) fn ensure_panel(&mut self, ctx: &egui::Context, id: PanelId) {
        if self.panel_loading.contains(&id) {
            return;
        }
        let stale = match (id.auto_refresh(), self.panel_loaded_at.get(&id)) {
            (Some(seconds), Some(at)) => {
                ctx.request_repaint_after(Duration::from_secs(seconds));
                at.elapsed() >= Duration::from_secs(seconds)
            }
            _ => false,
        };
        if !self.panels.contains_key(&id) || stale {
            self.apply_pending(Pending::PanelLoad(id));
        }
    }

    pub(super) fn page_panel(&mut self, ui: &mut egui::Ui, id: PanelId) {
        let Some(panel) = self.panels.remove(&id) else {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(egui::RichText::new(self.tr("ui.loading")).color(self.muted()));
            });
            return;
        };
        self.draw_panel(ui, id, &panel);
        self.panels.insert(id, panel);
    }

    fn draw_panel(&mut self, ui: &mut egui::Ui, id: PanelId, panel: &Panel) {
        let muted = self.muted();
        let lang = self.lang;
        let (accent, dark) = (self.accent_color(), self.dark);
        let filter = self.panel_filter.trim().to_lowercase();
        let loading = self.panel_loading.contains(&id);
        let mut run: Vec<Cmd> = Vec::new();
        let mut confirm: Option<Confirmation> = None;
        let mut reload = false;
        let mut inputs = std::mem::take(&mut self.panel_inputs);
        let mut rejected: Option<String> = None;
        let mut close_output = false;
        let (filter_hint, reload_text) = (self.tr("ui.filter"), self.tr("ui.reload"));
        let (empty_text, nomatch_text, close_text) = (self.tr("ui.empty"), self.tr("ui.nomatch"), self.tr("ui.close"));

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.panel_filter)
                    .desired_width(200.0)
                    .hint_text(filter_hint),
            );
            if ui.button(reload_text).clicked() {
                reload = true;
            }
            if loading {
                ui.add(egui::Spinner::new().size(14.0));
            }
        });
        ui.add_space(6.0);

        let output_height = if self.panel_output.is_some() { 170.0 } else { 0.0 };
        // Pagination plutôt que défilement : chaque page tient dans la fenêtre.
        if self.panel_page_for != Some(id) {
            self.panel_page = 0;
            self.panel_page_for = Some(id);
        }
        let per_page = super::elastic::Elastic::new(ui)
            .fit_rows((super::elastic::remaining(ui) - output_height - 150.0).max(90.0), 36.0, 4, 60);
        {
                if !panel.metrics.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for metric in &panel.metrics {
                            let tone = match metric.level {
                                Level::Ok => Some(self.ok_color()),
                                Level::Warn => Some(self.warn_color()),
                                Level::Bad => Some(self.danger_color()),
                                Level::Info => None,
                            };
                            let response = egui::Frame::none()
                                .fill(self.card_color())
                                .stroke(egui::Stroke::new(1.0_f32, self.border_color()))
                                .rounding(12.0_f32)
                                .inner_margin(egui::Margin {
                                    left: 16.0,
                                    right: 14.0,
                                    top: 9.0,
                                    bottom: 9.0,
                                })
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.label(
                                            egui::RichText::new(tr(lang, &metric.label))
                                                .size(10.5)
                                                .color(muted),
                                        );
                                        ui.label(
                                            egui::RichText::new(tr(lang, &metric.value))
                                                .strong()
                                                .size(14.5)
                                                .color(tone.unwrap_or(ui.visuals().text_color())),
                                        );
                                    });
                                });
                            // Barre de gravité à gauche de la tuile (verdict, alertes…).
                            if let Some(color) = tone {
                                let rect = response.response.rect;
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.min + egui::vec2(0.0, 8.0),
                                        egui::vec2(3.0, rect.height() - 16.0),
                                    ),
                                    2.0,
                                    color,
                                );
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                for error in &panel.errors {
                    ui.label(
                        egui::RichText::new(tr(lang, error))
                            .size(10.5)
                            .color(egui::Color32::from_rgb(224, 92, 92)),
                    );
                }

                for (index, section) in panel.sections.iter().enumerate() {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(tr(lang, &section.title).to_uppercase())
                            .size(10.0)
                            .strong()
                            .color(muted),
                    );
                    if let Some(note) = &section.note {
                        ui.label(egui::RichText::new(tr(lang, note)).size(10.0).color(muted));
                    }
                    if !section.actions.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for action in &section.actions {
                                if ui.button(tr(lang, &action.label)).clicked() {
                                    match &action.confirm {
                                        Some(detail) => {
                                            confirm = Some(Confirmation {
                                                title: tr(lang, &action.label),
                                                detail: tr(lang, detail),
                                                pending: Pending::PanelRun(action.clone(), id),
                                            })
                                        }
                                        None => run.push(action.clone()),
                                    }
                                }
                            }
                        });
                    }
                    for chart in &section.charts {
                        draw_chart(ui, chart, lang, accent, muted, dark);
                    }
                    if let Some(text) = &section.text {
                        egui::ScrollArea::vertical()
                            .id_salt(("panel-text", index))
                            .max_height(220.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(text.lines().map(tx).collect::<Vec<_>>().join("\n")).monospace().size(10.5))
                                        .wrap(),
                                );
                            });
                    }
                    if !section.columns.is_empty() {
                        let visible: Vec<&orbisys::panel::Row> = section
                            .rows
                            .iter()
                            .filter(|row| {
                                filter.is_empty()
                                    || row.cells.iter().any(|cell| cell.to_lowercase().contains(&filter))
                            })
                            .collect();
                        if visible.is_empty() {
                            ui.label(
                                egui::RichText::new(if section.rows.is_empty() {
                                    empty_text
                                } else {
                                    nomatch_text
                                })
                                .size(10.5)
                                .color(muted),
                            );
                        } else {
                            egui::Grid::new(("panel-grid", index))
                                .striped(true)
                                .num_columns(section.columns.len() + 1)
                                .spacing([18.0, 10.0])
                                .min_row_height(26.0)
                                .show(ui, |ui| {
                                    for column in &section.columns {
                                        ui.label(
                                            egui::RichText::new(tr(lang, column).to_uppercase())
                                                .monospace()
                                                .size(9.5)
                                                .color(muted),
                                        );
                                    }
                                    ui.label("");
                                    ui.end_row();
                                    let (first, last) = super::elastic::page_bounds(
                                        self.panel_page,
                                        per_page,
                                        visible.len(),
                                    );
                                    for row in &visible[first..last] {
                                        for (position, cell) in row.cells.iter().enumerate() {
                                            let limit = if row.actions.is_empty() { 90 } else { 46 };
                                            let mut text = egui::RichText::new(util::ellipsis(&tx(cell), limit)).size(10.5);
                                            if row.bad {
                                                text = text.color(egui::Color32::from_rgb(224, 92, 92));
                                            }
                                            if position == 0 {
                                                text = text.monospace();
                                            }
                                            // Colonne « Gravité » : pastille teintée (Élevée, Moyenne, Faible, Info).
                                            let tone = if position == 0 {
                                                match cell.as_str() {
                                                    "Élevée" | "Critique" => Some(self.danger_color()),
                                                    "Moyenne" => Some(self.warn_color()),
                                                    "Faible" => Some(accent),
                                                    "Info" => Some(self.muted()),
                                                    _ => None,
                                                }
                                            } else {
                                                None
                                            };
                                            if let Some(color) = tone {
                                                egui::Frame::none()
                                                    .fill(color.gamma_multiply(0.14))
                                                    .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
                                                    .rounding(11.0_f32)
                                                    .inner_margin(egui::Margin::symmetric(10.0, 2.0))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            egui::RichText::new(tx(cell))
                                                                .size(10.5)
                                                                .strong()
                                                                .color(color),
                                                        );
                                                    });
                                            } else {
                                                ui.label(text).on_hover_text(cell);
                                            }
                                        }
                                        for _ in row.cells.len()..section.columns.len() {
                                            ui.label("");
                                        }
                                        ui.horizontal(|ui| {
                                            for action in &row.actions {
                                                let destructive = ["Terminer", "Arrêter", "Supprimer", "Forcer", "Mettre en quarantaine"]
                                                    .iter()
                                                    .any(|word| action.label.starts_with(word));
                                                let danger = self.danger_color();
                                                let button = if destructive {
                                                    egui::Button::new(
                                                        egui::RichText::new(tr(lang, &action.label)).size(11.0).color(danger),
                                                    )
                                                    .fill(danger.gamma_multiply(0.10))
                                                    .stroke(egui::Stroke::new(1.0_f32, danger.gamma_multiply(0.45)))
                                                    .small()
                                                } else {
                                                    egui::Button::new(egui::RichText::new(tr(lang, &action.label)).size(11.0)).small()
                                                };
                                                if ui.add(button).clicked() {
                                                    match &action.confirm {
                                                        Some(detail) => {
                                                            confirm = Some(Confirmation {
                                                                title: action.label.clone(),
                                                                detail: detail.clone(),
                                                                pending: Pending::PanelRun(action.clone(), id),
                                                            })
                                                        }
                                                        None => run.push(action.clone()),
                                                    }
                                                }
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                            super::elastic::pager_bar(
                                ui,
                                &mut self.panel_page,
                                per_page,
                                visible.len(),
                            );
                        }
                    }
                    if let Some(form) = &section.form {
                        let values = inputs
                            .entry((id, index))
                            .or_insert_with(|| form.fields.iter().map(|f| f.default.clone()).collect());
                        egui::Grid::new(("panel-form", index)).num_columns(2).show(ui, |ui| {
                            for (field, value) in form.fields.iter().zip(values.iter_mut()) {
                                ui.label(egui::RichText::new(tr(lang, &field.label)).size(10.5));
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .desired_width(380.0)
                                        .hint_text(tr(lang, &field.hint)),
                                );
                                ui.end_row();
                            }
                        });
                        if ui.button(tr(lang, &form.submit)).clicked() {
                            match (form.build)(values) {
                                Ok(cmd) => match &cmd.confirm {
                                    Some(detail) => {
                                        confirm = Some(Confirmation {
                                            title: tr(lang, &cmd.label),
                                            detail: tr(lang, detail),
                                            pending: Pending::PanelRun(cmd.clone(), id),
                                        })
                                    }
                                    None => run.push(cmd),
                                },
                                Err(message) => rejected = Some(message),
                            }
                        }
                    }
                }
        }

        if let Some((title, text)) = &self.panel_output {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong().size(11.0));
                if ui.small_button(close_text).clicked() {
                    close_output = true;
                }
            });
            egui::ScrollArea::both()
                .id_salt("panel-output")
                .max_height(130.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(10.5)).wrap());
                });
        }

        self.panel_inputs = inputs;
        if close_output {
            self.panel_output = None;
        }
        if let Some(message) = rejected {
            self.toast(message, false);
        }
        if let Some(confirmation) = confirm {
            self.confirm = Some(confirmation);
        }
        if reload {
            self.apply_pending(Pending::PanelLoad(id));
        }
        for cmd in run {
            self.apply_pending(Pending::PanelRun(cmd, id));
        }
    }
}
