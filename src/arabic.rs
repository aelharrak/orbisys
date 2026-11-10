//! Mise en forme de l'arabe pour un moteur de rendu qui ne sait ni relier les
//! lettres ni inverser le sens d'écriture (egui).
//!
//! `shape` transforme un texte logique en texte **visuel** : chaque lettre est
//! remplacée par sa forme de présentation (isolée, finale, initiale ou
//! médiane), les ligatures lam-alif sont formées, puis l'ordre est inversé pour
//! un affichage de gauche à droite. Les segments latins et les chiffres gardent
//! leur ordre. La disposition de l'interface elle-même n'est pas inversée.

/// `(isolée, jonction à gauche possible)`: les lettres à double jonction ont
/// quatre formes consécutives (isolée, finale, initiale, médiane), les autres deux.
fn forms(ch: char) -> Option<(u32, bool)> {
    let (base, dual) = match ch {
        '\u{0621}' => (0xFE80, false),
        '\u{0622}' => (0xFE81, false),
        '\u{0623}' => (0xFE83, false),
        '\u{0624}' => (0xFE85, false),
        '\u{0625}' => (0xFE87, false),
        '\u{0626}' => (0xFE89, true),
        '\u{0627}' => (0xFE8D, false),
        '\u{0628}' => (0xFE8F, true),
        '\u{0629}' => (0xFE93, false),
        '\u{062A}' => (0xFE95, true),
        '\u{062B}' => (0xFE99, true),
        '\u{062C}' => (0xFE9D, true),
        '\u{062D}' => (0xFEA1, true),
        '\u{062E}' => (0xFEA5, true),
        '\u{062F}' => (0xFEA9, false),
        '\u{0630}' => (0xFEAB, false),
        '\u{0631}' => (0xFEAD, false),
        '\u{0632}' => (0xFEAF, false),
        '\u{0633}' => (0xFEB1, true),
        '\u{0634}' => (0xFEB5, true),
        '\u{0635}' => (0xFEB9, true),
        '\u{0636}' => (0xFEBD, true),
        '\u{0637}' => (0xFEC1, true),
        '\u{0638}' => (0xFEC5, true),
        '\u{0639}' => (0xFEC9, true),
        '\u{063A}' => (0xFECD, true),
        '\u{0641}' => (0xFED1, true),
        '\u{0642}' => (0xFED5, true),
        '\u{0643}' => (0xFED9, true),
        '\u{0644}' => (0xFEDD, true),
        '\u{0645}' => (0xFEE1, true),
        '\u{0646}' => (0xFEE5, true),
        '\u{0647}' => (0xFEE9, true),
        '\u{0648}' => (0xFEED, false),
        '\u{0649}' => (0xFEEF, false),
        '\u{064A}' => (0xFEF1, true),
        _ => return None,
    };
    Some((base, dual))
}

fn is_arabic_letter(ch: char) -> bool {
    forms(ch).is_some()
}

/// Signe diacritique : il se rattache à la lettre précédente et n'interrompt pas la jonction.
fn is_mark(ch: char) -> bool {
    ('\u{064B}'..='\u{065F}').contains(&ch) || ch == '\u{0670}'
}

/// Lettre arabe, signe ou tatweel : fait partie d'un segment arabe.
fn is_arabic_part(ch: char) -> bool {
    is_arabic_letter(ch) || is_mark(ch) || ch == '\u{0640}'
}

/// Ligature lam + alif : `(isolée, finale)`.
fn lam_alef(next: char) -> Option<(u32, u32)> {
    match next {
        '\u{0622}' => Some((0xFEF5, 0xFEF6)),
        '\u{0623}' => Some((0xFEF7, 0xFEF8)),
        '\u{0625}' => Some((0xFEF9, 0xFEFA)),
        '\u{0627}' => Some((0xFEFB, 0xFEFC)),
        _ => None,
    }
}

/// Met en forme un segment composé uniquement de lettres arabes (et de leurs
/// signes) : formes contextuelles et ligatures, ordre visuel (droite à gauche).
fn shape_run(run: &[char]) -> String {
    // Unités : (lettre de base, signes qui la suivent)
    let mut units: Vec<(char, Vec<char>)> = Vec::new();
    for &ch in run {
        if is_mark(ch) {
            if let Some(last) = units.last_mut() {
                last.1.push(ch);
                continue;
            }
        }
        units.push((ch, Vec::new()));
    }
    let n = units.len();
    let joins_prev = |index: usize| -> bool {
        // La lettre précédente relie-t-elle vers la suivante (double jonction) ?
        index > 0 && forms(units[index - 1].0).is_some_and(|(_, dual)| dual)
    };
    let mut visual: Vec<String> = Vec::new();
    let mut i = 0;
    while i < n {
        let (ch, marks) = &units[i];
        let mut out = String::new();
        if *ch == '\u{0644}' && i + 1 < n {
            if let Some((iso, fin)) = lam_alef(units[i + 1].0) {
                let code = if joins_prev(i) { fin } else { iso };
                out.push(char::from_u32(code).unwrap_or(*ch));
                out.extend(marks.iter());
                out.extend(units[i + 1].1.iter());
                visual.push(out);
                i += 2;
                continue;
            }
        }
        match forms(*ch) {
            Some((base, dual)) => {
                let prev = joins_prev(i);
                let next = dual && i + 1 < n && forms(units[i + 1].0).is_some();
                let code = match (prev, next, dual) {
                    (false, false, _) => base,
                    (true, false, _) => base + 1,
                    (false, true, true) => base + 2,
                    (true, true, true) => base + 3,
                    (false, true, false) | (true, true, false) => base, // inatteignable : `next` exige `dual`
                };
                out.push(char::from_u32(code).unwrap_or(*ch));
            }
            None => out.push(*ch),
        }
        out.extend(marks.iter());
        visual.push(out);
        i += 1;
    }
    visual.reverse();
    visual.concat()
}

/// Met en forme un mot : segments arabes et segments non arabes alternent ; l'ordre
/// des segments est inversé, chaque segment arabe l'est aussi, les autres non.
fn shape_word(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut segments: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let arabic = is_arabic_part(chars[i]);
        let start = i;
        while i < chars.len() && is_arabic_part(chars[i]) == arabic {
            i += 1;
        }
        let slice = &chars[start..i];
        segments.push(if arabic { shape_run(slice) } else { slice.iter().collect() });
    }
    segments.reverse();
    segments.concat()
}

/// Contient-il de l'arabe ?
pub fn has_arabic(text: &str) -> bool {
    text.chars().any(is_arabic_part)
}

/// Texte logique → texte visuel. Un texte sans arabe est renvoyé tel quel.
pub fn shape(text: &str) -> String {
    if !has_arabic(text) {
        return text.to_string();
    }
    text.split('\n')
        .map(|line| {
            let mut words: Vec<String> = line.split(' ').map(shape_word).collect();
            words.reverse();
            words.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_other_scripts_alone() {
        assert_eq!(shape("Hello 42"), "Hello 42");
        assert_eq!(shape("日本語"), "日本語");
        assert_eq!(shape(""), "");
    }

    #[test]
    fn single_letters_use_isolated_forms() {
        assert_eq!(shape("ب"), "\u{FE8F}");
        assert_eq!(shape("ا"), "\u{FE8D}");
    }

    #[test]
    fn joins_letters_and_reverses_visually() {
        // « بب » : initiale puis finale, affichées de droite à gauche : finale à gauche.
        assert_eq!(shape("بب"), "\u{FE90}\u{FE91}");
        // « بيت » : ب initiale, ي médiane, ت finale.
        assert_eq!(shape("بيت"), "\u{FE96}\u{FEF4}\u{FE91}");
    }

    #[test]
    fn right_joining_letters_break_the_connection() {
        // « دب » : د ne relie pas vers la gauche, ب reste isolée.
        assert_eq!(shape("دب"), "\u{FE8F}\u{FEA9}");
    }

    #[test]
    fn forms_lam_alef_ligatures() {
        assert_eq!(shape("لا"), "\u{FEFB}");
        // « بلا » : la ligature est finale après une lettre qui relie.
        assert_eq!(shape("بلا"), "\u{FEFC}\u{FE91}");
        assert_eq!(shape("لأ"), "\u{FEF7}");
    }

    #[test]
    fn reverses_word_order_and_keeps_latin_in_order() {
        let shaped = shape("بحث VPN");
        assert!(shaped.starts_with("VPN "), "{shaped}");
        assert_eq!(shape("a b"), "a b");
        // Latin collé à de l'arabe : « وVPN » → VPN reste lisible et à gauche du « و ».
        assert_eq!(shape("وVPN"), "VPN\u{FEED}");
    }

    #[test]
    fn marks_stay_attached_to_their_letter() {
        let shaped = shape("بَ");
        assert_eq!(shaped, "\u{FE8F}\u{064E}");
    }

    #[test]
    fn multi_line_text_keeps_its_lines() {
        let shaped = shape("ا\nب");
        assert_eq!(shaped.lines().count(), 2);
    }
}
