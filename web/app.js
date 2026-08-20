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
    const payloadBytes = encoder.encode(payload == null ? "" : String(payload));
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

function renderInputFields(inputsResult, prefills) {
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
      const label = document.createElement("label");
      label.className = "input-field";
      const typeName = input.type ?? "Float64";
      label.textContent = `${name} (${typeName})`;
      const control = document.createElement("input");
      control.type = "number";
      control.step = "any";
      control.dataset.input = name;
      const prefill = prefills[name];
      if (typeof prefill === "number" && Number.isFinite(prefill)) {
        control.value = String(prefill);
      }
      label.appendChild(control);
      fields.appendChild(label);
    }
  }
  panel.hidden = count === 0;
}

function refreshPaneChrome(result) {
  showDesugared(result);
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

function collectGiven() {
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
    "Ops",
    [
      ["Run", "Tier-0 interpreter (strict-f64); not compiled Rust"],
      ["Check", "admit the package; diagnostics only"],
      ["Plan", "goal requests and resolution plans"],
      ["Intent Graph", "SIR MIG canonical form"],
      ["Generate Rust", "in-memory rust-backend files; not executed here"],
      ["Format", "comment-preserving formatter"],
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

function wireUi() {
  $("btn-run")?.addEventListener("click", guard(runRun));
  $("btn-run-given")?.addEventListener("click", guard(runWithGiven));
  $("btn-check")?.addEventListener("click", guard(runCheck));
  $("btn-plan")?.addEventListener("click", guard(runPlan));
  $("btn-mig")?.addEventListener("click", guard(runMig));
  $("btn-generate")?.addEventListener("click", guard(runGenerate));
  $("btn-format")?.addEventListener("click", guard(runFormat));
  $("examples")?.addEventListener(
    "change",
    guard((event) => {
      const editor = $("editor");
      if (editor && event.target.value) {
        editor.value = event.target.value;
        writeSourceToHash(editor.value);
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
    debounce(() => writeSourceToHash(sourcePayload()), 250),
  );
  window.addEventListener(
    "hashchange",
    guard(() => {
      const fromHash = readSourceFromHash();
      const editor = $("editor");
      if (editor && fromHash && editor.value !== fromHash) {
        editor.value = fromHash;
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
      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        if (event.shiftKey) {
          runCheck();
        } else {
          runRun();
        }
      }
    }),
  );
}

export async function boot() {
  showWasmMissing(false);
  wireUi();
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
