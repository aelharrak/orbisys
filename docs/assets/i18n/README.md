# Traductions du site

Le français est la langue source. Chaque texte « feuille » des pages (un élément
sans enfant, ou dont tout le contenu est du texte) est traduit par `assets/lang.js`
en cherchant **son texte français** dans le dictionnaire de la langue choisie, puis
dans le dictionnaire anglais, puis en le laissant en français. Rien n'est à étiqueter
dans le HTML.

| Fichier | Langue |
|---|---|
| `en.js` | English (repli commun : couverture complète) |
| `de.js`, `es.js`, `pt.js`, `it.js`, `nl.js` | Deutsch, Español, Português, Italiano, Nederlands |
| `ja.js`, `ko.js`, `zh.js` | 日本語, 한국어, 中文 |
| `ar.js` | العربية (écriture de droite à gauche : le navigateur gère le sens et la jonction des lettres) |

Format : un objet `clé française → traduction`, plus deux clés spéciales,
`__notice__` (bandeau « page partiellement traduite ») et `__close__`.

## Corriger ou compléter une traduction

1. Voir ce qui manque : `python3 docs/tools/check-i18n.py --missing de`.
2. Ajouter la ligne dans `assets/i18n/de.js` : `"Texte français exact": "Traduction",`.
3. Vérifier la couverture : `python3 docs/tools/check-i18n.py`.

## Si un texte français change

Sa clé change aussi : la traduction n'est plus trouvée et le texte retombe sur
l'anglais, puis sur le français. `check-i18n.py` signale la clé devenue obsolète ;
renommez-la dans chaque dictionnaire.

## Ajouter une langue

1. Copier `en.js` en `xx.js` (ligne `window.UM_T["xx"]`) et traduire.
2. L'ajouter à `LANGS` dans `assets/lang.js` (avec `rtl: true` pour une langue de droite à gauche)
   et à `LANGS` dans `docs/tools/check-i18n.py`.

## Limites

- Les paragraphes qui mêlent du texte et des éléments en ligne (`<b>`, `<code>`, `<i>`, liens)
  sont traduits comme un tout : la clé est le HTML interne **sans attributs** (espaces
  normalisés), et le moteur remet les attributs (par exemple `href`) dans l'ordre des balises.
  Une traduction doit donc contenir **les mêmes balises, dans le même nombre**.
- Les blocs contenant d'autres éléments (listes, images…) ne sont pas traités comme un tout :
  leurs feuilles le sont une à une.
