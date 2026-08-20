# emath User Manual & Dogfooding Field Guide

Welcome to **emath**, a Rust-first language, compiler, and optimization workbench for turning mathematical intent into inspectable, executable software.

This manual serves as your complete operating guide for dogfooding, testing, and developing with emath. It covers the core mathematical philosophy, the full Obsidian Laboratory web workbench, the `.emath` language syntax, all nine viewport engines, and five step-by-step hands-on tutorials.

---

## Table of Contents

1. [Part I: The Core Concept of emath](#part-i-the-core-concept-of-emath)
   - [Declarative Mathematical Intent](#declarative-mathematical-intent)
   - [The Deterministic Pipeline](#the-deterministic-pipeline)
   - [Zero-Ceremony Desugaring](#zero-ceremony-desugaring)
2. [Part II: The Obsidian Laboratory Workbench](#part-ii-the-obsidian-laboratory-workbench)
   - [Header Action Bar & Control Clusters](#header-action-bar--control-clusters)
   - [Editor Pane, Live Sliders, and Desugaring Inspector](#editor-pane-live-sliders-and-desugaring-inspector)
   - [Viewport Panes (The 9 Output Engines)](#viewport-panes-the-9-output-engines)
   - [The 4-Tab Legend & Cheatsheet Drawer](#the-4-tab-legend--cheatsheet-drawer)
   - [Keyboard Shortcuts Reference](#keyboard-shortcuts-reference)
3. [Part III: The .emath Language Reference](#part-iii-the-emath-language-reference)
   - [Structure of a .emath File](#structure-of-a-emath-file)
   - [Declarations: function vs package vs policy](#declarations-function-vs-package-vs-policy)
   - [Sections: inputs, definitions, goals, tests, compile](#sections-inputs-definitions-goals-tests-compile)
   - [Types, Domains, and Dimensions](#types-domains-and-dimensions)
   - [Mathematical Symbols: Unicode and ASCII/LaTeX Aliases](#mathematical-symbols-unicode-and-asciilatex-aliases)
4. [Part IV: 5 Hands-On Dogfooding Walkthroughs](#part-iv-5-hands-on-dogfooding-walkthroughs)
   - [Tutorial 1: The 60-Second Scratchpad (Quick Calculations)](#tutorial-1-the-60-second-scratchpad-quick-calculations)
   - [Tutorial 2: Dynamic Function Plotting (2D Visualizer)](#tutorial-2-dynamic-function-plotting-2d-visualizer)
   - [Tutorial 3: Mathematical Intent Typography & LaTeX Export](#tutorial-3-mathematical-intent-typography--latex-export)
   - [Tutorial 4: Finite Worlds, Cayley Matrices & Axiomatic Laws](#tutorial-4-finite-worlds-cayley-matrices--axiomatic-laws)
   - [Tutorial 5: Compiling Declarative Math to Executable Rust](#tutorial-5-compiling-declarative-math-to-executable-rust)
5. [Part V: CLI & Terminal Reference](#part-v-cli--terminal-reference)
   - [Running the Local Workbench (`emath web`)](#running-the-local-workbench-emath-web)
   - [Batch Validation & Artifact Verification](#batch-validation--artifact-verification)
   - [Diagnostic Codes & Error Recovery](#diagnostic-codes--error-recovery)

---

## Part I: The Core Concept of emath

### Declarative Mathematical Intent

Traditional programming languages force you to turn mathematical formulas into procedural steps: memory allocations, iteration loops, order-dependent assignments, and low-level float primitives. 

In **emath**, you write the mathematical relationships as declarative equations. You state:
- What the domain and inputs are (`inputs: x: Float64`).
- What mathematical laws and equations govern the system (`y = x * x * x`).
- What goals you want achieved (`goal evaluate y`, `goal differentiate y wrt x`).
- What tests and assertions prove correctness (`given x = 3`, `expect y == 27`).

The emath compiler analyzes your declarations, constructs a Mathematical Intermediate Graph (MIG), synthesizes an optimal execution plan, verifies all constraints, and generates optimized Rust software or evaluates results in-browser via a strict-f64 interpreter.

### The Deterministic Pipeline

```
  ┌───────────────────────────┐
  │      .emath Source        │  (Unicode math or ASCII/LaTeX)
  └─────────────┬─────────────┘
                │
                ▼
  ┌───────────────────────────┐
  │  Desugarer & Parser AST   │  (Lossless round-trippable AST)
  └─────────────┬─────────────┘
                │
                ▼
  ┌───────────────────────────┐
  │   Semantic IR & Checker   │  (Type checking, units, dimensions)
  └─────────────┬─────────────┘
                │
                ▼
  ┌───────────────────────────┐
  │ Mathematical Graph (MIG)  │  (Canonical content-addressed graph)
  └─────────────┬─────────────┘
                │
                ▼
  ┌───────────────────────────┐
  │   Goal & Plan Synthesis   │  (Provider selection, execution DAG)
  └─────────────┬─────────────┘
                │
        ┌───────┴───────┐
        ▼               ▼
  ┌───────────┐   ┌───────────┐
  │ Interpreter│  │Rust Backend│ (Strict f64 execution or
  │ (In-Browser│  │ (Cargo lib │  production Rust code)
  └───────────┘   └───────────┘
```

### Zero-Ceremony Desugaring

To make experimentation frictionless, emath allows **bare expressions** in the playground. If you write:

```emath
y = x * x + 4
```

The emath desugarer automatically synthesizes a complete, well-formed package:

```emath
emath function Pane:
    inputs:
        x: Float64
    definitions:
        y = x * x + 4
```

You can inspect the desugared output at any time by expanding the **Desugared AST** drawer above the editor.

---

## Part II: The Obsidian Laboratory Workbench

When you launch `emath web`, you enter the **Obsidian Laboratory**, a high-density, dark-themed scientific workbench designed for rapid math authoring and interactive verification.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [≡] emath lab  [Examples ▼]  [▶ Run] [✓ Check] │ [📋][🕸][⚙][⌥F] │ [∑][◧] [⌘K][🔗]│
├───────────────────────────────────┬────────────────────────────────────┤
│ EDITOR PANE                       │ VIEWPORT PANE                      │
│ ▾ Desugared AST                   │ [▶ Run][📈 Plot][∑ Math][✦ Genesis]│
│ ┌───────────────────────────────┐ │ [⚠ Diag][📋 Plan][🕸 MIG][⚙ Gen]   │
│ │ 1 emath function Cube:        │ ├────────────────────────────────────┤
│ │ 2   inputs:                   │ │ PASS test_three                    │
│ │ 3     x: Float64              │ │   given x = 3                      │
│ │ 4   definitions:              │ │   y = 27                           │
│ │ 5     y = x * x * x           │ │ 1 test, 1 passed, 0 failed         │
│ │ 6   tests:                    │ │                                    │
│ │ 7     test_three:             │ │                                    │
│ │ 8       given x = 3           │ │                                    │
│ │ 9       expect y == 27        │ │                                    │
│ └───────────────────────────────┘ │                                    │
│ [Live Sliders: x: [──●──] 3.0]    │                                    │
├───────────────────────────────────┴────────────────────────────────────┤
│ STATUS BAR: [● ready]  run 2 ms pass  127.0.0.1:7878  strict-f64       │
└────────────────────────────────────────────────────────────────────────┘
```

### Header Action Bar & Control Clusters

The top toolbar is divided into four functional clusters:

1. **Brand & Presets Cluster**:
   - **Logo**: Displays active emath engine status.
   - **Examples Dropdown**: Instant access to curated tutorials and capstone examples.

2. **Execution Cluster**:
   - **Run (`▶ Run` / `Ctrl+R` / `Cmd+Enter`)**: Executes the program in the strict-f64 interpreter and evaluates all test cases and worked examples.
   - **Check / Admit (`✓ Check` / `Shift+Cmd+Enter`)**: Validates syntax, types, and proof invariants without running execution loops.
   - **Auto-Run Toggle**: Automatically triggers evaluation on every keystroke after a 250ms debounce.

3. **Lowering & Compiler Cluster**:
   - **Plan (`📋 Plan` / `Alt+P`)**: Compiles declared goals into an execution plan.
   - **MIG (`🕸 Intent Graph` / `Alt+G`)**: Emits the Mathematical Intermediate Graph.
   - **Rust (`⚙ Generated` / `Alt+C`)**: Lowers declarative math to in-memory Rust source code.
   - **Format (`⌥F` / `Shift+Alt+F`)**: Re-formats `.emath` code using the lossless AST formatter.

4. **View & Utilities Cluster**:
   - **Symbolify (`∑ / ASCII` / `Shift+Cmd+Y`)**: Bi-directional converter between Unicode math symbols (`α, β, θ, √, ×`) and ASCII/LaTeX aliases (`\alpha, \beta, \theta, sqrt, *`).
   - **Swap Layout (`◧ Swap` / `Cmd+\`)**: Flips the editor and viewport panes (Left-Right vs Right-Left).
   - **Legend (`⌘K` / `Ctrl+K`)**: Opens the 4-tab interactive cheatsheet and keyboard reference.
   - **Share (`🔗 Share`)**: Encodes editor source into a shareable URL and copies it to clipboard.

---

### Editor Pane, Live Sliders, and Desugaring Inspector

The left pane contains the math authoring environment:

- **Indentation & Tab Handling**: Tab key inserts clean 4-space indentation; Shift+Tab unindents selected blocks.
- **Desugared AST Inspector**: Click the disclosure header above the editor to see exactly how shorthand or bare expressions are structured into strict semantic definitions.
- **Live Parameter Sliders**: When inputs are detected (e.g. `x: Float64`), interactive slider controls appear below the editor. Dragging sliders updates `given` parameter values in real time without editing source text.

---

### Viewport Panes (The 9 Output Engines)

The right pane provides nine dedicated engines:

#### 1. `▶ Run` (Interpreter & Test Assertions)
Executes all `tests:` and worked `example:` blocks using the in-browser `interpreted-strict-f64` runtime.
- Displays computed intermediate values and final outputs.
- Highlights assertion passes (`PASS`) in emerald and failures (`FAIL`) in crimson.
- Worked examples without `expect` assertions compute and display output values without error.

#### 2. `📈 Plot 2D` (Interactive Function Visualizer)
A high-performance continuous 2D function plotter.
- **X and Y Selectors**: Choose which input variable to sweep on the X-axis and which output to plot on the Y-axis.
- **Range & Sampling**: Configure Min X, Max X, and sample resolution (up to 1,000 points).
- **Secondary Parameter Sliders**: If the function takes multiple inputs (e.g. `v0`, `theta`, `g`), interactive sliders let you adjust secondary parameters live while watching the curve update.
- **Canvas Interaction**: Click and drag to pan the viewport; mouse wheel to zoom in and out.
- **Inspection Tooltips**: Hover over the curve to inspect precise coordinate coordinates `(x, y)`.
- **Auto-Scale & Reset**: Automatically fits the Y-axis bounds to curve extrema, or resets to default range.
- **Export PNG**: One-click export of the plot canvas as a high-resolution PNG image.

#### 3. `∑ Math Intent` (Mathematical Typography & LaTeX)
Renders declarative definitions as publication-quality mathematical formulas.
- Automatically formats fractions (`/`), exponents (`^`), subscripts (`_`), square roots (`sqrt`), and Greek symbols.
- Displays mathematical signature badges (e.g. $\text{Function } f(x \in \mathbb{R}) \implies \text{Outputs}$).
- **Copy LaTeX**: Copies full `\begin{aligned}` LaTeX markup directly to clipboard.
- **Show Raw LaTeX**: Toggles raw LaTeX source view.

#### 4. `✦ Finite Worlds` (Axiomatic Algebra & Cayley Explorer)
An interactive discrete algebra laboratory for testing axiomatic structures:
- **Preset Finite Algebras**:
  - Boolean Algebra $\mathbb{B}_2$ ($\{0, 1\}$)
  - Kleene 3-Valued Logic $\mathbb{K}_3$ ($\{0, \frac{1}{2}, 1\}$)
  - Belnap 4-Valued Logic $\mathcal{B}_4$ ($\{N, F, T, B\}$)
  - Klein 4-Group $V_4$ ($\{e, a, b, c\}$)
  - Cyclic Rings $\mathbb{Z}/3\mathbb{Z}$ and $\mathbb{Z}/5\mathbb{Z}$
  - Custom Matrix Builder
- **Cayley Operation Matrix**: Interactive operation tables with color-coded cells and breakdown tooltips.
- **Axiomatic Law Verifier**: Automatically verifies five foundational algebraic properties:
  - *Associativity*: $(a \star b) \star c = a \star (b \star c)$
  - *Commutativity*: $a \star b = b \star a$
  - *Identity Element*: $\exists e \text{ s.t. } a \star e = a$
  - *Inverse Elements*: $\forall a, \exists a^{-1} \text{ s.t. } a \star a^{-1} = e$
  - *Idempotency*: $a \star a = a$
- **Morphism Finder**: Computes all valid endomorphisms, automorphisms, and symmetries across the finite set.

#### 5. `⚠ Diagnostics` (Semantic Compiler Diagnostics)
Provides structured compiler diagnostics with diagnostic error codes, source code line markers, problem explanations, and actionable fix suggestions.

#### 6. `📋 Plan` (Goal Synthesis & Execution DAG)
Shows the compiler's synthesized execution plan for declared goals. Displays step-by-step dependency DAGs, cost metrics, and assigned execution providers (such as native strict-f64 or symbolic resolvers).

#### 7. `🕸 Intent Graph` (MIG Intermediate Representation)
Displays the Mathematical Intermediate Graph (MIG). Shows canonical content-addressed graph hashes, node counts, and edge connectivity.

#### 8. `⚙ Generated` (Rust Backend Code Generator)
Displays production Rust code generated from high-level mathematical declarations. Includes generated `Cargo.toml`, module files, state structs, constructor validation logic, and evaluated methods.

#### 9. `{ } Raw JSON` (Low-Level WASM RPC Inspector)
Displays raw JSON RPC responses from the WASM compiler engine, useful for automated tool integration and low-level debugging.

---

### The 4-Tab Legend & Cheatsheet Drawer

Pressing `Cmd+K` or `Ctrl+K` (or clicking **Legend**) opens the interactive drawer with search filtering:

1. **Shortcuts**: Comprehensive hotkey listing for execution, navigation, and editing.
2. **Symbols**: Complete Unicode and ASCII math symbol catalog (Greek lowercase/uppercase, operators, relations, sets, calculus).
3. **Language**: Grammar reference covering declarations, blocks, types, goals, and assertions.
4. **Diagnostics**: Catalog of standard diagnostic codes (`E-PARSE`, `E-TYPE`, `E-GOAL`, `E-SYNTH`, `E-LAW`).

---

### Keyboard Shortcuts Reference

| Shortcut | Action | Description |
| :--- | :--- | :--- |
| `Ctrl+R` or `Cmd+Enter` | **Run Engine** | Execute in-browser interpreter |
| `Shift+Cmd+Enter` | **Check / Admit** | Verify types and syntax without execution |
| `Alt+P` or `Option+P` | **Plan Synthesis** | Synthesize goal execution plan |
| `Alt+G` or `Option+G` | **Intent Graph** | View MIG semantic graph |
| `Alt+C` or `Option+C` | **Generate Rust** | Lower math to Rust code |
| `Shift+Alt+F` | **Format Source** | Lossless AST code formatting |
| `Shift+Cmd+Y` | **Symbolify Toggle** | Convert Unicode math $\leftrightarrow$ ASCII/LaTeX |
| `Cmd+\` or `Ctrl+\` | **Swap Panes** | Toggle Left-Right editor layout |
| `Cmd+K` or `Ctrl+K` | **Legend Drawer** | Open cheatsheet and symbol reference |
| `Alt+1` through `Alt+9` | **Switch Tabs** | Jump directly to Viewport panels 1 through 9 |
| `Escape` | **Close Overlay** | Dismiss modal drawers |

---

## Part III: The .emath Language Reference

### Structure of a .emath File

A standard `.emath` file consists of declarations with structured sections:

```emath
emath function Kinematics:
    inputs:
        v0: Float64
        theta: Float64
        t: Float64
        g: Float64

    outputs:
        x: Float64
        y: Float64

    definitions:
        x = v0 * cos(theta) * t
        y = v0 * sin(theta) * t - 0.5 * g * t * t

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <launch_test>:
            given v0 = 20, theta = 0.785398, t = 1.0, g = 9.81
            expect x > 0
```

### Declarations: function vs package vs policy

- `emath function <Name>`: A stateless mathematical function mapping inputs to defined outputs.
- `emath package <Name>`: A collection of related mathematical definitions, types, and goals.
- `emath policy <Name>`: A stateful mathematical model with verified constructor invariants, internal state fields, and evaluated methods.

### Sections

- `inputs:` Declares named input parameters and their types.
- `outputs:` Declares named output parameters and their types.
- `state:` (In policies) Declares internal state variables.
- `constructors:` (In policies) Defines constructor invariants (`require <condition>`).
- `definitions:` Declares algebraic equalities and system formulas.
- `goals:` Declares compiler objectives (e.g. `evaluate <y>: produce rust.library`).
- `tests:` Declares verification test cases with `example <name>:` blocks, `given` inputs, and `expect` assertions.

### Types, Domains, and Dimensions

- `Float64` / `Real`: 64-bit IEEE-754 floating point numbers.
- `Bool`: Boolean truth values (`true`, `false`).
- `NonNegative<Real>`: Real numbers constrained to $[0, \infty)$.
- `Unit`: Dimensional units such as `s` (seconds), `m` (meters), `kg` (kilograms).

### Mathematical Symbols: Unicode and ASCII/LaTeX Aliases

emath supports both native Unicode glyphs and ASCII/LaTeX aliases:

| Unicode Glyph | ASCII / LaTeX Alias | Description |
| :---: | :---: | :--- |
| `α` | `\alpha` | Alpha |
| `β` | `\beta` | Beta |
| `θ` | `\theta` | Theta (angle) |
| `π` | `\pi` | Pi constant ($3.14159\dots$) |
| `√` | `\sqrt` | Square root |
| `·` or `*` | `*` or `\cdot` | Multiplication |
| `≤` | `<=` | Less than or equal |
| `≥` | `>=` | Greater than or equal |
| `≠` | `!=` | Not equal |
| `∈` | `\in` | Element of set |
| `ℝ` | `\real` or `Float64` | Real numbers |

---

## Part IV: 5 Hands-On Dogfooding Walkthroughs

### Tutorial 1: The 60-Second Scratchpad (Quick Calculations)

**Goal**: Verify that bare equations evaluate immediately in the interpreter.

1. Launch the workbench with `emath web` (or select the **Run** tab).
2. Clear the editor and enter the following single line:
   ```emath
   y = 3 * x + 7
   ```
3. Look at the **Live Parameter Sliders** below the editor: an `x` slider appears automatically.
4. Drag the `x` slider to `4.0`.
5. Press `Ctrl+R` (or `Cmd+Enter`).
6. **Expected Output in `Run` tab**:
   ```
   Pane:
     x = 4
     y = 19
   ```
7. Click the **Desugared AST** disclosure above the editor to verify that emath desugared the bare equation into a full `emath function Pane` with `inputs: x: Float64`.

---

### Tutorial 2: Dynamic Function Plotting (2D Visualizer)

**Goal**: Plot a damped wave with live parameter adjustments.

1. Paste the following model into the editor:
   ```emath
   emath function DampedOscillator:
       inputs:
           x: Float64

       outputs:
           y: Float64

       definitions:
           y = exp(-0.1 * x) * sin(x)

       goals:
           evaluate <y>:
               produce rust.library

       tests:
           example <origin>:
               given x = 0
               expect y == 0
   ```
2. Press `Alt+2` to switch to the **Plot 2D** tab.
3. Observe the plotted damped wave on the canvas.
4. Set **Min X** to `0` and **Max X** to `20`.
5. Drag the `x` parameter slider to inspect evaluated coordinates at points along the curve.
6. Click and drag on the canvas to pan across the domain; scroll to zoom.
7. Click **Export PNG** to save the generated graph to your downloads folder.

---

### Tutorial 3: Mathematical Intent Typography & LaTeX Export

**Goal**: Verify mathematical typesetting and export equations to LaTeX.

1. Paste the following aerodynamic drag equation into the editor:
   ```emath
   emath function AerodynamicDrag:
       inputs:
           rho: Float64
           v: Float64
           cd: Float64
           area: Float64

       outputs:
           drag_force: Float64

       definitions:
           drag_force = 0.5 * rho * (v * v) * cd * area

       goals:
           evaluate <drag_force>:
               produce rust.library
   ```
2. Press `Shift+Cmd+Y` to symbolify the text into Unicode math.
3. Press `Alt+3` to switch to the **Math Intent** tab.
4. Verify that the formulas render with mathematical fractions, exponents, and Greek letters ($\rho$).
5. Click **Copy LaTeX** to copy publication-ready LaTeX markup to your clipboard.
6. Click **Show Raw LaTeX** to inspect the generated `\begin{aligned}` LaTeX source.

---

### Tutorial 4: Finite Worlds, Cayley Matrices & Axiomatic Laws

**Goal**: Explore non-classical algebraic worlds and verify axiomatic group properties.

1. Press `Alt+4` to switch to the **Finite Worlds** tab.
2. In the **Preset World** dropdown, select **Klein 4-Group V₄ ({e, a, b, c})**.
3. Hover over cells in the Cayley table to view operation products (e.g. $a \star b = c$, $a \star a = e$).
4. Check the **Axiomatic Laws Verification** panel:
   - *Associativity*: Green checkmark (Holds for all $x, y, z$).
   - *Commutativity*: Green checkmark (Abelian group).
   - *Identity Element*: Green checkmark ($e$).
   - *Inverse Elements*: Green checkmark (Every element is its own inverse).
   - *Idempotency*: Red cross (Fails for non-identity elements).
5. Switch the preset to **Kleene 3-Valued Logic 𝕂₃ ({0, ½, 1})** and select the **AND (∧)** operator to inspect three-valued logic truth tables.
6. Click **Find Morphisms** to compute all structure-preserving symmetries of the algebra.

---

### Tutorial 5: Compiling Declarative Math to Executable Rust

**Goal**: Compile high-level mathematical intent into an in-memory Rust crate.

1. Paste the following affine scoring model into the editor:
   ```emath
   emath function Scorer:
       inputs:
           x: Float64

       outputs:
           y: Float64

       definitions:
           y = 2.5 * x + 10.0

       goals:
           evaluate <y>:
               produce rust.library
   ```
2. Press `Alt+8` to switch to the **Generated** tab.
3. In the **Target File** dropdown, select `src/lib.rs`.
4. Inspect the generated Rust code:
   - Notice the pure, zero-allocation Rust function `pub fn scorer(x: f64) -> f64`.
   - Select `Cargo.toml` to view the generated crate manifest with zero external dependencies.

---

## Part V: CLI & Terminal Reference

### Running the Local Workbench (`emath web`)

To launch the web playground locally from your terminal:

```console
$ emath web
```

Options:
- `--port <PORT>`: Specify local port (default: `7878`).
- `--no-open`: Start the HTTP server without automatically opening a browser window.
- `--dist <PATH>`: Point to a custom web distribution build.

*(Note: `emath serve` is fully supported as a backwards-compatible alias).*

### Batch Validation & Artifact Verification

Verify, plan, and execute `.emath` files from the command line:

```console
# Check syntax and semantic types
$ emath check path/to/file.emath

# Execute tests through the interpreter
$ emath run path/to/file.emath

# Synthesize goal execution plan
$ emath plan path/to/file.emath

# Build and verify a compiled Cargo artifact
$ emath build path/to/file.emath --out target/emath

# Run code formatter
$ emath fmt path/to/file.emath
```

### Diagnostic Codes & Error Recovery

When the compiler detects an issue, it emits an honest, structured diagnostic code:

| Code | Severity | Description & Fix |
| :--- | :--- | :--- |
| `E-PARSE-001` | Error | Syntax error. Check for missing colons after section headers or unmatched parentheses. |
| `E-TYPE-001` | Error | Type mismatch. Ensure operands match expected numerical or boolean domains. |
| `N-TYPE-001` | Note | Default type inference note (e.g. input `x` defaulted to `Float64`). |
| `E-NAME-001` | Error | Undefined identifier. Check that all variables in definitions are declared in `inputs:` or defined earlier. |
| `E-GOAL-001` | Error | Unresolvable goal. The requested transformation has no registered provider. |
| `E-INVAR-001` | Error | Constructor invariant violation. Argument fails a `require` precondition. |

---

*End of Manual. For technical questions and contributions, reach out to the project repository maintainers.*
