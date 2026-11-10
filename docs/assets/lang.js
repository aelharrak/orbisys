/* OrbiSys — traduction du site et sélecteur de langue. Sans dépendance.
 *
 * Le français est la langue source : chaque texte « feuille » de la page (un
 * élément sans enfant, ou dont tout le contenu est du texte) est traduit en
 * cherchant son texte français dans le dictionnaire de la langue choisie
 * (assets/i18n/<code>.js), puis dans le dictionnaire anglais, puis en le laissant
 * en français. Rien n'est à étiqueter dans le HTML.
 */
(function () {
  "use strict";

  var LANGS = [
    { code: "en", flag: "🇬🇧", name: "English" },
    { code: "fr", flag: "🇫🇷", name: "Français" },
    { code: "de", flag: "🇩🇪", name: "Deutsch" },
    { code: "es", flag: "🇪🇸", name: "Español" },
    { code: "pt", flag: "🇵🇹", name: "Português" },
    { code: "it", flag: "🇮🇹", name: "Italiano" },
    { code: "nl", flag: "🇳🇱", name: "Nederlands" },
    { code: "ja", flag: "🇯🇵", name: "日本語" },
    { code: "ko", flag: "🇰🇷", name: "한국어" },
    { code: "zh", flag: "🇨🇳", name: "中文" },
    { code: "ar", flag: "🇸🇦", name: "العربية", rtl: true }
  ];
  var SOURCE = "fr";
  var SKIP_TAGS = { SCRIPT: 1, STYLE: 1, PRE: 1, CODE: 1, SVG: 1, TEXTAREA: 1, OPTION: 1, SELECT: 1, NOSCRIPT: 1 };
  var ATTRS = ["alt", "aria-label", "title"];

  function norm(text) { return text.replace(/\s+/g, " ").trim(); }
  function find(code) { return LANGS.filter(function (l) { return l.code === code; })[0]; }
  function remember(value) { try { localStorage.setItem("um-lang", value); } catch (e) { /* stockage indisponible */ } }
  function recall() { try { return localStorage.getItem("um-lang"); } catch (e) { return null; } }

  // Langue : ?lang=xx, sinon choix mémorisé, sinon langue du navigateur, sinon français.
  function initial() {
    var query = /[?&]lang=([a-zA-Z-]+)/.exec(location.search);
    var wanted = (query && query[1]) || recall() || (navigator.language || "fr");
    var short = String(wanted).slice(0, 2).toLowerCase();
    return find(short) ? short : SOURCE;
  }

  var INLINE = { B: 1, I: 1, EM: 1, STRONG: 1, CODE: 1, A: 1, SPAN: 1, BR: 1, KBD: 1, SMALL: 1 };

  // Un bloc « mixte » mêle du texte et des éléments en ligne (<b>, <code>, <a>…) : il est
  // traduit d'un seul tenant, sa clé étant son HTML français sans attributs.
  function hasOwnText(el) {
    return Array.prototype.some.call(el.childNodes, function (node) {
      return node.nodeType === 3 && norm(node.nodeValue);
    });
  }
  function isMixed(el) {
    if (!el.children.length || !hasOwnText(el)) return false;
    return Array.prototype.every.call(el.querySelectorAll("*"), function (child) { return INLINE[child.tagName]; });
  }
  function mixedKey(el) {
    var clone = el.cloneNode(true);
    Array.prototype.forEach.call(clone.querySelectorAll("*"), function (child) {
      Array.prototype.slice.call(child.attributes).forEach(function (attr) { child.removeAttribute(attr.name); });
    });
    return norm(clone.innerHTML);
  }
  // Attributs d'origine des éléments en ligne, par nom de balise et par ordre d'apparition.
  function attributeQueues(el) {
    var queues = {};
    Array.prototype.forEach.call(el.querySelectorAll("*"), function (child) {
      var list = queues[child.tagName] || (queues[child.tagName] = []);
      list.push(Array.prototype.map.call(child.attributes, function (a) { return [a.name, a.value]; }));
    });
    return queues;
  }
  // Remplace le contenu de `el` par du HTML traduit, en rendant leurs attributs aux éléments en ligne.
  function setMixed(el, html, queues) {
    var holder = document.createElement("template");
    holder.innerHTML = html;
    var used = {};
    Array.prototype.forEach.call(holder.content.querySelectorAll("*"), function (child) {
      var index = used[child.tagName] || 0;
      used[child.tagName] = index + 1;
      var list = queues[child.tagName] && queues[child.tagName][index];
      if (list) list.forEach(function (pair) { child.setAttribute(pair[0], pair[1]); });
    });
    el.replaceChildren.apply(el, Array.prototype.slice.call(holder.content.childNodes));
  }

  // Éléments traduisibles, collectés une fois sur le texte français d'origine.
  var items = [];
  function collect() {
    var claimed = [];
    var all = document.body.querySelectorAll("*");
    Array.prototype.forEach.call(all, function (el) {
      if (SKIP_TAGS[el.tagName] || el.closest("[data-no-i18n], .code, #lang-select-wrap")) return;
      if (claimed.some(function (c) { return c.contains(el); })) return;
      ATTRS.forEach(function (attr) {
        var value = el.getAttribute(attr);
        if (value && norm(value)) items.push({ el: el, attr: attr, key: norm(value) });
      });
      if (el.children.length) {                            // pas une feuille : bloc mixte ?
        if (isMixed(el)) {
          claimed.push(el);
          items.push({ el: el, attr: null, mixed: true, key: mixedKey(el), queues: attributeQueues(el) });
        }
        return;
      }
      var key = norm(el.textContent);
      if (!key || !/[A-Za-zÀ-ÿ]{2}/.test(key)) return;
      // Feuille dans un parent qui a du texte propre : contenu mixte non pris en charge.
      var parent = el.parentElement;
      if (parent && hasOwnText(parent)) return;
      items.push({ el: el, attr: null, key: key });
    });
    items.push({ el: null, attr: "title", key: norm(document.title), doc: true });
  }

  var loaded = {};
  function load(code, done) {
    if (code === SOURCE || (window.UM_T && window.UM_T[code])) { done(); return; }
    if (loaded[code]) { loaded[code].push(done); return; }
    loaded[code] = [done];
    var script = document.createElement("script");
    script.src = "assets/i18n/" + code + ".js";
    var finish = function () { var waiting = loaded[code]; loaded[code] = null; waiting.forEach(function (fn) { fn(); }); };
    script.onload = finish;
    script.onerror = finish;                                // dictionnaire absent : repli sur le français
    document.head.appendChild(script);
  }

  function lookup(code, key) {
    var T = window.UM_T || {};
    if (code !== SOURCE && T[code] && T[code][key]) return { text: T[code][key], level: code };
    if (code !== SOURCE && T.en && T.en[key]) return { text: T.en[key], level: "en" };
    return { text: key, level: SOURCE };
  }

  function apply(code) {
    var lang = find(code) || find(SOURCE);
    items.forEach(function (item) {
      var hit = lookup(lang.code, item.key);
      if (hit.level === SOURCE) item.restore(); else item.set(hit.text);
    });
    document.documentElement.lang = lang.code;
    document.documentElement.dir = lang.rtl ? "rtl" : "ltr";
    markCurrent(lang.code);
  }

  // Pour chaque élément : mémoriser son état français d'origine et savoir le remplacer.
  function snapshot() {
    items.forEach(function (item) {
      if (item.doc) {
        var title = document.title;
        item.set = function (text) { document.title = text; };
        item.restore = function () { document.title = title; };
      } else if (item.attr) {
        var value = item.el.getAttribute(item.attr);
        item.set = function (text) { item.el.setAttribute(item.attr, text); };
        item.restore = function () { item.el.setAttribute(item.attr, value); };
      } else if (item.mixed) {
        var nodes = Array.prototype.map.call(item.el.childNodes, function (n) { return n.cloneNode(true); });
        item.set = function (html) { setMixed(item.el, html, item.queues); };
        item.restore = function () {
          item.el.replaceChildren.apply(item.el, nodes.map(function (n) { return n.cloneNode(true); }));
        };
      } else {
        var text = item.el.textContent;
        item.set = function (t) { item.el.textContent = t; };
        item.restore = function () { item.el.textContent = text; };
      }
    });
  }

  var currentCode = SOURCE;
  var langButton = null;
  var langOptions = [];

  // Drapeau et sélection courante du sélecteur de langue.
  function markCurrent(code) {
    var lang = find(code) || find(SOURCE);
    currentCode = lang.code;
    if (!langButton) return;
    var flag = langButton.querySelector(".lang-flag");
    if (flag) flag.textContent = lang.flag;
    var codeLabel = langButton.querySelector(".lang-code");
    if (codeLabel) codeLabel.textContent = lang.code.toUpperCase();
    langButton.setAttribute("aria-label", lang.name);
    langButton.title = lang.name;
    langOptions.forEach(function (option) {
      option.setAttribute("aria-selected", option.getAttribute("data-code") === lang.code ? "true" : "false");
    });
  }

  // Sélecteur maison : le <select> natif dessine sa propre bordure à l'ouverture,
  // impossible à retirer. Le bouton et la liste sont donc sans contour, et la liste
  // est ajoutée au <body> pour ne pas être rognée par l'en-tête ni par le menu mobile.
  function selector(current) {
    var nav = document.querySelector(".nav");
    if (!nav) return;

    var wrap = document.createElement("span");
    wrap.id = "lang-select-wrap";

    var button = document.createElement("button");
    button.type = "button";
    button.id = "lang-select";
    button.setAttribute("aria-haspopup", "listbox");
    button.setAttribute("aria-expanded", "false");
    button.setAttribute("aria-controls", "lang-menu");
    var flag = document.createElement("span");
    flag.className = "lang-flag";
    flag.setAttribute("aria-hidden", "true");
    button.appendChild(flag);
    var codeLabel = document.createElement("span");
    codeLabel.className = "lang-code";
    codeLabel.setAttribute("aria-hidden", "true");
    button.appendChild(codeLabel);
    var caret = document.createElement("span");
    caret.className = "lang-caret";
    caret.setAttribute("aria-hidden", "true");
    caret.textContent = "⌄";
    button.appendChild(caret);

    var menu = document.createElement("div");
    menu.id = "lang-menu";
    menu.setAttribute("role", "listbox");
    menu.setAttribute("aria-label", "Language / Langue");
    menu.hidden = true;

    var active = 0;

    function indexOf(code) {
      for (var i = 0; i < LANGS.length; i++) { if (LANGS[i].code === code) return i; }
      return 0;
    }
    function highlight(index) {
      if (!langOptions.length) return;
      active = (index + langOptions.length) % langOptions.length;
      langOptions.forEach(function (option, i) {
        option.classList.toggle("is-active", i === active);
      });
      button.setAttribute("aria-activedescendant", langOptions[active].id);
      if (langOptions[active].scrollIntoView) langOptions[active].scrollIntoView({ block: "nearest" });
    }
    function place() {
      var rect = button.getBoundingClientRect();
      menu.style.top = Math.round(rect.bottom + 6) + "px";
      if (document.documentElement.dir === "rtl") {
        menu.style.left = Math.round(rect.left) + "px";
        menu.style.right = "auto";
      } else {
        menu.style.left = "auto";
        menu.style.right = Math.round(window.innerWidth - rect.right) + "px";
      }
    }
    function open() {
      if (!menu.hidden) return;
      place();
      menu.hidden = false;
      button.setAttribute("aria-expanded", "true");
      highlight(indexOf(currentCode));
    }
    function close(returnFocus) {
      if (menu.hidden) return;
      menu.hidden = true;
      button.setAttribute("aria-expanded", "false");
      button.removeAttribute("aria-activedescendant");
      langOptions.forEach(function (option) { option.classList.remove("is-active"); });
      if (returnFocus) button.focus();
    }
    function choose(code) {
      close(true);
      remember(code);
      load(code, function () { apply(code); });
    }

    LANGS.forEach(function (lang) {
      var option = document.createElement("div");
      option.id = "lang-option-" + lang.code;
      option.className = "lang-option";
      option.setAttribute("role", "option");
      option.setAttribute("data-code", lang.code);
      option.setAttribute("aria-label", lang.name);
      option.title = lang.name;
      option.tabIndex = -1;
      option.textContent = lang.flag;
      option.addEventListener("click", function () { choose(lang.code); });
      menu.appendChild(option);
      langOptions.push(option);
    });

    button.addEventListener("click", function () {
      if (menu.hidden) open(); else close(true);
    });
    button.addEventListener("keydown", function (event) {
      var key = event.key;
      if (key === "ArrowDown" || key === "ArrowUp") {
        event.preventDefault();
        if (menu.hidden) open();
        else highlight(active + (key === "ArrowDown" ? 1 : -1));
      } else if (key === "Enter" || key === " ") {
        if (!menu.hidden) { event.preventDefault(); choose(LANGS[active].code); }
      } else if (key === "Escape" && !menu.hidden) {
        event.preventDefault();
        close(true);
      } else if (key === "Tab") {
        close(false);
      }
    });

    document.addEventListener("mousedown", function (event) {
      if (menu.hidden || menu.contains(event.target) || button.contains(event.target)) return;
      close(false);
    });
    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && !menu.hidden) close(true);
    });
    window.addEventListener("resize", function () { close(false); });
    window.addEventListener("scroll", function (event) {
      if (event.target === menu || menu.contains(event.target)) return;
      close(false);
    }, true);

    langButton = button;
    wrap.appendChild(button);
    var theme = document.getElementById("theme");
    if (theme && theme.parentNode === nav) nav.insertBefore(wrap, theme);
    else nav.appendChild(wrap);
    document.body.appendChild(menu);
    markCurrent(current);
  }

  document.addEventListener("DOMContentLoaded", function () {
    collect();
    snapshot();
    // Clés collectées (utilisées par tools/check-i18n.py --browser pour comparer avec la liste Python).
    window.UM_KEYS = items.map(function (item) { return item.key; });
    var code = initial();
    selector(code);
    if (code !== SOURCE) load(code, function () { apply(code); });
  });
})();
