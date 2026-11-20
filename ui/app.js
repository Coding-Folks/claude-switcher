// Claude Switcher panel. Talks to Rust via Tauri commands (see src-tauri/src/commands.rs),
// or to mock-invoke.js in a plain browser (?mock=1).
(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri ? tauri.core.invoke : window.__MOCK__.invoke;
  const listen = tauri ? tauri.event.listen : window.__MOCK__.listen;
  const $ = (id) => document.getElementById(id);
  const GB = 1024 ** 3;
  const RESERVE = 2 * GB;          // keep 2 GB free for Windows (spec: confirm below 2 GB)
  const FALLBACK_INSTANCE = 1.5 * GB;

  let accounts = [];
  let settings = { autostart: false, free_ram: 0, total_ram: 0 };
  let menuFor = null;              // { id, button }

  const fmtGB = (b) => (b / GB).toLocaleString(window.LANG === "id" ? "id-ID" : "en-US", { minimumFractionDigits: 1, maximumFractionDigits: 1 }) + " GB";
  const announce = (msg) => { $("live").textContent = ""; requestAnimationFrame(() => { $("live").textContent = msg; }); };
  const showError = (err, vars) => {
    const id = err && err.message_id && window.STRINGS[err.message_id] ? err.message_id : "err.generic";
    $("alert").textContent = t(id, { detail: (err && (err.detail || err.message)) || String(err), ...vars });
  };
  const clearError = () => { $("alert").textContent = ""; };

  // ---------- render ----------
  function render() {
    const running = accounts.filter((a) => a.state === "Running");
    $("count").textContent = running.length ? t("running_count", { n: running.length }) : t("none_running");
    renderRam(running);
    renderAccounts();
    $("autostart").checked = !!settings.autostart;
    renderUpdateChecks(settings.update_checks);
    renderLinkAsk(settings.link_handler);
    fitWindow();
  }

  function renderRam(running) {
    const total = settings.total_ram || 1;
    const free = settings.free_ram;
    const claude = running.reduce((s, a) => s + (a.bytes || 0), 0);
    const other = Math.max(0, total - free - claude);
    const avg = running.length ? claude / running.length : FALLBACK_INSTANCE;
    const more = Math.max(0, Math.floor((free - RESERVE) / avg));
    $("ram-finding").textContent = more > 0
      ? t("ram_finding", { free: fmtGB(free), n: more })
      : t("ram_finding_low", { free: fmtGB(free) });

    const strip = $("ram-strip");
    strip.setAttribute("aria-label", t("ram_strip_label", { claude: fmtGB(claude), other: fmtGB(other), free: fmtGB(free), total: fmtGB(total) }));
    strip.replaceChildren(
      ...running.map((a) => seg("seg-claude", a.bytes / total)),
      seg("seg-other", other / total),
    );
    $("ram-legend").replaceChildren(
      legend("key-claude", `${t("ram_legend_claude")} ${fmtGB(claude)}`),
      legend("key-other", `${t("ram_legend_other")} ${fmtGB(other)}`),
      legend("key-free", `${t("ram_legend_free")} ${fmtGB(free)}`),
    );
  }
  const seg = (cls, share) => { const s = document.createElement("span"); s.className = cls; s.style.width = (share * 100).toFixed(2) + "%"; return s; };
  const legend = (cls, text) => { const li = document.createElement("li"); li.className = "num"; li.innerHTML = `<i class="${cls}" aria-hidden="true"></i>`; li.append(text); return li; };

  function stateText(a) {
    switch (a.state) {
      case "Running": return t("state_running", { mem: a.bytes ? fmtGB(a.bytes) : "—" });
      case "Opening": return t("state_opening");
      case "Problem": return t("state_problem");
      default: return t("state_stopped");
    }
  }

  // More than this many accounts: a search field above the list (the list itself scrolls from ~6 rows).
  const SEARCH_FROM = 9;
  function shownAccounts() {
    const many = accounts.length >= SEARCH_FROM;
    $("search-box").hidden = !many;
    if (!many) $("search").value = "";
    const q = $("search").value.trim().toLowerCase();
    const shown = q ? accounts.filter((a) => a.account.label.toLowerCase().includes(q)) : accounts;
    $("no-match").hidden = shown.length > 0;
    if (!shown.length) $("no-match").textContent = t("no_match", { q: $("search").value.trim() });
    $("accounts").hidden = !shown.length;
    return shown;
  }

  // Where focus goes when the control it was on has disappeared: the search, else the first account.
  function focusFallback() {
    const search = $("search");
    if (!$("search-box").hidden) return search.focus();
    const first = $("accounts").querySelector('[data-role="primary"]:not(:disabled)');
    (first || $("add-open")).focus();
  }

  // ---------- usage ----------
  const numberFmt = () => new Intl.NumberFormat(window.LANG === "id" ? "id-ID" : "en-US", { maximumFractionDigits: 1 });
  function tokens(n) {
    if (n < 1000) return String(n);
    if (n < 1e6) return numberFmt().format(n / 1000) + " " + t("tok_k");
    return numberFmt().format(n / 1e6) + " " + t("tok_m");
  }
  const maxFiveH = () => Math.max(0, ...accounts.filter((x) => x.usage).map((x) => x.usage.five_h));
  const clock = (ms) => new Date(ms).toLocaleTimeString(window.LANG === "id" ? "id-ID" : "en-US", { hour: "2-digit", minute: "2-digit" });
  function left(ms) {
    const min = Math.max(0, Math.ceil((ms - Date.now()) / 60000));
    return t("left", { h: Math.floor(min / 60), m: min % 60 });
  }
  setInterval(() => { if (accounts.some((x) => x.usage && x.usage.limited_until)) render(); }, 30000);

  function renderAccounts() {
    const list = $("accounts");
    const focusedId = document.activeElement && document.activeElement.closest(".account")?.dataset.id;
    const focusedRole = document.activeElement?.dataset.role;
    list.replaceChildren(...shownAccounts().map((a) => {
      const li = document.createElement("li");
      li.className = "account is-" + a.state.toLowerCase();
      li.dataset.id = a.account.id;
      const stateId = "state-" + a.account.id;
      const running = a.state === "Running";
      li.innerHTML = `
        <div class="who">
          <span class="name"></span>
          <span class="state num" id="${stateId}"><span class="dot" aria-hidden="true"></span><span class="state-text"></span></span>
        </div>
        <button type="button" class="ghost" data-role="primary" aria-describedby="${stateId}"></button>
        <button type="button" class="kebab" data-role="menu" aria-haspopup="menu" aria-expanded="false">
          <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><circle cx="8" cy="3" r="1.5"/><circle cx="8" cy="8" r="1.5"/><circle cx="8" cy="13" r="1.5"/></svg>
        </button>`;
      li.querySelector(".name").textContent = a.account.label;
      if (a.usage) {
        // Recent Claude Code use, or until when the account is at its limit.
        const usage = document.createElement("span");
        usage.className = "usage num"; usage.id = "usage-" + a.account.id;
        const limited = a.usage.limited_until && a.usage.limited_until > Date.now();
        li.classList.toggle("is-limited", !!limited);
        if (limited) {
          usage.textContent = t("limited", { time: clock(a.usage.limited_until), left: left(a.usage.limited_until) });
        } else {
          const bar = document.createElement("span");
          bar.className = "use-bar"; bar.setAttribute("aria-hidden", "true");
          const fill = document.createElement("span");
          const scale = a.usage.limit_tokens || maxFiveH() || 1;
          fill.style.width = Math.min(100, Math.round((a.usage.five_h / scale) * 100)) + "%";
          bar.append(fill);
          let text = t("usage_5h", { n: tokens(a.usage.five_h) });
          if (a.usage.limit_tokens) text += " · " + t("usage_pct", { p: Math.round((a.usage.five_h / a.usage.limit_tokens) * 100) });
          usage.append(bar, document.createTextNode(text));
        }
        li.querySelector(".who").append(usage);
      }
      li.querySelector(".state-text").textContent = stateText(a);
      // Which kind of account this is, read with the status through aria-describedby (Main has none).
      const kind = { same_user: t("kind_same"), windows_user: t("kind_windows") }[a.account.kind];
      if (kind) li.querySelector(".state-text").textContent += " · " + kind;
      const primary = li.querySelector('[data-role="primary"]');
      const problem = a.state === "Problem";
      primary.textContent = problem ? t("menu_repair") : running ? t("show") : t("open");
      primary.setAttribute("aria-label", `${primary.textContent} ${a.account.label}`);
      if (a.usage) primary.setAttribute("aria-describedby", `${stateId} usage-${a.account.id} usage-help`);
      primary.disabled = a.state === "Opening";
      const kebab = li.querySelector('[data-role="menu"]');
      kebab.setAttribute("aria-label", t("options_for", { label: a.account.label }));
      primary.addEventListener("click", () => (problem ? menuAction("repair", a) : openAccount(a.account.id, false)));
      kebab.addEventListener("click", () => toggleMenu(a, kebab));
      return li;
    }));
    if (focusedId) {
      const el = list.querySelector(`.account[data-id="${focusedId}"] [data-role="${focusedRole || "primary"}"]`);
      if (el && !el.disabled) el.focus();
      else focusFallback(); // the row was filtered out or removed
    } else if (document.activeElement === document.body || document.activeElement?.closest("[hidden]")) {
      focusFallback(); // e.g. the search was hidden with focus inside (nine accounts down to eight)
    }
  }

  // ---------- data ----------
  async function refresh() {
    try {
      [accounts, settings] = await Promise.all([invoke("list_accounts"), invoke("get_settings")]);
      if (settings.lang && settings.lang !== window.LANG) applyLang(settings.lang);
      render();
      // Rust hands the backup path out once; say it once (spec §6).
      if (settings.recovered_backup) showError({ message_id: "err.store_corrupt", detail: settings.recovered_backup });
    } catch (e) { showError(e); }
  }
  const byId = (id) => accounts.find((a) => a.account.id === id);

  // ---------- actions ----------
  async function openAccount(id, confirmedLowRam) {
    clearError();
    const a = byId(id);
    try {
      const outcome = await invoke("open_account", { id, confirmedLowRam });
      if (outcome === "NeedsLowRamConfirm") {
        const ok = await confirmDialog({
          title: t("lowram_title", { free: fmtGB(settings.free_ram) }),
          body: t("lowram_body", { label: a.account.label }),
          ok: t("lowram_confirm"),
        });
        if (ok) return openAccount(id, true);
        return;
      }
      const said = { Focused: "focused", SlotRecreated: "slot_recreated" }[outcome] || "opened";
      announce(t(said, { label: a.account.label }));
    } catch (e) { showError(e); }
    await refresh();
  }

  async function menuAction(action, a) {
    clearError();
    const label = a.account.label;
    try {
      if (action === "mark") {
        const def = new Date(Date.now() + 5 * 3600000);
        const hhmm = String(def.getHours()).padStart(2, "0") + ":" + String(def.getMinutes()).padStart(2, "0");
        const value = await confirmDialog({ title: t("mark_title", { label }), body: t("mark_body"), ok: t("mark_ok"), input: hhmm, inputLabel: t("mark_label") });
        if (value === null) return;
        const m = /^(\d{1,2})[:.](\d{2})$/.exec(value.trim());
        if (!m || +m[1] > 23 || +m[2] > 59) return showError({ message_id: "err.mark_time" });
        const until = new Date(); until.setHours(+m[1], +m[2], 0, 0);
        if (until.getTime() <= Date.now()) until.setDate(until.getDate() + 1);
        await invoke("mark_limited", { id: a.account.id, until: until.getTime() });
        await refresh();
      } else if (action === "unmark") {
        await invoke("clear_limit", { id: a.account.id });
        await refresh();
      } else if (action === "continue") {
        await continueSession(a);
      } else if (action === "close") {
        await invoke("close_account", { id: a.account.id });
        announce(t("closed", { label }));
      } else if (action === "rename") {
        const value = await confirmDialog({ title: t("rename_title", { label }), ok: t("rename_submit"), input: label });
        if (value && value.trim() && value.trim() !== label) {
          await invoke("rename_account", { id: a.account.id, label: value.trim() });
          announce(t("renamed", { label: value.trim() }));
        }
      } else if (action === "import") {
        const ok = await confirmDialog({ title: t("import_title", { label }), body: t(a.account.kind === "same_user" ? "import_body_same" : "import_body"), ok: t("import_confirm") });
        if (ok) {
          await invoke("import_history", { id: a.account.id });
          announce(t("imported", { label }));
        }
      } else if (action === "repair") {
        await invoke("repair_account", { id: a.account.id });
        announce(t("repaired", { label }));
      } else if (action === "remove") {
        const ok = await confirmDialog({
          title: t("remove_title", { label }),
          body: a.account.kind === "same_user" ? t("remove_body_same") : t("remove_body", { user: a.account.windows_user }),
          ok: t("remove_confirm"),
          danger: true,
        });
        if (ok) {
          await invoke("remove_account", { id: a.account.id });
          announce(t("removed", { label }));
          await refresh();
          $("add-open").focus();
          return;
        }
      }
    } catch (e) { showError(e); }
    await refresh();
  }

  // ---------- menu (WAI-ARIA menu button) ----------
  function toggleMenu(a, button) {
    if (menuFor && menuFor.button === button) return closeMenu(true);
    closeMenu(false);
    const menu = $("menu");
    const items = [];
    if (a.state === "Running") items.push(["close", t("menu_close")]);
    items.push(["rename", t("menu_rename")], ["repair", t("menu_repair")]);
    // Continue a session elsewhere: Main and this user's slots, when another of them exists.
    const canContinue = (x) => x.account.kind === "main" || x.account.kind === "same_user";
    if (canContinue(a) && accounts.some((x) => x.account.id !== a.account.id && canContinue(x))) items.push(["continue", t("menu_continue")]);
    if (a.usage) {
      const manual = a.usage.limit_source === "manual" && a.usage.limited_until > Date.now();
      items.push(manual ? ["unmark", t("menu_unmark")] : ["mark", t("menu_mark")]);
    }
    if (a.account.kind !== "main") items.push(["import", t("menu_import")], ["remove", t("menu_remove")]);
    menu.replaceChildren(...items.map(([action, text]) => {
      const b = document.createElement("button");
      b.type = "button"; b.setAttribute("role", "menuitem"); b.tabIndex = -1; b.textContent = text;
      if (action === "remove") b.className = "menu-danger";
      b.addEventListener("click", () => { closeMenu(true); menuAction(action, a); });
      return b;
    }));
    menu.setAttribute("aria-label", t("options_for", { label: a.account.label }));
    menu.hidden = false;
    const r = button.getBoundingClientRect();
    const top = Math.min(r.bottom + 4, window.innerHeight - menu.offsetHeight - 8);
    menu.style.top = Math.max(8, top) + "px";
    menu.style.left = Math.max(8, r.right - menu.offsetWidth) + "px";
    button.setAttribute("aria-expanded", "true");
    menuFor = { id: a.account.id, button };
    menu.querySelector('[role="menuitem"]').focus();
  }
  function closeMenu(returnFocus) {
    if (!menuFor) return;
    $("menu").hidden = true;
    menuFor.button.setAttribute("aria-expanded", "false");
    if (returnFocus) menuFor.button.focus();
    menuFor = null;
  }
  $("menu").addEventListener("keydown", (e) => {
    const items = [...$("menu").querySelectorAll('[role="menuitem"]')];
    const i = items.indexOf(document.activeElement);
    if (e.key === "ArrowDown") { e.preventDefault(); items[(i + 1) % items.length].focus(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); items[(i - 1 + items.length) % items.length].focus(); }
    else if (e.key === "Home") { e.preventDefault(); items[0].focus(); }
    else if (e.key === "End") { e.preventDefault(); items[items.length - 1].focus(); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(true); }
    else if (e.key === "Tab") { closeMenu(false); }
  });
  document.addEventListener("pointerdown", (e) => {
    if (menuFor && !$("menu").contains(e.target) && !menuFor.button.contains(e.target)) closeMenu(false);
  });

  // ---------- dialog ----------
  function confirmDialog({ title, body, ok, danger, input, inputLabel }) {
    document.querySelector('#confirm-field label').textContent = inputLabel || t("rename_label");
    const dlg = $("confirm");
    const opener = document.activeElement;
    $("confirm-title").textContent = title;
    $("confirm-body").textContent = body || "";
    $("confirm-body").hidden = !body;
    $("confirm-ok").textContent = ok;
    $("confirm-ok").className = danger ? "primary danger" : "primary";
    const field = $("confirm-field");
    field.hidden = input === undefined;
    if (input !== undefined) $("confirm-input").value = input;
    dlg.showModal();
    (input !== undefined ? $("confirm-input") : $("confirm-cancel")).focus();
    return new Promise((resolve) => {
      dlg.addEventListener("close", () => {
        const accepted = dlg.returnValue === "ok";
        if (opener && document.contains(opener)) opener.focus();
        resolve(accepted ? (input !== undefined ? $("confirm-input").value : true) : (input !== undefined ? null : false));
      }, { once: true });
    });
  }
  $("confirm-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); $("confirm").close("ok"); }
  });

  // ---------- add account ----------
  function setAddOpen(open) {
    $("add-form").hidden = !open;
    $("add-open").hidden = open;
    $("add-open").setAttribute("aria-expanded", String(open));
    if (open) {
      document.querySelector("input[name=mode][value=same_user]").checked = true;
      $("add-label").focus();
      fitWindow();
      // With several accounts the form starts below the fold: bring its whole height, up to Buat akun, into view.
      $("add-form").scrollIntoView({ block: "end" });
    } else { $("add-error").textContent = ""; $("add-open").focus(); }
    fitWindow();
  }
  $("add-open").addEventListener("click", () => setAddOpen(true));
  $("add-cancel").addEventListener("click", () => setAddOpen(false));
  $("add-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    clearError();
    const label = $("add-label").value.trim();
    if (!label) { $("add-error").textContent = t("add_empty"); $("add-label").focus(); return; }
    $("add-error").textContent = "";
    const submit = $("add-submit");
    submit.disabled = true; $("add-label").setAttribute("aria-busy", "true");
    try {
      const mode = document.querySelector("input[name=mode]:checked").value;
      await invoke("add_account", { label, mode });
      announce(t("add_done", { label }));
      $("add-label").value = "";
      setAddOpen(false);
    } catch (err) {
      showError(err);            // form keeps the typed label so the user can retry
      $("add-label").focus();
    } finally {
      submit.disabled = false; $("add-label").removeAttribute("aria-busy");
    }
    await refresh();
  });

  // ---------- keyboard ----------
  $("accounts").addEventListener("keydown", (e) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const role = document.activeElement?.dataset.role;
    if (!role) return;
    const rows = [...$("accounts").querySelectorAll(".account")];
    const i = rows.indexOf(document.activeElement.closest(".account"));
    const next = rows[i + (e.key === "ArrowDown" ? 1 : -1)];
    if (next) { e.preventDefault(); next.querySelector(`[data-role="${role}"]`).focus(); }
  });
  // The panel is a tray flyout: Tab off either end would move focus out of the WebView, the window
  // would lose focus and hide. Wrap instead, like any flyout (Esc still closes it).
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || $("confirm").open || !$("menu").hidden) return;
    const items = [...document.querySelectorAll(".panel button, .panel input")]
      .filter((el) => !el.disabled && el.offsetParent !== null);
    if (!items.length) return;
    const first = items[0], last = items[items.length - 1];
    if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    else if (e.shiftKey && (document.activeElement === first || !items.includes(document.activeElement))) { e.preventDefault(); last.focus(); }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    if ($("confirm").open) return;                // the dialog handles its own Escape
    if (!$("add-form").hidden) { setAddOpen(false); return; }
    invoke("hide_panel").catch(() => {});
  });

  $("autostart").addEventListener("change", async (e) => {
    try { await invoke("set_autostart", { enabled: e.target.checked }); }
    catch (err) { e.target.checked = !e.target.checked; showError(err); }
  });

  // Size the native window to the content (Tauri only; max 480px, spec §5).
  function fitWindow() {
    if (!tauri || !tauri.window) return;
    // Grow with the content up to the screen's work area (spec said 480px; too short once the add form opens).
    const max = Math.max(480, (window.screen.availHeight || 480) - 32);
    const h = Math.min(max, Math.ceil(document.querySelector(".panel").scrollHeight));
    // Rust resizes it and keeps the edge next to the taskbar in place (a plain setSize keeps the
    // top-left corner, so a shorter panel drifted up, away from the tray).
    invoke("fit_panel", { height: h }).catch(() => {});
  }

  // ---------- update ----------
  // A newer release on GitHub: say so, and install it only when the user presses Perbarui.
  let update = null, updating = false, updatePct = 0;
  function renderUpdate() {
    $("update").hidden = !update;
    if (!update) return;
    $("update-msg").textContent = updating
      ? t("update_downloading", { p: updatePct })
      : t("update_available", { v: update.version });
    $("update-notes").hidden = updating;
    $("update-go").disabled = updating;
  }
  async function refreshUpdate() {
    try { update = await invoke("get_update"); } catch { update = null; }
    renderUpdate();
  }
  listen("update-progress", (e) => { updatePct = e.payload.percent ?? updatePct; renderUpdate(); });
  $("update-go").addEventListener("click", async () => {
    updating = true; updatePct = 0; renderUpdate();
    try { await invoke("install_update"); } // on success the app restarts into the new version
    catch (err) { updating = false; renderUpdate(); showError(err); }
  });
  $("update-notes").addEventListener("click", () => invoke("open_update_notes").catch(showError));

  // Checks only with consent: asked once (#updates-ask), then the footer toggle.
  function renderUpdateChecks(mode) {
    const available = mode && mode !== "unavailable";
    $("updates-ask").hidden = mode !== "unasked";
    $("updates-switch").hidden = !available || mode === "unasked";
    $("updates").checked = mode === "on";
  }
  async function setUpdateChecks(on, focusAfter) {
    try {
      await invoke("set_update_checks", { enabled: on });
      settings.update_checks = on ? "on" : "off";
      if (!on) { update = null; renderUpdate(); }
      renderUpdateChecks(settings.update_checks);
      if (focusAfter) $("updates").focus(); // the question is gone; its answer lives on in the toggle
    } catch (err) { renderUpdateChecks(settings.update_checks); showError(err); }
  }
  $("updates-yes").addEventListener("click", () => setUpdateChecks(true, true));
  $("updates-no").addEventListener("click", () => setUpdateChecks(false, true));
  $("updates").addEventListener("change", (e) => setUpdateChecks(e.target.checked, false));

  // ---------- continue a session in another account ----------
  function ago(ms) {
    const min = Math.max(0, Math.round((Date.now() - ms) / 60000));
    if (min < 1) return t("ago_now");
    if (min < 60) return t("ago_min", { n: min });
    if (min < 60 * 24) return t("ago_hour", { n: Math.round(min / 60) });
    return t("ago_day", { n: Math.round(min / 1440) });
  }
  const continueTo = makePicker($("continue-to"), $("continue-to-list"), $("continue-to-value"), (id) => {
    continueTo.select(id); continueNote();
  });
  let continueFrom = null;
  function continueNote() {
    const target = byId(continueTo.value), source = byId(continueFrom);
    if (!target) return;
    let note = t(target.state === "Running" ? "continue_note_running" : "continue_note", { label: target.account.label });
    if (source && source.state === "Running") note += " " + t("continue_note_source", { label: source.account.label });
    $("continue-note").textContent = note;
  }
  async function continueSession(menuAccount) {
    // A status refresh rebuilds `accounts`: work with the current entries, compared by id.
    const a = byId(menuAccount.account.id) || menuAccount;
    continueFrom = a.account.id;
    const sessions = await invoke("list_sessions", { id: a.account.id });
    if (!sessions.length) return showError({ message_id: "continue_none" }, { label: a.account.label });
    const targets = accounts.filter((x) => x.account.id !== a.account.id && (x.account.kind === "main" || x.account.kind === "same_user"));
    $("continue-from").textContent = t("continue_from", { label: a.account.label });
    $("continue-list").replaceChildren(...sessions.map((s, i) => {
      const label = document.createElement("label");
      label.className = "session";
      label.innerHTML = '<input type="radio" name="session"><span class="s-text"><span class="s-title"></span><span class="s-meta"></span></span>';
      const input = label.querySelector("input");
      input.value = s.id; input.checked = i === 0;
      label.querySelector(".s-title").textContent = s.title || t("untitled");
      label.querySelector(".s-meta").textContent = [s.folder, ago(s.last_activity)].filter(Boolean).join(" · ");
      return label;
    }));
    continueTo.set(targets.map((x) => ({ value: x.account.id, label: x.account.label })), targets[0].account.id);
    continueNote();
    const dlg = $("continue");
    const opener = document.activeElement;
    const answer = await new Promise((resolve) => {
      dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true });
      dlg.returnValue = "";
      dlg.showModal();
      dlg.querySelector('input[name="session"]:checked').focus();
    });
    if (opener && opener.isConnected) opener.focus();
    if (answer !== "ok") return;
    const sessionId = dlg.querySelector('input[name="session"]:checked').value;
    const to = byId(continueTo.value);
    const title = await invoke("continue_session", { sessionId, from: a.account.id, to: to.account.id });
    announce(t("continued", { title: title || t("untitled"), label: to.account.label }));
    await refresh();
  }

  // ---------- claude: links ----------
  // Claude Desktop claims claude: links for itself; the user has to pick Claude Switcher once.
  let linkLater = false;
  function renderLinkAsk(mode) { $("link-ask").hidden = linkLater || mode !== "other"; }
  $("link-fix").addEventListener("click", () => invoke("open_link_settings").catch(showError));
  $("link-later").addEventListener("click", () => {
    linkLater = true; renderLinkAsk(settings.link_handler);
    $("add-open").focus(); // the card is gone; keep focus in the panel
  });

  // ---------- search ----------
  $("search").addEventListener("input", () => renderAccounts());
  $("search").addEventListener("keydown", (e) => {
    if (e.key === "Escape" && $("search").value) {
      // First Esc clears the search; the next one closes the panel as usual.
      e.preventDefault(); e.stopPropagation();
      $("search").value = ""; renderAccounts();
    } else if (e.key === "ArrowDown") {
      const first = $("accounts").querySelector('[data-role="primary"]:not(:disabled)');
      if (first) { e.preventDefault(); first.focus(); }
    }
  });

  // ---------- language ----------
  // The picker is the landing page's Select (a popover listbox), built as the ARIA "select-only
  // combobox": focus stays on the button, aria-activedescendant names the highlighted option.
  // Used for the language and for the account a session continues in.
  const CHECK = '<svg class="check" viewBox="0 0 16 16" fill="none" aria-hidden="true" focusable="false"><path d="m3.5 8.5 3 3 6-7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  function makePicker(button, list, valueEl, onChoose) {
    let items = [], selected = null, active = -1;
    const options = () => [...list.children];
    function highlight(i) {
      const opts = options();
      if (!opts.length) return;
      active = Math.max(0, Math.min(opts.length - 1, i));
      opts.forEach((o, k) => o.classList.toggle("is-active", k === active));
      button.setAttribute("aria-activedescendant", opts[active].id);
    }
    function open() {
      if (!list.hidden || !items.length) return;
      list.hidden = false;
      button.setAttribute("aria-expanded", "true");
      highlight(items.findIndex((it) => it.value === selected));
      // Open upward when there is no room below (the picker sits at the bottom of the panel).
      list.classList.remove("up");
      const r = button.getBoundingClientRect();
      if (r.bottom + 6 + list.offsetHeight > window.innerHeight - 8) list.classList.add("up");
    }
    function close() {
      if (list.hidden) return;
      list.hidden = true;
      button.setAttribute("aria-expanded", "false");
      button.removeAttribute("aria-activedescendant");
    }
    function choose(value) {
      close();
      button.focus();
      if (value === selected) return;
      onChoose(value);
    }
    function render() {
      list.replaceChildren(...items.map((it) => {
        const li = document.createElement("li");
        li.id = (list.id === "lang-list" ? "lang-opt" : list.id) + "-" + it.value; li.setAttribute("role", "option"); li.dataset.value = it.value;
        if (it.lang) li.lang = it.lang;
        li.setAttribute("aria-selected", String(it.value === selected));
        li.innerHTML = CHECK; li.prepend(it.label);
        li.addEventListener("pointerdown", (e) => e.preventDefault()); // keep focus on the button
        li.addEventListener("click", () => choose(it.value));
        li.addEventListener("pointermove", () => highlight(options().indexOf(li)));
        return li;
      }));
      const cur = items.find((it) => it.value === selected);
      valueEl.textContent = cur ? cur.label : "";
      if (cur && cur.lang) valueEl.lang = cur.lang;
    }
    button.addEventListener("click", () => (list.hidden ? open() : close()));
    button.addEventListener("keydown", (e) => {
      const isOpen = !list.hidden, n = options().length;
      if (!isOpen && ["Enter", " ", "ArrowDown", "ArrowUp"].includes(e.key)) { e.preventDefault(); open(); return; }
      if (!isOpen) return;
      if (e.key === "ArrowDown") { e.preventDefault(); highlight(active + 1); }
      else if (e.key === "ArrowUp") { e.preventDefault(); highlight(active - 1); }
      else if (e.key === "Home") { e.preventDefault(); highlight(0); }
      else if (e.key === "End") { e.preventDefault(); highlight(n - 1); }
      else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); choose(options()[active].dataset.value); }
      else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
      else if (e.key === "Tab") { close(); }
      else if (e.key.length === 1 && /\S/.test(e.key)) {
        // Type a letter: the next option whose name starts with it.
        const opts = options(), k = e.key.toLowerCase();
        for (let step = 1; step <= n; step++) {
          const i = (active + step) % n;
          if (opts[i].textContent.trim().toLowerCase().startsWith(k)) { highlight(i); break; }
        }
      }
    });
    document.addEventListener("pointerdown", (e) => { if (!list.parentElement.contains(e.target)) close(); });
    button.addEventListener("blur", () => close());
    return {
      set(newItems, value) { items = newItems; selected = value; render(); },
      get value() { return selected; },
      select(value) { selected = value; render(); },
    };
  }

  const langPicker = makePicker($("lang"), $("lang-list"), $("lang-value"), async (code) => {
    const prev = window.LANG;
    applyLang(code);
    render();
    try { await invoke("set_language", { lang: window.LANG }); }
    catch (err) { applyLang(prev); render(); showError(err); }
  });

  function applyLang(lang) {
    setLang(lang);
    document.querySelectorAll("[data-s]").forEach((el) => { el.textContent = t(el.dataset.s); });
    document.querySelectorAll("[data-ph]").forEach((el) => { el.placeholder = t(el.dataset.ph); });
    if (typeof renderUpdate === "function") renderUpdate();
    langPicker.set(window.LANGS.map(([code, name]) => ({ value: code, label: name, lang: code })), window.LANG);
  }

  // ---------- boot ----------
  applyLang(window.LANG);
  listen("status", () => { refresh(); refreshUpdate(); });
  refreshUpdate();
  listen("open-add", () => setAddOpen(true)); // tray menu "Add account…"
  listen("progress", (ev) => announce(t("progress_" + ev.payload.step)));
  refresh().then(() => {
    const first = $("accounts").querySelector('[data-role="primary"]:not(:disabled)');
    if (first) first.focus();
  });
})();
