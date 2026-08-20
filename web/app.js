const WASM_URL = "/emath.wasm";
const WASM_MISSING = "emath.wasm not found — run `cargo xtask build-web`";
const SOURCE_HASH_PREFIX = "#src=";

const $ = (id) => document.getElementById(id);

let emRun = null;
let generatedFiles = [];

export function showWasmMissing(visible = true) {
  const banner = $("wasm-missing");
  if (banner) {
    banner.hidden = !visible;
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
  const { em_alloc, em_free, em_run, memory } = instance.exports;
  if (typeof em_alloc !== "function" || typeof em_run !== "function") {
    throw new Error("wasm module missing em_alloc/em_run");
  }
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  return function emRunOp(op, payload) {
    const opBytes = encoder.encode(String(op));
    const payloadBytes = encoder.encode(
      payload === null || payload === undefined ? "" : String(payload),
    );
    const opPtr = em_alloc(opBytes.length);
    const payloadPtr = em_alloc(payloadBytes.length);
    new Uint8Array(memory.buffer, opPtr, opBytes.length).set(opBytes);
    if (payloadBytes.length > 0) {
      new Uint8Array(memory.buffer, payloadPtr, payloadBytes.length).set(payloadBytes);
    }
    const ret = em_run(opPtr, opBytes.length, payloadPtr, payloadBytes.length);
    const result = typeof ret === "bigint" ? ret : BigInt(ret);
    const ptr = Number(result >> 32n);
    const len = Number(result & 0xffffffffn);
    const jsonBytes = new Uint8Array(memory.buffer, ptr, len).slice();
    const text = decoder.decode(jsonBytes);
    if (typeof em_free === "function") {
      em_free(ptr, len);
      em_free(opPtr, opBytes.length);
      em_free(payloadPtr, payloadBytes.length);
    }
    return JSON.parse(text);
  };
}

export async function instantiateWasm(url = WASM_URL) {
  const response = await fetch(url);
  if (!response.ok) {
    const error = new Error(WASM_MISSING);
    error.code = "WASM_MISSING";
    throw error;
  }
  if (typeof WebAssembly.instantiateStreaming === "function") {
    try {
      return await WebAssembly.instantiateStreaming(response, {});
    } catch {
      const retry = await fetch(url);
      const bytes = await retry.arrayBuffer();
      return WebAssembly.instantiate(bytes, {});
    }
  }
  const bytes = await response.arrayBuffer();
  return WebAssembly.instantiate(bytes, {});
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

function showTab(name) {
  for (const button of document.querySelectorAll(".tabs [data-tab]")) {
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
  if (!node) {
    return;
  }
  node.replaceChildren();
  const items = Array.isArray(result.diagnostics) ? result.diagnostics : [];
  if (items.length === 0) {
    node.textContent = "no diagnostics — package admits";
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
  return (...args) => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => fn(...args), ms);
  };
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
  if (examples.length > 0 && editor && !editor.value) {
    editor.value = examples[0].source ?? "";
    select.selectedIndex = 1;
    writeSourceToHash(editor.value);
  }
}

const LEGEND = [
  [
    "Sections",
    [
      ["inputs", "declared parameters; bound by given or the pane fields"],
      ["outputs", "named results of the declaration"],
      ["definitions", "equations that compute outputs from inputs/state"],
      ["goals", "what to produce (evaluate, produce rust.library)"],
      ["tests", "contains example <name>: blocks"],
      ["state", "persistent fields for emath policy"],
      ["constructors", "Self: assignment plus require/ensure"],
      ["compile", "target/profile/numeric model"],
      ["exports", "names visible outside the package"],
      ["about", "prose metadata; not executed"],
      ["evidence", "checker certificates attached to a goal"],
      ["host", "lab/host experiment binding"],
    ],
  ],
  [
    "Tests",
    [
      ["example <name>:", "one test; empty body is a worked example"],
      ["example name:", "same, two-word head"],
      ["given x = …", "binds an input or constructor parameter"],
      ["expect …", "Boolean assertion; omit for a computed/worked run"],
    ],
  ],
  [
    "Ops & Tools",
    [
      ["Run", "Tier-0 interpreter (strict-f64); not compiled Rust (Ctrl+Enter)"],
      ["Check", "admit the package; diagnostics only (Ctrl+Shift+Enter)"],
      ["Plan", "goal requests and resolution plans"],
      ["Intent Graph", "SIR MIG canonical form"],
      ["Generate Rust", "in-memory rust-backend files; not executed here"],
      ["Format", "comment-preserving formatter"],
      ["Symbolify", "toggle LaTeX aliases (\\alpha) and Unicode math (α) (Ctrl+Shift+Y)"],
      ["Swap Panes", "swap editor and output pane positions"],
    ],
  ],
  [
    "Editor Shortcuts",
    [
      ["Tab", "indent 4 spaces (multi-line selection or cursor)"],
      ["Shift+Tab", "outdent up to 4 spaces"],
      ["Enter", "auto-indent with indent increase after ':'"],
      ["Ctrl/Cmd+Shift+Y", "toggle Symbolify / ASCII-fy on selection or buffer"],
      ["Alt+S", "toggle Symbolify / ASCII-fy"],
    ],
  ],
  [
    "Tabs",
    [
      ["Run", "values and test verdicts"],
      ["Diagnostics", "E-* / N-* codes from admit"],
      ["Plan", "planner requests"],
      ["Intent Graph", "MIG"],
      ["Generated", "rust-backend files"],
      ["Raw JSON", "engine response"],
    ],
  ],
  [
    "Tiers / authority",
    [
      ["interpreted-strict-f64", "browser run; IEEE binary64 + platform libm"],
      ["structural", "genesis authority when no checker ran"],
      ["tested / certified / proved", "never invented by Run or empty checker_receipts"],
    ],
  ],
  ["Notes", [["N-TYPE-001", "untyped head-arg / free name defaulted to Float64"]]],
  [
    "Error families",
    [
      ["E-SYN-*", "syntax/layout"],
      ["E-NAME-*", "names/visibility"],
      ["E-SEC-*", "section outside the Phase 1 subset"],
      ["E-TYPE-*", "type/refinement"],
      ["E-UNIT-*", "units"],
      ["E-KIND-*", "custom kind"],
      ["E-GOAL-*", "requests/planning"],
      ["E-GEN-*", "semantic genesis"],
      ["E-LOCK-*", "meaning lock"],
      ["E-NUM-*", "numeric models"],
      ["E-HOST-*", "host/lab"],
      ["E-TLT-*", "tooling/CLI"],
    ],
  ],
];

function renderLegend() {
  const body = $("legend-body");
  if (!body) {
    return;
  }
  body.replaceChildren();
  for (const [title, rows] of LEGEND) {
    const group = document.createElement("section");
    group.className = "legend-group";
    const heading = document.createElement("h3");
    heading.textContent = title;
    group.appendChild(heading);
    for (const [itemName, itemRole] of rows) {
      const row = document.createElement("div");
      row.className = "legend-row";
      row.dataset.search = `${itemName} ${itemRole}`.toLowerCase();
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = itemName;
      const role = document.createElement("span");
      role.textContent = itemRole;
      row.appendChild(name);
      row.appendChild(role);
      group.appendChild(row);
    }
    body.appendChild(group);
  }
}

function filterLegend(query) {
  const needle = query.trim().toLowerCase();
  for (const row of document.querySelectorAll("#legend-body .legend-row")) {
    row.classList.toggle("hidden", Boolean(needle) && !row.dataset.search.includes(needle));
  }
}

function openLegend() {
  const overlay = $("legend");
  if (!overlay) {
    return;
  }
  if (!$("legend-body")?.childElementCount) {
    renderLegend();
  }
  overlay.hidden = false;
  $("legend-search")?.focus();
}

function closeLegend() {
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
  writeSourceToHash(editor.value);
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
      ? "Panes swapped (Editor Right, Output Left) — click to reset"
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
  points: [],
  inputs: [],
  outputs: [],
  secondaryValues: {},
  canvasInitialized: false,
};

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

  canvas.addEventListener("mousedown", (e) => {
    plotState.isDragging = true;
    plotState.dragStart = { x: e.clientX, y: e.clientY };
    plotState.dragBounds = {
      minX: plotState.minX,
      maxX: plotState.maxX,
      minY: plotState.minY,
      maxY: plotState.maxY,
    };
  });

  window.addEventListener("mousemove", (e) => {
    if (plotState.isDragging && plotState.dragBounds) {
      const rect = canvas.getBoundingClientRect();
      const dx = ((e.clientX - plotState.dragStart.x) / rect.width) * (plotState.dragBounds.maxX - plotState.dragBounds.minX);
      const dy = ((e.clientY - plotState.dragStart.y) / rect.height) * (plotState.dragBounds.maxY - plotState.dragBounds.minY);
      plotState.minX = plotState.dragBounds.minX - dx;
      plotState.maxX = plotState.dragBounds.maxX - dx;
      plotState.minY = plotState.dragBounds.minY + dy;
      plotState.maxY = plotState.dragBounds.maxY + dy;
      plotState.autoScaleY = false;
      const minXInput = $("plot-min-x");
      const maxXInput = $("plot-max-x");
      if (minXInput) minXInput.value = plotState.minX.toFixed(2);
      if (maxXInput) maxXInput.value = plotState.maxX.toFixed(2);
      drawPlot();
    }
  });

  window.addEventListener("mouseup", () => {
    plotState.isDragging = false;
    plotState.dragBounds = null;
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
    const minXInput = $("plot-min-x");
    const maxXInput = $("plot-max-x");
    if (minXInput) minXInput.value = plotState.minX.toFixed(2);
    if (maxXInput) maxXInput.value = plotState.maxX.toFixed(2);
    drawPlot();
  }, { passive: false });

  canvas.addEventListener("mousemove", (e) => {
    if (plotState.isDragging || plotState.points.length === 0) return;
    const rect = canvas.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    // Convert mouseX to math X
    const mathX = plotState.minX + (mouseX / rect.width) * (plotState.maxX - plotState.minX);

    // Find closest point
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
      const canvasX = ((closest.x - plotState.minX) / (plotState.maxX - plotState.minX)) * rect.width;
      const canvasY = ((plotState.maxY - closest.y) / (plotState.maxY - plotState.minY)) * rect.height;
      tooltip.style.left = `${canvasX}px`;
      tooltip.style.top = `${canvasY}px`;
      tooltip.textContent = `${plotState.xVar ?? "x"} = ${closest.x.toFixed(3)}, ${plotState.yVar ?? "y"} = ${closest.y.toFixed(3)}`;
      tooltip.hidden = false;
    }
  });

  canvas.addEventListener("mouseleave", () => {
    const tooltip = $("plot-tooltip");
    if (tooltip) tooltip.hidden = true;
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
  if (rect.width <= 0 || rect.height <= 0) return;

  const dpr = window.devicePixelRatio || 1;
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.scale(dpr, dpr);

  const width = rect.width;
  const height = rect.height;

  // Compute points
  const points = [];
  const xVar = plotState.xVar ?? "x";
  const yVar = plotState.yVar ?? "y";
  const numSamples = plotState.samples;
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
  }

  // Clear
  ctx.fillStyle = "#151619";
  ctx.fillRect(0, 0, width, height);

  // Coordinate transforms
  const toScreenX = (x) => ((x - plotState.minX) / (plotState.maxX - plotState.minX)) * width;
  const toScreenY = (y) => ((plotState.maxY - y) / (plotState.maxY - plotState.minY)) * height;

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

    if (line.startsWith("emath function") || line.startsWith("function")) {
      const match = line.match(/(?:emath\s+)?function\s+([a-zA-Z0-9_]+)/);
      if (match) currentDecl = match[1];
      inDefinitions = false;
      inInputs = false;
    } else if (line.startsWith("inputs:")) {
      inInputs = true;
      inDefinitions = false;
    } else if (line.startsWith("definitions:")) {
      inDefinitions = true;
      inInputs = false;
    } else if (inInputs && line.includes(":")) {
      const parts = line.split(":");
      inputsList.push({ name: parts[0].trim(), type: parts[1].trim() });
    } else if (inDefinitions && line.includes("=")) {
      const [lhs, rhs] = line.split("=").map((s) => s.trim());
      equations.push({ lhs, rhs });
    }
  }

  // Create Decl Card
  const card = document.createElement("div");
  card.className = "math-decl-card";

  const title = document.createElement("div");
  title.className = "math-decl-title";
  const inputsSig = inputsList.map((inp) => `${symbolify(inp.name)} ∈ ℝ`).join(", ");
  title.textContent = `Function ${currentDecl}(${inputsSig}) ⟹ Outputs`;
  card.appendChild(title);

  const eqList = document.createElement("div");
  eqList.className = "math-equation-list";

  latexLines.push(`\\text{Function } \\mathrm{${currentDecl}}(${inputsList.map((i) => asciify(i.name)).join(", ")})`);
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

  card.appendChild(eqList);
  container.appendChild(card);

  rawPre.textContent = latexLines.join("\n");
}

export function formatMathExprHtml(expr) {
  let res = symbolify(expr);
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

  card.innerHTML = `
    <div><strong>Automorphism Group Aut(${world.name}):</strong> |Aut| = ${isoCount} (Total Endomorphisms: ${homCount})</div>
    <div style="margin-top: 0.4rem;">Structure Preserving Mappings:</div>
    <ul style="margin: 0.3rem 0; padding-left: 1.2rem;">
      ${validMorphisms
        .map(
          (m, idx) =>
            `<li>ϕ_${idx + 1}: { ${elements.map((el, i) => `${el} ↦ ${m.map[i]}`).join(", ")} } ${m.isBijective ? "<em>(Isomorphism ≅)</em>" : "<em>(Endomorphism)</em>"}</li>`,
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
  $("plot-min-x")?.addEventListener("input", guard(() => { plotState.minX = Number($("plot-min-x").value); drawPlot(); }));
  $("plot-max-x")?.addEventListener("input", guard(() => { plotState.maxX = Number($("plot-max-x").value); drawPlot(); }));
  $("plot-samples")?.addEventListener("change", guard(() => { plotState.samples = Number($("plot-samples").value); drawPlot(); }));
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
        writeSourceToHash(editor.value);
        updateSymbolifyButton();
      }
    }),
  );
  $("btn-help")?.addEventListener("click", guard(openLegend));
  $("btn-share")?.addEventListener("click", guard(shareSource));
  $("btn-legend-close")?.addEventListener("click", guard(closeLegend));
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
  $("editor")?.addEventListener(
    "input",
    debounce(() => {
      writeSourceToHash(sourcePayload());
      updateSymbolifyButton();
    }, 250),
  );
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
      const file = generatedFiles[index];
      const node = $("out-generated");
      if (node) {
        node.textContent = file?.content ?? "";
      }
    }),
  );
  for (const button of document.querySelectorAll(".tabs [data-tab]")) {
    button.addEventListener(
      "click",
      guard(() => showTab(button.dataset.tab)),
    );
  }
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
  } catch (error) {
    if (error.code === "WASM_MISSING" || String(error.message).includes("emath.wasm not found")) {
      showWasmMissing(true);
    }
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
