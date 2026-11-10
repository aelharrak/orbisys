/* OrbiSys — comportements du site (thème, onglets, copie, visionneuse). Sans dépendance. */
(function () {
  "use strict";
  var root = document.documentElement;
  var store = {
    get: function (k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
    set: function (k, v) { try { localStorage.setItem(k, v); } catch (e) { /* stockage indisponible */ } }
  };

  // Thème : choix mémorisé, sinon clair par défaut.
  function applyTheme(theme) {
    var dark = theme === "dark";
    root.setAttribute("data-theme", dark ? "dark" : "light");
    var button = document.getElementById("theme");
    if (button) {
      button.setAttribute("aria-pressed", dark ? "true" : "false");
      var icon = button.querySelector(".theme-icon");
      if (icon) icon.textContent = dark ? "☾" : "☀";
    }
  }
  var saved = store.get("um-theme");
  applyTheme(saved || "light");
  document.addEventListener("DOMContentLoaded", function () {
    var button = document.getElementById("theme");
    if (button) {
      applyTheme(root.getAttribute("data-theme"));
      button.addEventListener("click", function () {
        var next = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
        applyTheme(next);
        store.set("um-theme", next);
      });
    }

    // Menu compact sur téléphone (bouton ajouté ici : sans texte à traduire).
    var nav = document.querySelector(".site-header .nav");
    if (nav) {
      var burger = document.createElement("button");
      burger.type = "button";
      burger.className = "burger";
      burger.setAttribute("data-no-i18n", "");
      burger.setAttribute("aria-label", "Menu");
      burger.setAttribute("aria-expanded", "false");
      burger.textContent = "\u2630";
      nav.parentNode.insertBefore(burger, nav);
      burger.addEventListener("click", function () {
        var open = nav.getAttribute("data-open") !== "true";
        nav.setAttribute("data-open", open ? "true" : "false");
        burger.setAttribute("aria-expanded", open ? "true" : "false");
      });
      nav.addEventListener("click", function (e) {
        if (e.target.closest("a")) { nav.setAttribute("data-open", "false"); burger.setAttribute("aria-expanded", "false"); }
      });
    }

    // Boutons « Copier » sur les blocs de code.
    document.querySelectorAll(".code").forEach(function (block) {
      var pre = block.querySelector("pre");
      if (!pre || block.querySelector(".copy")) return;
      var copy = document.createElement("button");
      copy.type = "button";
      copy.className = "copy";
      copy.textContent = "Copier";
      copy.addEventListener("click", function () {
        var text = pre.innerText.replace(/^\$ /gm, "");
        var done = function () { copy.textContent = "Copié ✓"; setTimeout(function () { copy.textContent = "Copier"; }, 1600); };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () { copy.textContent = "Non disponible"; });
        } else { copy.textContent = "Non disponible"; }
      });
      block.appendChild(copy);
    });

    // Onglets accessibles (flèches, Accueil, Fin).
    document.querySelectorAll(".tabs").forEach(function (tabs) {
      var list = tabs.querySelector('[role="tablist"]');
      var items = Array.prototype.slice.call(list.querySelectorAll('[role="tab"]'));
      function select(tab) {
        items.forEach(function (t) {
          var on = t === tab;
          t.setAttribute("aria-selected", on ? "true" : "false");
          t.tabIndex = on ? 0 : -1;
          document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
        });
      }
      items.forEach(function (tab, i) {
        tab.addEventListener("click", function () { select(tab); });
        tab.addEventListener("keydown", function (e) {
          var n = null;
          if (e.key === "ArrowRight") n = items[(i + 1) % items.length];
          if (e.key === "ArrowLeft") n = items[(i - 1 + items.length) % items.length];
          if (e.key === "Home") n = items[0];
          if (e.key === "End") n = items[items.length - 1];
          if (n) { e.preventDefault(); select(n); n.focus(); }
        });
      });
    });

    // Visionneuse d'images (élément <dialog>) : clic pour agrandir, Échap pour fermer.
    var dialog = document.getElementById("viewer");
    if (dialog && typeof dialog.showModal === "function") {
      var big = dialog.querySelector("img");
      var caption = dialog.querySelector("p");
      document.querySelectorAll(".shot button").forEach(function (b) {
        b.addEventListener("click", function () {
          var img = b.querySelector("img");
          big.src = img.currentSrc || img.src;
          big.alt = img.alt;
          var fig = b.closest("figure").querySelector("figcaption");
          caption.textContent = fig ? fig.innerText : img.alt;
          dialog.showModal();
        });
      });
      dialog.addEventListener("click", function (e) { if (e.target === dialog) dialog.close(); });
    }
  });
})();
