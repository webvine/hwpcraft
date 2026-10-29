# Local RHWP patches

HwpCraft builds against the sibling `../rhwp` checkout (MIT, Copyright (c)
2025-2026 Edward Kim; license text in `assets/licenses/LICENSE-RHWP-MIT.txt`).
The patches contain RHWP source lines and remain under RHWP's MIT license. A few changes that
HwpCraft needs but upstream does not have yet are kept here as patch files, so
the sibling tree can be updated and the changes applied again.

Rules:

- Every change made in `../rhwp` gets a numbered patch in this folder and a row
  below. Nothing is changed in `../rhwp` without a patch here.
- Patches are plain `git diff` output relative to the RHWP root, applied in
  file-name order.
- `scripts/rhwp-patches.ps1` checks, reverts and applies them.
  `scripts/sync-siblings.ps1` reverts them before pulling RHWP and applies them
  again afterwards. CI applies them after checking out RHWP.
- When an update makes a patch fail to apply, rebase it by hand on the new RHWP
  source, regenerate the file with `git -C ../rhwp diff -- <files>`, and update
  the base version below. Drop a patch once upstream contains an equivalent fix.

| Patch | Base | Files | Why | Upstream |
|---|---|---|---|---|
| `0001-pdf-share-font-face-data.patch` | v0.8.7 (`1a76570e8`) | `src/renderer/pdf.rs` | PDF export spent most of its time re-mapping font files: usvg asks fontdb for face data once per glyph, and `Source::File` faces are mapped and parsed again on every call. Mapping each face once (`make_shared_face_data`) removes that cost. | Not reported yet |
| `0002-pdf-parallel-page-conversion.patch` | v0.8.7 + 0001 | `src/renderer/pdf.rs` | Converts PDF pages on up to `min(available_parallelism, 8)` threads (`RHWP_PDF_THREADS` overrides; `1` keeps the sequential path). Pages are claimed in order, results and warnings are merged in page order, and the first failing page's error is returned as before, so the output is byte-identical. No new dependencies (`std::thread::scope`). | Not reported yet |
| `0003-release-page-render-caches.patch` | v0.8.7 | `src/document_core/queries/rendering.rs` | Adds the public `DocumentCore::release_page_render_caches`, a wrapper around the crate-private cache invalidation. RHWP keeps a render tree per page without a bound, so exporting every page to PDF left about 640 MB cached on a 381-page document. HwpCraft calls it after each PDF export; the model and pagination are untouched and later renders rebuild only the requested pages. | Not reported yet |
| `0004-layout-diagnostics-switch.patch` | v0.8.7 | `src/renderer/layout.rs`, `src/renderer/layout/paragraph_layout.rs` | Adds the public `set_layout_diagnostics(bool)` switch for the `LAYOUT_OVERFLOW*` and `LAYOUT_TABLE_OVERLAP` stderr lines (default on; `RHWP_LAYOUT_DIAG=0` starts with it off). The values are still collected for `take_overflows` and `take_table_overlaps`. The stdio MCP server turns the lines off at start, because a client shows the server's stderr as its log and long documents printed thousands of lines. | Not reported yet |
| `0005-char-format-deferred-in-batch.patch` | v0.8.7 | `src/document_core/commands/formatting.rs` | `apply_char_format_native` ends with `rebuild_paragraph_deferred_in_batch` instead of `rebuild_section`, the same tail its undo counterpart `set_char_shape_id_native` already uses. Outside a batch the helper calls `rebuild_section`, so behaviour is unchanged; inside a batch it recomposes the one paragraph and leaves pagination to `end_batch`. HwpCraft applies character formatting to many paragraphs inside one batch: 51 selected paragraphs of the 381-page sample, bold, 163 ms -> 137 ms on top of the HwpCraft-side change (616 ms before both). HWPX bytes and page SVGs are unchanged. | Not reported yet |
| `0006-relayout-paragraph.patch` | v0.8.7 + 0005 | `src/document_core/commands/formatting.rs` | Adds the public `relayout_paragraph_native(section, paragraph, cell)`: after the caller changed one paragraph's model directly (character-shape runs), it rebuilds only that paragraph's derived state with the existing `rebuild_paragraph_deferred_in_batch` (body) or `mark_cell_control_dirty` + `rebuild_section_deferred_in_batch` (cell); stored line segments are left as the caller set them. HwpCraft used `set_document`, which lays out the whole document. 381-page sample: Enter at a paragraph end 232 -> 10 ms, first keystroke after a pending format 472 -> 13 ms, pending format 499 -> 22 ms; HWPX bytes, page SVGs and undo results are unchanged, and the page count now matches the reopened file. | Not reported yet |
| `0007-from-bytes-prepared.patch` | v0.8.7 | `src/document_core/commands/document.rs` | Adds the public `DocumentCore::from_bytes_prepared(data, password, prepare)`: `prepare` may change the parsed `Document` after load fix-ups and before the first compose and pagination; an `Err` becomes `HwpError::InvalidFile`. `from_bytes` and `from_bytes_with_password` are unchanged. HwpCraft restored its page-number fields with `set_document` after loading, which laid the whole document out a second time. 381-page sample .hwp open 769 -> 437 ms; 24 sample .hwp files on that path give the same page count, page SVGs and HWP/HWPX save bytes as before. The HWPX automatic-number and page-number paths use it too: 381-page sample as HWPX 971 -> 395 ms; 96 HWPX files and 4 password-protected HWPX cases give the same page count, page SVGs, model, HWPX/HWP save bytes and error messages. | Not reported yet |

Measurements for 0001 (381-page sample `2025 행정업무운영 편람(최종).hwp`, release CLI
`convert … .pdf`, Windows): 372 s before, 84 s after; the two PDFs are byte-identical.

Measurements for 0002 (same sample and command, after 0001): 1 thread 63.1 s / 1,185 MB peak working set, 4 threads 25.9 s / 1,564 MB, 8 threads 16.2 s / 1,943 MB, default 16.7 s / 1,962 MB; every PDF is byte-identical to the 0001-only output (SHA-256 75168B36…), and stderr is unchanged. Each concurrent page holds about 94 MB of parse and conversion buffers; lower `RHWP_PDF_THREADS` on small-memory machines.
