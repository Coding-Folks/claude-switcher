// Browser-only stand-in for Tauri's invoke/listen, used with ?mock=1 (UI tests and design checks).
// ?lang=en starts in English; ?fail=add makes add_account fail with UAC cancelled; ?ram=low sets free RAM to 1.5 GB.
(function () {
  const params = new URLSearchParams(location.search);
  if (!params.has("mock")) return;

  const GB = 1024 ** 3;
  const accounts = [
    { account: { id: "a-main", label: "Utama", kind: "main", windows_user: null, created_at: "2026-11-12T08:00:00Z" }, state: "Running", bytes: Math.round(1.42 * GB) },
    { account: { id: "a-kantor", label: "Kantor", kind: "windows_user", windows_user: "claude-kantor", created_at: "2026-11-12T09:10:00Z" }, state: "Running", bytes: Math.round(1.18 * GB) },
    { account: { id: "a-klien", label: "Klien", kind: "windows_user", windows_user: "claude-klien", created_at: "2026-11-12T09:40:00Z" }, state: "Stopped", bytes: null },
    { account: { id: "a-riset", label: "Riset", kind: "windows_user", windows_user: "claude-riset", created_at: "2026-11-12T10:05:00Z" }, state: "Problem", bytes: null },
    { account: { id: "a-slotb", label: "Slot B", kind: "same_user", windows_user: null, created_at: "2026-11-12T11:00:00Z", data_root: "C:/Users/alice/AppData/Local/ClaudeSwitcher/slots/a-slotb/Local" }, state: params.get("slot") === "unsupported" ? "Problem" : "Stopped", bytes: null },
  ];
  // ?many=12: twelve accounts, for the scrolling list and the search.
  const many = Number(params.get("many")) || 0;
  const extra = ["Tim Desain", "Tim Data", "Keuangan", "Hukum", "Riset Pasar", "Klien Beta", "Klien Gamma"];
  for (let i = 0; accounts.length < many && i < extra.length; i++) {
    const id = "a-x" + i;
    accounts.push({ account: { id, label: extra[i], kind: "same_user", windows_user: null, created_at: "2026-11-13T08:00:00Z", data_root: `C:/Users/alice/AppData/Local/ClaudeSwitcher/slots/${id}/Local` }, state: "Stopped", bytes: null });
  }
  // Usage: Main and same-user rows; ?limited=1 puts Utama at its limit for 2 h 14 m.
  const usageOf = {
    "a-main": { five_h: 1250000, seven_d: 9000000, limited_until: params.get("limited") ? Date.now() + (2 * 60 + 14) * 60000 : null, limit_source: params.get("limited") ? "auto" : null, limit_tokens: null },
    "a-slotb": { five_h: 420000, seven_d: 2100000, limited_until: null, limit_source: null, limit_tokens: 1400000 },
  };
  for (const a of accounts) {
    a.usage = usageOf[a.account.id] || (a.account.kind === "windows_user" ? null : { five_h: 0, seven_d: 0, limited_until: null, limit_source: null, limit_tokens: null });
  }
  // English sample names for ?lang=en (README screenshots); the Indonesian ones are what the tests use.
  if (params.get("lang") === "en") {
    const en = { Utama: "Main", Kantor: "Work", Klien: "Client", Riset: "Research" };
    for (const a of accounts) {
      const l = en[a.account.label];
      if (l) { a.account.label = l; if (a.account.windows_user) a.account.windows_user = "claude-" + l.toLowerCase(); }
    }
  }
  const settings = {
    autostart: true,
    lang: params.get("lang") || "id",
    total_ram: Math.round(23.69 * GB),
    free_ram: params.get("ram") === "low" ? Math.round(1.5 * GB) : Math.round(9.2 * GB),
    // ?updates=unasked|on|off|unavailable (default: on with ?update, else off)
    update_checks: params.get("updates") || (params.has("update") ? "on" : "off"),
    // ?link=other|ours: who opens claude: links (default: none, no same-user account matters)
    link_handler: params.get("link") || "none",
    version: "0.2.2",
    recovered_backup: params.has("corrupt") ? "C:/Users/alice/AppData/Roaming/ClaudeSwitcher/accounts.json.bad-20261112-170000" : null,
  };
  const listeners = {};
  const emit = (ev, payload) => (listeners[ev] || []).forEach((cb) => cb({ payload }));
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const find = (id) => {
    const a = accounts.find((x) => x.account.id === id);
    if (!a) throw { code: "NotFound", message_id: "err.not_found", detail: null };
    return a;
  };

  const handlers = {
    async list_accounts() { return structuredClone(accounts); },
    async get_settings() { const out = { ...settings }; settings.recovered_backup = null; return out; },
    async set_autostart({ enabled }) { settings.autostart = enabled; },
    async set_language({ lang }) {
      settings.lang = lang;
      const main = accounts.find((a) => a.account.kind === "main");
      if (["Utama", "Main"].includes(main.account.label)) main.account.label = lang === "id" ? "Utama" : "Main";
      emit("status");
    },
    // ?update=1: version 0.3.0 is out; ?update=fail: installing it fails.
    async get_update() {
      return params.has("update") ? { version: "0.3.0", notes_url: "https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.3.0" } : null;
    },
    async install_update() {
      for (const percent of [10, 45, 80, 100]) { await wait(60); emit("update-progress", { percent }); }
      if (params.get("update") === "fail") throw { code: "Io", message_id: "err.update_failed", detail: "signature" };
    },
    async open_update_notes() {},
    async open_link_settings() {},
    // ?newer=1: "Check now" finds 0.3.0
    async check_update_now() {
      return params.has("newer") || params.has("update") ? { version: "0.3.0", notes_url: "https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.3.0" } : null;
    },
    async open_url({ which }) { window.__MOCK__.lastUrl = which; },
    async mark_limited({ id, until }) { const u = find(id).usage; u.limited_until = until; u.limit_source = "manual"; },
    async clear_limit({ id }) { const u = find(id).usage; u.limited_until = null; u.limit_source = null; },
    // ?sessions=0: no sessions; ?continue=newer: the destination has a newer copy.
    async list_sessions() {
      if (params.get("sessions") === "0") return [];
      const now = Date.now();
      return [
        { id: "local_a", title: "Sesi terakhir wedding app", folder: "Coding Folks", last_activity: now - 5 * 60000 },
        { id: "local_b", title: "Fix login Google", folder: "claude-switcher", last_activity: now - 2 * 3600000 },
        { id: "local_c", title: "", folder: "landing-page", last_activity: now - 3 * 86400000 },
      ];
    },
    async continue_session({ sessionId, from, to }) {
      if (params.get("continue") === "newer") throw { code: "Invalid", message_id: "err.continue_newer", detail: null };
      const src = find(from); src.state = "Stopped"; src.bytes = null;
      const a = find(to); a.state = "Running"; a.bytes = Math.round(0.9 * GB); emit("status", null);
      return { local_a: "Sesi terakhir wedding app", local_b: "Fix login Google" }[sessionId] || "";
    },
    async set_update_checks({ enabled }) { settings.update_checks = enabled ? "on" : "off"; },
    async hide_panel() { document.documentElement.dataset.hidden = "true"; },
    async open_account({ id, confirmedLowRam }) {
      const a = find(id);
      if (a.state === "Running") return "Focused";
      if (settings.free_ram < 2 * GB && !confirmedLowRam) return "NeedsLowRamConfirm";
      a.state = "Opening"; emit("status", null);
      await wait(150);
      a.state = "Running"; a.bytes = Math.round(0.9 * GB); emit("status", null);
      return a.account.kind === "same_user" && params.get("slot") === "missing" ? "SlotRecreated" : "Launched";
    },
    async close_account({ id }) { const a = find(id); a.state = "Stopped"; a.bytes = null; emit("status", null); },
    async rename_account({ id, label }) { find(id).account.label = label; },
    async repair_account({ id }) {
      if (find(id).account.kind === "same_user" && params.get("slot") === "unsupported") throw { code: "Invalid", message_id: "err.slot_unsupported", detail: null }; find(id).state = "Stopped"; return { fixed: ["password"] }; },
    async import_history({ id }) {
      find(id);
      if (params.get("fail") === "import") throw { code: "NotFound", message_id: "err.import_no_signin", detail: null };
      await wait(60);
    },
    async remove_account({ id }) { accounts.splice(accounts.indexOf(find(id)), 1); },
    async add_account({ label, mode }) {
      const same = mode === "same_user";
      for (const step of same ? ["prepare_slot", "launch"] : ["vault", "create_user", "grant", "bootstrap", "launch"]) {
        emit("progress", { step, account_label: label });
        await wait(60);
        if (params.get("fail") === "add" && step === "create_user") {
          throw { code: "UacCancelled", message_id: "err.uac_cancelled", detail: null };
        }
      }
      const slug = label.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "akun";
      const id = "a-" + Date.now();
      const a = same
        ? { account: { id, label, kind: "same_user", windows_user: null, created_at: new Date().toISOString(), data_root: "C:/slots/" + id + "/Local" }, state: "Running", bytes: Math.round(0.8 * GB) }
        : { account: { id, label, kind: "windows_user", windows_user: "claude-" + slug, created_at: new Date().toISOString() }, state: "Running", bytes: Math.round(0.8 * GB) };
      accounts.push(a);
      return a.account;
    },
  };

  window.__MOCK__ = {
    calls: {}, // how often each command ran (tests)
    invoke: async (cmd, args) => {
      window.__MOCK__.calls[cmd] = (window.__MOCK__.calls[cmd] || 0) + 1;
      if (!handlers[cmd]) throw { code: "Io", message_id: "err.generic", detail: "unknown command " + cmd };
      return handlers[cmd](args || {});
    },
    listen: async (ev, cb) => { (listeners[ev] ||= []).push(cb); return () => {}; },
    emit, // tests stand in for Rust events (e.g. the tray menu)
  };
})();
