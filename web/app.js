export const WASM_URL =
  (typeof window !== "undefined" && window.EMATH_WASM_URL) ||
  "/emath.wasm";
const WASM_MISSING = "emath.wasm not found (run `cargo xtask build-web`)";
const SOURCE_HASH_PREFIX = "#src=";

const $ = (id) => document.getElementById(id);

let emRun = null;
let generatedFiles = [];

export function showWasmMissing(visible = true, message = null, retryFn = null) {
  const banner = $("wasm-missing");
  if (!banner) {
    return;
  }
  banner.hidden = !visible;
  if (visible) {
    banner.textContent = "";
    const msgSpan = document.createElement("span");
    msgSpan.className = "banner-message";
    msgSpan.textContent = message || WASM_MISSING;
    banner.appendChild(msgSpan);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn btn-sm btn-banner-retry";
    btn.textContent = "Retry";
    btn.onclick = () => {
      if (typeof retryFn === "function") {
        retryFn();
      } else {
        window.location.reload();
      }
    };
    banner.appendChild(btn);
  }
}

export function formatDiagnostic(item) {
  const severity = item.severity ?? "error";
  const code = item.code ?? "";
  const start = item.start ?? 0;
  const end = item.end ?? 0;
  const message = item.message ?? "";
  return `${severity} ${code} [${start}..${end}]: ${message}`;
}

export function makeEmRun(instance) {
  const { em_alloc, em_free, em_run, em_init, memory } = instance.exports;
  if (typeof em_alloc !== "function" || typeof em_run !== "function") {
    throw new Error("wasm module missing em_alloc/em_run");
  }
  if (typeof memory !== "object" || memory === null || !(memory.buffer instanceof ArrayBuffer)) {
    throw new Error("wasm module missing WebAssembly.Memory export");
  }
  if (typeof em_init === "function") {
    em_init();
  }
  const encoder = new TextEncoder();
  // fatal: reject non-UTF-8 at the ABI edge instead of inserting U+FFFD
  const decoder = new TextDecoder("utf-8", { fatal: true });

  return function emRunOp(op, payload) {
    const opBytes = encoder.encode(String(op));
    const payloadBytes = encoder.encode(
      payload === null || payload === undefined ? "" : String(payload),
    );
    const opPtr = em_alloc(opBytes.length);
    const payloadPtr = em_alloc(payloadBytes.length);
    let ptr = 0;
    let len = 0;
    try {
      // em_alloc(0) → 0 is the null/empty convention; nonzero len must mint.
      if (opBytes.length > 0 && opPtr === 0) {
        throw new Error("wasm em_alloc failed for op buffer");
      }
      if (payloadBytes.length > 0 && payloadPtr === 0) {
        throw new Error("wasm em_alloc failed for payload buffer");
      }
      if (opBytes.length > 0) {
        new Uint8Array(memory.buffer, opPtr, opBytes.length).set(opBytes);
      }
      if (payloadBytes.length > 0) {
        new Uint8Array(memory.buffer, payloadPtr, payloadBytes.length).set(payloadBytes);
      }
      const ret = em_run(opPtr, opBytes.length, payloadPtr, payloadBytes.length);
      // Packed (ptr:u32, len:u32) in a u64; Number() is exact for the low 32 bits.
      const result = typeof ret === "bigint" ? ret : BigInt(ret);
      ptr = Number(result >> 32n);
      len = Number(result & 0xffffffffn);
      if (
        !Number.isFinite(ptr) ||
        !Number.isFinite(len) ||
        !Number.isInteger(ptr) ||
        !Number.isInteger(len) ||
        ptr < 0 ||
        len < 0
      ) {
        throw new Error("wasm em_run returned a non-finite ptr/len pair");
      }
      // Empty pack (0,0): oversized-response refuse or alloc failure.
      if (ptr === 0 && len === 0) {
        throw new Error("wasm em_run returned an empty pack (response too large or alloc failed)");
      }
      if (ptr === 0 && len !== 0) {
        throw new Error("wasm em_run returned a null ptr with nonzero len");
      }
      // Bounds: view must fit the current linear memory (re-read buffer after
      // em_run — growth during dispatch detaches prior ArrayBuffers).
      const buf = memory.buffer;
      if (ptr + len > buf.byteLength) {
        throw new Error(
          `wasm em_run ptr/len out of bounds (ptr=${ptr}, len=${len}, memory=${buf.byteLength})`,
        );
      }
      const jsonBytes = new Uint8Array(buf, ptr, len);
      let text;
      try {
        text = decoder.decode(jsonBytes);
      } catch (err) {
        const detail = err instanceof Error ? err.message : String(err);
        throw new Error(`wasm em_run returned invalid UTF-8: ${detail}`);
      }
      try {
        return JSON.parse(text);
      } catch (err) {
        const detail = err instanceof Error ? err.message : String(err);
        throw new Error(`wasm em_run returned invalid JSON: ${detail}`);
      }
    } finally {
      if (typeof em_free === "function") {
        // em_free(0, _) is a documented no-op; always pair every mint.
        if (ptr !== 0) {
          em_free(ptr, len);
        }
        em_free(opPtr, opBytes.length);
        em_free(payloadPtr, payloadBytes.length);
      }
    }
  };
}

export async function instantiateWasm(url = WASM_URL) {
  let response;
  try {
    response = await fetch(url);
  } catch (netErr) {
    const isOffline = typeof navigator !== "undefined" && !navigator.onLine;
    const msg = isOffline
      ? `Offline: unable to fetch ${url}. If working offline, ensure emath.wasm is cached by loading once online. Or run 'cargo xtask serve-web' locally.`
      : `Network error fetching ${url} (${netErr.message || netErr}). Check server connection or run 'cargo xtask serve-web'.`;
    const error = new Error(msg);
    error.code = isOffline ? "ERR_OFFLINE" : "ERR_NETWORK";
    error.cause = netErr;
    throw error;
  }

  if (!response.ok) {
    const detail = `HTTP ${response.status} ${response.statusText || ""}`.trim();
    const error = new Error(
      response.status === 404
        ? `emath.wasm not found (${detail}). Run 'cargo xtask build-web' to compile the WebAssembly engine.`
        : `Failed to load emath.wasm (${detail}). Run 'cargo xtask build-web' to recompile.`
    );
    error.code = response.status === 404 ? "WASM_MISSING" : `HTTP_${response.status}`;
    error.status = response.status;
    throw error;
  }

  if (typeof WebAssembly.instantiateStreaming === "function") {
    try {
      return await WebAssembly.instantiateStreaming(response, {});
    } catch {
      try {
        const retry = await fetch(url);
        const bytes = await retry.arrayBuffer();
        return await WebAssembly.instantiate(bytes, {});
      } catch (fallbackErr) {
        const error = new Error(
          `WebAssembly compilation failed: ${fallbackErr.message || fallbackErr}. Run 'cargo xtask build-web' to recompile.`
        );
        error.code = "WASM_COMPILE_FAIL";
        error.cause = fallbackErr;
        throw error;
      }
    }
  }

  try {
    const bytes = await response.arrayBuffer();
    return await WebAssembly.instantiate(bytes, {});
  } catch (instantiateErr) {
    const error = new Error(
      `WebAssembly instantiation failed: ${instantiateErr.message || instantiateErr}. Run 'cargo xtask build-web' to recompile.`
    );
    error.code = "WASM_INSTANTIATE_FAIL";
    error.cause = instantiateErr;
    throw error;
  }
}

function setStatus(text, kind) {
  const status = $("status");
  if (!status) {
    return;
  }
  status.textContent = text;
  status.classList.remove("ok", "fail");
  if (kind) {
    status.classList.add(kind);
  }
}

export function showTab(name) {
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.classList.toggle("active", button.dataset.tab === name);
  }
  for (const panel of document.querySelectorAll(".tab-panel")) {
    panel.classList.toggle("active", panel.id === `panel-${name}`);
  }
  if (name === "plot") {
    updatePlotView();
  } else if (name === "math") {
    updateMathView();
  } else if (name === "genesis") {
    updateGenesisView();
  }
}

function setRaw(result) {
  const node = $("out-raw");
  if (node) {
    node.textContent = JSON.stringify(result, null, 2);
  }
}

function renderDiagnostics(result) {
  const node = $("out-diagnostics");
  const badge = $("badge-diag");
  const items = Array.isArray(result.diagnostics) ? result.diagnostics : [];
  
  if (badge) {
    const errorCount = items.filter((d) => (d.severity ?? "error") === "error").length;
    badge.textContent = String(items.length);
    badge.hidden = items.length === 0;
    badge.classList.toggle("error", errorCount > 0);
  }

  if (!node) {
    return;
  }
  node.replaceChildren();
  if (items.length === 0) {
    node.textContent = "no diagnostics: package admits";
    return;
  }
  for (const item of items) {
    const line = document.createElement("div");
    const severity = item.severity ?? "error";
    line.className = `diag diag-${severity}`;
    line.textContent = formatDiagnostic(item);
    node.appendChild(line);
  }
}

function renderPlan(result) {
  const node = $("out-plan");
  if (!node) {
    return;
  }
  const requests = Array.isArray(result.requests) ? result.requests : [];
  const lines = requests.map((request) => {
    const kind = request.kind ?? "";
    const target = request.target ?? "";
    const produce = request.produce ?? "";
    return `${kind} ${target} ${produce}`.trim();
  });
  const plans = JSON.stringify(result.plans ?? [], null, 2);
  node.textContent = [...lines, "", plans].join("\n");
}

function renderMig(result) {
  const node = $("out-mig");
  if (!node) {
    return;
  }
  const canonical = result.canonical ?? "";
  const nodes = result.nodes ?? 0;
  const edges = result.edges ?? 0;
  node.textContent = `${canonical}\n\nnodes: ${nodes}, edges: ${edges}`;
}

function renderGenerated(result) {
  const select = $("generated-files");
  const node = $("out-generated");
  if (!select || !node) {
    return;
  }
  generatedFiles = Array.isArray(result.files) ? result.files : [];
  select.replaceChildren();
  if (generatedFiles.length === 0) {
    node.textContent = Array.isArray(result.diagnostics)
      ? result.diagnostics.map(formatDiagnostic).join("\n")
      : "no generated files";
    return;
  }
  for (const [index, file] of generatedFiles.entries()) {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = file.path ?? `file-${index}`;
    select.appendChild(option);
  }
  node.textContent = generatedFiles[0].content ?? "";
}

function guard(handler) {
  return (...args) => {
    try {
      const result = handler(...args);
      if (result && typeof result.then === "function") {
        result.catch((error) => {
          setStatus(`fail: ${error.message || error}`, "fail");
        });
      }
    } catch (error) {
      setStatus(`fail: ${error.message || error}`, "fail");
    }
  };
}

function runOp(op, payload, after) {
  if (!emRun) {
    throw new Error("engine not loaded");
  }
  const started = performance.now();
  try {
    const result = emRun(op, payload);
    const ms = Math.round(performance.now() - started);
    const ok = result && result.ok !== false;
    setStatus(`${op} ${ms} ms ${ok ? "ok" : "fail"}`, ok ? "ok" : "fail");
    setRaw(result);
    after(result);
    return result;
  } catch (error) {
    const ms = Math.round(performance.now() - started);
    setStatus(`${op} ${ms} ms fail: ${error.message || error}`, "fail");
    throw error;
  }
}

function sourcePayload() {
  return $("editor")?.value ?? "";
}

function formatRunValue(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }
  if (value === null || value === undefined) {
    return String(value);
  }
  return String(value);
}

function readSourceFromHash() {
  const hash = window.location.hash || "";
  if (!hash.startsWith(SOURCE_HASH_PREFIX)) {
    return "";
  }
  try {
    return decodeURIComponent(hash.slice(SOURCE_HASH_PREFIX.length));
  } catch {
    return "";
  }
}

function writeSourceToHash(source) {
  const next = SOURCE_HASH_PREFIX + encodeURIComponent(source ?? "");
  if (window.location.hash === next) {
    return;
  }
  const url = `${window.location.pathname}${window.location.search}${next}`;
  history.replaceState(null, "", url);
}

function debounce(fn, ms) {
  let timer = 0;
  const debounced = (...args) => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => fn(...args), ms);
  };
  debounced.cancel = () => {
    window.clearTimeout(timer);
    timer = 0;
  };
  return debounced;
}

function renderRun(result) {
  const node = $("out-run");
  if (!node) {
    return;
  }
  node.replaceChildren();
  const diagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
  if (diagnostics.length > 0 && !Array.isArray(result.declarations)) {
    for (const item of diagnostics) {
      const line = document.createElement("div");
      const severity = item.severity ?? "error";
      line.className = `diag diag-${severity}`;
      line.textContent = formatDiagnostic(item);
      node.appendChild(line);
    }
    return;
  }
  const badge = document.createElement("div");
  badge.className = "tier-badge";
  badge.textContent = result.tier ?? "interpreted-strict-f64";
  node.appendChild(badge);
  const declarations = Array.isArray(result.declarations) ? result.declarations : [];
  for (const declaration of declarations) {
    const heading = document.createElement("div");
    heading.className = "run-decl";
    heading.textContent = declaration.name ?? "";
    node.appendChild(heading);
    const tests = Array.isArray(declaration.tests) ? declaration.tests : [];
    if (tests.length === 0) {
      const note = document.createElement("div");
      note.className = "run-note";
      note.textContent =
        declaration.note ??
        "no examples; add a worked example or use input fields";
      node.appendChild(note);
      continue;
    }
    for (const test of tests) {
      const row = document.createElement("div");
      const refused = typeof test.refusal === "string";
      const computedOnly = test.computed === true;
      const passed = test.expect_passed === true && !refused && !computedOnly;
      row.className = computedOnly ? "run-computed" : passed ? "run-pass" : "run-fail";
      const label = computedOnly ? "COMPUTED" : refused ? "REFUSED" : passed ? "PASS" : "FAIL";
      const given = test.given && typeof test.given === "object" ? test.given : {};
      const givenText = Object.entries(given)
        .map(([name, value]) => `${name} = ${formatRunValue(value)}`)
        .join(", ");
      row.textContent = givenText
        ? `${label} ${test.name ?? ""}  given ${givenText}`
        : `${label} ${test.name ?? ""}`;
      node.appendChild(row);
      if (refused && test.reason) {
        const reason = document.createElement("div");
        reason.className = "run-note";
        reason.textContent = test.reason;
        node.appendChild(reason);
      }
      const computed = { ...(test.definitions ?? {}), ...(test.outputs ?? {}) };
      for (const [name, value] of Object.entries(computed)) {
        const line = document.createElement("div");
        line.textContent = `${name} = ${formatRunValue(value)}`;
        node.appendChild(line);
      }
    }
  }
  const summary = result.summary;
  if (summary) {
    const line = document.createElement("div");
    line.className = "run-summary";
    const tests = summary.tests ?? 0;
    const passed = summary.passed ?? 0;
    const failed = summary.failed ?? 0;
    const computed = summary.computed ?? 0;
    const testWord = tests === 1 ? "test" : "tests";
    line.textContent =
      computed > 0
        ? `${tests} ${testWord}, ${passed} passed, ${failed} failed, ${computed} computed`
        : `${tests} ${testWord}, ${passed} passed, ${failed} failed`;
    node.appendChild(line);
  }
}

function firstGivens(result) {
  const declarations = Array.isArray(result.declarations) ? result.declarations : [];
  for (const declaration of declarations) {
    const tests = Array.isArray(declaration.tests) ? declaration.tests : [];
    for (const test of tests) {
      if (test.given && typeof test.given === "object") {
        return test.given;
      }
    }
  }
  return {};
}

function showDesugared(result) {
  const banner = $("desugar-banner");
  const sourceNode = $("desugar-source");
  if (!banner || !sourceNode) {
    return;
  }
  if (typeof result.desugared_source === "string") {
    sourceNode.textContent = result.desugared_source;
    banner.hidden = false;
    return;
  }
  sourceNode.textContent = "";
  banner.hidden = true;
}

let liveEvalDebounceTimer = null;

export function updateLiveOutputs(result) {
  const bar = $("live-outputs-bar");
  const list = $("live-outputs-list");
  if (!bar || !list) return;
  const declarations = Array.isArray(result.declarations) ? result.declarations : [];
  const outputs = [];
  for (const decl of declarations) {
    const tests = Array.isArray(decl.tests) ? decl.tests : [];
    for (const test of tests) {
      const computed = { ...(test.definitions ?? {}), ...(test.outputs ?? {}) };
      for (const [name, val] of Object.entries(computed)) {
        outputs.push({ name, val });
      }
    }
  }
  if (outputs.length === 0) {
    bar.hidden = true;
    return;
  }
  list.replaceChildren();
  for (const { name, val } of outputs) {
    const pill = document.createElement("span");
    pill.className = "output-pill";
    pill.textContent = `${name} = ${formatRunValue(val)}`;
    list.appendChild(pill);
  }
  bar.hidden = false;
}

export function renderInputFields(inputsResult, prefills) {
  const panel = $("inputs-panel");
  const fields = $("input-fields");
  if (!panel || !fields) {
    return;
  }
  fields.replaceChildren();
  const declarations = Array.isArray(inputsResult.declarations) ? inputsResult.declarations : [];
  let count = 0;
  for (const declaration of declarations) {
    const inputs = Array.isArray(declaration.inputs) ? declaration.inputs : [];
    for (const input of inputs) {
      const name = input.name ?? "";
      if (!name) {
        continue;
      }
      count += 1;
      const card = document.createElement("div");
      card.className = "input-card";

      const header = document.createElement("div");
      header.className = "input-card-header";
      const nameSpan = document.createElement("span");
      nameSpan.className = "input-name";
      nameSpan.textContent = name;
      const typeSpan = document.createElement("span");
      typeSpan.className = "input-type";
      typeSpan.textContent = input.type ?? "Float64";
      header.appendChild(nameSpan);
      header.appendChild(typeSpan);
      card.appendChild(header);

      const row = document.createElement("div");
      row.className = "input-controls-row";

      const numInput = document.createElement("input");
      numInput.type = "number";
      numInput.step = "any";
      numInput.dataset.input = name;

      let initialVal = 0;
      const prefill = prefills[name];
      if (typeof prefill === "number" && Number.isFinite(prefill)) {
        initialVal = prefill;
      }
      numInput.value = String(initialVal);

      // Adaptive slider bounds
      let min = initialVal <= 0 ? (initialVal < -10 ? initialVal * 2 : -20) : -10;
      let max = initialVal >= 0 ? (initialVal > 10 ? initialVal * 2 : 20) : 10;
      if (min === max) {
        min = -10;
        max = 10;
      }
      let step = (max - min) / 200;
      if (step > 1) step = 1;
      else if (step < 0.001) step = 0.001;

      const rangeInput = document.createElement("input");
      rangeInput.type = "range";
      rangeInput.min = String(min);
      rangeInput.max = String(max);
      rangeInput.step = String(step);
      rangeInput.value = String(initialVal);

      numInput.addEventListener("input", () => {
        const val = Number(numInput.value);
        if (Number.isFinite(val)) {
          if (val < Number(rangeInput.min)) rangeInput.min = String(val * 1.5);
          if (val > Number(rangeInput.max)) rangeInput.max = String(val * 1.5);
          rangeInput.value = String(val);
        }
        triggerLiveEval();
      });

      rangeInput.addEventListener("input", () => {
        numInput.value = rangeInput.value;
        triggerLiveEval();
      });

      row.appendChild(numInput);
      row.appendChild(rangeInput);
      card.appendChild(row);
      fields.appendChild(card);
    }
  }
  panel.hidden = count === 0;
}

function triggerLiveEval() {
  const autoRun = $("chk-auto-run");
  if (autoRun && !autoRun.checked) return;
  if (liveEvalDebounceTimer) clearTimeout(liveEvalDebounceTimer);
  liveEvalDebounceTimer = setTimeout(() => {
    if (!emRun) return;
    try {
      const payload = JSON.stringify({ source: sourcePayload(), given: collectGiven() });
      const result = emRun("run", payload);
      renderRun(result);
      updateLiveOutputs(result);
      if ($("panel-plot")?.classList.contains("active")) {
        drawPlot();
      }
    } catch {}
  }, 25);
}

function refreshPaneChrome(result) {
  showDesugared(result);
  updateLiveOutputs(result);
  if (!emRun) {
    return;
  }
  try {
    const inputsResult = emRun("inputs", sourcePayload());
    renderInputFields(inputsResult, firstGivens(result));
  } catch {
    // inputs op is best-effort chrome; the primary result already rendered
  }
}

export function collectGiven() {
  const given = {};
  for (const control of document.querySelectorAll("#input-fields input[data-input]")) {
    const name = control.dataset.input;
    const value = Number(control.value);
    if (name && Number.isFinite(value)) {
      given[name] = value;
    }
  }
  return given;
}

function runRun() {
  const result = runOp("run", sourcePayload(), (result) => {
    if (Array.isArray(result.diagnostics) && result.diagnostics.length > 0 && !result.declarations) {
      renderDiagnostics(result);
      renderRun(result);
      refreshPaneChrome(result);
      showTab("run");
      return;
    }
    renderRun(result);
    refreshPaneChrome(result);
    showTab("run");
  });
  return result;
}

function runWithGiven() {
  const payload = JSON.stringify({ source: sourcePayload(), given: collectGiven() });
  const result = runOp("run", payload, (result) => {
    if (Array.isArray(result.diagnostics) && result.diagnostics.length > 0 && !result.declarations) {
      renderDiagnostics(result);
      renderRun(result);
      refreshPaneChrome(result);
      showTab("run");
      return;
    }
    renderRun(result);
    refreshPaneChrome(result);
    showTab("run");
  });
  return result;
}

function runCheck() {
  const result = runOp("check", sourcePayload(), (result) => {
    renderDiagnostics(result);
    refreshPaneChrome(result);
  });
  showTab("diagnostics");
  return result;
}

function runPlan() {
  const result = runOp("plan", sourcePayload(), (result) => {
    renderDiagnostics(result);
    renderPlan(result);
  });
  showTab("plan");
  return result;
}

function runMig() {
  const result = runOp("mig", sourcePayload(), renderMig);
  showTab("mig");
  return result;
}

function runGenerate() {
  const result = runOp("generate", sourcePayload(), (result) => {
    if (Array.isArray(result.files) && result.files.length > 0) {
      renderGenerated(result);
      showTab("generated");
      return;
    }
    renderDiagnostics(result);
    renderGenerated(result);
    showTab("diagnostics");
  });
  return result;
}

function runFormat() {
  const result = runOp("format", sourcePayload(), (result) => {
    if (typeof result.formatted === "string") {
      const editor = $("editor");
      if (editor) {
        editor.value = result.formatted;
      }
    }
    if (Array.isArray(result.diagnostics) && result.diagnostics.length > 0) {
      renderDiagnostics(result);
      showTab("diagnostics");
      return;
    }
    showTab("raw");
  });
  return result;
}

function fillExamples(result) {
  const select = $("examples");
  if (!select) {
    return;
  }
  select.replaceChildren();
  const examples = Array.isArray(result.examples) ? result.examples : [];
  const blank = document.createElement("option");
  blank.value = "";
  blank.textContent = "(select example)";
  select.appendChild(blank);
  for (const example of examples) {
    const option = document.createElement("option");
    option.value = example.source ?? "";
    option.textContent = example.name ?? "example";
    select.appendChild(option);
  }
  const editor = $("editor");
  const fromHash = readSourceFromHash();
  if (fromHash && editor) {
    editor.value = fromHash;
    return;
  }
  try {
    const draft = localStorage.getItem("emath_editor_draft");
    if (draft && editor) {
      editor.value = draft;
      return;
    }
  } catch {}
  if (examples.length > 0 && editor && !editor.value) {
    editor.value = examples[0].source ?? "";
    select.selectedIndex = 1;
  }
}

let currentLegendTab = "shortcuts";

const LEGEND_SHORTCUTS = [
  {
    category: "Execution & Lowering",
    items: [
      { key: "Ctrl+R / ⌘↵", action: "Run Engine", desc: "Execute strict-f64 interpreter in browser" },
      { key: "⇧⌘↵", action: "Check / Admit", desc: "Admit package, verify types and proofs without execution" },
      { key: "⌥P / Alt+P", action: "Plan Synthesis", desc: "Compile goal requests and inspect execution plans" },
      { key: "⌥G / Alt+G", action: "Intent Graph (MIG)", desc: "View Semantic Intermediate Representation MIG graph" },
      { key: "⌥C / Alt+C", action: "Generate Rust", desc: "Lower code to in-memory rust-backend artifacts" },
      { key: "⇧⌥F", action: "Format Source", desc: "Comment-preserving AST source formatter" },
    ],
  },
  {
    category: "Workspace & View",
    items: [
      { key: "⇧⌘Y / Alt+S", action: "Symbolify / ASCII-fy", desc: "Toggle Unicode math (α, ∀, ∫) and LaTeX aliases (\\alpha)" },
      { key: "⌘\\ / Ctrl+\\", action: "Swap Panes", desc: "Toggle editor and output pane positions" },
      { key: "⌘K / Ctrl+K / F1", action: "Cheatsheet & Legend", desc: "Open this interactive reference drawer" },
      { key: "Esc", action: "Close Modal", desc: "Dismiss active modal, overlay, or search" },
    ],
  },
  {
    category: "Tab Switcher",
    items: [
      { key: "⌥1", action: "Tab 1: Run", desc: "Values, test verdicts, and interpreter output" },
      { key: "⌥2", action: "Tab 2: Plot", desc: "Interactive 2D function plotter with coordinate grid" },
      { key: "⌥3", action: "Tab 3: Math", desc: "High-precision mathematical intent & LaTeX viewer" },
      { key: "⌥4", action: "Tab 4: Genesis", desc: "Finite worlds, Cayley matrices, and morphism explorer" },
      { key: "⌥5", action: "Tab 5: Diagnostics", desc: "Compiler diagnostics, type errors, and notes" },
      { key: "⌥6", action: "Tab 6: Plan", desc: "Planner goal lowering requests and resolution steps" },
      { key: "⌥7", action: "Tab 7: Intent Graph", desc: "MIG canonical graph topology with nodes & edges" },
      { key: "⌥8", action: "Tab 8: Generated", desc: "Lowered Rust backend source files" },
      { key: "⌥9", action: "Tab 9: Raw JSON", desc: "Raw WASM engine JSON response packet" },
    ],
  },
  {
    category: "Editor Indentation",
    items: [
      { key: "Tab", action: "Indent", desc: "Indent line or selection by 4 spaces" },
      { key: "Shift+Tab", action: "Outdent", desc: "Outdent line or selection up to 4 spaces" },
      { key: "Enter", action: "Smart Enter", desc: "Auto-indent newline and increase indent after ':'" },
      { key: "Backspace", action: "Smart Backspace", desc: "Delete 4 spaces at indent boundary" },
    ],
  },
];

const LEGEND_SYMBOLS = [
  {
    category: "Greek Lowercase & Uppercase",
    symbols: [
      { sym: "α", latex: "\\alpha", ascii: "alpha", desc: "First parameter / variable" },
      { sym: "β", latex: "\\beta", ascii: "beta", desc: "Second parameter / weight" },
      { sym: "γ", latex: "\\gamma", ascii: "gamma", desc: "Third parameter / Lorentz factor" },
      { sym: "δ", latex: "\\delta", ascii: "delta", desc: "Variation / Kronecker delta" },
      { sym: "ε", latex: "\\epsilon", ascii: "epsilon", desc: "Error bound / small perturbation" },
      { sym: "θ", latex: "\\theta", ascii: "theta", desc: "Angle / parameter vector" },
      { sym: "λ", latex: "\\lambda", ascii: "lambda", desc: "Eigenvalue / step rate / decay" },
      { sym: "μ", latex: "\\mu", ascii: "mu", desc: "Mean / coefficient of friction" },
      { sym: "π", latex: "\\pi", ascii: "pi", desc: "Archimedes constant (3.14159...)" },
      { sym: "σ", latex: "\\sigma", ascii: "sigma", desc: "Standard deviation / stress tensor" },
      { sym: "τ", latex: "\\tau", ascii: "tau", desc: "Torque / time constant / 2π" },
      { sym: "φ", latex: "\\phi", ascii: "phi", desc: "Golden ratio / scalar potential" },
      { sym: "ω", latex: "\\omega", ascii: "omega", desc: "Angular frequency" },
      { sym: "Δ", latex: "\\Delta", ascii: "Delta", desc: "Difference / Laplace operator" },
      { sym: "Σ", latex: "\\Sigma", ascii: "Sigma", desc: "Summation / alphabet signature" },
      { sym: "Ω", latex: "\\Omega", ascii: "Omega", desc: "Sample space / domain" },
    ],
  },
  {
    category: "Logic & Set Theory",
    symbols: [
      { sym: "∀", latex: "\\forall", ascii: "forall", desc: "Universal quantifier (for all)" },
      { sym: "∃", latex: "\\exists", ascii: "exists", desc: "Existential quantifier (there exists)" },
      { sym: "∈", latex: "\\in", ascii: "in", desc: "Element of set" },
      { sym: "∉", latex: "\\notin", ascii: "notin", desc: "Not an element of" },
      { sym: "⊆", latex: "\\subseteq", ascii: "subset_eq", desc: "Subset or equal" },
      { sym: "⊂", latex: "\\subset", ascii: "subset", desc: "Strict subset" },
      { sym: "∪", latex: "\\cup", ascii: "union", desc: "Set union" },
      { sym: "∩", latex: "\\cap", ascii: "intersection", desc: "Set intersection" },
      { sym: "∅", latex: "\\emptyset", ascii: "empty_set", desc: "Empty set" },
      { sym: "∧", latex: "\\land", ascii: "/\\", desc: "Logical conjunction (AND)" },
      { sym: "∨", latex: "\\lor", ascii: "\\/", desc: "Logical disjunction (OR)" },
      { sym: "¬", latex: "\\neg", ascii: "~", desc: "Logical negation (NOT)" },
      { sym: "⇒", latex: "\\implies", ascii: "=>", desc: "Material implication" },
      { sym: "⇔", latex: "\\iff", ascii: "<=>", desc: "Logical equivalence (if and only if)" },
      { sym: "⊤", latex: "\\top", ascii: "true", desc: "Top / true truth value" },
      { sym: "⊥", latex: "\\bot", ascii: "false", desc: "Bottom / false / absurdity" },
    ],
  },
  {
    category: "Calculus, Operators & Relations",
    symbols: [
      { sym: "∂", latex: "\\partial", ascii: "partial", desc: "Partial derivative" },
      { sym: "∇", latex: "\\nabla", ascii: "grad", desc: "Gradient / Del operator" },
      { sym: "∫", latex: "\\int", ascii: "int", desc: "Integral operator" },
      { sym: "∑", latex: "\\sum", ascii: "sum", desc: "Series summation" },
      { sym: "∏", latex: "\\prod", ascii: "prod", desc: "Series product" },
      { sym: "√", latex: "\\sqrt", ascii: "sqrt", desc: "Square root" },
      { sym: "∞", latex: "\\infty", ascii: "infinity", desc: "Infinity" },
      { sym: "≤", latex: "\\le", ascii: "<=", desc: "Less than or equal" },
      { sym: "≥", latex: "\\ge", ascii: ">=", desc: "Greater than or equal" },
      { sym: "≠", latex: "\\ne", ascii: "!=", desc: "Not equal" },
      { sym: "≈", latex: "\\approx", ascii: "~=", desc: "Approximately equal" },
      { sym: "≡", latex: "\\equiv", ascii: "===", desc: "Congruence / identity" },
      { sym: "→", latex: "\\to", ascii: "->", desc: "Mapping / right arrow" },
      { sym: "∘", latex: "\\circ", ascii: "o", desc: "Function composition" },
      { sym: "×", latex: "\\times", ascii: "*", desc: "Cartesian product / cross product" },
      { sym: "·", latex: "\\cdot", ascii: ".", desc: "Dot product / scalar product" },
    ],
  },
];

const LEGEND_SYNTAX = [
  {
    category: "Package & Imports",
    desc: "Declaring top-level package and importing definitions",
    code: `package "physics.kinematics"

use math.calculus.*`,
  },
  {
    category: "Function Definitions & Inputs",
    desc: "Defining pure mathematical functions with typed parameters",
    code: `inputs:
    v0: Float64 = 10.0
    theta: Float64 = 0.785398
    g: Float64 = 9.81

definitions:
    vx = v0 * cos(theta)
    vy = v0 * sin(theta)
    range = (v0^2 * sin(2 * theta)) / g`,
  },
  {
    category: "Theorems & Proofs",
    desc: "Formally specifying mathematical theorems and invariants",
    code: `theorem PythagoreanIdentity(theta: Float64):
    sin(theta)^2 + cos(theta)^2 = 1.0
proof:
    identity trigonometric_pythagorean`,
  },
  {
    category: "Assertions & Test Cases",
    desc: "Worked examples and assertions executed in interpreted mode",
    code: `example projectile_45_deg:
    given v0 = 20.0
    given theta = 0.785398
    expect range > 40.0`,
  },
  {
    category: "Finite Worlds & Genesis",
    desc: "Discrete algebraic structures with elements and Cayley matrices",
    code: `world KleinFourGroup:
    elements: [e, a, b, c]
    op: [
        [e, a, b, c],
        [a, e, c, b],
        [b, c, e, a],
        [c, b, a, e]
    ]`,
  },
];

const LEGEND_DIAGNOSTICS = [
  {
    category: "Diagnostic Families",
    items: [
      { code: "E-SYN-*", domain: "Syntax & Layout", desc: "Unbalanced delimiters, indentation violations, or invalid token sequences." },
      { code: "E-NAME-*", domain: "Names & Visibility", desc: "Unbound identifier, conflicting shadow definitions, or private access." },
      { code: "E-SEC-*", domain: "Section Layout", desc: "Disallowed section or misplaced construct outside the active dialect subset." },
      { code: "E-TYPE-*", domain: "Type & Refinement", desc: "Incompatible types, failed subtyping check, or violated refinement predicate." },
      { code: "E-UNIT-*", domain: "Dimensional Analysis", desc: "Mismatched physical units (e.g. adding meters to seconds)." },
      { code: "E-KIND-*", domain: "Kinds & Universes", desc: "Incompatible kind universe or invalid higher-order construct." },
      { code: "E-GOAL-*", domain: "Planning & Synthesis", desc: "Unresolvable lowering goal or circular derivation dependency." },
      { code: "E-GEN-*", domain: "Genesis & Laws", desc: "Algebraic law failure (e.g. associativity, identity, or inverse witness violation)." },
      { code: "E-LOCK-*", domain: "Meaning Lock", desc: "Deterministic hash drift against certified golden semantic lock." },
      { code: "E-NUM-*", domain: "Numeric Model", desc: "IEEE-754 binary64 domain error, NaN division, or overflow in strict mode." },
      { code: "E-HOST-*", domain: "Host Runtime", desc: "WASM memory boundary violation or foreign host bridge communication failure." },
      { code: "E-TLT-*", domain: "Tooling & CLI", desc: "CLI argument discrepancy or environment configuration error." },
      { code: "N-TYPE-001", domain: "Inference Note", desc: "Untyped free variable or function head argument defaulted to Float64." },
    ],
  },
];

export function renderLegend(tab = currentLegendTab, query = "") {
  const body = $("legend-body");
  if (!body) return;
  body.replaceChildren();

  const needle = query.trim().toLowerCase();

  if (tab === "shortcuts") {
    for (const group of LEGEND_SHORTCUTS) {
      const section = document.createElement("div");
      section.className = "legend-section";
      const heading = document.createElement("div");
      heading.className = "legend-section-title";
      heading.textContent = group.category;
      section.appendChild(heading);

      const grid = document.createElement("div");
      grid.className = "legend-grid";
      let matchCount = 0;

      for (const item of group.items) {
        const searchText = `${item.key} ${item.action} ${item.desc}`.toLowerCase();
        if (needle && !searchText.includes(needle)) continue;
        matchCount++;

        const card = document.createElement("div");
        card.className = "legend-card";
        const descSpan = document.createElement("span");
        descSpan.className = "legend-card-desc";
        descSpan.textContent = item.action;
        descSpan.title = item.desc;

        const kbd = document.createElement("kbd");
        kbd.textContent = item.key;

        card.appendChild(descSpan);
        card.appendChild(kbd);
        grid.appendChild(card);
      }

      if (matchCount > 0) {
        section.appendChild(grid);
        body.appendChild(section);
      }
    }
  } else if (tab === "symbols") {
    for (const group of LEGEND_SYMBOLS) {
      const section = document.createElement("div");
      section.className = "legend-section";
      const heading = document.createElement("div");
      heading.className = "legend-section-title";
      heading.textContent = group.category;
      section.appendChild(heading);

      const table = document.createElement("table");
      table.className = "legend-table";
      // ubs:ignore — static HTML literal only (no interpolated data)
      table.innerHTML = `
        <thead>
          <tr>
            <th style="width:3.2rem;text-align:center;">Glyph</th>
            <th style="width:7.5rem;">LaTeX</th>
            <th style="width:6.5rem;">ASCII</th>
            <th>Description</th>
          </tr>
        </thead>
      `;
      const tbody = document.createElement("tbody");
      let matchCount = 0;

      for (const sym of group.symbols) {
        const searchText = `${sym.sym} ${sym.latex} ${sym.ascii} ${sym.desc}`.toLowerCase();
        if (needle && !searchText.includes(needle)) continue;
        matchCount++;

        const tr = document.createElement("tr");
        // ubs:ignore — all interpolations pass through escapeHtml
        tr.innerHTML = `
          <td class="sym-sample">${escapeHtml(sym.sym)}</td>
          <td><code>${escapeHtml(sym.latex)}</code></td>
          <td><code>${escapeHtml(sym.ascii)}</code></td>
          <td>${escapeHtml(sym.desc)}</td>
        `;
        tbody.appendChild(tr);
      }

      if (matchCount > 0) {
        table.appendChild(tbody);
        section.appendChild(table);
        body.appendChild(section);
      }
    }
  } else if (tab === "syntax") {
    for (const item of LEGEND_SYNTAX) {
      const searchText = `${item.category} ${item.desc} ${item.code}`.toLowerCase();
      if (needle && !searchText.includes(needle)) continue;

      const section = document.createElement("div");
      section.className = "legend-section";
      const heading = document.createElement("div");
      heading.className = "legend-section-title";
      heading.textContent = `${item.category}: ${item.desc}`;
      section.appendChild(heading);

      const pre = document.createElement("pre");
      pre.className = "syntax-block";
      pre.textContent = item.code;
      section.appendChild(pre);
      body.appendChild(section);
    }
  } else if (tab === "diagnostics") {
    for (const group of LEGEND_DIAGNOSTICS) {
      const section = document.createElement("div");
      section.className = "legend-section";
      const heading = document.createElement("div");
      heading.className = "legend-section-title";
      heading.textContent = group.category;
      section.appendChild(heading);

      const table = document.createElement("table");
      table.className = "legend-table";
      // ubs:ignore — static HTML literal only (no interpolated data)
      table.innerHTML = `
        <thead>
          <tr>
            <th style="width:7.5rem;">Code / Family</th>
            <th style="width:9.5rem;">Domain</th>
            <th>Description & Remediation</th>
          </tr>
        </thead>
      `;
      const tbody = document.createElement("tbody");
      let matchCount = 0;

      for (const item of group.items) {
        const searchText = `${item.code} ${item.domain} ${item.desc}`.toLowerCase();
        if (needle && !searchText.includes(needle)) continue;
        matchCount++;

        const tr = document.createElement("tr");
        // ubs:ignore — all interpolations pass through escapeHtml
        tr.innerHTML = `
          <td><code>${escapeHtml(item.code)}</code></td>
          <td style="color:#93c5fd;">${escapeHtml(item.domain)}</td>
          <td>${escapeHtml(item.desc)}</td>
        `;
        tbody.appendChild(tr);
      }

      if (matchCount > 0) {
        table.appendChild(tbody);
        section.appendChild(table);
        body.appendChild(section);
      }
    }
  }

  if (body.children.length === 0) {
    const empty = document.createElement("div");
    empty.style.color = "var(--text-muted)";
    empty.style.fontSize = "0.85rem";
    empty.style.padding = "1rem 0";
    empty.textContent = `No matches found for "${query}"`;
    body.appendChild(empty);
  }
}

export function switchLegendTab(tabId) {
  currentLegendTab = tabId;
  for (const btn of document.querySelectorAll(".legend-tab-btn")) {
    btn.classList.toggle("active", btn.dataset.legendTab === tabId);
  }
  const searchInput = $("legend-search");
  renderLegend(currentLegendTab, searchInput ? searchInput.value : "");
}

function filterLegend(query) {
  renderLegend(currentLegendTab, query);
}

export function openLegend(initialTab = "shortcuts") {
  const overlay = $("legend");
  if (!overlay) return;
  switchLegendTab(initialTab);
  overlay.hidden = false;
  $("legend-search")?.focus();
}

export function closeLegend() {
  const overlay = $("legend");
  if (overlay) {
    overlay.hidden = true;
  }
}

function shareSource() {
  writeSourceToHash(sourcePayload());
  const href = window.location.href;
  if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
    navigator.clipboard.writeText(href).then(
      () => setStatus("share url copied", "ok"),
      () => setStatus(`share: ${href}`, "ok"),
    );
    return;
  }
  setStatus(`share: ${href}`, "ok");
}

export const SYMBOL_MAP = [
  // Greek Lowercase
  ["\\alpha", "α"],
  ["\\beta", "β"],
  ["\\gamma", "γ"],
  ["\\delta", "δ"],
  ["\\epsilon", "ε"],
  ["\\varepsilon", "ε"],
  ["\\zeta", "ζ"],
  ["\\eta", "η"],
  ["\\theta", "θ"],
  ["\\vartheta", "ϑ"],
  ["\\iota", "ι"],
  ["\\kappa", "κ"],
  ["\\lambda", "λ"],
  ["\\mu", "μ"],
  ["\\nu", "ν"],
  ["\\xi", "ξ"],
  ["\\pi", "π"],
  ["\\varpi", "ϖ"],
  ["\\rho", "ρ"],
  ["\\varrho", "ϱ"],
  ["\\sigma", "σ"],
  ["\\varsigma", "ς"],
  ["\\tau", "τ"],
  ["\\upsilon", "υ"],
  ["\\phi", "φ"],
  ["\\varphi", "ϕ"],
  ["\\chi", "χ"],
  ["\\psi", "ψ"],
  ["\\omega", "ω"],

  // Greek Uppercase
  ["\\Gamma", "Γ"],
  ["\\Delta", "Δ"],
  ["\\Theta", "Θ"],
  ["\\Lambda", "Λ"],
  ["\\Xi", "Ξ"],
  ["\\Pi", "Π"],
  ["\\Sigma", "Σ"],
  ["\\Upsilon", "Υ"],
  ["\\Phi", "Φ"],
  ["\\Psi", "Ψ"],
  ["\\Omega", "Ω"],

  // Operators & Calculus
  ["\\partial", "∂"],
  ["\\nabla", "∇"],
  ["\\infty", "∞"],
  ["\\iiint", "∭"],
  ["\\iint", "∬"],
  ["\\oint", "∮"],
  ["\\int", "∫"],
  ["\\sum", "∑"],
  ["\\prod", "∏"],
  ["\\coprod", "∐"],
  ["\\sqrt", "√"],

  // Relations & Comparison
  ["\\approx", "≈"],
  ["\\equiv", "≡"],
  ["\\simeq", "≃"],
  ["\\cong", "≅"],
  ["\\propto", "∝"],
  ["\\neq", "≠"],
  ["\\ne", "≠"],
  ["\\leq", "≤"],
  ["\\le", "≤"],
  ["\\geq", "≥"],
  ["\\ge", "≥"],
  ["\\pm", "±"],
  ["\\mp", "∓"],
  ["\\times", "×"],
  ["\\cdot", "·"],
  ["\\circ", "∘"],
  ["\\bullet", "∙"],
  ["\\oplus", "⊕"],
  ["\\otimes", "⊗"],
  ["\\odot", "⊙"],

  // Logic & Set Theory
  ["\\forall", "∀"],
  ["\\exists", "∃"],
  ["\\nexists", "∄"],
  ["\\notin", "∉"],
  ["\\in", "∈"],
  ["\\ni", "∋"],
  ["\\subseteq", "⊆"],
  ["\\supseteq", "⊇"],
  ["\\subset", "⊂"],
  ["\\supset", "⊃"],
  ["\\cap", "∩"],
  ["\\cup", "∪"],
  ["\\setminus", "∖"],
  ["\\emptyset", "∅"],
  ["\\land", "∧"],
  ["\\lor", "∨"],
  ["\\neg", "¬"],
  ["\\top", "⊤"],
  ["\\bot", "⊥"],

  // Arrows
  ["\\leftrightarrow", "↔"],
  ["\\Leftrightarrow", "⇔"],
  ["\\leftarrow", "←"],
  ["\\Leftarrow", "⇐"],
  ["\\rightarrow", "→"],
  ["\\Rightarrow", "⇒"],
  ["\\to", "→"],
  ["\\mapsto", "↦"],
];

const SYMBOL_ENTRIES = [...SYMBOL_MAP].sort((a, b) => b[0].length - a[0].length);

const CANONICAL_ASCII_MAP = new Map([
  ["α", "\\alpha"],
  ["β", "\\beta"],
  ["γ", "\\gamma"],
  ["δ", "\\delta"],
  ["ε", "\\epsilon"],
  ["ζ", "\\zeta"],
  ["η", "\\eta"],
  ["θ", "\\theta"],
  ["ϑ", "\\vartheta"],
  ["ι", "\\iota"],
  ["κ", "\\kappa"],
  ["λ", "\\lambda"],
  ["μ", "\\mu"],
  ["ν", "\\nu"],
  ["ξ", "\\xi"],
  ["π", "\\pi"],
  ["ϖ", "\\varpi"],
  ["ρ", "\\rho"],
  ["ϱ", "\\varrho"],
  ["σ", "\\sigma"],
  ["ς", "\\varsigma"],
  ["τ", "\\tau"],
  ["υ", "\\upsilon"],
  ["φ", "\\phi"],
  ["ϕ", "\\varphi"],
  ["χ", "\\chi"],
  ["ψ", "\\psi"],
  ["ω", "\\omega"],
  ["Γ", "\\Gamma"],
  ["Δ", "\\Delta"],
  ["Θ", "\\Theta"],
  ["Λ", "\\Lambda"],
  ["Ξ", "\\Xi"],
  ["Π", "\\Pi"],
  ["Σ", "\\Sigma"],
  ["Υ", "\\Upsilon"],
  ["Φ", "\\Phi"],
  ["Ψ", "\\Psi"],
  ["Ω", "\\Omega"],
  ["∂", "\\partial"],
  ["∇", "\\nabla"],
  ["∞", "\\infty"],
  ["∭", "\\iiint"],
  ["∬", "\\iint"],
  ["∮", "\\oint"],
  ["∫", "\\int"],
  ["∑", "\\sum"],
  ["∏", "\\prod"],
  ["∐", "\\coprod"],
  ["√", "\\sqrt"],
  ["≈", "\\approx"],
  ["≡", "\\equiv"],
  ["≃", "\\simeq"],
  ["≅", "\\cong"],
  ["∝", "\\propto"],
  ["≠", "\\ne"],
  ["≤", "\\le"],
  ["≥", "\\ge"],
  ["±", "\\pm"],
  ["∓", "\\mp"],
  ["×", "\\times"],
  ["·", "\\cdot"],
  ["∘", "\\circ"],
  ["∙", "\\bullet"],
  ["⊕", "\\oplus"],
  ["⊗", "\\otimes"],
  ["⊙", "\\odot"],
  ["∀", "\\forall"],
  ["∃", "\\exists"],
  ["∄", "\\nexists"],
  ["∉", "\\notin"],
  ["∈", "\\in"],
  ["∋", "\\ni"],
  ["⊆", "\\subseteq"],
  ["⊇", "\\supseteq"],
  ["⊂", "\\subset"],
  ["⊃", "\\supset"],
  ["∩", "\\cap"],
  ["∪", "\\cup"],
  ["∖", "\\setminus"],
  ["∅", "\\emptyset"],
  ["∧", "\\land"],
  ["∨", "\\lor"],
  ["¬", "\\neg"],
  ["⊤", "\\top"],
  ["⊥", "\\bot"],
  ["↔", "\\leftrightarrow"],
  ["⇔", "\\Leftrightarrow"],
  ["←", "\\leftarrow"],
  ["⇐", "\\Leftarrow"],
  ["→", "\\to"],
  ["⇒", "\\Rightarrow"],
  ["↦", "\\mapsto"],
]);

/** Escape text before any innerHTML interpolation (XSS sink hardening). */
export function escapeHtml(text) {
  return String(text)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function symbolify(text) {
  let result = text;
  for (const [latex, unicode] of SYMBOL_ENTRIES) {
    const escaped = latex.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(`${escaped}(?![a-zA-Z])`, "g");
    result = result.replace(regex, unicode);
  }
  return result;
}

export function asciify(text) {
  let result = text;
  for (const [unicode, latex] of CANONICAL_ASCII_MAP) {
    if (result.includes(unicode)) {
      result = result.replaceAll(unicode, latex);
    }
  }
  return result;
}

export function hasUnicodeMath(text) {
  return SYMBOL_MAP.some(([, unicode]) => text.includes(unicode));
}

export function hasLatexAliases(text) {
  return SYMBOL_MAP.some(([latex]) => {
    const escaped = latex.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(`${escaped}(?![a-zA-Z])`);
    return regex.test(text);
  });
}

export function updateSymbolifyButton() {
  const btn = $("btn-symbolify");
  if (!btn) return;
  const editor = $("editor");
  const value = editor ? editor.value : "";
  const isUnicode = hasUnicodeMath(value);
  btn.textContent = isUnicode ? "\\a ASCII-fy" : "α Symbolify";
  btn.title = isUnicode
    ? "Convert Unicode symbols back to \\alpha LaTeX/ASCII (Ctrl/Cmd+Shift+Y)"
    : "Convert \\alpha LaTeX/ASCII to Unicode symbols (Ctrl/Cmd+Shift+Y)";
}

export function toggleSymbolify() {
  const editor = $("editor");
  if (!editor) return;
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const hasSelection = start !== end;
  const targetText = hasSelection ? editor.value.slice(start, end) : editor.value;

  const isUnicode = hasUnicodeMath(targetText);
  const converted = isUnicode ? asciify(targetText) : symbolify(targetText);

  if (hasSelection) {
    editor.value = editor.value.slice(0, start) + converted + editor.value.slice(end);
    editor.selectionStart = start;
    editor.selectionEnd = start + converted.length;
  } else {
    editor.value = converted;
    editor.selectionStart = editor.selectionEnd = Math.min(start, editor.value.length);
  }

  editor.dispatchEvent(new Event("input", { bubbles: true }));
  try {
    localStorage.setItem("emath_editor_draft", editor.value);
  } catch {}
  updateSymbolifyButton();

  setStatus(
    isUnicode ? "converted to ASCII/LaTeX aliases" : "converted to Unicode math symbols",
    "ok",
  );
}

const STORAGE_LAYOUT_KEY = "emath_pane_layout";

export function applyPaneLayout(swapped) {
  const main = document.querySelector("main");
  if (!main) return;
  main.classList.toggle("layout-swapped", swapped);
  const btn = $("btn-swap-layout");
  if (btn) {
    btn.classList.toggle("active", swapped);
    btn.title = swapped
      ? "Panes swapped (Editor Right, Output Left); click to reset"
      : "Swap editor and output pane positions";
  }
  try {
    localStorage.setItem(STORAGE_LAYOUT_KEY, swapped ? "swapped" : "default");
  } catch {}
}

export function togglePaneLayout() {
  const main = document.querySelector("main");
  const isSwapped = main ? main.classList.contains("layout-swapped") : false;
  applyPaneLayout(!isSwapped);
  setStatus(!isSwapped ? "layout: editor on right" : "layout: editor on left", "ok");
}

export function handleEditorTab(editor, event) {
  event.preventDefault();
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const value = editor.value;

  if (start !== end && value.slice(start, end).includes("\n")) {
    const lineStart = value.lastIndexOf("\n", start - 1) + 1;
    const lineEnd = value.indexOf("\n", end) !== -1 ? value.indexOf("\n", end) : value.length;
    const selectedBlock = value.slice(lineStart, lineEnd);
    const lines = selectedBlock.split("\n");
    const indented = lines.map((line) => "    " + line).join("\n");

    editor.value = value.slice(0, lineStart) + indented + value.slice(lineEnd);
    editor.selectionStart = start + 4;
    editor.selectionEnd = end + lines.length * 4;
  } else {
    const before = value.slice(0, start);
    const after = value.slice(end);
    editor.value = before + "    " + after;
    editor.selectionStart = editor.selectionEnd = start + 4;
  }
  editor.dispatchEvent(new Event("input", { bubbles: true }));
}

export function handleEditorShiftTab(editor, event) {
  event.preventDefault();
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const value = editor.value;

  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const lineEnd = value.indexOf("\n", end) !== -1 ? value.indexOf("\n", end) : value.length;
  const selectedBlock = value.slice(lineStart, lineEnd);
  const lines = selectedBlock.split("\n");

  let removedFirstLine = 0;
  let totalRemoved = 0;

  const unindented = lines
    .map((line, idx) => {
      let spacesToRemove = 0;
      if (line.startsWith("    ")) {
        spacesToRemove = 4;
      } else if (line.startsWith("\t")) {
        spacesToRemove = 1;
      } else {
        const match = line.match(/^ {1,3}/);
        if (match) spacesToRemove = match[0].length;
      }
      if (idx === 0) removedFirstLine = spacesToRemove;
      totalRemoved += spacesToRemove;
      return line.slice(spacesToRemove);
    })
    .join("\n");

  editor.value = value.slice(0, lineStart) + unindented + value.slice(lineEnd);
  editor.selectionStart = Math.max(lineStart, start - removedFirstLine);
  editor.selectionEnd = Math.max(editor.selectionStart, end - totalRemoved);
  editor.dispatchEvent(new Event("input", { bubbles: true }));
}

export function handleEditorEnter(editor, event) {
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const value = editor.value;

  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const currentLine = value.slice(lineStart, start);
  const indentMatch = currentLine.match(/^[ \t]*/);
  let indent = indentMatch ? indentMatch[0] : "";

  const trimmed = currentLine.trim();
  if (trimmed.endsWith(":")) {
    indent += "    ";
  }

  event.preventDefault();
  const before = value.slice(0, start);
  const after = value.slice(end);
  const insert = "\n" + indent;
  editor.value = before + insert + after;
  editor.selectionStart = editor.selectionEnd = start + insert.length;
  editor.dispatchEvent(new Event("input", { bubbles: true }));
}

export function handleEditorBackspace(editor, event) {
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  if (start !== end) return false;
  const value = editor.value;
  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const linePrefix = value.slice(lineStart, start);
  if (/^ +$/.test(linePrefix) && linePrefix.length % 4 === 0 && linePrefix.length >= 4) {
    event.preventDefault();
    editor.value = value.slice(0, start - 4) + value.slice(start);
    editor.selectionStart = editor.selectionEnd = start - 4;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  }
  return false;
}

// ============================================================================
// 2D Function Plotter
// ============================================================================

export const plotState = {
  xVar: null,
  yVar: null,
  minX: -10,
  maxX: 10,
  minY: -10,
  maxY: 10,
  samples: 200,
  autoScaleY: true,
  isDragging: false,
  dragStart: { x: 0, y: 0 },
  dragBounds: null,
  isPinching: false,
  pinchStart: null,
  points: [],
  inputs: [],
  outputs: [],
  secondaryValues: {},
  canvasInitialized: false,
};

/** Expand a collapsed axis so screen transforms never divide by zero. */
function ensurePlotSpan(axis) {
  const minKey = axis === "x" ? "minX" : "minY";
  const maxKey = axis === "x" ? "maxX" : "maxY";
  let min = plotState[minKey];
  let max = plotState[maxKey];
  if (!Number.isFinite(min) || !Number.isFinite(max)) {
    plotState[minKey] = -10;
    plotState[maxKey] = 10;
    return;
  }
  if (max < min) {
    plotState[minKey] = max;
    plotState[maxKey] = min;
    min = plotState[minKey];
    max = plotState[maxKey];
  }
  const span = max - min;
  if (!(span > 0)) {
    const pad = Math.max(Math.abs(min) * 1e-6, 1e-6, 1);
    plotState[minKey] = min - pad;
    plotState[maxKey] = max + pad;
  }
}

export function updatePlotView() {
  if (!emRun) return;
  try {
    const inputsResult = emRun("inputs", sourcePayload());
    const decls = Array.isArray(inputsResult.declarations) ? inputsResult.declarations : [];
    const inputs = [];
    for (const decl of decls) {
      for (const input of decl.inputs ?? []) {
        if (input.name && !inputs.includes(input.name)) {
          inputs.push(input.name);
        }
      }
    }
    if (inputs.length === 0) {
      const src = sourcePayload();
      const inMatch = src.match(/inputs:\s*([\s\S]*?)(?:outputs:|definitions:|goals:|tests:|compile:|$)/);
      if (inMatch) {
        for (const line of inMatch[1].split("\n")) {
          const parts = line.trim().split(":");
          if (parts[0] && !parts[0].startsWith("#")) inputs.push(parts[0].trim());
        }
      }
      if (inputs.length === 0) inputs.push("x");
    }
    plotState.inputs = inputs;

    // Run to find available outputs
    const runResult = emRun("run", JSON.stringify({ source: sourcePayload(), given: collectGiven() }));
    const outputs = [];
    for (const decl of runResult.declarations ?? []) {
      for (const test of decl.tests ?? []) {
        const computed = { ...(test.definitions ?? {}), ...(test.outputs ?? {}) };
        for (const name of Object.keys(computed)) {
          if (!outputs.includes(name)) outputs.push(name);
        }
      }
    }
    if (outputs.length === 0) {
      const src = sourcePayload();
      const outMatch = src.match(/(?:outputs:|definitions:)\s*([\s\S]*?)(?:goals:|tests:|compile:|$)/);
      if (outMatch) {
        for (const line of outMatch[1].split("\n")) {
          const parts = line.trim().split(/[:=]/);
          if (parts[0] && !parts[0].startsWith("#") && !inputs.includes(parts[0].trim())) {
            outputs.push(parts[0].trim());
          }
        }
      }
      if (outputs.length === 0) outputs.push("y");
    }
    plotState.outputs = outputs;

    const xSelect = $("plot-x-var");
    const ySelect = $("plot-y-var");

    if (xSelect && inputs.length > 0) {
      const currentX = xSelect.value;
      xSelect.replaceChildren();
      for (const input of inputs) {
        const opt = document.createElement("option");
        opt.value = input;
        opt.textContent = input;
        xSelect.appendChild(opt);
      }
      if (inputs.includes(currentX)) {
        xSelect.value = currentX;
      } else if (inputs.includes("x")) {
        xSelect.value = "x";
      } else {
        xSelect.value = inputs[0];
      }
      plotState.xVar = xSelect.value;
    }

    if (ySelect && outputs.length > 0) {
      const currentY = ySelect.value;
      ySelect.replaceChildren();
      for (const out of outputs) {
        const opt = document.createElement("option");
        opt.value = out;
        opt.textContent = out;
        ySelect.appendChild(opt);
      }
      if (outputs.includes(currentY)) {
        ySelect.value = currentY;
      } else if (outputs.includes("y")) {
        ySelect.value = "y";
      } else {
        ySelect.value = outputs[0];
      }
      plotState.yVar = ySelect.value;
    }

    // Render secondary parameter sliders
    const secContainer = $("plot-secondary-params");
    const secSliders = $("plot-secondary-sliders");
    if (secContainer && secSliders) {
      const secondaries = inputs.filter((name) => name !== plotState.xVar);
      if (secondaries.length > 0) {
        secSliders.replaceChildren();
        const currentGivens = collectGiven();
        for (const sec of secondaries) {
          const item = document.createElement("div");
          item.className = "sec-slider-item";
          const label = document.createElement("span");
          label.textContent = `${sec}:`;
          const valDisplay = document.createElement("span");
          valDisplay.className = "sec-val";
          const initialVal = currentGivens[sec] ?? 1;
          valDisplay.textContent = String(initialVal);

          const slider = document.createElement("input");
          slider.type = "range";
          slider.min = initialVal < 0 ? String(initialVal * 2) : "-20";
          slider.max = initialVal > 0 ? String(initialVal * 2) : "20";
          slider.step = "0.1";
          slider.value = String(initialVal);
          plotState.secondaryValues[sec] = initialVal;

          slider.addEventListener("input", () => {
            const val = Number(slider.value);
            if (!Number.isFinite(val)) {
              return;
            }
            valDisplay.textContent = String(val);
            plotState.secondaryValues[sec] = val;
            drawPlot();
          });

          item.appendChild(label);
          item.appendChild(slider);
          item.appendChild(valDisplay);
          secSliders.appendChild(item);
        }
        secContainer.hidden = false;
      } else {
        secContainer.hidden = true;
      }
    }

    setupPlotCanvas();
    drawPlot();
  } catch {}
}

export function setupPlotCanvas() {
  if (plotState.canvasInitialized) return;
  const canvas = $("plot-canvas");
  if (!canvas) return;
  plotState.canvasInitialized = true;

  const activePointers = new Map();

  function syncPlotInputs() {
    const minXInput = $("plot-min-x");
    const maxXInput = $("plot-max-x");
    if (minXInput) minXInput.value = plotState.minX.toFixed(2);
    if (maxXInput) maxXInput.value = plotState.maxX.toFixed(2);
  }

  function getPinchDist() {
    const pts = Array.from(activePointers.values());
    if (pts.length < 2) return 0;
    return Math.hypot(pts[0].clientX - pts[1].clientX, pts[0].clientY - pts[1].clientY);
  }

  canvas.addEventListener("pointerdown", (e) => {
    try {
      canvas.setPointerCapture(e.pointerId);
    } catch {}
    activePointers.set(e.pointerId, { clientX: e.clientX, clientY: e.clientY });

    if (activePointers.size === 1) {
      plotState.isDragging = true;
      plotState.isPinching = false;
      plotState.dragStart = { x: e.clientX, y: e.clientY };
      plotState.dragBounds = {
        minX: plotState.minX,
        maxX: plotState.maxX,
        minY: plotState.minY,
        maxY: plotState.maxY,
      };
    } else if (activePointers.size >= 2) {
      plotState.isDragging = false;
      plotState.isPinching = true;
      const dist = getPinchDist();
      plotState.pinchStart = {
        dist: dist > 0 ? dist : 1,
        minX: plotState.minX,
        maxX: plotState.maxX,
        minY: plotState.minY,
        maxY: plotState.maxY,
      };
    }
  });

  canvas.addEventListener("pointermove", (e) => {
    if (activePointers.has(e.pointerId)) {
      activePointers.set(e.pointerId, { clientX: e.clientX, clientY: e.clientY });
    }

    if (plotState.isPinching && activePointers.size >= 2 && plotState.pinchStart) {
      const currentDist = getPinchDist();
      if (currentDist > 0 && plotState.pinchStart.dist > 0) {
        const zoomFactor = plotState.pinchStart.dist / currentDist;
        const midX = (plotState.pinchStart.minX + plotState.pinchStart.maxX) / 2;
        const midY = (plotState.pinchStart.minY + plotState.pinchStart.maxY) / 2;
        const spanX = (plotState.pinchStart.maxX - plotState.pinchStart.minX) * zoomFactor;
        const spanY = (plotState.pinchStart.maxY - plotState.pinchStart.minY) * zoomFactor;
        plotState.minX = midX - spanX / 2;
        plotState.maxX = midX + spanX / 2;
        plotState.minY = midY - spanY / 2;
        plotState.maxY = midY + spanY / 2;
        plotState.autoScaleY = false;
        syncPlotInputs();
        drawPlot();
      }
      return;
    }

    if (plotState.isDragging && plotState.dragBounds && activePointers.size === 1) {
      const rect = canvas.getBoundingClientRect();
      const dx = ((e.clientX - plotState.dragStart.x) / rect.width) * (plotState.dragBounds.maxX - plotState.dragBounds.minX);
      const dy = ((e.clientY - plotState.dragStart.y) / rect.height) * (plotState.dragBounds.maxY - plotState.dragBounds.minY);
      plotState.minX = plotState.dragBounds.minX - dx;
      plotState.maxX = plotState.dragBounds.maxX - dx;
      plotState.minY = plotState.dragBounds.minY + dy;
      plotState.maxY = plotState.dragBounds.maxY + dy;
      plotState.autoScaleY = false;
      syncPlotInputs();
      drawPlot();
      return;
    }

    if (activePointers.size === 0 && plotState.points.length > 0) {
      const rect = canvas.getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mathX = plotState.minX + (mouseX / rect.width) * (plotState.maxX - plotState.minX);

      let closest = null;
      let minDist = Infinity;
      for (const pt of plotState.points) {
        const dist = Math.abs(pt.x - mathX);
        if (dist < minDist) {
          minDist = dist;
          closest = pt;
        }
      }

      const tooltip = $("plot-tooltip");
      if (closest && tooltip && Number.isFinite(closest.y)) {
        ensurePlotSpan("x");
        ensurePlotSpan("y");
        const spanX = plotState.maxX - plotState.minX;
        const spanY = plotState.maxY - plotState.minY;
        const canvasX = ((closest.x - plotState.minX) / spanX) * rect.width;
        const canvasY = ((plotState.maxY - closest.y) / spanY) * rect.height;
        tooltip.style.left = `${canvasX}px`;
        tooltip.style.top = `${canvasY}px`;
        tooltip.textContent = `${plotState.xVar ?? "x"} = ${closest.x.toFixed(3)}, ${plotState.yVar ?? "y"} = ${closest.y.toFixed(3)}`;
        tooltip.hidden = false;
      }
    }
  });

  const handlePointerEnd = (e) => {
    if (canvas.hasPointerCapture(e.pointerId)) {
      try {
        canvas.releasePointerCapture(e.pointerId);
      } catch {}
    }
    activePointers.delete(e.pointerId);

    if (activePointers.size === 0) {
      plotState.isDragging = false;
      plotState.isPinching = false;
      plotState.dragBounds = null;
      plotState.pinchStart = null;
    } else if (activePointers.size === 1) {
      plotState.isPinching = false;
      plotState.pinchStart = null;
      const remaining = Array.from(activePointers.values())[0];
      plotState.isDragging = true;
      plotState.dragStart = { x: remaining.clientX, y: remaining.clientY };
      plotState.dragBounds = {
        minX: plotState.minX,
        maxX: plotState.maxX,
        minY: plotState.minY,
        maxY: plotState.maxY,
      };
    }
  };

  canvas.addEventListener("pointerup", handlePointerEnd);
  canvas.addEventListener("pointercancel", handlePointerEnd);

  canvas.addEventListener("pointerleave", () => {
    if (activePointers.size === 0) {
      const tooltip = $("plot-tooltip");
      if (tooltip) tooltip.hidden = true;
    }
  });

  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 0.85 : 1.15;
    const midX = (plotState.minX + plotState.maxX) / 2;
    const midY = (plotState.minY + plotState.maxY) / 2;
    const spanX = (plotState.maxX - plotState.minX) * zoomFactor;
    const spanY = (plotState.maxY - plotState.minY) * zoomFactor;
    plotState.minX = midX - spanX / 2;
    plotState.maxX = midX + spanX / 2;
    plotState.minY = midY - spanY / 2;
    plotState.maxY = midY + spanY / 2;
    plotState.autoScaleY = false;
    syncPlotInputs();
    drawPlot();
  }, { passive: false });

  if (window.ResizeObserver && canvas.parentElement) {
    const ro = new ResizeObserver(() => {
      const parent = canvas.parentElement;
      if (parent && parent.offsetWidth > 0 && parent.offsetHeight > 0) {
        drawPlot();
      }
    });
    ro.observe(canvas.parentElement);
  }
  window.addEventListener("resize", () => {
    const parent = canvas.parentElement;
    if (parent && parent.offsetWidth > 0 && parent.offsetHeight > 0) {
      drawPlot();
    }
  });
}

export function drawPlot() {
  const canvas = $("plot-canvas");
  if (!canvas || !emRun) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const minXInput = $("plot-min-x");
  const maxXInput = $("plot-max-x");
  const samplesInput = $("plot-samples");
  if (minXInput && Number.isFinite(Number(minXInput.value))) plotState.minX = Number(minXInput.value);
  if (maxXInput && Number.isFinite(Number(maxXInput.value))) plotState.maxX = Number(maxXInput.value);
  if (samplesInput && Number.isFinite(Number(samplesInput.value))) plotState.samples = Math.max(10, Math.min(1000, Number(samplesInput.value)));

  const parent = canvas.parentElement;
  if (!parent) return;
  const rect = parent.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    requestAnimationFrame(() => drawPlot());
    return;
  }

  const dpr = Math.min(window.devicePixelRatio || 1, 2.5);
  canvas.width = Math.round(rect.width * dpr);
  canvas.height = Math.round(rect.height * dpr);
  ctx.scale(dpr, dpr);

  const width = rect.width;
  const height = rect.height;

  // Degenerate axis spans → Inf/NaN screen coords; expand before sampling.
  ensurePlotSpan("x");
  ensurePlotSpan("y");

  // Compute points
  const points = [];
  const xVar = plotState.xVar ?? "x";
  const yVar = plotState.yVar ?? "y";
  const numSamples = Math.max(2, plotState.samples | 0);
  const step = (plotState.maxX - plotState.minX) / (numSamples - 1);

  const baseGivens = { ...collectGiven(), ...plotState.secondaryValues };

  let minY = Infinity;
  let maxY = -Infinity;

  for (let i = 0; i < numSamples; i++) {
    const xi = plotState.minX + i * step;
    const given = { ...baseGivens, [xVar]: xi };
    try {
      const payload = JSON.stringify({ source: sourcePayload(), given });
      const result = emRun("run", payload);
      let yi = NaN;
      for (const decl of result.declarations ?? []) {
        for (const test of decl.tests ?? []) {
          const computed = { ...(test.definitions ?? {}), ...(test.outputs ?? {}) };
          if (computed[yVar] !== undefined && Number.isFinite(computed[yVar])) {
            yi = computed[yVar];
            break;
          }
        }
      }
      if (Number.isFinite(yi)) {
        points.push({ x: xi, y: yi });
        if (yi < minY) minY = yi;
        if (yi > maxY) maxY = yi;
      } else {
        points.push({ x: xi, y: NaN });
      }
    } catch {
      points.push({ x: xi, y: NaN });
    }
  }
  plotState.points = points;

  if (plotState.autoScaleY && Number.isFinite(minY) && Number.isFinite(maxY)) {
    if (minY === maxY) {
      minY -= 1;
      maxY += 1;
    }
    const pad = (maxY - minY) * 0.1;
    plotState.minY = minY - pad;
    plotState.maxY = maxY + pad;
    ensurePlotSpan("y");
  }

  // Clear
  ctx.fillStyle = "#151619";
  ctx.fillRect(0, 0, width, height);

  // Coordinate transforms (spans already normalized above)
  const spanX = plotState.maxX - plotState.minX;
  const spanY = plotState.maxY - plotState.minY;
  const toScreenX = (x) => ((x - plotState.minX) / spanX) * width;
  const toScreenY = (y) => ((plotState.maxY - y) / spanY) * height;

  // Draw Grid
  ctx.strokeStyle = "#25272e";
  ctx.lineWidth = 1;
  ctx.font = "10px ui-monospace, SFMono-Regular, Menlo, monospace";
  ctx.fillStyle = "#71717a";

  const numGridX = 8;
  const stepGridX = (plotState.maxX - plotState.minX) / numGridX;
  for (let i = 0; i <= numGridX; i++) {
    const gx = plotState.minX + i * stepGridX;
    const sx = toScreenX(gx);
    ctx.beginPath();
    ctx.moveTo(sx, 0);
    ctx.lineTo(sx, height);
    ctx.stroke();
    ctx.fillText(gx.toFixed(1), sx + 4, height - 8);
  }

  const numGridY = 6;
  const stepGridY = (plotState.maxY - plotState.minY) / numGridY;
  for (let i = 0; i <= numGridY; i++) {
    const gy = plotState.minY + i * stepGridY;
    const sy = toScreenY(gy);
    ctx.beginPath();
    ctx.moveTo(0, sy);
    ctx.lineTo(width, sy);
    ctx.stroke();
    ctx.fillText(gy.toFixed(1), 6, sy - 4);
  }

  // Draw Axes
  ctx.strokeStyle = "#4b4d57";
  ctx.lineWidth = 1.5;

  const originX = toScreenX(0);
  const originY = toScreenY(0);

  // Y-axis (x = 0)
  if (originX >= 0 && originX <= width) {
    ctx.beginPath();
    ctx.moveTo(originX, 0);
    ctx.lineTo(originX, height);
    ctx.stroke();
  }

  // X-axis (y = 0)
  if (originY >= 0 && originY <= height) {
    ctx.beginPath();
    ctx.moveTo(0, originY);
    ctx.lineTo(width, originY);
    ctx.stroke();
  }

  // Draw Curve
  ctx.strokeStyle = "#7aa2f7";
  ctx.lineWidth = 2.5;
  ctx.beginPath();

  let started = false;
  for (const pt of points) {
    if (!Number.isFinite(pt.y)) {
      started = false;
      continue;
    }
    const sx = toScreenX(pt.x);
    const sy = toScreenY(pt.y);
    if (!started) {
      ctx.moveTo(sx, sy);
      started = true;
    } else {
      ctx.lineTo(sx, sy);
    }
  }
  ctx.stroke();
}

export function autoScalePlot() {
  plotState.autoScaleY = true;
  drawPlot();
}

export function resetPlotView() {
  plotState.minX = -10;
  plotState.maxX = 10;
  plotState.minY = -10;
  plotState.maxY = 10;
  plotState.autoScaleY = true;
  plotState.isDragging = false;
  plotState.isPinching = false;
  plotState.dragBounds = null;
  plotState.pinchStart = null;
  const minXInput = $("plot-min-x");
  const maxXInput = $("plot-max-x");
  if (minXInput) minXInput.value = "-10";
  if (maxXInput) maxXInput.value = "10";
  drawPlot();
}

export function exportPlotPng() {
  const canvas = $("plot-canvas");
  if (!canvas) return;
  const link = document.createElement("a");
  link.download = `emath-plot-${plotState.yVar ?? "y"}.png`;
  link.href = canvas.toDataURL("image/png");
  link.click();
}

// ============================================================================
// Mathematical Intent Typography View
// ============================================================================

export function updateMathView() {
  const container = $("math-rendered");
  const rawPre = $("math-latex-raw");
  if (!container || !rawPre) return;

  const source = sourcePayload();
  container.replaceChildren();

  const lines = source.split("\n");
  const latexLines = [];

  let currentDecl = "Main";
  let inDefinitions = false;
  let inInputs = false;
  const inputsList = [];
  const equations = [];

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;

    if (line.startsWith("emath function") || line.startsWith("function") || line.startsWith("package")) {
      const match = line.match(/(?:emath\s+)?(?:function|package)\s+([a-zA-Z0-9_]+)/);
      if (match) currentDecl = match[1];
      inDefinitions = false;
      inInputs = false;
    } else if (line.startsWith("inputs:")) {
      inInputs = true;
      inDefinitions = false;
    } else if (line.startsWith("definitions:") || line.startsWith("equations:")) {
      inDefinitions = true;
      inInputs = false;
    } else if (line.startsWith("goals:") || line.startsWith("tests:") || line.startsWith("compile:") || line.startsWith("outputs:") || line.startsWith("evidence:") || line.startsWith("state:")) {
      inDefinitions = false;
      inInputs = false;
    } else if (inInputs && line.includes(":")) {
      const parts = line.split(":");
      inputsList.push({ name: parts[0].trim(), type: parts[1].trim() });
    } else if (inDefinitions && line.includes("=") && !line.includes("==")) {
      const [lhs, rhs] = line.split("=").map((s) => s.trim());
      if (lhs && rhs) equations.push({ lhs, rhs });
    } else if (!inDefinitions && !inInputs && line.includes("=") && !line.includes("==") && !line.startsWith("expect ") && !line.startsWith("given ")) {
      const [lhs, rhs] = line.split("=").map((s) => s.trim());
      if (lhs && rhs && !lhs.includes(" ")) {
        equations.push({ lhs, rhs });
      }
    }
  }

  // Create Decl Card
  const card = document.createElement("div");
  card.className = "math-decl-card";

  const title = document.createElement("div");
  title.className = "math-decl-title";
  const inputsSig = inputsList.length > 0
    ? inputsList.map((inp) => `${symbolify(inp.name)} ∈ ℝ`).join(", ")
    : "x ∈ ℝ";
  title.textContent = `Function ${currentDecl}(${inputsSig}) ⟹ Outputs`;
  card.appendChild(title);

  const eqList = document.createElement("div");
  eqList.className = "math-equation-list";

  if (equations.length === 0) {
    const emptyRow = document.createElement("div");
    emptyRow.className = "math-equation-row";
    emptyRow.style.color = "var(--text-muted)";
    emptyRow.textContent = "No mathematical definitions detected. Add equations (e.g. y = x * x) in the editor.";
    eqList.appendChild(emptyRow);
  } else {
    latexLines.push(`\\text{Function } \\mathrm{${currentDecl}}(${inputsList.length > 0 ? inputsList.map((i) => asciify(i.name)).join(", ") : "x"})`);
    latexLines.push("\\begin{aligned}");

    for (const eq of equations) {
      const row = document.createElement("div");
      row.className = "math-equation-row";

      const lhsSpan = document.createElement("span");
      lhsSpan.className = "math-lhs";
      lhsSpan.textContent = symbolify(eq.lhs);

      const eqSpan = document.createElement("span");
      eqSpan.className = "math-eq";
      eqSpan.textContent = "=";

      const rhsSpan = document.createElement("span");
      rhsSpan.className = "math-rhs";
      // ubs:ignore — formatMathExprHtml escapeHtml-sanitizes editor text first
      rhsSpan.innerHTML = formatMathExprHtml(eq.rhs);

      row.appendChild(lhsSpan);
      row.appendChild(eqSpan);
      row.appendChild(rhsSpan);
      eqList.appendChild(row);

      const latexLhs = asciify(eq.lhs);
      const latexRhs = formatMathExprLatex(eq.rhs);
      latexLines.push(`  ${latexLhs} &= ${latexRhs} \\\\`);
    }

    latexLines.push("\\end{aligned}");
  }

  card.appendChild(eqList);
  container.appendChild(card);

  rawPre.textContent = latexLines.join("\n");
}

export function formatMathExprHtml(expr) {
  // Escape editor-derived text first; only trusted span wrappers are added after.
  let res = escapeHtml(symbolify(expr));
  // Fractions: a / b
  res = res.replace(/([a-zA-Z0-9_().]+)\s*\/\s*([a-zA-Z0-9_().]+)/g, '<span class="math-frac"><span class="math-num">$1</span><span class="math-den">$2</span></span>');
  // Exponents: a * a or a ^ b
  res = res.replace(/([a-zA-Z0-9_]+)\s*\^\s*([0-9]+|[a-zA-Z])/g, '$1<span class="math-sup">$2</span>');
  // Subscripts: a_b
  res = res.replace(/([a-zA-Z0-9_]+)_([a-zA-Z0-9]+)/g, '$1<span class="math-sub">$2</span>');
  // Sqrt: sqrt(x)
  res = res.replace(/sqrt\(([^)]+)\)/g, '<span class="math-sqrt"><span class="math-sqrt-sign">√</span><span>$1</span></span>');
  // Multiplications
  res = res.replace(/\s*\*\s*/g, " · ");
  return res;
}

export function formatMathExprLatex(expr) {
  let res = asciify(expr);
  // Fractions
  res = res.replace(/([a-zA-Z0-9_().]+)\s*\/\s*([a-zA-Z0-9_().]+)/g, "\\frac{$1}{$2}");
  // Sqrt
  res = res.replace(/sqrt\(([^)]+)\)/g, "\\sqrt{$1}");
  // Multiplication
  res = res.replace(/\s*\*\s*/g, " \\cdot ");
  // Trig
  res = res.replace(/\b(sin|cos|tan|exp|ln|log)\b/g, "\\$1");
  return res;
}

export function copyLatexToClipboard() {
  const rawPre = $("math-latex-raw");
  if (!rawPre) return;
  const text = rawPre.textContent;
  if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
    navigator.clipboard.writeText(text).then(
      () => setStatus("LaTeX equations copied to clipboard", "ok"),
      () => setStatus("failed to copy LaTeX", "fail"),
    );
  }
}

export function toggleLatexRaw() {
  const container = $("math-latex-container");
  const btn = $("btn-toggle-latex-raw");
  if (!container || !btn) return;
  container.hidden = !container.hidden;
  btn.textContent = container.hidden ? "Show Raw LaTeX" : "Hide Raw LaTeX";
}

// ============================================================================
// Finite Worlds & Morphisms Explorer (Genesis)
// ============================================================================

export const GENESIS_WORLDS = {
  b2: {
    name: "Boolean Algebra 𝔹₂",
    elements: ["0", "1"],
    operators: {
      "∧ (AND)": [
        ["0", "0"],
        ["0", "1"],
      ],
      "∨ (OR)": [
        ["0", "1"],
        ["1", "1"],
      ],
      "⊕ (XOR)": [
        ["0", "1"],
        ["1", "0"],
      ],
      "→ (IMPLIES)": [
        ["1", "1"],
        ["0", "1"],
      ],
      "↑ (NAND)": [
        ["1", "1"],
        ["1", "0"],
      ],
    },
  },
  k3: {
    name: "Kleene 3-Valued Logic 𝕂₃",
    elements: ["F", "U", "T"],
    operators: {
      "∧ (Min/AND)": [
        ["F", "F", "F"],
        ["F", "U", "U"],
        ["F", "U", "T"],
      ],
      "∨ (Max/OR)": [
        ["F", "U", "T"],
        ["U", "U", "T"],
        ["T", "T", "T"],
      ],
    },
  },
  b4: {
    name: "Belnap 4-Valued Logic ℬ₄",
    elements: ["N", "F", "T", "B"],
    operators: {
      "∧ (Truth Conjunction)": [
        ["N", "F", "N", "F"],
        ["F", "F", "F", "F"],
        ["N", "F", "T", "B"],
        ["F", "F", "B", "B"],
      ],
      "∨ (Truth Disjunction)": [
        ["N", "N", "T", "T"],
        ["N", "F", "T", "B"],
        ["T", "T", "T", "T"],
        ["T", "B", "T", "B"],
      ],
      "⊗ (Information Consensus)": [
        ["N", "N", "N", "N"],
        ["N", "F", "N", "F"],
        ["N", "N", "T", "T"],
        ["N", "F", "T", "B"],
      ],
    },
  },
  v4: {
    name: "Klein 4-Group V₄",
    elements: ["e", "a", "b", "c"],
    operators: {
      "★ (Group Op)": [
        ["e", "a", "b", "c"],
        ["a", "e", "c", "b"],
        ["b", "c", "e", "a"],
        ["c", "b", "a", "e"],
      ],
    },
  },
  z3: {
    name: "Cyclic Ring ℤ/3ℤ",
    elements: ["0", "1", "2"],
    operators: {
      "+₃ (Add mod 3)": [
        ["0", "1", "2"],
        ["1", "2", "0"],
        ["2", "0", "1"],
      ],
      "×₃ (Mul mod 3)": [
        ["0", "0", "0"],
        ["0", "1", "2"],
        ["0", "2", "1"],
      ],
    },
  },
  z5: {
    name: "Cyclic Ring ℤ/5ℤ",
    elements: ["0", "1", "2", "3", "4"],
    operators: {
      "+₅ (Add mod 5)": [
        ["0", "1", "2", "3", "4"],
        ["1", "2", "3", "4", "0"],
        ["2", "3", "4", "0", "1"],
        ["3", "4", "0", "1", "2"],
        ["4", "0", "1", "2", "3"],
      ],
      "×₅ (Mul mod 5)": [
        ["0", "0", "0", "0", "0"],
        ["0", "1", "2", "3", "4"],
        ["0", "2", "4", "1", "3"],
        ["0", "3", "1", "4", "2"],
        ["0", "4", "3", "2", "1"],
      ],
    },
  },
};

export const genesisState = {
  selectedPreset: "b2",
  selectedOp: "∧ (AND)",
  customElements: ["0", "1"],
  customTable: [
    ["0", "0"],
    ["0", "1"],
  ],
};

export function updateGenesisView() {
  const presetSelect = $("genesis-world-preset");
  const opSelect = $("genesis-op-select");
  if (!presetSelect || !opSelect) return;

  const presetKey = presetSelect.value;
  genesisState.selectedPreset = presetKey;

  const world = GENESIS_WORLDS[presetKey];
  if (world) {
    opSelect.replaceChildren();
    for (const opName of Object.keys(world.operators)) {
      const opt = document.createElement("option");
      opt.value = opName;
      opt.textContent = opName;
      opSelect.appendChild(opt);
    }
    if (world.operators[genesisState.selectedOp]) {
      opSelect.value = genesisState.selectedOp;
    } else {
      opSelect.selectedIndex = 0;
      genesisState.selectedOp = opSelect.value;
    }
    const symSpan = $("genesis-op-symbol");
    if (symSpan) symSpan.textContent = genesisState.selectedOp.split(" ")[0];
    renderGenesisMatrix(world.elements, world.operators[genesisState.selectedOp]);
    verifyGenesisLaws(world.elements, world.operators[genesisState.selectedOp]);
  }
}

export function renderGenesisMatrix(elements, table) {
  const wrapper = $("genesis-matrix-table-wrapper");
  if (!wrapper) return;
  wrapper.replaceChildren();

  const tableEl = document.createElement("table");
  tableEl.className = "genesis-table";

  // Header row
  const headerRow = document.createElement("tr");
  const corner = document.createElement("th");
  corner.textContent = "★";
  headerRow.appendChild(corner);
  for (const el of elements) {
    const th = document.createElement("th");
    th.textContent = el;
    headerRow.appendChild(th);
  }
  tableEl.appendChild(headerRow);

  // Rows
  elements.forEach((rowEl, rowIdx) => {
    const tr = document.createElement("tr");
    const rowHeader = document.createElement("th");
    rowHeader.textContent = rowEl;
    tr.appendChild(rowHeader);

    elements.forEach((colEl, colIdx) => {
      const td = document.createElement("td");
      const val = table[rowIdx]?.[colIdx] ?? elements[0];
      td.textContent = val;

      td.addEventListener("mouseenter", () => {
        const info = $("genesis-cell-info");
        if (info) info.textContent = `${rowEl} ★ ${colEl} = ${val}`;
      });

      tr.appendChild(td);
    });
    tableEl.appendChild(tr);
  });

  wrapper.appendChild(tableEl);
}

export function verifyGenesisLaws(elements, table) {
  const list = $("genesis-laws-list");
  if (!list) return;
  list.replaceChildren();

  const n = elements.length;
  const elMap = new Map();
  elements.forEach((el, idx) => elMap.set(el, idx));

  const apply = (a, b) => {
    const ai = elMap.get(a);
    const bi = elMap.get(b);
    return table[ai]?.[bi];
  };

  // 1. Associativity: (a * b) * c == a * (b * c)
  let assocPass = true;
  let assocWitness = "";
  for (let i = 0; i < n && assocPass; i++) {
    for (let j = 0; j < n && assocPass; j++) {
      for (let k = 0; k < n; k++) {
        const a = elements[i], b = elements[j], c = elements[k];
        const lhs = apply(apply(a, b), c);
        const rhs = apply(a, apply(b, c));
        if (lhs !== rhs) {
          assocPass = false;
          assocWitness = `Counterexample: (${a} ★ ${b}) ★ ${c} = ${lhs} ≠ ${rhs} = ${a} ★ (${b} ★ ${c})`;
          break;
        }
      }
    }
  }

  // 2. Commutativity: a * b == b * a
  let commPass = true;
  let commWitness = "";
  for (let i = 0; i < n && commPass; i++) {
    for (let j = 0; j < n; j++) {
      const a = elements[i], b = elements[j];
      const ab = apply(a, b);
      const ba = apply(b, a);
      if (ab !== ba) {
        commPass = false;
        commWitness = `Counterexample: ${a} ★ ${b} = ${ab} ≠ ${ba} = ${b} ★ ${a}`;
        break;
      }
    }
  }

  // 3. Identity: exists e s.t. e * a == a * e == a
  let identity = null;
  for (const candidate of elements) {
    let isId = true;
    for (const a of elements) {
      if (apply(candidate, a) !== a || apply(a, candidate) !== a) {
        isId = false;
        break;
      }
    }
    if (isId) {
      identity = candidate;
      break;
    }
  }

  // 4. Inverses (if identity exists)
  let invertPass = Boolean(identity);
  let invertWitness = "";
  if (identity) {
    for (const a of elements) {
      let hasInv = false;
      for (const b of elements) {
        if (apply(a, b) === identity && apply(b, a) === identity) {
          hasInv = true;
          break;
        }
      }
      if (!hasInv) {
        invertPass = false;
        invertWitness = `Element ${a} has no two-sided inverse for identity ${identity}`;
        break;
      }
    }
  } else {
    invertWitness = "No identity element exists in carrier";
  }

  // 5. Idempotency: a * a == a
  let idemPass = true;
  let idemWitness = "";
  for (const a of elements) {
    if (apply(a, a) !== a) {
      idemPass = false;
      idemWitness = `Element ${a} ★ ${a} = ${apply(a, a)} ≠ ${a}`;
      break;
    }
  }

  const laws = [
    {
      name: "Associativity",
      formula: "∀ a, b, c ∈ S: (a ★ b) ★ c = a ★ (b ★ c)",
      pass: assocPass,
      witness: assocWitness,
    },
    {
      name: "Commutativity",
      formula: "∀ a, b ∈ S: a ★ b = b ★ a",
      pass: commPass,
      witness: commWitness,
    },
    {
      name: "Identity Element",
      formula: "∃ e ∈ S s.t. ∀ a ∈ S: e ★ a = a ★ e = a",
      pass: Boolean(identity),
      witness: identity ? `Identity element e = ${identity}` : "No two-sided identity element found",
    },
    {
      name: "Invertibility",
      formula: "∀ a ∈ S, ∃ a⁻¹ ∈ S: a ★ a⁻¹ = a⁻¹ ★ a = e",
      pass: invertPass,
      witness: invertWitness,
    },
    {
      name: "Idempotency",
      formula: "∀ a ∈ S: a ★ a = a",
      pass: idemPass,
      witness: idemWitness,
    },
  ];

  for (const law of laws) {
    const card = document.createElement("div");
    card.className = "genesis-law-card";

    const header = document.createElement("div");
    header.className = "genesis-law-header";
    const name = document.createElement("span");
    name.className = "genesis-law-name";
    name.textContent = law.name;

    const status = document.createElement("span");
    status.className = `genesis-law-status ${law.pass ? "pass" : "fail"}`;
    status.textContent = law.pass ? "ADMITTED" : "REFUSED";

    header.appendChild(name);
    header.appendChild(status);
    card.appendChild(header);

    const formula = document.createElement("div");
    formula.className = "genesis-law-formula";
    formula.textContent = law.formula;
    card.appendChild(formula);

    if (law.witness) {
      const witness = document.createElement("div");
      witness.className = "genesis-law-witness";
      witness.textContent = law.witness;
      card.appendChild(witness);
    }

    list.appendChild(card);
  }
}

export function findGenesisMorphisms() {
  const section = $("genesis-morphisms-section");
  const output = $("genesis-morphisms-output");
  if (!section || !output) return;

  const presetKey = genesisState.selectedPreset;
  const world = GENESIS_WORLDS[presetKey];
  if (!world) return;

  const elements = world.elements;
  const table = world.operators[genesisState.selectedOp];
  const n = elements.length;
  const elMap = new Map();
  elements.forEach((el, idx) => elMap.set(el, idx));

  const apply = (a, b) => table[elMap.get(a)][elMap.get(b)];

  output.replaceChildren();

  // Find automorphisms phi: S -> S
  const generateMaps = (idx, current) => {
    if (idx === n) return [current];
    const res = [];
    for (let i = 0; i < n; i++) {
      res.push(...generateMaps(idx + 1, [...current, elements[i]]));
    }
    return res;
  };

  const allMaps = generateMaps(0, []);
  const validMorphisms = [];

  for (const candidate of allMaps) {
    const phi = (x) => candidate[elMap.get(x)];
    let isHom = true;
    for (let i = 0; i < n && isHom; i++) {
      for (let j = 0; j < n; j++) {
        const a = elements[i], b = elements[j];
        if (phi(apply(a, b)) !== apply(phi(a), phi(b))) {
          isHom = false;
          break;
        }
      }
    }
    if (isHom) {
      const isBijective = new Set(candidate).size === n;
      validMorphisms.push({ map: candidate, isBijective });
    }
  }

  const card = document.createElement("div");
  card.className = "genesis-morphism-card";

  const isoCount = validMorphisms.filter((m) => m.isBijective).length;
  const homCount = validMorphisms.length;

  // ubs:ignore — world/elements escaped; isoCount/homCount are numbers
  card.innerHTML = `
    <div><strong>Automorphism Group Aut(${escapeHtml(world.name)}):</strong> |Aut| = ${isoCount} (Total Endomorphisms: ${homCount})</div>
    <div style="margin-top: 0.4rem;">Structure Preserving Mappings:</div>
    <ul style="margin: 0.3rem 0; padding-left: 1.2rem;">
      ${validMorphisms
        .map(
          (m, idx) =>
            `<li>ϕ_${idx + 1}: { ${elements.map((el, i) => `${escapeHtml(el)} ↦ ${escapeHtml(m.map[i])}`).join(", ")} } ${m.isBijective ? "<em>(Isomorphism ≅)</em>" : "<em>(Endomorphism)</em>"}</li>`,
        )
        .join("")}
    </ul>
  `;

  output.appendChild(card);
  section.hidden = false;
}

function wireUi() {
  $("btn-run")?.addEventListener("click", guard(runRun));
  $("btn-run-given")?.addEventListener("click", guard(runWithGiven));
  $("btn-check")?.addEventListener("click", guard(runCheck));
  $("btn-plan")?.addEventListener("click", guard(runPlan));
  $("btn-mig")?.addEventListener("click", guard(runMig));
  $("btn-generate")?.addEventListener("click", guard(runGenerate));
  $("btn-format")?.addEventListener("click", guard(runFormat));
  $("btn-symbolify")?.addEventListener("click", guard(toggleSymbolify));
  $("btn-swap-layout")?.addEventListener("click", guard(togglePaneLayout));
  $("plot-x-var")?.addEventListener("change", guard(() => { plotState.xVar = $("plot-x-var").value; drawPlot(); }));
  $("plot-y-var")?.addEventListener("change", guard(() => { plotState.yVar = $("plot-y-var").value; drawPlot(); }));
  $("plot-min-x")?.addEventListener("input", guard(() => {
    const v = Number($("plot-min-x").value);
    if (Number.isFinite(v)) {
      plotState.minX = v;
      drawPlot();
    }
  }));
  $("plot-max-x")?.addEventListener("input", guard(() => {
    const v = Number($("plot-max-x").value);
    if (Number.isFinite(v)) {
      plotState.maxX = v;
      drawPlot();
    }
  }));
  $("plot-samples")?.addEventListener("change", guard(() => {
    const v = Number($("plot-samples").value);
    if (Number.isFinite(v)) {
      plotState.samples = Math.max(10, Math.min(1000, v));
      drawPlot();
    }
  }));
  $("btn-plot-autoscale")?.addEventListener("click", guard(autoScalePlot));
  $("btn-plot-reset")?.addEventListener("click", guard(resetPlotView));
  $("btn-plot-export")?.addEventListener("click", guard(exportPlotPng));
  $("btn-copy-latex")?.addEventListener("click", guard(copyLatexToClipboard));
  $("btn-toggle-latex-raw")?.addEventListener("click", guard(toggleLatexRaw));
  $("genesis-world-preset")?.addEventListener("change", guard(updateGenesisView));
  $("genesis-op-select")?.addEventListener("change", guard(() => { genesisState.selectedOp = $("genesis-op-select").value; updateGenesisView(); }));
  $("btn-find-morphisms")?.addEventListener("click", guard(findGenesisMorphisms));
  $("chk-auto-run")?.addEventListener("change", guard((e) => {
    if (e.target.checked) {
      triggerLiveEval();
    }
  }));
  $("examples")?.addEventListener(
    "change",
    guard((event) => {
      const editor = $("editor");
      if (editor && event.target.value) {
        editor.value = event.target.value;
        try {
          localStorage.setItem("emath_editor_draft", editor.value);
        } catch {}
        updateSymbolifyButton();
      }
    }),
  );
  $("btn-help")?.addEventListener("click", guard(() => openLegend("shortcuts")));
  $("btn-share")?.addEventListener("click", guard(shareSource));
  $("btn-legend-close")?.addEventListener("click", guard(closeLegend));
  for (const tabBtn of document.querySelectorAll(".legend-tab-btn")) {
    tabBtn.addEventListener(
      "click",
      guard(() => switchLegendTab(tabBtn.dataset.legendTab)),
    );
  }
  $("legend")?.addEventListener(
    "click",
    guard((event) => {
      if (event.target === $("legend")) {
        closeLegend();
      }
    }),
  );
  $("legend-search")?.addEventListener(
    "input",
    guard((event) => filterLegend(event.target.value ?? "")),
  );
  const editorDraftDebounced = debounce(() => {
    try {
      localStorage.setItem("emath_editor_draft", sourcePayload());
    } catch {}
    updateSymbolifyButton();
  }, 250);
  $("editor")?.addEventListener("input", editorDraftDebounced);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      if (liveEvalDebounceTimer) {
        clearTimeout(liveEvalDebounceTimer);
        liveEvalDebounceTimer = null;
      }
      editorDraftDebounced.cancel();
    }
  });
  window.addEventListener(
    "hashchange",
    guard(() => {
      const fromHash = readSourceFromHash();
      const editor = $("editor");
      if (editor && fromHash && editor.value !== fromHash) {
        editor.value = fromHash;
        updateSymbolifyButton();
      }
    }),
  );
  $("generated-files")?.addEventListener(
    "change",
    guard((event) => {
      const index = Number(event.target.value);
      const file = Number.isInteger(index) ? generatedFiles[index] : undefined;
      const node = $("out-generated");
      if (node) {
        node.textContent = file?.content ?? "";
      }
    }),
  );
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.addEventListener(
      "click",
      guard(() => showTab(button.dataset.tab)),
    );
  }
  window.addEventListener(
    "keydown",
    guard((event) => {
      if (event.key === "Escape") {
        const legend = $("legend");
        if (legend && !legend.hidden) {
          event.preventDefault();
          closeLegend();
          return;
        }
      }
      if (((event.ctrlKey || event.metaKey) && (event.key === "k" || event.key === "K")) || event.key === "F1") {
        event.preventDefault();
        openLegend();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key === "\\") {
        event.preventDefault();
        togglePaneLayout();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && (event.key === "r" || event.key === "R") && !event.shiftKey && !event.altKey) {
        event.preventDefault();
        runRun();
        return;
      }
      if (event.shiftKey && event.altKey && (event.key === "f" || event.key === "F")) {
        event.preventDefault();
        runFormat();
        return;
      }
      if (event.altKey && !event.shiftKey && !event.ctrlKey && !event.metaKey && (event.key === "p" || event.key === "P")) {
        event.preventDefault();
        runPlan();
        return;
      }
      if (event.altKey && !event.shiftKey && !event.ctrlKey && !event.metaKey && (event.key === "g" || event.key === "G")) {
        event.preventDefault();
        runMig();
        return;
      }
      if (event.altKey && !event.shiftKey && !event.ctrlKey && !event.metaKey && (event.key === "c" || event.key === "C")) {
        event.preventDefault();
        runGenerate();
        return;
      }
      if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        const num = parseInt(event.key, 10);
        if (!isNaN(num) && num >= 1 && num <= 9) {
          const tabNames = ["run", "plot", "math", "genesis", "diagnostics", "plan", "mig", "generated", "raw"];
          if (tabNames[num - 1]) {
            event.preventDefault();
            showTab(tabNames[num - 1]);
            return;
          }
        }
      }
    }),
  );
  $("editor")?.addEventListener(
    "keydown",
    guard((event) => {
      const editor = event.target;
      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        if (event.shiftKey) {
          runCheck();
        } else {
          runRun();
        }
        return;
      }
      if (
        (event.ctrlKey || event.metaKey) &&
        event.shiftKey &&
        (event.key === "Y" || event.key === "y")
      ) {
        event.preventDefault();
        toggleSymbolify();
        return;
      }
      if (event.altKey && (event.key === "s" || event.key === "S")) {
        event.preventDefault();
        toggleSymbolify();
        return;
      }
      if (event.key === "Tab") {
        if (event.shiftKey) {
          handleEditorShiftTab(editor, event);
        } else {
          handleEditorTab(editor, event);
        }
        return;
      }
      if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && !event.altKey) {
        handleEditorEnter(editor, event);
        return;
      }
      if (event.key === "Backspace" && !event.ctrlKey && !event.metaKey && !event.altKey) {
        if (handleEditorBackspace(editor, event)) {
          return;
        }
      }
    }),
  );
}

export async function boot() {
  showWasmMissing(false);
  try {
    const savedLayout = localStorage.getItem(STORAGE_LAYOUT_KEY);
    applyPaneLayout(savedLayout === "swapped");
  } catch {}
  wireUi();
  updateSymbolifyButton();
  try {
    const wasm = await instantiateWasm(WASM_URL);
    emRun = makeEmRun(wasm.instance);
    showWasmMissing(false);
  } catch (error) {
    showWasmMissing(true, error.message, () => boot());
    setStatus(`fail: ${error.message || error}`, "fail");
    return;
  }
  try {
    const started = performance.now();
    const version = emRun("version", "");
    const ms = Math.round(performance.now() - started);
    setRaw(version);
    const span = $("version");
    if (span) {
      span.textContent = version.version ?? "";
    }
    setStatus(
      `version ${ms} ms ${version.ok === false ? "fail" : "ok"}`,
      version.ok === false ? "fail" : "ok",
    );
  } catch (error) {
    setStatus(`version fail: ${error.message || error}`, "fail");
  }
  try {
    const examples = emRun("examples", "");
    setRaw(examples);
    fillExamples(examples);
  } catch (error) {
    setStatus(`examples fail: ${error.message || error}`, "fail");
  }
}

if (typeof document !== "undefined") {
  boot().catch((error) => {
    setStatus(`fail: ${error.message || error}`, "fail");
  });
}
