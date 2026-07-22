const invoke = window.__TAURI__?.core?.invoke;
const statusEl = document.getElementById("status");
const rowsEl = document.getElementById("vm-rows");

function setStatus(msg) {
  statusEl.textContent = msg;
}

function render(state) {
  rowsEl.innerHTML = "";
  for (const vm of state.vms) {
    const tr = document.createElement("tr");

    const running = vm.state === "running";
    const consoleTitle = running
      ? `Open VNC console (127.0.0.1:${5900 + vm.vnc_display})`
      : "VM must be running";

    tr.innerHTML = `
      <td>${vm.name}<br /><span style="color:#9aa0aa;font-size:12px">${vm.id}</span></td>
      <td><span class="state ${vm.state}">${vm.state}</span></td>
      <td>${vm.cpus}</td>
      <td>${(vm.memory_mb / 1024).toFixed(1)} GiB</td>
      <td>${vm.disk_gb} GiB</td>
      <td>${vm.snapshots.length}</td>
      <td class="actions">
        <button data-act="start" data-id="${vm.id}" ${running ? "disabled" : ""}>Start</button>
        <button data-act="stop" data-id="${vm.id}" ${running ? "" : "disabled"}>Stop</button>
        <button data-act="snapshot" data-id="${vm.id}">Snapshot</button>
        <button data-act="console" data-id="${vm.id}" ${running ? "" : "disabled"} title="${consoleTitle}">Console</button>
      </td>`;
    rowsEl.appendChild(tr);
  }
}

async function refresh() {
  const state = await invoke("vm_list");
  render(state);
}

async function act(cmd, args, okMsg) {
  try {
    const res = await invoke(cmd, args);
    if (cmd === "open_console") {
      setStatus(`VNC console launched → ${res}`);
    } else {
      render(res);
      setStatus(okMsg);
    }
  } catch (e) {
    setStatus(`Error: ${e}`);
  }
}

document.getElementById("btn-create").addEventListener("click", () => {
  // one-click create with sensible defaults; no wizard (Sprint 0 UX finding)
  act("vm_create", { name: null }, "VM created with defaults (2 vCPU / 2 GiB / 20 GiB qcow2)");
});

rowsEl.addEventListener("click", (ev) => {
  const btn = ev.target.closest("button[data-act]");
  if (!btn || btn.disabled) return;
  const id = btn.dataset.id;
  switch (btn.dataset.act) {
    case "start": return act("vm_start", { id }, `${id} started (stub)`);
    case "stop": return act("vm_stop", { id }, `${id} stopped (stub)`);
    case "snapshot": return act("vm_snapshot", { id }, `snapshot taken for ${id} (stub)`);
    case "console": return act("open_console", { id });
  }
});

if (invoke) {
  refresh().catch((e) => setStatus(`Failed to load VM state: ${e}`));
} else {
  setStatus("Tauri runtime not detected — run via `cargo tauri dev`.");
}
