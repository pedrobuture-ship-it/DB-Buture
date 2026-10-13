// Injected before the app loads: routes Tauri IPC to the dev bridge (src-tauri/examples/dev_bridge.rs).
(() => {
  const BRIDGE = "http://127.0.0.1:1430/invoke/";
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    transformCallback: () => 0,
    unregisterCallback: () => {},
    invoke: async (cmd, args) => {
      const res = await fetch(BRIDGE + cmd, { method: "POST", body: JSON.stringify(args || {}) });
      const json = await res.json();
      if ("err" in json) throw json.err;
      return json.ok;
    },
  };
})();
