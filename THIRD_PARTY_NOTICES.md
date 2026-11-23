# Third-party notices

This file identifies third-party material included by this repository as currently inspected. It is not a legal opinion or a certification that any distribution is compliant. Preserve upstream notices and license texts when redistributing.

## Einstein Puzzle classic bitmap assets

The 40 unmodified BMP files in `assets/classic/` are sourced from the Debian-packaged Einstein 2.0 DFSG source archive and are documented there as Copyright 2003–2005 Alexander Babichev / Flowix, GPL version 2 or later. See `assets/classic/README.md`, `assets/classic/SHA256SUMS`, and the referenced Debian copyright notice for provenance and file-level hashes. The project does not claim authorship of these images. The bundled F3 is the Debian variant; the F3 image from Einstein Puzzle 2.2 is not included. Do not replace or supplement assets with files from the historical `einstein.res`, historical TTFs, WAV/help resources, package icon, or v2.2 F3 absent separately verified permission and notices.

## Rust dependencies

The current native GUI inventory is `licenses/gui/inventory-wayland.json`; the earlier `licenses/gui/inventory.json` is retained as historical evidence. Current lock/graph matching and exact notice hashes are verified by `python3 validation/licenses/gui_notices.py verify` (189 registry crates, 345 archive notices, 8 pinned upstream references for 14 crates). The archived graph is for x86_64-unknown-linux-gnu and is not a complete packaged-binary or system-library SBOM. The GUI uses eframe 0.37.0 with `accesskit`, `default_fonts`, `glow`, `wayland`, and `x11` features. Individual crates use their declared license expressions, including alternatives (`OR`) and cumulative obligations (`AND`); do not treat an OR as cumulative or collapse an AND into a choice. Embedded epaint fonts/icons have their own notices (including OFL and UFL material) in the corresponding archive under `licenses/gui/archives/epaint_default_fonts-0.37.0/fonts/`.

CLI/MCP adapter dependency notices for the currently documented production graph are in `licenses/adapters/`. These notices do not replace any obligations for dependencies added by later feature or target changes.

## Distribution compatibility note

The repository declares GPL-2.0-or-later for its own workspace and classic bitmap assets. A distributed combined work that includes Apache-2.0-only components cannot be offered as GPL-2.0-only; GPL version 3 or later is the compatible project-level option, while every component's own terms remain applicable. This statement does not determine whether a specific combination is a derivative or aggregate. Consult qualified counsel for that question.
