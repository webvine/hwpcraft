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

Measurements for 0001 (381-page sample `2025 행정업무운영 편람(최종).hwp`, release CLI
`convert … .pdf`, Windows): 372 s before, 84 s after; the two PDFs are byte-identical.
