# emath Web Workbench

Interactive browser workbench for emath running client-side WebAssembly (`emath-wasm`).

## Overview

The web workbench provides an in-browser interface for evaluating mathematical specifications, running verification gates, inspecting typed semantic IR, and testing code generation without local installation.

- **Zero-Setup Execution**: Runs purely client-side via WebAssembly compiled from `crates/emath-wasm`.
- **Dual Pipeline Controls**: Supports both interactive evaluation (`Run` / `⌘+Enter`) and formal compiler verification (`Check` / `Shift+⌘+Enter`).
- **Real-Time Diagnostics**: Surfaces structured error envelopes (`E-*`), capability notes, and type admission receipts directly in the editor pane.

## Building and Running Locally

The workbench assets and WebAssembly binary are managed via `cargo xtask`:

```bash
# 1. Build emath-wasm for wasm32-unknown-unknown and stage assets to web/dist/
cargo xtask build-web

# 2. Start local development server (defaults to port 8080)
cargo xtask serve-web [port]
```

Then navigate to `http://localhost:8080` in any modern web browser.

## Assets Structure

- `index.html`: Workbench single-page interface layout and editor controls.
- `app.js`: Client-side workbench controller, WebAssembly runtime loader, and event handlers.
- `style.css`: Clean, dark-mode-first editor design system.
