/* Runecord Developers — shared behaviour */
(function () {
  "use strict";

  var root = document.documentElement;
  var themeBtn = document.querySelector(".theme-toggle");
  var menuBtn = document.querySelector(".menu-btn");
  var sidebar = document.getElementById("sidebar");
  var backdrop = document.querySelector(".sidebar-backdrop");
  var toast = document.getElementById("toast");
  var toastTimer = null;

  function showToast(message) {
    if (!toast) return;
    toast.textContent = message;
    toast.classList.add("show");
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(function () {
      toast.classList.remove("show");
    }, 2400);
  }

  /* ---------- Theme ---------- */

  function setTheme(theme) {
    root.setAttribute("data-theme", theme);
    try {
      localStorage.setItem("runecord-docs-theme", theme);
    } catch (e) {}

    if (themeBtn) {
      var next = theme === "dark" ? "light" : "dark";
      themeBtn.setAttribute("aria-label", "Switch to " + next + " theme");
      themeBtn.setAttribute("title", "Switch to " + next + " theme");
    }

    var meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", theme === "dark" ? "#020617" : "#f4f6fb");
  }

  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      setTheme(root.getAttribute("data-theme") === "light" ? "dark" : "light");
    });
    setTheme(root.getAttribute("data-theme") === "light" ? "light" : "dark");
  }

  /* ---------- Mobile drawer ---------- */

  function closeDrawer() {
    document.body.classList.remove("drawer-open");
    if (menuBtn) {
      menuBtn.setAttribute("aria-expanded", "false");
      menuBtn.setAttribute("aria-label", "Open documentation menu");
    }
  }

  if (menuBtn && sidebar) {
    menuBtn.addEventListener("click", function () {
      var open = !document.body.classList.contains("drawer-open");
      document.body.classList.toggle("drawer-open", open);
      menuBtn.setAttribute("aria-expanded", open ? "true" : "false");
      menuBtn.setAttribute("aria-label", open ? "Close documentation menu" : "Open documentation menu");
    });

    sidebar.addEventListener("click", function (event) {
      if (event.target.closest("a")) closeDrawer();
    });
  }

  if (backdrop) backdrop.addEventListener("click", closeDrawer);

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape") closeDrawer();
  });

  window.addEventListener("resize", function () {
    if (window.innerWidth > 980) closeDrawer();
  });

  /* ---------- Active nav ---------- */

  var here = window.location.pathname.split("/").pop() || "index.html";

  document.querySelectorAll(".side-link, .topnav a").forEach(function (link) {
    var href = link.getAttribute("href") || "";
    var file = href.split("#")[0];
    if (file && file === here) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  });

  /* ---------- Copy helpers ---------- */

  function fallbackCopy(text) {
    var area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    try {
      document.execCommand("copy");
    } catch (e) {}
    document.body.removeChild(area);
  }

  function copyText(text, button) {
    var done = function () {
      if (button) {
        button.classList.add("is-copied");
        var label = button.querySelector(".copy-label");
        if (label) {
          var old = label.textContent;
          label.textContent = "Copied";
          window.setTimeout(function () {
            label.textContent = old;
            button.classList.remove("is-copied");
          }, 1600);
        } else {
          window.setTimeout(function () {
            button.classList.remove("is-copied");
          }, 1600);
        }
      }
      showToast("Copied to clipboard");
    };

    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done).catch(function () {
        fallbackCopy(text);
        done();
      });
    } else {
      fallbackCopy(text);
      done();
    }
  }

  document.addEventListener("click", function (event) {
    var button = event.target.closest("[data-copy]");
    if (!button) return;

    var value = button.getAttribute("data-copy");
    if (!value) {
      var block = button.closest(".code") || button.closest(".console");
      var pre = block && block.querySelector("pre:not([hidden])");
      value = pre ? pre.innerText.replace(/\s+$/, "") : "";
    }
    if (value) copyText(value, button);
  });

  /* ---------- Tabs ---------- */

  document.querySelectorAll("[data-tabs]").forEach(function (group) {
    var buttons = group.querySelectorAll("[data-tab]");

    function select(name) {
      buttons.forEach(function (button) {
        var active = button.getAttribute("data-tab") === name;
        button.setAttribute("aria-selected", active ? "true" : "false");
        button.setAttribute("tabindex", active ? "0" : "-1");
      });

      var target = group.getAttribute("data-tabs");
      document.querySelectorAll('[data-panel-group="' + target + '"] [data-panel]').forEach(function (panel) {
        panel.hidden = panel.getAttribute("data-panel") !== name;
      });
    }

    buttons.forEach(function (button) {
      button.addEventListener("click", function () {
        select(button.getAttribute("data-tab"));
      });
    });
  });

  /* ---------- Endpoint glue ---------- */

  function slugFor(method, path) {
    var clean = path
      .toLowerCase()
      .replace(/^\//, "")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
    return "ep-" + method.toLowerCase() + "-" + clean;
  }

  var apiRoot = document.getElementById("api-groups");
  var apiCount = null;
  var apiFilter = null;
  var visibleTotal = 0;

  if (apiRoot && window.RUNECORD_API) {
    var groups = window.RUNECORD_API.groups;
    var html = "";
    var total = 0;

    groups.forEach(function (group) {
      html +=
        '<section class="api-group" data-group="' + group.id + '">' +
        '<div class="api-group-head"><h2 id="group-' + group.id + '">' + group.name + "</h2>" +
        '<span class="api-group-count">' + group.endpoints.length + "</span></div>" +
        '<p class="api-group-blurb">' + group.blurb + "</p>";
      group.endpoints.forEach(function (endpoint) {
        var id = slugFor(endpoint.m, endpoint.p);
        total += 1;
        html +=
          '<div class="endpoint" id="' + id + '" data-search="' +
          (endpoint.m + " " + endpoint.p + " " + endpoint.d + " " + group.name).toLowerCase().replace(/"/g, "&quot;") +
          '" data-group="' + group.id + '">' +
          '<div class="ep-head">' +
          '<span class="method ' + endpoint.m.toLowerCase() + '">' + endpoint.m + "</span>" +
          '<code class="ep-path">' + endpoint.p + "</code>" +
          '<span class="ep-auth">' + endpoint.a + "</span>" +
          '<button class="ep-copy" type="button" data-copy="' + endpoint.p + '" aria-label="Copy ' + endpoint.p + '">' +
          '<svg class="icon" aria-hidden="true"><use href="assets/img/icons.svg#i-copy"></use></svg>' +
          "</button></div>" +
          '<p class="ep-desc">' + endpoint.d + "</p></div>";
      });
      html += "</section>";
    });

    html += '<div class="api-empty" id="api-empty" hidden>No endpoints match that filter. Try a route, method or resource name.</div>';
    apiRoot.innerHTML = html;
    visibleTotal = total;

    apiFilter = document.getElementById("api-filter");
    apiCount = document.getElementById("api-count");
    var chipsRoot = document.getElementById("api-chips");
    var activeGroup = "all";

    if (chipsRoot) {
      var chipHtml = '<button class="chip is-active" type="button" data-group="all">All</button>';
      groups.forEach(function (group) {
        chipHtml += '<button class="chip" type="button" data-group="' + group.id + '">' + group.name + "</button>";
      });
      chipsRoot.innerHTML = chipHtml;
    }

    function applyFilter() {
      var query = (apiFilter && apiFilter.value || "").trim().toLowerCase();
      var shown = 0;

      document.querySelectorAll(".endpoint").forEach(function (row) {
        var group = row.getAttribute("data-group");
        var text = row.getAttribute("data-search") || "";
        var match = (activeGroup === "all" || group === activeGroup) && (!query || text.indexOf(query) !== -1);
        row.hidden = !match;
        if (match) shown += 1;
      });

      document.querySelectorAll(".api-group").forEach(function (section) {
        var items = section.querySelectorAll(".endpoint");
        var any = false;
        items.forEach(function (item) {
          if (!item.hidden) any = true;
        });
        section.hidden = !any;
      });

      var empty = document.getElementById("api-empty");
      if (empty) empty.hidden = shown !== 0;
      if (apiCount) apiCount.textContent = shown + " / " + visibleTotal;
    }

    if (apiFilter) apiFilter.addEventListener("input", applyFilter);

    if (chipsRoot) {
      chipsRoot.addEventListener("click", function (event) {
        var chip = event.target.closest(".chip");
        if (!chip) return;
        activeGroup = chip.getAttribute("data-group");
        chipsRoot.querySelectorAll(".chip").forEach(function (item) {
          item.classList.toggle("is-active", item === chip);
        });
        applyFilter();
      });
    }

    applyFilter();

    if (window.location.hash) {
      var target = document.getElementById(window.location.hash.slice(1));
      if (target) window.setTimeout(function () { target.scrollIntoView(); }, 60);
    }
  }

  var v1Root = document.getElementById("v1-endpoints");

  if (v1Root && window.RUNECORD_V1) {
    var v1Html =
      '<div class="table-wrap"><table><thead><tr><th>Method</th><th>Path</th><th>Purpose</th></tr></thead><tbody>';
    window.RUNECORD_V1.forEach(function (endpoint) {
      v1Html +=
        '<tr id="' + slugFor(endpoint.m, endpoint.p) + '">' +
        "<td><code>" + endpoint.m + "</code></td>" +
        "<td><code>" + endpoint.p + "</code></td>" +
        "<td>" + endpoint.d + "</td></tr>";
    });
    v1Html += "</tbody></table></div>";
    v1Root.innerHTML = v1Html;

    if (window.location.hash) {
      var v1Target = document.getElementById(window.location.hash.slice(1));
      if (v1Target) {
        window.setTimeout(function () {
          v1Target.scrollIntoView();
        }, 60);
      }
    }
  }

  /* ---------- Table of contents ---------- */

  var tocList = document.getElementById("toc-list");
  var docContent = document.querySelector(".doc-content");

  if (tocList && docContent) {
    var headings = docContent.querySelectorAll("h2[id], h3[id]");
    var tocHtml = "";

    headings.forEach(function (heading) {
      tocHtml +=
        '<a href="#' + heading.id + '" class="' + (heading.tagName === "H3" ? "toc-h3" : "toc-h2") + '">' +
        heading.textContent + "</a>";
    });

    tocList.innerHTML = tocHtml;

    var links = Array.prototype.slice.call(tocList.querySelectorAll("a"));
    if ("IntersectionObserver" in window && links.length) {
      var spy = new IntersectionObserver(
        function (entries) {
          entries.forEach(function (entry) {
            if (!entry.isIntersecting) return;
            links.forEach(function (link) {
              link.classList.toggle("is-active", link.getAttribute("href") === "#" + entry.target.id);
            });
          });
        },
        { rootMargin: "-90px 0px -72% 0px", threshold: 0 }
      );

      headings.forEach(function (heading) {
        spy.observe(heading);
      });
    }
  }

  /* ---------- Search ---------- */

  var dialog = document.getElementById("search-dialog");
  var dialogInput = document.getElementById("search-input");
  var resultsRoot = document.getElementById("search-results");
  var searchOpeners = document.querySelectorAll("[data-search-open]");
  var searchClose = document.querySelector(".search-close");

  var docIndex = [
    { t: "Overview", s: "Developer platform", h: "index.html", k: "home portal start base url" },
    { t: "Getting started", s: "Docs", h: "getting-started.html", k: "install run server login token curl first request bot" },
    { t: "Authentication", s: "Docs", h: "authentication.html", k: "bearer bot token session login register logout scopes" },
    { t: "HTTP API reference", s: "Reference", h: "api-reference.html", k: "endpoints routes methods rest" },
    { t: "Gateway", s: "Reference", h: "gateway.html", k: "websocket events heartbeat identify resume intents ready" },
    { t: "Bots & applications", s: "Build", h: "bots.html", k: "bot token commands interactions webhooks apps" },
    { t: "Discord-compatible API", s: "Reference", h: "discord-compat.html", k: "v1 discord payloads errors snowflakes" },
    { t: "Rate limits", s: "Authentication", h: "authentication.html#rate-limits", k: "429 headers retry bucket global" },
    { t: "Permissions", s: "Authentication", h: "authentication.html#permissions", k: "bitfield role overwrites administrator" },
    { t: "Webhooks", s: "Bots & applications", h: "bots.html#webhooks", k: "execute webhook token channel" },
    { t: "Interactions", s: "Bots & applications", h: "bots.html#interactions", k: "command callback application" }
  ];

  if (window.RUNECORD_API) {
    window.RUNECORD_API.groups.forEach(function (group) {
      group.endpoints.forEach(function (endpoint) {
        docIndex.push({
          t: endpoint.m + " " + endpoint.p,
          s: group.name,
          h: "api-reference.html#" + slugFor(endpoint.m, endpoint.p),
          k: endpoint.d + " " + endpoint.a,
          api: true
        });
      });
    });
  }

  if (window.RUNECORD_V1) {
    window.RUNECORD_V1.forEach(function (endpoint) {
      docIndex.push({
        t: endpoint.m + " " + endpoint.p,
        s: "Discord-compatible API",
        h: "discord-compat.html#" + slugFor(endpoint.m, endpoint.p),
        k: endpoint.d,
        api: true
      });
    });
  }

  var activeResult = 0;
  var renderedResults = [];

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function renderResults(query) {
    if (!resultsRoot) return;
    var q = query.trim().toLowerCase();
    var matches;

    if (!q) {
      matches = docIndex.slice(0, 7);
    } else {
      matches = docIndex
        .filter(function (item) {
          return (item.t + " " + item.s + " " + (item.k || "")).toLowerCase().indexOf(q) !== -1;
        })
        .slice(0, 12);
    }

    renderedResults = matches;
    activeResult = 0;

    if (!matches.length) {
      resultsRoot.innerHTML = '<div class="search-empty">No results for “' + escapeHtml(query) + '”.</div>';
      return;
    }

    resultsRoot.innerHTML = matches
      .map(function (item, index) {
        return (
          '<a class="search-result' + (index === 0 ? " is-active" : "") + '" href="' + item.h + '">' +
          '<span class="search-type">' + (item.api ? "API" : "Doc") + "</span>" +
          "<div><strong>" + escapeHtml(item.t) + "</strong><small>" + escapeHtml(item.s) + "</small></div></a>"
        );
      })
      .join("");
  }

  function openSearch() {
    if (!dialog) return;
    renderResults("");
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    if (dialogInput) {
      dialogInput.value = "";
      window.setTimeout(function () { dialogInput.focus(); }, 40);
    }
  }

  function closeSearch() {
    if (!dialog) return;
    if (typeof dialog.close === "function") dialog.close();
    else dialog.removeAttribute("open");
  }

  searchOpeners.forEach(function (button) {
    button.addEventListener("click", openSearch);
  });

  if (searchClose) searchClose.addEventListener("click", closeSearch);

  if (dialog) {
    dialog.addEventListener("click", function (event) {
      if (event.target === dialog) closeSearch();
    });
  }

  document.addEventListener("keydown", function (event) {
    var tag = (event.target.tagName || "").toLowerCase();
    var typing = tag === "input" || tag === "textarea" || event.target.isContentEditable;

    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      openSearch();
    } else if (event.key === "/" && !typing) {
      event.preventDefault();
      openSearch();
    } else if (dialog && dialog.open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault();
      if (!renderedResults.length) return;
      activeResult = (activeResult + (event.key === "ArrowDown" ? 1 : -1) + renderedResults.length) % renderedResults.length;
      var rows = resultsRoot.querySelectorAll(".search-result");
      rows.forEach(function (row, index) {
        row.classList.toggle("is-active", index === activeResult);
      });
      if (rows[activeResult]) rows[activeResult].scrollIntoView({ block: "nearest" });
    } else if (dialog && dialog.open && event.key === "Enter" && renderedResults[activeResult]) {
      window.location.href = renderedResults[activeResult].h;
    }
  });

  if (dialogInput) {
    dialogInput.addEventListener("input", function () {
      renderResults(dialogInput.value);
    });
  }

  /* ---------- Docs search links ---------- */

  document.querySelectorAll("[data-doc-search]").forEach(function (button) {
    button.addEventListener("click", function () {
      openSearch();
      if (dialogInput) {
        dialogInput.value = button.getAttribute("data-doc-search") || "";
        renderResults(dialogInput.value);
      }
    });
  });

  /* ---------- Year ---------- */

  var year = document.getElementById("year");
  if (year) year.textContent = String(new Date().getFullYear());
})();
