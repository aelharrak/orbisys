#!/usr/bin/env python3
"""Mesure la couverture des traductions du site (docs/assets/i18n/<code>.js).

    python3 docs/tools/check-i18n.py            # tableau de couverture
    python3 docs/tools/check-i18n.py --missing de   # textes encore à traduire en allemand
    python3 docs/tools/check-i18n.py --strict   # échoue s'il reste des clés obsolètes

Un « texte » est un élément feuille des pages HTML (voir assets/lang.js) : sa clé
est son texte français. Une clé du dictionnaire qui ne correspond plus à aucun
texte des pages est dite obsolète : le texte français a changé, il faut la renommer.
"""
import json
import os
import re
import sys
from html import escape as html_escape
from html.parser import HTMLParser

DOCS = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
PAGES = ["index.html", "utilisateurs.html", "developpeurs.html", "404.html"]
LANGS = ["en", "de", "es", "pt", "it", "nl", "ja", "ko", "zh", "ar"]
INLINE = {"b", "i", "em", "strong", "code", "a", "span", "br", "kbd", "small"}
SKIP = {"script", "style", "pre", "svg", "code", "textarea", "option", "select", "noscript"}
VOID = {"br", "img", "meta", "link", "input", "hr"}


class Collector(HTMLParser):
    """Reproduit le moteur du site (assets/lang.js) : feuilles de texte et blocs mixtes."""

    def __init__(self):
        super().__init__()
        self.stack, self.nodes = [], []

    def handle_starttag(self, tag, attrs):
        node = {"tag": tag, "text": "", "child": False, "own": False, "html": "", "inline_only": True,
                "parent": self.stack[-1] if self.stack else None, "attrs": dict(attrs)}
        self.nodes.append(node)
        index = len(self.nodes) - 1
        for ancestor in self.stack:
            self.nodes[ancestor]["html"] += f"<{tag}>"
            if tag not in INLINE:
                self.nodes[ancestor]["inline_only"] = False
        if self.stack:
            self.nodes[self.stack[-1]]["child"] = True
        if tag not in VOID:
            self.stack.append(index)

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, -1, -1):
            if self.nodes[self.stack[i]]["tag"] == tag:
                for ancestor in self.stack[:i]:
                    self.nodes[ancestor]["html"] += f"</{tag}>"
                del self.stack[i:]
                break

    def handle_data(self, data):
        for ancestor in self.stack:
            self.nodes[ancestor]["html"] += html_escape(data, quote=False)
        if self.stack:
            node = self.nodes[self.stack[-1]]
            node["text"] += data
            if data.strip():
                node["own"] = True


def keys_of(path):
    parser = Collector()
    source = open(path, encoding="utf-8").read()
    parser.feed(source)
    keys = set()
    claimed = set()
    in_skip = set()
    for index, node in enumerate(parser.nodes):
        parent = node["parent"]
        skipped = node["tag"] in SKIP or "data-no-i18n" in node["attrs"] or (parent is not None and (parent in in_skip))
        if skipped:
            in_skip.add(index)
            continue
        if parent is not None and (parent in claimed):
            claimed.add(index)
            continue
        if node["child"]:
            if node["own"] and node["inline_only"]:
                claimed.add(index)
                key = re.sub(r"\s+", " ", node["html"]).strip()
                if key:
                    keys.add(key)
            continue
        text = re.sub(r"\s+", " ", node["text"]).strip()
        if not text or not re.search(r"[A-Za-zÀ-ÿ]{2}", text):
            continue
        if parent is not None and parser.nodes[parent]["own"]:
            continue
        keys.add(text)
    for node in parser.nodes:
        for attr in ("alt", "aria-label", "title"):
            value = node["attrs"].get(attr)
            if value and value.strip():
                keys.add(re.sub(r"\s+", " ", value).strip())
    title = re.search(r"<title>(.*?)</title>", source, re.S)
    if title:
        keys.add(re.sub(r"\s+", " ", title.group(1)).strip())
    return keys


def dictionary(code):
    text = open(os.path.join(DOCS, "assets", "i18n", f"{code}.js"), encoding="utf-8").read()
    match = re.search(r'window\.UM_T\["%s"\]=(\{.*\});\s*$' % code, text, re.S)
    return json.loads(match.group(1))


def main(argv):
    wanted = set()
    for page in PAGES:
        wanted |= keys_of(os.path.join(DOCS, page))
    print(f"{len(wanted)} textes traduisibles dans {len(PAGES)} pages\n")
    stale_total = 0
    if "--missing" in argv:
        code = argv[argv.index("--missing") + 1]
        have = dictionary(code)
        for key in sorted(wanted):
            if key not in have:
                print(key)
        return 0
    print(f"{'langue':8}{'traduits':>10}{'couverture':>12}{'obsolètes':>11}")
    for code in LANGS:
        have = dictionary(code)
        done = sum(1 for key in wanted if key in have)
        stale = [key for key in have if key not in wanted]
        stale_total += len(stale)
        print(f"{code:8}{done:>10}{done * 100 // len(wanted):>11}%{len(stale):>11}")
        for key in stale[:3]:
            print(f"          obsolète : {key[:70]}")
    return 1 if "--strict" in argv and stale_total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
