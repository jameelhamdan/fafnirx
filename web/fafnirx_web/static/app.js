// Small enhancements; every page still works without them.
function fxToast(text, kind) {
  const box = document.querySelector(".toasts");
  if (!box) return;
  const t = document.createElement("div");
  t.className = "toast " + (kind || "ok");
  const msg = document.createElement("span");
  msg.textContent = text;
  const x = document.createElement("button");
  x.className = "ghost";
  x.type = "button";
  x.setAttribute("aria-label", "Dismiss message");
  x.textContent = "×";
  x.onclick = () => t.remove();
  t.append(msg, x);
  box.append(t);
  setTimeout(() => t.remove(), 7000);
}

// Forms with data-confirm ask first, in a dialog instead of the browser's confirm box.
document.addEventListener("submit", (e) => {
  const form = e.target;
  const dlg = document.getElementById("confirm");
  if (form.dataset.confirm && !form.dataset.confirmed && dlg && dlg.showModal) {
    e.preventDefault();
    document.getElementById("confirm-title").textContent = form.dataset.confirmTitle || "Are you sure?";
    document.getElementById("confirm-text").textContent = form.dataset.confirm;
    const ok = document.getElementById("confirm-ok");
    ok.textContent = form.dataset.confirmLabel || "Confirm";
    ok.className = form.dataset.confirmKind === "primary" ? "primary" : "danger";
    dlg.returnValue = "cancel";
    dlg.addEventListener(
      "close",
      () => {
        if (dlg.returnValue === "ok") {
          form.dataset.confirmed = "1";
          form.requestSubmit();
        }
      },
      { once: true }
    );
    dlg.showModal();
    return;
  }
  // Show progress and stop double submits.
  const btn = e.submitter || form.querySelector("button[type=submit]");
  if (btn && !e.defaultPrevented) {
    const label = btn.textContent;
    setTimeout(() => {
      btn.disabled = true;
      btn.dataset.label = label;
      btn.textContent = "Working…";
    }, 0);
  }
});
// Coming back with the back button must not leave buttons stuck.
window.addEventListener("pageshow", () => {
  document.querySelectorAll("button[data-label]").forEach((b) => {
    b.disabled = false;
    b.textContent = b.dataset.label;
    delete b.dataset.label;
  });
  document.querySelectorAll("form[data-confirmed]").forEach((f) => delete f.dataset.confirmed);
});

document.addEventListener("alpine:init", () => {
  // Live device status on the home page.
  Alpine.data("live", () => ({
    data: {},
    start() {
      this.load();
      setInterval(() => this.load(), 10000);
    },
    async load() {
      try {
        const r = await fetch("/api/devices");
        if (r.ok) for (const d of await r.json()) this.data[d.name] = d;
      } catch (e) {}
    },
    online(name, fallback) {
      return name in this.data ? this.data[name].online : fallback;
    },
    field(name, key, fallback) {
      return name in this.data ? this.data[name][key] : fallback;
    },
  }));

  // Live status of sources or mirrors: polls faster while something runs, and says when a run ends.
  Alpine.data("jobs", (kind) => ({
    running: {},
    start() {
      this.tick();
    },
    async tick() {
      let any = false;
      try {
        const r = await fetch("/api/jobs");
        if (r.ok) {
          for (const j of (await r.json())[kind]) {
            this.paint(j);
            any = any || j.running;
          }
        }
      } catch (e) {}
      setTimeout(() => this.tick(), any ? 3000 : 15000);
    },
    paint(j) {
      const row = this.$el.querySelector('[data-name="' + CSS.escape(j.name) + '"]');
      if (!row) return;
      const f = (k) => row.querySelector('[data-f="' + k + '"]');
      f("dot").className = "dot " + j.dot;
      f("last").textContent = j.last;
      const msg = f("msg");
      msg.textContent = j.message;
      msg.hidden = !j.message;
      const run = f("run");
      if (run && !run.dataset.label) run.disabled = j.running;
      const files = f("files");
      if (files && j.files !== null) {
        files.hidden = false;
        files.textContent = j.files + (j.files === 1 ? " file" : " files") + " on the mirror";
      }
      if (this.running[j.name] && !j.running) fxToast(j.name + " finished", j.dot === "" ? "error" : "ok");
      this.running[j.name] = j.running;
    },
  }));

  Alpine.data("copy", (text) => ({
    done: false,
    async copy() {
      try {
        await navigator.clipboard.writeText(text);
        this.done = true;
        setTimeout(() => (this.done = false), 1500);
      } catch (e) {}
    },
  }));

  Alpine.data("theme", () => ({
    dark: true,
    init() {
      let saved = null;
      try { saved = localStorage.getItem("fx-theme"); } catch (e) {}
      this.dark = saved !== "light";
      this.apply();
    },
    toggle() {
      this.dark = !this.dark;
      try { localStorage.setItem("fx-theme", this.dark ? "dark" : "light"); } catch (e) {}
      this.apply();
    },
    apply() {
      document.documentElement.classList.toggle("dark", this.dark);
    },
  }));
});
