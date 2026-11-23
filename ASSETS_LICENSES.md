# Asset licenses and provenance

This inventory covers assets currently present in `assets/`. It does not grant rights beyond the identified upstream terms and is not a legal certification.

## `assets/classic/*.bmp` — 40 bitmap files

These files are copied unmodified from the Debian DFSG source archive for Einstein 2.0. The project provenance record reports Copyright 2003–2005 Alexander Babichev / Flowix and GPL version 2 or later, citing the Debian source package's copyright file. The exact source archive digest and each asset digest are recorded in `assets/classic/README.md` and `assets/classic/SHA256SUMS`. The repository does not claim ownership of these bitmaps. Their source-attribution/licensing basis is package metadata, not direct written authorization from each rightsholder.

The active GUI compiles bitmap bytes into its executable (`include_bytes!`); these files are not merely optional external theme data. Removing them would therefore break the current build. Keep them and the GPL terms for the current build unless the implementation is changed to an independently distributable optional asset pack and that change is reviewed. No files are deleted or silently substituted by this audit.

The F3 bitmap included here is the Debian variant. The v2.2 F3 has a different hash and is an external reference only; do not copy it into this repository or distribute it without independently established permission. Exclude the historical `einstein.res`, historical TTFs, WAVs, help text, package icon, and any unreviewed screenshot/capture or reference corpus from publication.

## Fonts and GUI-provided imagery

No historical game TTF is included in the audited asset directory. The egui/epaint default font and icon material is supplied through the Rust dependency graph rather than `assets/`; see `licenses/gui/archives/epaint_default_fonts-0.37.0/fonts/` and its upstream notices (including OFL and UFL texts). Do not present those fonts as project-owned or apply a single project asset license to them.

## Newly added assets

Before adding any asset, record its exact path, source URL/revision or creator, content hash, rights/permission evidence, applicable license and required attribution. If any field is unknown, keep the asset out of distributable builds pending resolution.
