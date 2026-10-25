# HwpCraft contributor notes

- This is a native Rust / egui desktop project. Keep all new HwpCraft source in this folder.
- Reference WordCraft's layering, but preserve RHWP's document model instead of converting it to a DOCX DOM.
- Do not edit the sibling `rhwp` or `wordcraft` projects as part of routine app work. A user-approved RHWP change is kept as a numbered patch in `patches/rhwp/` and recorded in its README; `scripts/rhwp-patches.ps1` checks/applies it and `scripts/sync-siblings.ps1` re-applies it after updating RHWP.
- Do not run `cargo fmt --all`: it follows path dependencies into RHWP. Format the seven HwpCraft packages with explicit `-p` arguments.
- Editing commands belong in `crates/engine`, reused by UI and CLI. Offsets count Unicode scalar values.
- Keep failed document loads, commands and saves from destroying the current document or an existing file.
- Preserve MIT notices. Do not bundle proprietary Hancom code, logos or fonts.
- Verify with `cargo test --locked --workspace` and `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- For page/IME/UI changes, run the actual desktop app. `--screenshot <png> --exit-after-screenshot` captures the rendered window.
- The launcher integration is local-only. Do not invent an upstream repository, publish, push, or send messages.
- Record unimplemented features and compatibility limits honestly in README.md.

- User-facing versions use `x.xx`: `VERSION` starts at `0.10`, then `0.11`, `0.12`, etc. Keep Cargo SemVer in sync (`0.10.0` internally); desktop/CLI/MCP/packaging display `VERSION`.
