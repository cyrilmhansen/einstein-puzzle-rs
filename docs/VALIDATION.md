# Validation and reproducibility

This note summarizes the checks that can be reproduced from this checkout. It intentionally contains no private paths, retained capture payloads, generated corpora, or links to local-only investigation files.

## Rust build and tests

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

The workspace contains three user-facing executables: `einstein-gui`, `einstein-cli`, and `einstein-mcp`. The GUI uses eframe 0.37 with Glow/Glutin, AccessKit and both X11 and Wayland features. The CLI and MCP adapters share the same session and storage implementation.

## CLI transcript

```sh
printf 'new 42\nstate\nexclude 1 1 2\nstate\nselect 1 1 5\nstate\nsave 0\nend\nstate\n' | \
  cargo run -q --locked -p einstein-adapters --bin einstein-cli -- --no-clock --save-dir /tmp/einstein-cli
printf 'new 7\nstate\nsave 0\nnew 8\nload 0\nstate\n' | \
  cargo run -q --locked -p einstein-adapters --bin einstein-cli -- --no-clock --save-dir /tmp/einstein-cli-reload
```

CLI options are `--save-dir DIRECTORY`, `--no-clock`, and `--help`/`-h`. Commands are `new SEED`, `state`, `select CATEGORY COLUMN VALUE`, `exclude CATEGORY COLUMN VALUE`, `toggle-hint ID`, `switch-hints`, `pause`, `resume`, `restart`, `end`, `save SLOT`, `load SLOT`. Categories, columns, and values are 1–6; slots are 0–9. `state` does not expose the hidden solution; there is no user-facing `solve` command.

## MCP stdio transcript

Run the MCP adapter directly with this reproducible JSON-RPC sequence; EOF terminates the server:

```sh
printf '%s\n' \
'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}' \
'{"jsonrpc":"2.0","method":"notifications/initialized"}' \
'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
'{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"einstein_new","arguments":{"seed":"42"}}}' \
'{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"einstein_state","arguments":{}}}' | \
 cargo run -q --locked -p einstein-adapters --bin einstein-mcp -- --no-clock --save-dir /tmp/einstein-mcp
```

Expected: initialize negotiates `2025-11-25`; `tools/list` returns 12 tools; calls to `einstein_new` and `einstein_state` return `ok=true`, status `playing`. This is stdio JSON-RPC, not a network server.

## Native GUI backend evidence

The recorded host was CachyOS KDE under a real Wayland session. Before enabling eframe's Wayland feature, the normal session reproduced Glutin `NotSupported("provided native window is not supported")`. The corrected feature graph passes the notice verifier and supports the Wayland path in both winit and glutin-winit.

- Real Wayland session: normal launch and an explicit Wayland-only launch remained alive for their bounded smoke windows without the previous startup error. No Wayland click or screenshot is claimed.
- X11/Xwayland: a test-owned window was identified by its process ID, captured, clicked and closed; no process remained. This does not establish Wayland interaction behavior.
- Xvfb is not used as a substitute for a real desktop session.

GUI labels currently appear in French (for example, `Nouvelle grille`, `Recommencer`, `Pause`, `Règles`). Keyboard navigation and named controls are implemented; a GUI label translation is not claimed as part of this validation.

## SDL comparison scope

The sampled SDL/Rust comparison records 18 scenario rows: **14 PASS, 2 DIFFERENCE, 1 BLOCKED, 1 UNMEASURED**. The unmeasured item is global equivalence; the blocked item concerns old portal-backed cleanup. The differences are end-of-game UI-flow differences, not proof of a rules-engine mismatch. This finite sample is not a global-equivalence claim or a 100-game SDL corpus. SDL saves and historical RNG seeds are not interoperable with the Rust implementation.

## Licensing checks

- Project and classic Debian bitmap assets: GPL-2.0-or-later; no MIT relicensing is asserted.
- A combined GUI distribution with Apache-2.0-only components may use the GPLv3-or-later project-level option; preserve each component's individual terms and notices.
- Verify the current GUI dependency notice inventory with `python3 validation/licenses/gui_notices.py verify`.
- Verify the 40 allowlisted bitmap hashes from the repository root with `sha256sum -c assets/classic/SHA256SUMS`.
- These checks are not a legal-compliance certification or a complete SBOM for dynamically loaded system libraries or a packaged artifact.
