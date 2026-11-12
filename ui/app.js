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

  const fmtGB = (b) => (b / GB).toLocaleString("id-ID", { minimumFractionDigits: 1, maximumFractionDigits: 1 }) + " GB";
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

  function renderAccounts() {
    const list = $("accounts");
    const focusedId = document.activeElement && document.activeElement.closest(".account")?.dataset.id;
    const focusedRole = document.activeElement?.dataset.role;
    list.replaceChildren(...accounts.map((a) => {
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
      li.querySelector(".state-text").textContent = stateText(a);
      // Which kind of account this is, read with the status through aria-describedby (Main has none).
      const kind = { same_user: t("kind_same"), windows_user: t("kind_windows") }[a.account.kind];
      if (kind) li.querySelector(".state-text").textContent += " · " + kind;
      const primary = li.querySelector('[data-role="primary"]');
      const problem = a.state === "Problem";
      primary.textContent = problem ? t("menu_repair") : running ? t("show") : t("open");
      primary.setAttribute("aria-label", `${primary.textContent} ${a.account.label}`);
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
    }
  }

  // ---------- data ----------
  async function refresh() {
    try {
      [accounts, settings] = await Promise.all([invoke("list_accounts"), invoke("get_settings")]);
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
      if (action === "close") {
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
  function confirmDialog({ title, body, ok, danger, input }) {
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
    const { LogicalSize, getCurrentWindow } = tauri.window;
    getCurrentWindow().setSize(new LogicalSize(320, h)).catch(() => {});
  }

  // ---------- boot ----------
  document.querySelectorAll("[data-s]").forEach((el) => { el.textContent = t(el.dataset.s); });
  listen("status", () => refresh());
  listen("progress", (ev) => announce(t("progress_" + ev.payload.step)));
  refresh().then(() => {
    const first = $("accounts").querySelector('[data-role="primary"]:not(:disabled)');
    if (first) first.focus();
  });
})();
