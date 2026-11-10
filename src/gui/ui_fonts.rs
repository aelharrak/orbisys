//! Polices de secours pour les écritures que la police d'egui ne couvre pas :
//! japonais, coréen, chinois (CJK) et arabe. On cherche une police déjà
//! installée sur le système ; rien n'est embarqué ni téléchargé.

use eframe::egui;
use orbisys::i18n::Lang;

/// Polices CJK courantes : (chemin, index dans la collection `.ttc` pour ja, ko, zh).
/// Noto Sans CJK range ses variantes dans l'ordre JP, KR, SC, TC, HK.
const CJK_FONTS: &[(&str, [u32; 3])] = &[
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", [0, 1, 2]),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", [0, 1, 2]),
    ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", [0, 1, 2]),
    ("/usr/share/fonts/OTF/NotoSansCJK-Regular.ttc", [0, 1, 2]),
    ("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", [0, 0, 0]),
    ("/usr/share/fonts/wenquanyi/wqy-zenhei/wqy-zenhei.ttc", [0, 0, 0]),
    ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", [0, 0, 0]),
    ("/usr/share/fonts/droid/DroidSansFallbackFull.ttf", [0, 0, 0]),
];

/// Polices couvrant les formes de présentation arabes (FE70–FEFF).
const ARABIC_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
    "/usr/share/fonts/noto/NotoNaskhArabic-Regular.ttf",
];

/// Police CJK installée : `(chemin, index)`. `slot` : 0 japonais, 1 coréen, 2 chinois.
fn find_cjk(slot: usize) -> Option<(&'static str, u32)> {
    CJK_FONTS
        .iter()
        .find(|(path, _)| std::path::Path::new(path).is_file())
        .map(|(path, indexes)| (*path, indexes[slot.min(2)]))
}

fn find_arabic() -> Option<&'static str> {
    ARABIC_FONTS.iter().find(|path| std::path::Path::new(path).is_file()).copied()
}

/// La langue demande-t-elle une police de secours ?
#[cfg(test)]
pub fn needs_fallback(lang: Lang) -> bool {
    matches!(lang, Lang::Ja | Lang::Ko | Lang::Zh | Lang::Ar)
}

/// Définitions de polices pour `lang`, et « la police nécessaire à cette langue a
/// été trouvée ». Les polices de secours CJK et arabe, quand elles sont installées,
/// sont toujours ajoutées en dernier : le sélecteur de langue affiche les noms
/// 日本語, 한국어, 中文 et العربية quelle que soit la langue de l'interface. Pour
/// le japonais, le coréen et le chinois, la variante de la police suit la langue.
pub fn definitions_for(lang: Lang) -> (egui::FontDefinitions, bool) {
    let mut definitions = egui::FontDefinitions::default();
    let mut add = |name: &str, path: &str, index: u32| -> bool {
        let Ok(bytes) = std::fs::read(path) else { return false };
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        definitions.font_data.insert(name.to_string(), data);
        // En dernier : les glyphes latins restent ceux de la police d'egui.
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions.families.entry(family).or_default().push(name.to_string());
        }
        true
    };
    let slot = match lang {
        Lang::Ko => 1,
        Lang::Zh => 2,
        _ => 0,
    };
    let cjk = find_cjk(slot).is_some_and(|(path, index)| add("um-cjk", path, index));
    let arabic = find_arabic().is_some_and(|path| add("um-arabic", path, 0));
    let found = match lang {
        Lang::Ja | Lang::Ko | Lang::Zh => cjk,
        Lang::Ar => arabic,
        _ => true,
    };
    // Icônes linéaires (Lucide) : famille nommée, utilisée uniquement via `icons::font`.
    definitions
        .font_data
        .insert("um-icons".to_string(), egui::FontData::from_static(super::icons::FONT));
    definitions
        .families
        .insert(egui::FontFamily::Name("icons".into()), vec!["um-icons".to_string()]);
    (definitions, found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_languages_need_no_fallback() {
        for lang in [Lang::Fr, Lang::En, Lang::De, Lang::Es, Lang::Pt, Lang::It, Lang::Nl] {
            assert!(!needs_fallback(lang));
            let (_, found) = definitions_for(lang);
            assert!(found, "{} n'exige aucune police de secours", lang.code());
        }
    }

    #[test]
    fn fallback_fonts_are_reported_not_assumed() {
        for lang in [Lang::Ja, Lang::Ko, Lang::Zh, Lang::Ar] {
            assert!(needs_fallback(lang));
            let (definitions, found) = definitions_for(lang);
            // Sans police installée, la fonction le signale au lieu de planter.
            let key = if lang == Lang::Ar { "um-arabic" } else { "um-cjk" };
            assert_eq!(found, definitions.font_data.contains_key(key), "{}", lang.code());
        }
    }
}
