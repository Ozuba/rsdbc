# rsdbc — a web-compatible CAN DBC editor in Rust

A fully functional editor for CAN `.dbc` databases, written in Rust with
[egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe).
The exact same code runs as a native desktop app and as a WebAssembly app in the
browser. Inspired by the sibling [`can-viewer`](../can-viewer) project, which
shares the egui + Trunk + WASM stack.

## Features

- **Load / edit / save** complete DBC files entirely in the browser (no backend).
- **Messages**: edit name, identifier (standard or 29-bit extended), DLC,
  transmitter and comment; add/delete messages.
- **Signals**: edit name, start bit, length, byte order (Intel/Motorola),
  signedness, factor, offset, min/max, unit, multiplexing (M / m*n*),
  receivers, comment, and value descriptions (`VAL_`).
- **Nodes** (`BU_`) and global **value tables** (`VAL_TABLE_`) editing.
- **Bit-layout visualiser**: a colour-coded byte/bit grid showing exactly where
  each signal lives in the frame (handles both Intel and Motorola ordering).
- **Live DBC text view** of the generated file, with one-click copy.
- **Drag-and-drop** a `.dbc` file anywhere onto the window to open it; export via
  a native save dialog (desktop) or a browser download (web).
- **Lossless round-trip**: a hand-written parser/writer preserves constructs it
  doesn't model (attribute definitions, signal groups, …) verbatim, and is
  verified to round-trip every database in the bundled `dbc_tests` corpus (82
  files from the cantools test suite: attributes, extended multiplexing, J1939,
  CAN-FD, cp1252 encoding, trailing `//` comments, …).

The built-in example is `socialledge.dbc` — a compact database with named nodes,
a multiplexed message, signed/scaled signals and value tables.

## Run it

### Web (WASM)

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk      # or: cargo binstall trunk
trunk serve --release    # open http://127.0.0.1:8080
```

`trunk build --release` produces a static site in `dist/` that can be hosted
anywhere.

### Deploy to GitHub Pages

The repo ships a workflow (`.github/workflows/deploy.yml`) that builds the WASM
site and publishes it to GitHub Pages on every push to `main`. To enable it:

1. Push this repo to GitHub.
2. In **Settings → Pages**, set **Source** to **GitHub Actions**.
3. Push to `main` (or run the workflow manually from the **Actions** tab).

The site is served at `https://<user>.github.io/<repo>/`. The workflow passes
`--public-url` so all asset paths resolve correctly under that subpath.

### Native desktop

```bash
cargo run --release
```

## Tests

```bash
cargo test
```

`cargo test` runs the parser/writer unit tests plus a round-trip integration
test (`tests/roundtrip.rs`) over every database in `dbc_tests`.

## Project layout

| Path | Purpose |
|------|---------|
| `src/dbc/model.rs`  | In-memory DBC data model |
| `src/dbc/parser.rs` | DBC text → model |
| `src/dbc/writer.rs` | model → canonical DBC text |
| `src/app.rs`        | egui editor UI (panels, signal editor, bit layout) |
| `src/platform.rs`   | Native vs. web file open/save |
| `src/main.rs`       | Native + WASM entry points |
