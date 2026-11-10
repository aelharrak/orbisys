//! L'exemple de rubrique de la page « Documentation développeurs » du site.
//! Il est compilé et exécuté ici pour que la documentation ne dérive pas du code.

use orbisys::panel::*;

/// Analyse `systemctl --failed --plain --no-legend` : noms d'unités.
pub fn parse_failed(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.contains('.'))
        .map(str::to_string)
        .collect()
}

pub fn failed_units_from(text: &str) -> Panel {
    let mut panel = Panel::default();
    let units = parse_failed(text);
    panel.metric(
        "En échec",
        units.len().to_string(),
        if units.is_empty() { Level::Ok } else { Level::Bad },
    );

    let rows = units
        .iter()
        .filter(|unit| orbisys::util::valid_unit_name(unit))
        .map(|unit| {
            Row::new(&[unit.as_str()]).bad(true).action(
                Cmd::new("Redémarrer", "systemctl", &["restart", unit])
                    .privileged() // via pkexec
                    .confirm(&format!("Redémarrer {unit} ?")), // fenêtre de confirmation
            )
        })
        .collect();
    panel.section(Section::new("Unités en échec").columns(&["Unité"]).rows(rows));
    panel
}

#[test]
fn example_panel_is_well_formed() {
    let panel = failed_units_from("nginx.service loaded failed failed Nginx\n--rm.service x\nsshd.service loaded failed failed SSH\n");
    assert_eq!(panel.metrics[0].value, "3");
    // Un nom qui commencerait par « - » est refusé par la validation : jamais d'option injectée.
    let rows = &panel.sections[0].rows;
    assert_eq!(rows.len(), 2);
    let action = &rows[0].actions[0];
    assert!(action.privileged && action.confirm.is_some());
    assert_eq!(action.args, ["restart", "nginx.service"]);
    assert!(panel.render_text().contains("nginx.service"));
}
