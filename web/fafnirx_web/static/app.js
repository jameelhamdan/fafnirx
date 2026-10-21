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
