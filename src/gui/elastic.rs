//! Mise en page élastique : la place réellement disponible est répartie au
//! lieu d'être codée en dur, et les longues listes sont paginées plutôt que
//! défilées.

use super::*;

/// Mesures dérivées de la fenêtre réelle.
pub(super) struct Elastic {
    pub(super) compact: bool,
    pub(super) width: f32,
    pub(super) height: f32,
}

/// Hauteur visible restante sous le curseur. Contrairement à
/// `Ui::available_height`, la mesure reste bornée à l'intérieur d'une zone
/// défilante, où la hauteur disponible est infinie.
pub(super) fn remaining(ui: &egui::Ui) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top()).max(0.0)
}

impl Elastic {
    pub(super) fn new(ui: &egui::Ui) -> Self {
        let width = ui.available_width();
        let height = remaining(ui);
        Self {
            compact: height < 720.0 || width < 1100.0,
            width,
            height,
        }
    }

    pub(super) fn gap(&self) -> f32 {
        if self.compact {
            8.0
        } else {
            12.0
        }
    }

    /// Deux colonnes dès que la largeur le permet : la hauteur visible double,
    /// ce qui évite le défilement sur les écrans denses.
    pub(super) fn two_columns(&self) -> bool {
        self.width >= 880.0
    }

    /// Nombre de lignes de `row` pixels qui tiennent dans `available`.
    pub(super) fn fit_rows(&self, available: f32, row: f32, min: usize, max: usize) -> usize {
        ((available.max(0.0) / row.max(1.0)).floor() as usize).clamp(min, max)
    }
}

/// Densité verticale de l'écran courant : marges réduites quand la place manque.
pub(super) fn density(ui: &mut egui::Ui, el: &Elastic) {
    ui.spacing_mut().item_spacing.y = if el.compact { 4.0 } else { 6.0 };
    ui.spacing_mut().button_padding.y = if el.compact { 2.0 } else { 3.0 };
}

/// Bornes de la page courante, sans dépasser `total`.
pub(super) fn page_bounds(page: usize, per_page: usize, total: usize) -> (usize, usize) {
    let per_page = per_page.max(1);
    let pages = total.div_ceil(per_page).max(1);
    let page = page.min(pages - 1);
    let start = (page * per_page).min(total);
    (start, (start + per_page).min(total))
}

/// Barre « ‹ 1–20 sur 212 › » : renvoie `true` si la page a changé.
pub(super) fn pager_bar(ui: &mut egui::Ui, page: &mut usize, per_page: usize, total: usize) -> bool {
    let per_page = per_page.max(1);
    let pages = total.div_ceil(per_page).max(1);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if ui
            .add_enabled(*page > 0, egui::Button::new("‹").small())
            .on_hover_text(ts("Précédent"))
            .clicked()
        {
            *page = page.saturating_sub(1);
            changed = true;
        }
        let (start, end) = page_bounds(*page, per_page, total);
        ui.label(
            egui::RichText::new(tf("{}–{} sur {}", &[&(start + 1), &end, &total]))
                .size(10.5)
                .color(ui.visuals().weak_text_color()),
        );
        if ui
            .add_enabled(*page + 1 < pages, egui::Button::new("›").small())
            .on_hover_text(ts("Suivant"))
            .clicked()
        {
            *page += 1;
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_bounds_stay_inside_the_list() {
        assert_eq!(page_bounds(0, 10, 0), (0, 0));
        assert_eq!(page_bounds(0, 10, 25), (0, 10));
        assert_eq!(page_bounds(2, 10, 25), (20, 25));
        // Une page devenue hors bornes (liste raccourcie) retombe sur la dernière.
        assert_eq!(page_bounds(7, 10, 25), (20, 25));
        assert_eq!(page_bounds(1, 0, 5), (1, 2));
    }

    #[test]
    fn rows_fit_the_available_height() {
        let el = Elastic {
            compact: true,
            width: 900.0,
            height: 600.0,
        };
        assert_eq!(el.fit_rows(600.0, 22.0, 3, 80), 27);
        assert_eq!(el.fit_rows(10.0, 22.0, 3, 80), 3);
        assert_eq!(el.fit_rows(10_000.0, 22.0, 3, 80), 80);
    }

    #[test]
    fn remaining_height_is_measured_against_the_visible_clip() {
        let ctx = egui::Context::default();
        let mut seen = 0.0_f32;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1024.0, 768.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(100.0);
                    seen = remaining(ui);
                });
            },
        );
        // La hauteur restante est positive et bornée par l'écran simulé.
        assert!(seen > 0.0, "hauteur visible nulle");
        assert!(seen <= 768.0, "hauteur non bornée : {seen}");
    }

    #[test]
    fn remaining_height_stays_bounded_inside_a_scroll_area() {
        let ctx = egui::Context::default();
        let mut inside = 0.0_f32;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1024.0, 768.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(50.0);
                            inside = remaining(ui);
                            // Contenu volontairement plus haut que la fenêtre :
                            // `available_height` vaudrait l'infini, pas `remaining`.
                            ui.allocate_space(egui::vec2(10.0, 5000.0));
                        });
                });
            },
        );
        assert!(inside > 0.0, "hauteur visible nulle dans la zone défilante");
        assert!(inside <= 768.0, "hauteur non bornée dans la zone défilante : {inside}");
    }
}
