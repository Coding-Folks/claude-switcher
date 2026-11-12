// Browser-only stand-in for Tauri's invoke/listen, used with ?mock=1 (UI tests and design checks).
// ?fail=add makes add_account fail with UAC cancelled; ?ram=low sets free RAM to 1.5 GB.
(function () {
  const params = new URLSearchParams(location.search);
  if (!params.has("mock")) return;

  const GB = 1024 ** 3;
  const accounts = [
    { account: { id: "a-main", label: "Utama", kind: "main", windows_user: null, created_at: "2026-11-12T08:00:00Z" }, state: "Running", bytes: Math.round(1.42 * GB) },
    { account: { id: "a-kantor", label: "Kantor", kind: "windows_user", windows_user: "claude-kantor", created_at: "2026-11-12T09:10:00Z" }, state: "Running", bytes: Math.round(1.18 * GB) },
    { account: { id: "a-klien", label: "Klien", kind: "windows_user", windows_user: "claude-klien", created_at: "2026-11-12T09:40:00Z" }, state: "Stopped", bytes: null },
    { account: { id: "a-riset", label: "Riset", kind: "windows_user", windows_user: "claude-riset", created_at: "2026-11-12T10:05:00Z" }, state: "Problem", bytes: null },
    { account: { id: "a-slotb", label: "Slot B", kind: "same_user", windows_user: null, created_at: "2026-11-12T11:00:00Z", data_root: "C:/Users/Irsal/AppData/Local/ClaudeSwitcher/slots/a-slotb/Local" }, state: params.get("slot") === "unsupported" ? "Problem" : "Stopped", bytes: null },
  ];
  const settings = {
    autostart: true,
    total_ram: Math.round(23.69 * GB),
    free_ram: params.get("ram") === "low" ? Math.round(1.5 * GB) : Math.round(9.2 * GB),
    recovered_backup: params.has("corrupt") ? "C:/Users/Irsal/AppData/Roaming/ClaudeSwitcher/accounts.json.bad-20261112-170000" : null,
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
    invoke: async (cmd, args) => {
      if (!handlers[cmd]) throw { code: "Io", message_id: "err.generic", detail: "unknown command " + cmd };
      return handlers[cmd](args || {});
    },
    listen: async (ev, cb) => { (listeners[ev] ||= []).push(cb); return () => {}; },
  };
})();
