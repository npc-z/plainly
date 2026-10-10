/* The main window's frontend.
 *
 * A renderer, not a second implementation: every list, every setting and every
 * word comes from the backend (`window`, `settings`, `strings`), and this file
 * decides only *how* they are drawn. It owns no sentence of its own — not even
 * a button label — because the interface's copy lives in one Rust module
 * (`strings.rs`) so that the interface can be translated without reading
 * JavaScript (spec §12).
 *
 * The one thing it decides for itself is the local calendar: which rows belong
 * under 今天, which only the webview knows, because only it knows this machine's
 * offset from UTC.
 *
 * No framework and no bundler: the screen is a list and a form, and a build step
 * would be a second toolchain to keep working for it.
 */

(() => {
  "use strict";

  /* Tauri's `withGlobalTauri` puts `invoke` on the window. When it is missing —
     opening `ui/index.html` in a browser — the page says so rather than drawing
     an interface that could never work. */
  const invoke = (window.__TAURI__ && window.__TAURI__.core)
    ? window.__TAURI__.core.invoke
    : null;

  const app = {
    strings: null,
    settingsStrings: null,
    screen: "history",
    history: null,
    settings: null,
    detail: null,
    selected: null,
    settingsSection: 0,
    now: 0,
  };

  /* ── helpers ──────────────────────────────────────────────────────── */

  const esc = (value) => String(value == null ? "" : value)
    .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

  const el = (id) => document.getElementById(id);

  /** The local calendar day of an instant, as the date a person would read.
   *
   *  Two instants on the same local day share it, which is what the 今天 /
   *  昨天 / 更早 grouping needs and what UTC cannot answer. Written from
   *  `Date`'s own local getters rather than by shifting the instant by an
   *  offset: `getTimezoneOffset()` is minutes *behind* UTC, its sign is the
   *  opposite of the one arithmetic wants, and a grouping that gets that wrong
   *  puts the day boundary in the middle of an afternoon. A string of the local
   *  date cannot be wrong in that way. */
  function localDay(seconds) {
    const date = new Date(seconds * 1000);
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${date.getFullYear()}-${month}-${day}`;
  }

  /** A moment, as the sidebar shows it: the time today, 昨天 plus the time
   *  yesterday, and the date further back. */
  function when(seconds) {
    const date = new Date(seconds * 1000);
    const hhmm = String(date.getHours()).padStart(2, "0") + ":" + String(date.getMinutes()).padStart(2, "0");
    const day = localDay(seconds);
    if (day === localDay(app.now)) return hhmm;
    if (day === localDay(app.now - 86_400)) return app.strings.group_yesterday + " " + hhmm;
    const mm = String(date.getMonth() + 1).padStart(2, "0");
    const dd = String(date.getDate()).padStart(2, "0");
    return mm + "-" + dd + " " + hhmm;
  }

  /** Say something for a moment, in the top bar rather than in a dialog: the
   *  window has no dialog of its own, and every message here is one sentence. */
  function say(message, bad) {
    const banner = el("banner");
    banner.textContent = message;
    banner.className = "banner" + (bad ? " bad" : "");
    banner.hidden = false;
    clearTimeout(say.timer);
    say.timer = setTimeout(() => { banner.hidden = true; }, bad ? 12000 : 6000);
  }

  /** Run a command, putting its failure in the banner rather than in the
   *  console: an error nobody sees is a button that appears to do nothing. */
  async function call(name, args) {
    if (!invoke) throw new Error(app.strings ? app.strings.outside_window : "no Tauri host");
    try {
      return await invoke(name, args || {});
    } catch (error) {
      // The banner is part of the interface, so a failure says so in the
      // interface's own words: the reason travels from the backend after the
      // colon. The one case with no copy yet is the very first call, before the
      // strings have arrived.
      say(app.strings ? app.strings.error_prefix + String(error) : String(error), true);
      throw error;
    }
  }

  /** Keep 今天 / 昨天 honest in a window that stays open across midnight.
   *
   *  The grouping is decided here from `app.now`, which arrived when the window
   *  opened. A window left open overnight would otherwise file this morning's
   *  records under yesterday — so the clock is refreshed whenever the list is
   *  about to change, and a new `now` is what makes the grouping re-decide. */
  function refreshClock() {
    app.now = Math.floor(Date.now() / 1000);
  }

  /** The store is shared: the panel and the CLI write to it while this window
   *  sits here, so coming back to the front is when the list is worth re-reading.
   *  It is the same fact the store's WAL journal exists for (spec §9), and it is
   *  why this window has no reason to own the data. */
  function watchForChanges() {
    window.addEventListener("focus", () => {
      reload().catch(() => {
        // A reload that failed has already said so in the banner; the window
        // stays as it was rather than emptying itself.
      });
    });
  }

  /* ── the shell ────────────────────────────────────────────────────── */

  /** The top bar: the window's name and its two tabs.
   *
   *  A function of its own because the broken-state screen draws before any
   *  `renderShell()` and still has to wear the same chrome — a window with two
   *  blank, dead tabs would look like a second failure. */
  function renderTopBar() {
    const t = app.strings;
    document.title = t.title;
    el("app-name").textContent = t.title;
    el("tab-history").textContent = t.tab_history;
    el("tab-settings").textContent = t.tab_settings;

    // The tabs are drawn whatever the state is, but they only *do* something
    // when there is a state to switch to: on the failure screen both screens
    // would throw, and a tab that looks live and is not is worse than a tab
    // that plainly is not.
    const usable = !!app.settings && !!app.history;
    for (const tab of document.querySelectorAll(".tbar nav a")) {
      tab.classList.toggle("on", usable && tab.dataset.screen === app.screen);
      tab.onclick = usable ? () => showScreen(tab.dataset.screen) : null;
    }
  }

  function renderShell() {
    // The grouping is the only thing in this window that depends on what day it
    // is, and a window left open overnight would otherwise keep last night's
    // 今天 / 昨天 boundary until the next search. Reading the clock on every
    // render costs one `Date.now()` and removes the question.
    refreshClock();

    // Re-rendering the list replaces the search box, so the caret a person is
    // typing with has to be put back: a debounced search fires while they are
    // still typing, and losing focus there means the next keystroke goes to the
    // window's default handler and disappears.
    const typed = document.activeElement === el("q") ? el("q") : null;
    const caret = typed ? [typed.selectionStart, typed.selectionEnd] : null;

    renderTopBar();
    el("body").innerHTML = app.screen === "settings" ? settingsScreen() : historyScreen();
    if (app.screen === "history") bindHistory(); else bindSettings();

    // Put the caret back where it was, including the part typed past the point
    // the render happened.
    if (typed) {
      const again = el("q");
      if (again) {
        again.focus();
        const start = Math.min(caret[0], again.value.length);
        const end = Math.min(caret[1], again.value.length);
        again.setSelectionRange(start, end);
      }
    }
  }

  function showScreen(screen) {
    app.screen = screen;
    // The first run opens on the Provider section (`settings_section` from the
    // backend) because that is where the empty state and its two ways out are,
    // and a tab round trip must not quietly move the person off it. Any other
    // time the section list starts at the top.
    if (screen === "settings" && !app.settings.provider.first_run) {
      app.settingsSection = 0;
    }
    renderShell();
  }

  /* ── history ──────────────────────────────────────────────────────── */

  function historyScreen() {
    const t = app.strings;
    const h = app.history;

    const chips = [chip(t.filter_all, h.tag === null, "tag", "")]
      .concat(h.tags.map((tag) => chip(tag, h.tag === tag, "tag", tag)))
      .join("");

    const rows = h.records.length
      ? grouped(h.records)
      : '<div class="empty">' + esc(h.has_records ? t.empty_search : t.empty_history) + "</div>";

    return `
      <div class="list">
        <div class="search">
          <input id="q" type="text" value="${esc(h.query)}" placeholder="${esc(t.search_placeholder)}">
          <div class="scope">${esc(t.search_scope_note)}</div>
        </div>
        <div class="chips">${chips}</div>
        ${rows}
      </div>
      <div class="detail">${app.detail ? detailView(app.detail) : '<p class="note">' + esc(t.select_a_record) + "</p>"}</div>`;
  }

  function chip(label, on, kind, value) {
    return `<span class="chip${on ? " on" : ""}" data-${kind}="${esc(value)}">${esc(label)}</span>`;
  }

  /** The rows, under 今天 / 昨天 / 更早.
   *
   * Each record is claimed by exactly one group — the first whose test it
   * passes — so the three groups partition the list rather than each taking a
   * copy of it. Order inside a group is the history's own: most recent first. */
  function grouped(records) {
    const t = app.strings;
    const today = localDay(app.now);
    const yesterday = localDay(app.now - 86_400);
    const groups = [
      { title: t.group_today, test: (day) => day === today },
      { title: t.group_yesterday, test: (day) => day === yesterday },
      { title: t.group_earlier, test: () => true },
    ];

    const claimed = new Set();
    const out = [];
    for (const group of groups) {
      const rows = records.filter((record) => !claimed.has(record.id) && group.test(localDay(record.last_seen)));
      if (!rows.length) continue;
      for (const row of rows) claimed.add(row.id);
      out.push(`<div class="group">${esc(group.title)}</div>`);
      out.push(rows.map(rowView).join(""));
    }
    return out.join("");
  }

  /** One row: the Passage, then everything spec §13 says a row has to show —
   *  provider, model, Level, the prompt version, the Tags and the moment. The
   *  version is the one that matters most: without it, the same Passage under
   *  two prompts reads as a duplicate. Thinking is not a row fact — it is two
   *  of the same model's answers with different budgets, not two records — and
   *  it is shown in the detail's provenance line instead. */
  function rowView(record) {
    const tags = record.tags.map((tag) => `<span class="tag" data-rowtag="${esc(tag)}">${esc(tag)}</span>`).join("");
    return `
      <div class="row${record.id === app.selected ? " on" : ""}" data-record="${record.id}">
        <div class="txt">${esc(record.snippet)}</div>
        <div class="meta">
          <span class="tag${record.local ? " local" : ""}">${esc(record.provider)} / ${esc(record.model)}</span>
          <span>${esc(record.level)}</span>
          <span title="${esc(record.prompt_version)}">${esc(record.prompt_label)} · ${esc(record.prompt_short)}</span>
          <span title="${esc(record.last_seen_utc)}">· ${esc(when(record.last_seen))}</span>
        </div>
        ${tags ? `<div class="tags">${tags}</div>` : ""}
      </div>`;
  }

  function detailView(record) {
    const t = app.strings;
    const glosses = record.glosses.length
      ? record.glosses.map((gloss) =>
          `<p class="gl"><code>${esc(gloss.expression)}</code> → ${esc(gloss.gloss)}</p>`).join("")
      : `<p class="note">${esc(t.glosses_empty)}</p>`;

    return `
      <h2>${esc(record.level)}</h2>
      <div class="meta">
        <span>${esc(record.provider)} / ${esc(record.model)}</span>
        <span>${esc(app.settingsStrings.thinking_label)} ${esc(record.thinking === "on" ? app.settingsStrings.thinking_on_short : app.settingsStrings.thinking_off_short)}</span>
        <span>${esc(record.level)}</span>
        <span>${esc(record.prompt_label)} · <span title="${esc(record.prompt_version)}">${esc(record.prompt_short)}</span></span>
        <span>· ${esc(when(record.last_seen_unix))}</span>
        ${record.local ? `<span class="tag local">${esc(t.local_badge)}</span>` : ""}
      </div>
      <h3>${esc(t.detail_original)}</h3>
      <div class="orig">${esc(record.passage)}</div>
      <h3>${esc(t.detail_comprehensible)}</h3>
      <p class="comp">${esc(record.comprehensible)}</p>
      <h3>${esc(t.detail_glosses)}</h3>
      ${glosses}
      <h3>${esc(t.detail_grammar)}</h3>
      ${record.grammar
        ? `<div class="grammar">${esc(record.grammar)}</div>`
        : `<p class="notneeded">${esc(t.grammar_not_needed)}</p>`}
      <h3>${esc(t.detail_translation)}</h3>
      <p class="zh">${esc(record.translation)}</p>
      <p class="note">${esc(t.provenance)}：${esc(record.provider)}/${esc(record.model)} · ${esc(record.source_language)}→${esc(record.native_language)} · ${esc(record.prompt_label)} · ${esc(app.settingsStrings.provenance_format)} v${record.artifact_version} · ${esc(app.settingsStrings.provenance_created)} ${esc(record.created_at)} · ${esc(app.settingsStrings.provenance_generated)} ${esc(record.generated_at)} · ${esc(app.settingsStrings.provenance_seen)} ${esc(record.last_seen)}</p>
      ${record.local ? `<div class="warnbox">${esc(t.local_provider_note)}</div>` : ""}
      <div class="actions">
        <button class="pri" data-act="copy" data-id="${record.id}">${esc(t.copy_markdown)}</button>
        <button data-act="export" data-id="${record.id}">${esc(t.export_this)}</button>
        <button data-act="delete" data-id="${record.id}" class="danger">${esc(t.delete_record)}</button>
      </div>
      <div class="field">
        <label>${esc(t.add_tag)}</label>
        <input id="newtag" type="text" placeholder="${esc(t.tag_placeholder)}">
      </div>
      <div class="tags">${record.tags.map((tag) =>
        `<span class="tag" data-untag="${esc(tag)}" title="${esc(app.settingsStrings.remove_tag)}">${esc(tag)} ×</span>`).join("")}</div>
      <p class="note">${esc(t.prompt_version_note)}</p>`;
  }

  function bindHistory() {
    const input = el("q");
    if (input) {
      input.oninput = () => {
        clearTimeout(bindHistory.timer);
        bindHistory.timer = setTimeout(() => search(input.value, app.history.tag), 150);
      };
      input.onkeydown = (event) => {
        if (event.key === "Enter") {
          clearTimeout(bindHistory.timer);
          search(input.value, app.history.tag);
        }
      };
    }

    for (const node of document.querySelectorAll("[data-tag]")) {
      node.onclick = () => {
        const value = node.dataset.tag;
        search(app.history.query, app.history.tag === value || value === "" ? null : value);
      };
    }

    for (const row of document.querySelectorAll(".row[data-record]")) {
      row.onclick = () => select(Number(row.dataset.record));
    }

    for (const tag of document.querySelectorAll("[data-rowtag]")) {
      tag.onclick = (event) => {
        event.stopPropagation();
        search(app.history.query, tag.dataset.rowtag);
      };
    }

    for (const button of document.querySelectorAll("[data-act]")) {
      button.onclick = () => act(button.dataset.act, Number(button.dataset.id));
    }

    const newtag = el("newtag");
    if (newtag) {
      newtag.onkeydown = (event) => {
        if (event.key !== "Enter" || !newtag.value.trim()) return;
        addTag(newtag.value.trim());
      };
    }
    for (const tag of document.querySelectorAll("[data-untag]")) {
      tag.onclick = () => untag(tag.dataset.untag);
    }
  }

  async function search(query, tag) {
    app.history = await call("search", { query, tag: tag || null });
    renderShell();
  }

  async function select(id) {
    app.selected = id;
    app.detail = await call("show", { id });
    renderShell();
    const row = document.querySelector(`.row[data-record="${id}"]`);
    if (row) row.scrollIntoView({ block: "nearest" });
  }

  async function act(action, id) {
    if (action === "copy") {
      await call("copy_markdown", { id });
      say(app.strings.copied);
      return;
    }
    if (action === "export") {
      // Markdown, which is what the button says and what the checklist checks:
      // `[app].export_format` belongs to the configuration file — v0's CLI takes
      // `--format` and never reads it — and it is not this button's control.
      const offer = await call("export", { format: "markdown", id });
      say(offer.summary + " → " + offer.path);
      return;
    }
    if (action === "delete") {
      if (!confirm(app.strings.delete_record_confirm)) return;
      app.history = await call("delete", { id });
      if (app.selected === id) { app.selected = null; app.detail = null; }
      renderShell();
    }
  }

  async function addTag(tag) {
    app.history = await call("tag", { id: app.selected, tag });
    app.detail = await call("show", { id: app.selected });
    renderShell();
  }

  async function untag(tag) {
    app.history = await call("untag", { id: app.selected, tag });
    // A Tag that was narrowing the list cannot go on narrowing it once the last
    // Record carrying it has lost it: the chip row would draw no active chip
    // while the list stayed filtered, and the only way out would be a control
    // the person can no longer see they are inside.
    if (app.history.tag === tag) {
      app.history = await call("search", { query: app.history.query, tag: null });
    }
    app.detail = await call("show", { id: app.selected });
    renderShell();
  }

  /* ── settings ─────────────────────────────────────────────────────── */

  function settingsScreen() {
    const rows = app.settings.sections.map((title, index) =>
      `<div class="row${index === app.settingsSection ? " on" : ""}" data-section="${index}">${esc(title)}</div>`).join("");

    return `
      <div class="list"><div class="sections">${rows}</div></div>
      <div class="detail">${settingsBody()}</div>`;
  }

  function settingsBody() {
    switch (app.settingsSection) {
      case 0: return levelsBody();
      case 1: return providersBody();
      case 2: return promptBody();
      case 3: return behaviorBody();
      default: return dataBody();
    }
  }

  /** The heading of one section, from the backend's own list. */
  const heading = (index) => app.settings.sections[index];

  /** A patch, sent and then reflected: the backend answers with the whole
   *  settings screen, so a value that did not stick is visible immediately. */
  async function patch(values) {
    try {
      app.settings = await call("update", { patch: values });
      renderShell();
      say(app.strings.saved);
    } catch (error) {
      // The write was refused, so the control still shows what was typed while
      // the file still holds what was there *or* something somebody else wrote.
      // Re-reading rather than redrawing from memory: the whole point of a
      // refusal is that this window's copy is the stale one. The banner has
      // already explained why.
      app.settings = await call("settings");
      renderShell();
    }
  }

  function option(value, label, selected) {
    const on = String(value) === String(selected);
    return `<option value="${esc(value)}"${on ? " selected" : ""}>${esc(label)}</option>`;
  }

  function levelsBody() {
    const s = app.settingsStrings;
    const level = app.settings.levels;
    const options = level.options.map((item) => option(item.value, item.label, level.level)).join("");
    const meaning = level.options.find((item) => item.value === level.level);
    const overridden = meaning && meaning.overridden ? s.overridden : "";

    return `
      <h2>${esc(heading(0))}</h2>
      <div class="field">
        <label>${esc(s.level)}</label>
        <select id="level">${options}</select>
        <div class="meaning">${esc(meaning ? meaning.meaning : "")}${esc(overridden)}</div>
        <p class="note">${esc(s.level_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.native_language)}</label>
        <input id="native" type="text" value="${esc(level.native_language)}" list="natives">
        <datalist id="natives">${level.native_language_options.map((name) => `<option value="${esc(name)}">`).join("")}</datalist>
        <p class="note">${esc(s.native_language_note)}</p>
      </div>
`;
  }

  function providersBody() {
    const s = app.settingsStrings;
    const t = app.strings;
    const provider = app.settings.provider;
    const listed = provider.options.map((item) =>
      option(item.name, item.label + (item.local ? s.provider_local : s.provider_cloud), provider.configured));

    // A configured provider name that is no preset — its own `[providers.<name>]`
    // table is the whole definition — is still the one in effect, and a select
    // that could not show it would silently claim another provider was selected.
    if (!provider.options.some((item) => item.name === provider.configured)) {
      listed.unshift(option(provider.configured, provider.configured, provider.configured));
    }
    const options = listed.join("");

    const current = provider.current;
    const currentBlock = current ? `
      <div class="field">
        <label>${esc(s.model)}</label>
        <input id="model" type="text" value="${esc(current.model)}">
        <p class="note">${esc(s.model_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.thinking)}</label>
        <select id="thinking">
          ${option("off", s.thinking_off, current.thinking)}
          ${option("on", s.thinking_on, current.thinking)}
        </select>
        ${current.thinking_supported ? "" : `<p class="note">${esc(s.thinking_unsupported)}</p>`}
        <p class="note">${esc(t.thinking_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.key)}</label>
        <div class="keytext">${esc(current.key.text)}</div>
        <div class="inline" style="margin-top:6px">
          <input id="key" type="password" placeholder="${esc(current.key.variable)}">
          <button id="store-key">${esc(s.key_save)}</button>
          <button id="clear-key">${esc(s.key_delete)}</button>
        </div>
        <p class="note">${esc(s.key_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.provider_in_use)}</label>
        <div class="keytext">${esc(current.label)} · ${esc(current.endpoint)} · ${esc(current.model)}${current.local ? " · " + esc(s.provider_local) : ""}</div>
      </div>` : `
      <div class="field">
        <label class="bad">${esc(provider.current_problem || "")}</label>
      </div>`;

    const empty = provider.first_run ? `
      <div class="warnbox">
        <b>${esc(t.first_run_title)}</b>
        <p class="note" style="margin:6px 0 0">${esc(t.first_run_body)}</p>
        <p class="note" style="margin:6px 0 0">${esc(t.first_run_cloud)}</p>
        <p class="note" style="margin:6px 0 0">${esc(t.first_run_local)}</p>
      </div>` : "";

    return `
      <h2>${esc(heading(1))}</h2>
      ${empty}
      <div class="field">
        <label>${esc(s.provider_configured)}</label>
        <select id="provider">${options}</select>
        <p class="note">${esc(s.provider_configured_note)}</p>
      </div>
      ${currentBlock}`;
  }

  function promptBody() {
    const s = app.settingsStrings;
    const prompt = app.settings.prompt;

    return `
      <h2>${esc(heading(2))}</h2>
      <div class="field">
        <label>${esc(s.prompt_version)}</label>
        <div class="keytext">${esc(prompt.label)} · <code class="path">${esc(prompt.version)}</code></div>
        <p class="note">${esc(s.prompt_version_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.prompt_appendix)}</label>
        <textarea id="appendix">${esc(prompt.appendix)}</textarea>
        <p class="note">${esc(s.prompt_appendix_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.prompt_factory)}</label>
        <textarea readonly>${esc(prompt.factory_text)}</textarea>
      </div>
      <div class="field">
        <label>${esc(s.prompt_meanings)}</label>
        ${prompt.level_meanings.map((item) =>
          `<div class="keytext" style="margin-bottom:4px"><b>${esc(item.label)}</b> — ${esc(item.meaning)}${item.overridden ? esc(s.overridden) : ""}</div>`).join("")}
        <p class="note">${esc(s.prompt_meanings_note)}</p>
      </div>`;
  }

  function behaviorBody() {
    const s = app.settingsStrings;
    const t = app.strings;
    const b = app.settings.behavior;

    const corners = b.panel_corners.map(([value, label]) => option(value, label, b.panel_corner)).join("");
    const listed = b.outputs.map((item) => option(item.value, item.label, b.panel_display));

    // A stored output name this session cannot enumerate — a connector name
    // written by another build or by hand, which is the spelling
    // `default_config.toml` documents — is still what the file says, and a
    // select that could not show it would quietly claim "跟随当前显示器".
    if (!b.outputs.some((item) => item.value === b.panel_display)) {
      listed.unshift(option(b.panel_display, b.panel_display, b.panel_display));
    }
    const outputs = listed.join("");
    const sections = [
      ["show_original", 0], ["show_comprehensible", 1], ["show_glosses", 2],
      ["show_grammar", 3], ["show_translation", 4],
    ];

    return `
      <h2>${esc(heading(3))}</h2>
      <p class="note">${esc(b.panel_settings_honoured ? "" : s.panel_settings_note)}</p>
      <div class="field">
        <label>${esc(s.copy_auto)}</label>
        <div class="inline">
          <input id="copy-auto" type="checkbox" ${b.copy_auto_popup ? "checked" : ""} ${b.copy_auto_popup_availability.enabled ? "" : "disabled"}>
          <span class="note">${esc(b.copy_auto_popup_availability.note)}</span>
        </div>
        ${b.copy_auto_popup_honoured ? "" : `<p class="note">${esc(t.copy_auto_not_honoured)}</p>`}
      </div>
      <div class="field">
        <label>${esc(s.panel_corner)}</label>
        <select id="corner">${corners}</select>
        <p class="note">${esc(s.panel_corner_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.panel_display)}</label>
        <select id="display">${outputs}</select>
      </div>
      <div class="field">
        <label>${esc(s.panel_autohide)}</label>
        <div class="inline">
          <input id="autohide" type="checkbox" ${b.panel_autohide ? "checked" : ""}>
          <input id="autohide-seconds" type="number" min="${b.autohide_min}" max="${b.autohide_max}" value="${b.panel_autohide_seconds}" ${b.panel_autohide ? "" : "disabled"}>
          <span>${esc(s.seconds)}</span>
        </div>
        <p class="note">${esc(s.panel_autohide_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.panel_sections)}</label>
        ${sections.map(([field, index]) =>
          `<label class="inline" style="font-weight:400;margin:2px 0">
             <input type="checkbox" data-section-field="${field}" ${b.sections[field.replace("show_", "")] ? "checked" : ""}>
             <span>${esc(s.sections[index])}</span>
           </label>`).join("")}
      </div>`;
  }

  function dataBody() {
    const s = app.settingsStrings;
    const t = app.strings;
    const d = app.settings.data;

    return `
      <h2>${esc(heading(4))}</h2>
      <div class="field">
        <label>${esc(s.export_all)}</label>
        <div class="actions">
          ${d.exports.map((item) =>
            `<button data-export="${esc(item.value)}">${esc(item.label)}</button>`).join("")}
        </div>
        <p class="note">${esc(s.export_all_note)}</p>
      </div>
      <div class="field">
        <label>${esc(s.data_dir)}</label>
        <code class="path">${esc(d.data_dir)}</code>
        <div class="actions"><button id="open-data">${esc(s.open_data_dir)}</button></div>
      </div>
      <div class="field">
        <label>${esc(s.files)}</label>
        <div class="keytext">${esc(s.files_config)}：<code class="path">${esc(d.config_file)}</code></div>
        <div class="keytext">${esc(s.files_history)}：<code class="path">${esc(d.history_file)}</code></div>
      </div>
      <div class="field">
        <label class="bad">${esc(s.danger)}</label>
        <div class="actions"><button id="clear-history" class="danger">${esc(t.clear_history)}</button></div>
        <p class="note">${esc(s.danger_note)}</p>
      </div>`;
  }

  function bindSettings() {
    for (const row of document.querySelectorAll("[data-section]")) {
      row.onclick = () => {
        app.settingsSection = Number(row.dataset.section);
        renderShell();
      };
    }

    const on = (id, event, handler) => {
      const node = el(id);
      if (node) node[event] = handler;
    };

    on("level", "onchange", (event) => patch({ level: event.target.value }));
    on("native", "onchange", (event) => patch({ native_language: event.target.value }));
    on("provider", "onchange", (event) => patch({ provider: event.target.value }));
    on("model", "onchange", (event) => patch({ model: event.target.value }));
    on("thinking", "onchange", (event) => patch({ thinking: event.target.value }));
    on("appendix", "onchange", (event) => patch({ appendix: event.target.value }));
    on("copy-auto", "onchange", (event) => patch({ copy_auto_popup: event.target.checked }));
    on("corner", "onchange", (event) => patch({ panel_corner: event.target.value }));
    on("display", "onchange", (event) => patch({ panel_display: event.target.value }));
    on("autohide", "onchange", (event) => patch({ panel_autohide: event.target.checked }));
    on("autohide-seconds", "onchange", (event) => patch({ panel_autohide_seconds: Number(event.target.value) }));

    on("store-key", "onclick", async () => {
      const field = el("key");
      if (!field.value) return;
      const outcome = await call("store_key", { provider: app.settings.provider.current.name, secret: field.value });
      field.value = "";
      await reload();
      // A key that never reached the keyring is announced in the warning colour:
      // it works for this window and no longer.
      say(outcome.message, !outcome.durable);
    });

    on("clear-key", "onclick", async () => {
      const outcome = await call("clear_key", { provider: app.settings.provider.current.name });
      await reload();
      say(outcome.message, !outcome.durable);
    });

    for (const field of document.querySelectorAll("[data-section-field]")) {
      field.onchange = () => patch({ [field.dataset.sectionField]: field.checked });
    }

    for (const button of document.querySelectorAll("[data-export]")) {
      button.onclick = async () => {
        const offer = await call("export", { format: button.dataset.export, id: null });
        say(offer.summary + " → " + offer.path);
      };
    }

    on("open-data", "onclick", async () => {
      // The data section names the path either way, so a desktop that would not
      // open it is reported as what it is: a note beside the directory the
      // person can still open by hand, in the warning colour.
      const reply = await call("open_data_dir");
      say(reply.message, !reply.opened);
    });

    on("clear-history", "onclick", async () => {
      if (!confirm(app.strings.clear_history_confirm)) return;
      const reply = await call("clear");
      app.history = reply.history;
      app.selected = null;
      app.detail = null;
      say(app.settingsStrings.clear_done.replace("{removed}", reply.removed));
      renderShell();
    });
  }

  /** Re-read everything the window shows.
   *
   *  Used after the two actions that are not a settings patch — storing or
   *  clearing a key, and the window coming back to the front — because both can
   *  change more than the settings screen: storing the first key ends the first
   *  run, and the History screen's whole point is that the panel and the CLI are
   *  writing to the same store. */
  async function reload() {
    // A focus can arrive between the listener being bound and the first state
    // being adopted — the browser is free to deliver it whenever the window is
    // raised — and there is nothing to reload before then.
    if (!app.settings || !app.history) return;

    const state = await call("window");

    // The screen is *not* adopted on every reload: the backend's answer is where
    // the window should *open*, and the person may since have navigated — to
    // Settings, or to another of its sections. The one thing that does move is
    // the first-run boundary, in whichever direction it moved: leaving it is
    // what storing the first key does, and that is a change of what the window
    // should show rather than a navigation the person made.
    if (app.settings.provider.first_run !== state.settings.provider.first_run) {
      app.screen = state.screen;
      app.settingsSection = state.settings_section;
    }

    app.now = state.now;
    app.settings = state.settings;

    // The list is re-read through the same search the list is already showing:
    // the *window's* history is the whole history, and a window that had a query
    // typed into it would lose it — and show more rows than it should — if the
    // focus refresh ignored what the person had asked for.
    //
    // A Tag is sent only while the history still carries it: the Tag the filter
    // is on can be taken off another surface, and re-sending it would pin the
    // window to a filter nothing can satisfy, with only 全部 as a way out.
    const query = app.history.query;
    const tag = app.history.tag && state.history.tags.includes(app.history.tag)
      ? app.history.tag
      : null;
    app.history = await call("search", { query, tag });

    if (app.selected !== null && app.history.records.some((row) => row.id === app.selected)) {
      // The Record is still here, but it may not be the same Explanation: the
      // store overwrites a Record in place when the same question is asked again
      // (spec §9), so a regeneration from the panel would otherwise leave the
      // row's version new and the pane beside it old.
      app.detail = await call("show", { id: app.selected });
    } else if (app.selected !== null) {
      // The Record the detail was showing is gone — deleted from another
      // surface, or by a clear — so the pane goes back to asking for a choice
      // rather than showing something the list no longer has.
      app.selected = null;
      app.detail = null;
    }
    renderShell();
  }

  /* ── start ────────────────────────────────────────────────────────── */

  async function start() {
    // The copy is fetched before anything else, and before the host is checked
    // for: `strings` needs no window state, so a page opened outside the window
    // can still be told in the interface's own words rather than in a sentence
    // written here — this file's whole contract is that it owns none.
    if (invoke) {
      const strings = await invoke("strings");
      app.strings = strings.window;
      app.settingsStrings = strings.settings;
    }

    if (!invoke) {
      // The one place with no reachable copy: `strings` is itself a command, and
      // there is no host to ask. The sentence is the same one `strings.rs`
      // carries for the case where the host is there but the call has not been
      // made yet — `call` falls back to it below.
      el("body").innerHTML = '<div class="detail"><p class="note">' +
        "这个页面运行在窗口之外：请用 plainly-desktop 打开。" + "</p></div>";
      return;
    }

    let state;
    try {
      state = await call("window");
    } catch (error) {
      // The window opened but its state did not: a history store that is not a
      // SQLite file, a configuration directory that cannot be created. The
      // reason is the whole screen, because there is nothing else to show and
      // nothing the person can do inside this window until it is fixed.
      renderTopBar();
      el("body").innerHTML = '<div class="detail">' +
        '<h2 class="bad">' + esc(app.strings.error_prefix_title) + "</h2>" +
        '<p class="note">' + esc(String(error)) + "</p></div>";
      return;
    }
    app.now = state.now;
    app.screen = state.screen;
    app.settingsSection = state.settings_section;
    app.history = state.history;
    app.settings = state.settings;
    watchForChanges();
    renderShell();
  }

  start().catch((error) => {
    el("body").innerHTML = '<div class="detail"><p class="note bad">' + esc(String(error)) + "</p></div>";
  });
})();
