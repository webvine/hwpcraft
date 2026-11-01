# HwpCraft

[한국어](README.md) · **English**

**An open-source native editor that opens, edits and saves Korean HWP / HWPX documents — and a document engine that lets AI agents write Hangul documents through MCP and finish them as reports.**

Version **0.20** (latest release [0.19](https://github.com/webvine/hwpcraft/releases/tag/v0.19)) · MIT License · Windows · macOS · Linux · Rust

![The HwpCraft desktop window: Hangul-style menu tabs, Insert ribbon, format toolbar, rulers and page view, showing a report made through MCP](docs/images/hwpcraft-desktop.png)

*A report made only with MCP commands, opened in the desktop app (Insert tab). The table, chart, footnote and header were all added through MCP.*

## What is HwpCraft?

HWP is the most widely used document format in Korean government and business, yet the tools that edit and automate it are mostly closed. HwpCraft is a Rust program that works with HWP and HWPX documents directly on your machine, and it offers the same editing engine in three ways:

- **Desktop editor** (`hwpcraft`): edit documents with a Hangul-style ribbon, dialogs and shortcuts.
- **Command-line tool** (`hwpcraft-cli`): inspect, convert, render pages and batch-edit from JSON command files.
- **MCP server** (`hwpcraft-cli mcp`): exposes all 415 editing commands so AI agents such as Claude can create, edit, save and export documents to PDF.

It takes its application structure from the ArtCraft team's [WordCraft](https://github.com/storytold/wordcraft), its Hangul editing capabilities from the [RHWP](https://github.com/edwardkim/rhwp) document engine, and its user interface from [egui](https://github.com/emilk/egui). Both WordCraft and RHWP are MIT-licensed.

HwpCraft is an independent open-source project. It is not affiliated with, endorsed by, or derived from Hancom Inc. It ships no Hancom code, icons, logos or fonts. **It is not a 100 % replacement for Hancom Office Hangul.** Unimplemented features and known differences are listed under [Scope and limits](#scope-and-limits) rather than hidden.

License: **MIT** ([LICENSE](LICENSE)). See the [License](#license) section for scope and third-party components. Contributions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Highlights

- **Works on HWP / HWPX in their own model.** RHWP's document model is opened and saved as is, without conversion, so original objects survive as far as the engine allows. Exports PDF, DOCX and text, renders pages to PNG, and opens password-protected documents.
- **A screen familiar to Hangul users.** Menu and contextual tabs, a ribbon that folds in narrow windows, format toolbar, rulers, continuous page view, Hangul-style shortcuts (including prefix chords such as Ctrl+K,…) and the Windows Korean IME.
- **The editing a report needs.** Character and paragraph shapes, styles and style sets, tables (cell blocks, merging, formulas, table styles), pictures, shapes, text boxes, equations, 15 chart types, footnotes and endnotes, headers and footers, page numbers, columns, master pages, tables of contents, cross-references and indexes, memos with replies, find / replace, document compare and merge, hanja conversion and a character map.
- **MCP for AI automation.** An MCP server over stdio and loopback HTTP that takes both the current `2026-07-28` clients and 2025-era ones (2025-03-26, 06-18, 11-25; checked with Claude Code and the Codex CLI in 0.20). Engine commands run through `execute` / `batch`; Hangul automation action names and public ParameterSets run through `hwp_action`. It can also attach to a running desktop window so an agent and a person edit the same document.
- **Edits that never break your document.** Saves are written to a temporary file and swapped in. A failed command leaves the document and undo history untouched, and `batch {atomic:true}` rolls everything back when any step fails.
- **Fully offline.** No login, server or database; documents never leave your machine. MCP over HTTP listens on 127.0.0.1 only and requires a token.

## A look around

| | |
|---|---|
| ![Equation editor: structure and symbol tools, preview, script box](docs/images/equation-editor.png) | ![Character map: user, Unicode, Hangul (HNC) and KS tables, emoji, search](docs/images/character-map.png) |
| **Equation editor** — write equations with tool buttons and script, and edit them right in the preview. | **Character map** — Hangul's four character tables, emoji, search by name or code. |
| ![Memo pane: memo list beside the page, author and text editing](docs/images/memo-panel.png) | ![Print preview: page selection, n-up and watermark options](docs/images/print-preview.png) |
| **Memos** — attach memos to text ranges and edit author and text; replies work too. | **Print preview** — check n-up, watermark and page selection before printing. |
| ![Style sets: Report, Official and Compact sets, saving the document's styles](docs/images/style-sets.png) | ![Find and replace: case, whole word, ignore spaces, regular expressions, format conditions](docs/images/find-replace.png) |
| **Style sets** — pick a set of styles and restyle the document at once. | **Find and replace** — ignore spaces, wildcards, regular expressions, hanja and format conditions. |

## Download

Get the bundle for your system from [GitHub Releases](https://github.com/webvine/hwpcraft/releases). Each bundle holds the desktop app `hwpcraft`, the command-line tool `hwpcraft-cli` and the license files.

| System | File | First run |
|---|---|---|
| Windows x64 | `HwpCraft-0.19-windows-x64.zip` | Unzip and run `hwpcraft.exe` |
| macOS Apple silicon | `HwpCraft-0.19-macos-arm64.tar.gz` | Right-click `HwpCraft.app` → Open (not signed or notarized) |
| macOS Intel | `HwpCraft-0.19-macos-x64.tar.gz` | Same as above |
| Linux x86_64 | `HwpCraft-0.19-linux-x64.tar.gz` | `./install.sh` installs into `~/.local`; needs a Korean font (`fonts-noto-cjk` or `fonts-nanum`) |

To build from source, see [Build](#build).

## Quick start

### Desktop

```text
hwpcraft report.hwp
hwpcraft ./samples/HwpCraft-시작하기.hwpx
```

`samples/` holds HWP, HWPX and PDF examples made with HwpCraft itself.

### Command line

`hwpcraft-cli` analyses, converts, renders and batch-edits documents with the same engine as the desktop app:

```text
hwpcraft-cli --help
hwpcraft-cli commands                       # list editing commands
hwpcraft-cli info  input.hwp
hwpcraft-cli convert input.hwp output.hwpx
hwpcraft-cli render input.hwpx page.png 1
hwpcraft-cli edit --new commands.json output.hwpx
hwpcraft-cli mcp --help                     # MCP server
```

The `edit` command file is a JSON array such as `[{"command":"text.insert","params":{"text":"Hello"}},{"command":"page.setup","params":{"landscape":true}}]`, limited to 1 MB / 1,000 commands. The output file is written only when every command succeeds. Text offsets count Unicode scalar values.

### Let an AI write documents through MCP

Register `hwpcraft-cli` as a stdio server in your MCP client. Give the executable as an absolute path and name the folders the agent may read and write:

```json
{
  "mcpServers": {
    "hwpcraft": {
      "command": "C:/Tools/HwpCraft/hwpcraft-cli.exe",
      "args": ["mcp", "--transport", "stdio",
               "--automation-read-root", "C:/work/reports",
               "--automation-write-root", "C:/work/reports"]
    }
  }
}
```

![Page 1 of a report made with MCP commands: title, overview, table, column chart, equation, footnote and page number](docs/images/mcp-report-page.png)

*Page 1 of the report above, drawn by MCP `render_page`. Applying a style set, typing text, filling the table (`table.cells.set`), a table style, a caption, the chart (`chart.create`), the equation, footnote, memo, header and page number were all done by calling MCP tools from the official Python SDK.*

A typical agent session: `new_document` returns a `document_id`; `type_text` adds text; `execute` / `batch` apply formatting, tables, pictures and page setup; `render_page` draws pages to check them; `save_document` writes HWP / HWPX and exports PDF. `list_commands` finds commands and their parameters.

`hwpcraft-cli mcp` is a headless MCP server for `2026-07-28` and 2025-era clients (stdio by default; `--transport http --listen 127.0.0.1:7082` serves loopback HTTP, where every request needs `Authorization: Bearer <token>` with the token read from `mcp-http-token` in the settings folder or `--auth-token-file`, created readable only by you if missing): create Korean documents, edit text / formatting / tables / objects, save HWP / HWPX, export PDF / TXT, render PNG for checking, and restrict read / write roots. `hwp_action` accepts Hangul automation action names (`MoveLineEnd`, `CharShapeBold`, `DeleteWord` and many more) and some public ParameterSets, and runs the matching command; a registered name does not mean every original option is equivalent. `batch {atomic:true}` rolls every document change back when any command fails. Every tool declares an `outputSchema` for its structured result; for `execute` and `batch`, whose result depends on the command, only the common envelope is fixed. With **MCP 연결** (Tools tab) the desktop app serves the same protocol from its window on `127.0.0.1:7083` (same token); clients find the window's document with `list_documents` and edit it by that `document_id`, the window shows each edit at once and undo reverts it, and MCP cannot close it. It does not control Hancom software. Connections were checked with the official Python SDK `mcp` 2.3.0 and the TypeScript SDK `@modelcontextprotocol/client` 2.4.0.

More: [MCP connection and usage](docs/mcp.md), [command parameters and the report-writing recipe](docs/mcp-commands.md) (Korean).

## Final goal

The final goal of HwpCraft is for **an AI agent to write Hangul (HWP / HWPX) documents through MCP and finish them as reports, exported and ready to deliver, without a person at the keyboard**. The desktop editor is where people review and touch up the result; the centre of automation is the MCP server provided by `hwpcraft-cli mcp`.

To get there, HwpCraft aims to complete the following as fully as possible.

- **Expose the whole writing and editing workflow as MCP commands** — new documents, text input, character and paragraph shapes, styles, tables, pictures, charts, equations, footnotes and endnotes, headers and footers, page numbers, columns, bookmarks, hyperlinks, find / replace, hanja conversion, autotext and document comparison: everything the desktop can edit, an agent must be able to edit too. Hancom Office Hangul automation action names are accepted as they are (`hwp_action`) so existing macro and script knowledge carries over.
- **Provide the building blocks of reports** — cover pages, tables of contents, heading hierarchies, numbering, table and figure captions, references, page setup and print layout, assembled by commands, with templates and styles for consistent documents.
- **Automate output and verification** — save as HWP / HWPX, export PDF, render PNG to check the result, and roll the document back when any step of a `batch` fails so that no broken output is left behind.
- **Stay compatible with Hancom Office Hangul** — save exactly what Hangul reads and writes, on top of the RHWP engine, and record unsupported features honestly under [Scope and limits](#scope-and-limits) instead of hiding them.

Progress is tracked under [Features](docs/FEATURES.en.md) and in the release notes (`docs/`); remaining items live in the limits section.

## How HwpCraft is built

HwpCraft combines two open-source projects. This section states which part comes from where.

- **Application structure from ArtCraft** — HwpCraft follows the crate layering of [WordCraft](https://github.com/storytold/wordcraft) (MIT) by storytold's ArtCraft team: `doc` → `formats` → `render` → `engine` → `mcp` / `ui-egui` → `apps`. Ribbon width planning (`crates/ui-egui/src/ribbon_layout.rs`), safe file replacement (`crates/formats/src/file_access.rs`) and the MCP server's lifecycle / tools / session structure are adapted from WordCraft; the theme tokens follow DesignCraft and the dialog draft / commit pattern follows DeckCraft. The skeleton and workflow of the application belong to the ArtCraft family.
- **Hangul editing capabilities from RHWP** — HwpCraft calls [RHWP](https://github.com/edwardkim/rhwp) (MIT) directly for the HWP / HWPX parser and document model, typesetting, saving, SVG rendering, PDF export, and editing APIs such as character and paragraph shapes, tables, objects, footnotes and endnotes, headers and footers, page numbers, columns, bookmarks, hyperlinks, document passwords and privacy scanning. RHWP's document model is kept as is rather than converted into WordCraft's DOCX model. The ability to read, edit and save documents belongs to RHWP.
- **What HwpCraft implements itself** — the `Session` and command protocol that join the two (Unicode scalar offsets, table-cell addressing, selection / clipboard / caret, undo transactions), the per-area command modules that group RHWP APIs into Hangul-style commands, the Hangul-style ribbon, dialogs, shortcuts and IME handling, features RHWP does not provide (find / replace, sort, control-mark removal, character map), and the CLI and MCP surfaces.

No Hancom code or resources are used in any layer. Hancom Office Hangul is the reference for the behaviour and layout users expect, not a source of code.

```text
apps/hwpcraft (desktop)             apps/hwpcraft-cli (CLI · MCP entry)
        │                                   │
crates/ui-egui (ribbon, dialogs, IME)  crates/mcp (stdio / HTTP MCP server)
        └──────────────┬────────────────────┘
               crates/engine (Session · editing commands · undo)
                       │
     crates/render ── crates/formats ── crates/doc
                       │
          RHWP (../rhwp: HWP/HWPX model, typesetting, saving, SVG/PDF)
```

## Features at a glance

| Area | What you can do |
|---|---|
| Files | Open HWP, HWPX and HWP3; save HWP and HWPX; export PDF, DOCX and TXT; render pages to PNG; password-protected documents; save a block, insert a file; auto-recovery; document templates |
| Editing | Multi-paragraph selection, Hangul-style caret and delete shortcuts, 40 undo steps, copy / paste with formatting (HTML), find / replace (including headers, notes and text boxes), character map, autotext, key macros |
| Formatting | Character and paragraph shapes, styles and style sets, bullets and numbering, outline numbering, tabs, borders and backgrounds, highlighter, ruby text, drop caps |
| Tables | Create, select cell blocks, merge and split, row / column editing, formulas, table styles and user table styles, draw / erase, nested tables, captions |
| Objects | Pictures (with effects), shapes, text boxes, connectors, equation editor, 15 chart types, form objects, group / order / align / lock |
| Page and print | Page setup, page numbers, headers and footers, columns, sections, master pages, line numbers, page borders and backgrounds, print preview, n-up printing and watermarks |
| Review and tools | Memos and replies, tables of contents / tables / figures, cross-references, indexes, document compare and merge, hanja conversion, find and mask personal information, statistics |
| Automation | CLI batch editing, 16 MCP tools and 415 editing commands, Hangul automation action names (`hwp_action`) |

The detailed list is in [Features](docs/FEATURES.en.md).

## What changed in 0.20

- **Track changes**: Hangul documents with tracked changes open with the changes shown, and changes can be accepted or rejected one by one or all at once. With recording on, edits in the body and table cells are recorded as changes with author and date, and HWP / HWPX saves keep the tracking (0.19 refused to save such documents).
- **Pages and view**: vertical body text and manuscript paper, odd / even section start, master pages carried on across section breaks, last-page and optional-page master pages, date fields in headers and footers, object move guides, window arranging inside the monitor the window is on, choosing a printer in the print window.
- **Editing**: selections across sections and Select All over the whole document, multi-paragraph selection inside table cells and text boxes, cell blocks in nested tables, cross-references to outline numbers / headings and picture captions, moving a horizontal border for a cell block, all 15 chart kinds in the desktop chart dialog, objects with an outer margin drawn as in Hangul.
- **MCP**: 2025-era clients are accepted alongside the current `2026-07-28` (checked with Codex CLI and Claude Code). stdio requests run in the order sent, and several desktop windows can be attached separately. The 16 tools were checked, and the examples of 410 of the 415 editing commands (all but the 5 that use the OS clipboard) were called for real without errors.
- **Interface and speed** (release builds, same machine as 0.19): dialogs scroll in small windows, and the context menu, font size and spacing were tidied. Longest wheel-scroll frame in the 381-page handbook 160–200 ms → 20–27 ms, handbook HWP save 429 → 245 ms, HWP save of a 215-page report 273 → 108 ms, undo back to the just-opened handbook 290 → 37 ms.

Full details are in the [0.20 release notes](docs/RELEASE-0.20.md).

## Version history

Versions increase as `0.10 → 0.11 → 0.12 …` (`x.xx`; Cargo uses `0.10.0` internally).

| Version | Main changes |
|---|---|
| [0.20](docs/RELEASE-0.20.md) | Track changes (view, accept / reject, record), vertical body text and manuscript paper, sections carrying master pages on, 2025-era MCP clients, interface cleanup, faster saving, undo and scrolling |
| [0.19](docs/RELEASE-0.19.md) | Wider Hangul editing (tables of contents, cross-references, indexes, style sets, form objects, 15 chart types and more), matching MCP commands, faster layout, replace-all and PDF |
| [0.18](docs/RELEASE-0.18.md) | 11 chart types, failed atomic batches restore undo history and clipboards, MCP Tasks, `outputSchema` and progress notifications, faster editing, opening and PDF, UI cleanup |
| [0.17](docs/RELEASE-0.17.md) | Pending input formatting, editing headers, footers, text boxes and notes in place, MCP 2026-07-28 (per-document workers, HTTP token authentication), Windows, macOS and Linux packages |
| [0.16](docs/RELEASE-0.16.md) | Body memos, address-based nested-table editing, dynamic body page numbers, a subset of public automation parameters |
| [0.15](docs/RELEASE-0.15.md) | Paragraph moves, shared highlighter output for screen, PNG and PDF |
| [0.14](docs/RELEASE-0.14.md) | Caret, selection, clipboard and Hangul key behaviour, more formatting, tables, objects and page features, hanja conversion, document compare, autotext, `hwp_action` |
| [0.13](docs/RELEASE-0.13.md) | Previously unused RHWP APIs (password documents, privacy search and more), macOS and Linux support |
| [0.12](docs/RELEASE-0.12.md) | Document scroll bars and document-tab layout |
| [0.11](docs/RELEASE-0.11.md) | Cell padding from the ruler, page-corner marks |
| [0.10](docs/RELEASE-0.10.md) | Version scheme, Ctrl+wheel zoom |

Plans for later versions are in the [feature roadmap](docs/FEATURE-ROADMAP.md) (Korean).

## Build

Rust 1.98.1, pinned in `rust-toolchain.toml`. Windows x64 needs MSVC and the Visual Studio C++ build tools; macOS needs the Xcode command-line tools; Linux needs a C compiler plus X11 / Wayland and OpenGL development packages; file dialogs use xdg-desktop-portal (shipped by GNOME and KDE), so no GTK development packages are needed. `HwpCraft.ps1` (Windows) and `hwpcraft.sh` (macOS / Linux: `build`, `test`, `install`, `start`, `status`) wrap the same Cargo commands.

Windows installer: `scripts/build-installer.ps1` stages the same files as `HwpCraft.ps1 Install` and compiles `packaging/windows/HwpCraft.iss` with Inno Setup 6 into `output/installer/HwpCraft-<version>-windows-x64-setup.exe` (optional desktop shortcut and `.hwp` / `.hwpx` association).

Platform status (0.20): Windows x64 is fully tested, including clippy and the real desktop window, and Linux x86_64 ran the workspace tests for real under WSL. macOS (arm64 and x64) and Linux x86_64 run the workspace tests in the GitHub Actions release build; the desktop window has not been checked by hand on a Mac or a Linux desktop. Fonts come from a per-OS candidate list (Windows 맑은 고딕, macOS Apple SD Gothic Neo, Linux Noto Sans CJK KR / NanumGothic …), first installed wins. Linux needs a Korean font (`fonts-noto-cjk` or `fonts-nanum`); without one only Latin text and digits show, and Hangul appears only if another installed font covers it. The settings folder is `%APPDATA%\HwpCraft` on Windows, `~/Library/Application Support/HwpCraft` on macOS (an existing `~/.config/HwpCraft` keeps being used) and `$XDG_CONFIG_HOME/HwpCraft` or `~/.config/HwpCraft` on Linux. On macOS shortcuts use Cmd (⌘) instead of Ctrl. The macOS app is not signed or notarized: on first launch right-click → Open, or run `xattr -d com.apple.quarantine HwpCraft.app`; opening documents by double-clicking in Finder is not supported (open them from the app). On Linux, unpack the bundle and run `./install.sh` to install into `~/.local`. Launcher registration and portable ZIP packaging exist in the Windows script only.

HwpCraft depends on RHWP as a **sibling path dependency** (`../rhwp`, pinned to RHWP **v0.8.7**, commit `1a76570e`). Clone both repositories next to each other:

```text
git clone https://github.com/webvine/hwpcraft.git
git clone --branch v0.8.7 https://github.com/edwardkim/rhwp.git
cd hwpcraft
cargo build --release --locked -p hwpcraft -p hwpcraft-cli
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

One engine test opens a chart sample from `../rhwp/samples`, so the RHWP checkout must include its `samples` folder. HwpCraft's local RHWP patches live in `patches/rhwp/`; apply them before building with `pwsh -File scripts/rhwp-patches.ps1 -Mode Apply` (`scripts/sync-siblings.ps1` re-applies them after updating RHWP). On Windows, `HwpCraft.ps1` (`Build`, `Test`, `Install`, `Start`, `Status`) wraps the same commands and uses `../.build/hwpcraft` as the Cargo target directory; the `Launcher` and `Install` actions target a local launcher that is not part of this repository.

Run the desktop app:

```text
hwpcraft ./samples/HwpCraft-시작하기.hwpx
hwpcraft ./samples/HwpCraft-시작하기.hwpx --view replace --screenshot out.png --exit-after-screenshot
```

`--view` opens a dialog or ribbon tab directly for visual checks (for example `character`, `paragraph`, `table-create`, `page-setup`, `find`, `replace`, or a ribbon tab name such as `insert`).

Do not run `cargo fmt --all`: it follows the path dependency into RHWP. Format the HwpCraft packages with explicit `-p` arguments.

## Repository layout

| Path | Role |
|---|---|
| `crates/doc` | Document engine façade, Unicode cursor and table-cell addressing |
| `crates/formats` | HWP / HWPX load and save, PDF / TXT export, safe file replacement |
| `crates/render` | RHWP SVG → page bitmaps via resvg, hairline handling, installed font list |
| `crates/engine` | Session and command protocol, selection / clipboard / caret, find, sort and control marks, tab widths, formatting, tables and objects, page services |
| `crates/mcp` | stdio JSON-RPC MCP server: headless editing, tools, resources, command help |
| `crates/ui-egui` | Main window, continuous page view, keyboard / IME, ribbon, toolbars, rulers, status bar, dialogs |
| `apps/hwpcraft` | eframe / glow desktop executable |
| `apps/hwpcraft-cli` | Analysis, conversion, PNG rendering and batch-edit CLI, MCP entry point |
| `samples` | HWP / HWPX / PDF examples generated by HwpCraft itself |
| `docs` | Design notes, verification records and release notes (Korean) |

Feature areas plug in through `extensions.rs`, so each area adds its own commands, dialogs and shortcuts in its own files.

## Documentation

| Document | Contents |
|---|---|
| [Features](docs/FEATURES.en.md) | Detailed feature list by area |
| [MCP connection and usage](docs/mcp.md) | Connecting, metadata, document handles, tools, file permissions, Tasks (Korean) |
| [MCP command parameters](docs/mcp-commands.md) | Parameters and examples per command, report-writing recipe (Korean) |
| [MCP implementation](docs/mcp-implementation.md) · [MCP verification](docs/mcp-verification.md) | Server structure and verification records (Korean) |
| [Table interaction](docs/TABLE-INTERACTION.md) | Selecting, moving and resizing tables (Korean) |
| [Feature roadmap](docs/FEATURE-ROADMAP.md) | Plans for later versions (Korean) |
| [Release notes](docs/RELEASE-0.19.md) | Changes per version (`docs/RELEASE-0.xx.md`) |

## Scope and limits

### Not implemented (disabled in the ribbon and menus)

- MCP: the desktop connection edits the one document open in the window; click / key / screen capture, remote authentication and a select-all extension are not done (the official conformance suite passes every scenario that has a fixture). Both MCP `2026-07-28` and 2025-era clients (2025-03-26, 06-18, 11-25) are served (checked with the Codex CLI, Claude Code and the Python SDK 1.x / 2.x in 0.20). Over stdio, requests sent to one document run in the order they were sent; requests sent concurrently over HTTP have no defined order, so wait for each answer or use batch. A window document's PDF export is prepared over several frames so the window keeps responding, but HWP / HWPX saves run at once (a little over a second for a large document). HTTP accepts loopback only (no standalone GET SSE stream) and uses an explicit ID per document; each MCP process is an independent headless session. Cancelling a request does not roll back a synchronous edit or file write that already ran; a save cancelled while running stops just before the file is replaced and leaves the existing file. Progress notifications for long operations report elapsed time every 5 seconds, not per-page progress. The Tasks extension applies to the open and save tools only, and task state is known only by polling tasks/get (no task notifications).
- Security: saving distribution documents (a distribution document opens read-only: viewing and page images only; editing, saving, copying, printing / PDF and text extraction are refused in the desktop, MCP and CLI), selected-text protection, removing private-information protection, changing its password and document security settings (Hangul's encrypted private-information field and these settings are not publicly specified, so they are not supported; such fields in existing documents are kept, and finding and masking private information is offered instead). Review/tools: proofreading marks, recording new document-history versions and viewing older ones (the recorded list is shown), recording format changes as tracked changes, spelling, Hangul's built-in quick-correction list (a user list works for text typed in the desktop only, undone by one Ctrl+Z), user dictionaries, translation, script (JavaScript) macros, citations (an index works since 0.19: index marks after a word as Hangul stores them, added, edited and removed with first and second keys in HWP/HWPX, and an index at the caret in Hangul 2024's default 색인 만들기 layout — first-sound groups, 'word<tab>page, page', second keys indented; Hangul makes it in a new document, HwpCraft at the caret, and paragraphs holding index marks cannot be brought in by document merge or formatted auto text. A table of contents can be made from outline paragraphs or styles, and a list of tables or figures from captioned tables or pictures; all are written as ordinary paragraphs in "차례 N" / "표 차례" / "그림 차례" styles, not as Hangul's table-of-contents field, and one refresh updates all three; each entry links to a bookmark at what it lists, in the bookmark-link form Hangul-saved documents use and as an in-document link in DOCX, and MCP / the engine follow them with the `hyperlink.jump` family, the desktop with Ctrl+click; in PDF, table of contents entries and cross references link to their page too (0.19, RHWP patch 0039; dropped on n-up sheets and for pages outside the PDF)) and preferences. Cross references insert and refresh the page of an outline heading, a captioned table or picture, or a bookmark, and a table's or picture's caption number (the bare number, as Hangul shows it) or whole caption (stored as Hangul's own cross-reference field in HWP/HWPX; picture references follow Hancom's published command format, and only the table form was checked in Hangul). Whether Hangul reads the HWPX form of a table number or caption reference as such has not been confirmed yet. Headings can be referenced by their visible number ("2.") and their text like tables and pictures (0.20, in the form Hangul 2024 was measured to use). Splitting a heading paragraph with Enter at its very start can break the cross references to that heading after an HWPX save; insert them again before refreshing. A cross reference can go inside a table cell (one level deep) and into a paragraph that holds an object such as a table, picture, bookmark or footnote, but not directly after such an object; place it one character away (RHWP patch 0067). If an object later ends up right after a reference, a refresh skips that reference and lists it under skipped. Refreshing date fields also covers fields in table cells one level deep. Key macros record editing commands, typing and caret/cell moves (not caret moves made with the mouse), are saved by name and run with one rollback if a step fails. Document comparison, Hanja conversion and AutoText have the limited implementations described below.
- View: dragging an object draws move guides that snap to the edges and centres of the page, the body and other objects (View → Guides; Alt moves freely). Window arranging stays inside the work area of the monitor the window is on, and minimising all windows is supported (checked on a single-monitor PC only; several monitors are tested by parsing and choosing only). On macOS, shortcuts in menus, tooltips and the status bar show ⌘. The desktop MCP connection can be on in several windows at once; the first takes port 7083, the next 7084 and so on up to 7099, and a client registers each window's address under its own name. View colours use screen post-processing. File: Hancom's built-in templates (the template gallery offers only templates the user saved); HML export of documents not opened from HML. Auto-recovery saves a copy of a changed document every 5 minutes while idle in the `recovery` folder of the settings folder and offers it after an abnormal exit; the original file is never touched.
- Page: drawing tables, pictures and footnotes in vertical-text sections. Vertical body text (Latin text rotated or upright; the characters per column and page match measurements in Hangul 2024), section start position (as is, odd or even; as in Hangul, the page number skips without an empty page), creation and save date fields in headers and footers, and manuscript paper (400 characters A4 portrait, 200 characters A4 landscape, one character per cell, two Latin letters or digits per cell, trailing spaces hang at the line end) are supported. A section break carries the previous section's master pages on, as in Hangul. Print: the print window lets you choose a printer (paper printing uses the operating system's print command). Labels (A4 label tables, adding a page), N-up (1, 2, 4, 8 pages per sheet), print-only watermarks, odd / even pages, reverse order and copies are supported.
- Format: Style import/export and style sets use HwpCraft JSON; Hangul's style-set files (.sty) and its built-in set list are not used.
- Tables and objects: photo editing, 3D line and surface charts, verified Hancom-native chart compatibility, WordArt and OLE. Bubble sizes change only through `chart.configure` (`chart.data` edits X/Y values); the desktop chart dialog creates all 15 kinds too (its preview shows 'building' for a moment when it first opens). Visual equation editing covers the caret, typing, slot navigation and drag selection in the preview (the script field below follows only the caret, not the selection). Vertical text boxes, object alignment/selection and existing form values have basic support. Connector anchors are exact for paper-anchored objects and may drift for paragraph-anchored ones; line end-point editing excludes inline lines and connectors.
- Documents created in HwpCraft (or HWPX files HwpCraft wrote) are saved to HWPX without line layout (linesegarray), so Hangul and HwpCraft lay them out afresh when opening. Documents opened from Hangul keep their stored line layout, but heavily edited paragraphs may still carry stale layout computed by RHWP.
- Tab drop-down lists are ribbon command lists, not Hangul submenus.
- Direct table dragging targets top-level tables within one page. A table treated as a character goes to the body text position it is dropped on, as in Hangul (`object.move_inline`); a floating table moves by position. Multi-page tables are adjusted through properties. Table drawing and erasing work on tables with cell spacing too, on the cell grid of `table.info` (spacing not counted); RHWP draws the spacing inside the preceding cell, so no gap shows between cells.
- Cells of a table inside a table and text boxes are edited on the page since 0.19: a click there places the caret, and typing (including the Korean IME), Backspace, Delete, Enter (paragraph split), arrows, Home / End, Shift selection, Ctrl+Z, Ctrl+B / I / U, copy and paste work, each edit one undo step (RHWP patch 0046). Home / End go to the start / end of the line, a double / triple click selects the word / paragraph, Tab / Shift+Tab move to the next / previous cell of a nested table (a tab character in a text box), and the character and paragraph buttons of the ribbon and the formatting toolbar apply there too. Esc or a click in the body returns to the body caret. As in Hangul, a click on a text box's text places the caret there, a click near its frame (within 4 px) selects the box, and dragging a selected box moves it. A selection runs over the paragraphs of one cell or text box (not from one cell into another), and the character and paragraph format dialogs show and change the format there (0.20). Dragging into another cell of a nested table picks a cell block, and the ribbon's merge, split and row / column insert and delete work on that table (engine and MCP: `context.table.merge`, `split`, `insert_row`, `insert_column`, `delete_row`, `delete_column`, `cells` for borders and backgrounds, `info`; 0.20). Text boxes inside groups and the regions below use the menu that follows.
- Nested tables, headers / footers, text boxes and notes are edited from the Edit tab's **문서 영역 편집** (edit document regions) menu: character / paragraph format, split / merge, table create / delete. An object treated as a character is dragged to another text position (`object.move_inline`; at a memo or form-field start it goes inside, at its end outside), and several selected floating objects move together (`object.move_by`). Vertical text boxes are edited the same way (`context.v2.*`). Vertical headers / footers are still read-only. The body of a vertical-text section can be edited (↑↓ move within a column, ←→ between columns, and the caret is a bar across the column); nested areas inside it (table cells, text boxes) are read-only. In vertical-text sections centred and right-aligned paragraphs sit in the middle and at the end of their column; justified and distributed paragraphs start at the top without spreading the characters, and paragraph margins and indents do not move the column start (not compared with Hangul: no vertical sample). The public parameter sets `CharShape`, `ParagraphShape` and `TableCreate` list their supported fields through `hwp.parameters.schema`.

### Remaining typesetting and save limits

- Memos have no resolved state, as Hangul has none. A memo across several paragraphs is created and deleted the way Hangul stores one, beginning in the first paragraph and ending in the last (body paragraphs of one section; replies to it are not supported yet). For a memo with replies, the HWPX end markers (fieldEnd) close the outer memo first while Hangul closes the inner one first; they pair by beginIDRef. Whether Hangul rewrites a last-save date field's text when it saves is not confirmed; HwpCraft updates only the summary times on save and the field text on refresh. Date fields follow Hangul's samples (no zero padding for month, day or hour) and write only the confirmed Command parameter to HWPX.

- Some HWP documents with many large tables break pages differently from Hangul. The .hwp of the 2025 행정업무운영 편람 handbook lays out to 381 pages, fewer than Hangul's 383 (384 without the KoPub fonts), because large rows that run over several pages inside a cell, such as the appendix regulation tables, are filled slightly further than in Hangul before they break; the same document as .hwpx gives 383 pages. In chapter 5 of the same handbook the question 51 table is drawn taller than in Hangul, so the tables on later pages are placed differently, and on page 299 of the .hwp the first piece of a table runs about 5 px past the body bottom because its last row is compressed. The difference comes from RHWP's HWP5 layout path and is a known RHWP issue (#6842).
- Picture effects (shadow, glow, soft edge, reflection) can be set with `picture.effects` and are saved, reopened and drawn in both HWP and HWPX (effects in documents opened from .hwp are drawn too; 0.20). The glow, soft-edge and reflection shapes and the effect colours written to HWP have not been compared with Hangul yet.
- DOCX export writes character and paragraph formatting, tables (merges, borders, fills), pictures, equations (as pictures), headers and footers with page numbers, footnotes and endnotes, and columns. Shapes, text boxes and charts that push body text aside are cut from the page image and placed as pictures; shapes behind or in front of text and shapes inside tables or headers are left out. Floating pictures and tables are placed inline, and the result lists what could not be written (MCP `omitted`, CLI stderr, the desktop message bar). DOCX import and ODT are not supported yet.
- Line numbers: turned on per section with start, interval, distance from the body and numbering (per page, per section, continuing) (`page.line_numbers`, Page → 줄 번호, section settings), and drawn on body lines on screen and in PDF (RHWP patches 0020 and 0037). The settings are kept when saved as HWP or HWPX. Lines in tables, text boxes, notes, headers and footers are not counted, and the numbers' position and font have not been compared with Hangul's screen. The meaning of the numbering value (`restartType`) is not in the public documents; Hangul's automation table (`LineNumberRestart`) is followed (unconfirmed). In PDF the numbers are text, so text extraction returns them too (after the page's text). A paragraph's "skip line numbering" (Page → 줄 번호 → 현재 문단 건너뜀, `page.line_numbers.skip`) is read, saved and honoured in HWPX (RHWP patch 0042). Its place in HWP files is not in the public HWP 5.0 document, so HWP neither reads nor writes it: an HWPX original saved as HWP loses it.
- Paragraph border top / bottom spacing, as in Hangul, enlarges only the border box and not the paragraph's height, so a border wider than the gap to the next paragraph overlaps it (Hangul draws it the same way). Side spacing and some line styles (double lines) draw as single lines.
- A ruby text's base text counts toward line breaking (0.20); one at the very end of a paragraph is still measured as zero width.
- Objects with an outer margin are drawn inside it as in Hangul (left / top aligned tables and shapes in front of or behind text, and all pictures). Centre, right and bottom alignment, tables placed horizontally relative to the page and objects treated as characters are drawn as before because there is no evidence for them yet, and the later pieces of a table in front of or behind text that runs over several pages are still drawn higher by the top margin. Saved files are not affected.
- Master-page pictures and shapes are placed on the paper (in mm) and removed from the master-page dialog and through the engine / MCP (`page.master.object.*`). Pictures, shapes, text boxes and tables on a master page are resized and moved with the same object commands as in the body (`object.properties`, `object.size`, `object.move_by` …) through the address `page.master.object.list` gives (with `master`; the dialog's Edit), and the master page's text and the text of its table cells and text boxes are edited paragraph by paragraph with `context.v2.*` (`master_page` in `include`). A master page holding a floating table can still have its whole text replaced with `page.master.set` (the table stays in the first paragraph). A master page holding a table treated as a character or a control such as a page number is not rewritten as a whole, and a section that read two master pages of the same kind is not edited. Master-page objects cannot be dragged on the page.
- When a Hangul-made HWPX is edited and saved as HWPX again, the edited paragraphs are written without line layout so that Hangul lays them out afresh; reopened in HwpCraft, its page breaks may differ from the edited document (an HWP save keeps them).
- A document made in HwpCraft and saved as HWP stores its line layout as implementation rows (HWP line-segment flag bit 31) with the fresh layout its HWPX would get when read. How Hangul opens such an HWP is not measured yet (two RHWP sample HWP files carry the same flag).
- Highlights are saved as markpen marks and composited by HwpCraft's shared SVG path for screen, PNG and PDF/print. Highlights are drawn in the body, table cells at any depth, headers and footers (on every page they appear), footnotes, endnotes and text boxes; text boxes inside groups and tables inside headers are not covered. New numbered lists create new numbering definitions (RHWP's `set_numbering_restart` is not saved); curves are saved as dense polylines.
- Newly created/edited nested tables grow the necessary ancestor frames. Short frames in imported documents are normalized for display only (source and saved file unchanged) and within a fixed per-document work budget so opening never stalls. Documents over the budget or over 128 pages keep some or all tables in their original layout, so deeply nested content may still look clipped.
- Damaged tables whose stored row/column counts are smaller than their cells are repaired to the addressed extent on open.
- Page number and total-page fields show the number of the page their content lands on, with its character shape, number format and multi-digit width, on screen, in PNG and in PDF, in the body and also in table cells (nested ones too), text boxes, footnotes and endnotes (a table's repeated header row shows each page's own number; RHWP draws headers, footers and master pages per page). Fields in vertical cells or text boxes, text boxes inside groups, and tables or text boxes inside notes follow RHWP's drawing (Arabic digits, no width reserved). New page-number fields can still be inserted in the body only. HWP/HWPX/TXT page extraction refuses field-induced boundary differences; PDF page export is supported. In documents with page fields, body typing and deletion re-lay out only the edited paragraph (about 40 ms on a 381-page document); other edits such as paragraph splits re-lay out the whole display copy (about 0.3 s). Date codes currently insert literal text.
- Tables: darker / lighter cell shading stops one step from the starting colour as in Hangul, and the opposite button goes back to it (after another edit the colour at that point becomes the new start; how long Hangul remembers the start colour is assumed, not measured); dragging a border inside a cell block moves it for the block's rows or columns only (`range` of `table.move_border`, vertical and horizontal), as in Hangul, but a horizontal block move is refused when it would make a cell smaller than its content, and in multi-page tables, tables with cell spacing and nested tables (`context.table.move_border`); rows and columns inserted into such a ragged table follow its grid (RHWP patch 0027); table-style font application is missing; user table styles (`table.user_style.*`, settings folder table_styles) keep borders, fills and diagonal lines by cell role (first, odd/even body, last row/column) but not character shapes; and the table-creation dialog cannot pick a table style (apply one from the Table Design tab afterwards). An object treated as a character is dragged to another text position (`object.move_inline`; inside a memo or form field at its start, outside at its end), and several selected floating objects move together (`object.move_by`). Vertical text boxes are edited through the typed context commands (`context.v2.*`) like horizontal ones; vertical headers / footers and nested areas inside vertical-text sections stay read-only there. Objects: picture shadow / glow / soft edge / reflection (RHWP only preserves them), drawing gallery and saved formats are missing; a picture inserted in a table cell floats at that cell's top-left corner (square wrap); a picture treated as a character inside a cell is not supported yet; a free-form line follows the drag and a curve passes smoothly through clicked points as in Hangul, both stored as short line segments; outer-margin / text-box-margin presets and the shadow nudge unit are assumptions not verified against Hangul. Footnote "restart per page" renumbers from the start number on every page that has footnotes, on screen and in PDF (body marks and the footnote area, RHWP patch 0036); files keep running numbers. Note separator lengths take Hangul's presets 5 cm (−1) and, for endnotes, column width (−4), or 0–115 mm (the HWP 16-bit limit).
- Page copy / delete work on the text laid out on that page (a paragraph running over a page break contributes only its part on that page). Insert file lives in the File menu only (Ctrl+O stays Open). PDF print options (highlighter, drawing objects, pictures, memos) are not provided because RHWP has no option for them. Document history (Review tab, `review.history.list`) only lists the versions Hangul recorded in an HWP file (date, writer, description), and the last recorded version can be saved as HML (`review.history.save_last`). New versions are not recorded and older versions are not shown or restored (the differences between versions are not applied); that HML is HWPML 2.5, which HwpCraft cannot open. The record is kept when saved as HWP; HWPX history is not read. With the page outline turned off (Ctrl+G,L) each page shows only its body (no top / bottom margins, header, footer or page number) and pages run on with a dotted line between them; it changes the screen only and keeps the left and right margins. The grid is a screen-only 5 mm overlay. Facing pages only changes the page layout.
- Besides plain AutoText (상용구), body AutoText with formatting, tables, embedded pictures and basic shapes is kept in its own store. It goes in at the body caret (since 0.19 also mid-paragraph: a one-paragraph snippet without a table joins the text, a longer one goes between the halves of the split paragraph); insertion in a cell, groups, chart/OLE fragments and general external exchange are not supported.
- Hanja conversion picks candidates per syllable from the Unicode Unihan reading table; without a word dictionary it does not offer word-level candidates or frequency ordering like Hangul. Spell checking remains unavailable. Documents with Hangul change tracking (HWP and HWPX) open as the marked view: inserted text is underlined in blue, deleted text struck through in red and format changes underlined in green, on screen and in PNG/PDF (body and table cells). The changes can be listed (kind, text, author, date) and accepted or rejected one by one or all at once, and with recording on, typing, deleting, splitting and joining paragraphs in the body and table cells are recorded as changes with author and date (Review menu; MCP `document.track_changes`, `.accept`, `.reject`, `.record`; one undo step). Saving as HWP or HWPX keeps the tracking whether or not the document was edited: an unchanged HWP is written as read, an edited HWP gets the accepted result (BodyText) and the marked view (ViewText), and HWPX gets insertBegin/deleteBegin marks and the header's change and author lists. On nine tracked documents, accepting every change gives the result Hangul stored. The Review tab turns recording on and off, accepts or rejects the change at the caret or every change, and steps to the previous or next change (table cells included); the status bar shows whether the document holds tracked changes and whether recording is on. Limits: format changes cannot be rejected (their earlier format is unknown) and are not recorded; while recording, formatting, object, table-structure and section edits and formatted paste are refused; changes inside nested tables, headers, notes and text boxes are not listed; DOCX, page/block saves and password-protected HWP cannot hold the tracking and save the accepted result only; the recording state is not saved in the file. Tracked files HwpCraft wrote and the HWPX marks (built from the OWPML schema; no tracked HWPX sample exists) have not yet been opened in Hangul. Memos cover body and table-cell ranges with replies; there is no resolved state (Hangul has none), and memos across several body paragraphs are created and deleted since 0.20. Document compare lists added / removed / changed paragraphs and a side-by-side list of the current and the file's text, and merges chosen differences toward the file (0.19, `review.merge.plan` / `review.merge`, formatting, tables and pictures included, one undo step). Showing both documents next to each other with synchronised scrolling and merging into tracked changes are not available; with a different section count only sections of the same number are compared, and a section's first paragraph holding objects is not removed by a merge. "No shading" is reported by RHWP as white (#ffffff).
- Section break (`page.section_break`) starts a new section at the caret with the same page setup and columns; headers and footers carry on as in Hangul. As in Hangul, a new section has no master pages of its own and keeps using the previous section's both / odd / even master pages (RHWP patch 0061), so editing them changes every section that carries them on, and later sections of a document split in Hangul show the earlier section's master page. Hiding the master page on the new section's first page is honoured. Last-page and optional-page master pages are not carried on and stay on their own section (not compared with Hangul). Backspace at a section's start or Delete at the end of the previous one joins the two sections, which then follow the first section's settings and master pages (the paragraphs stay apart; pressing again joins them).
- Master pages: as in Hangul, an odd or even master page takes the place of the both-pages one on its pages, and the storage order and section flags match Hangul's. Master pages can be created and edited for both, odd, even, last and optional pages (the n-th page of a section). Optional-page master pages are saved only to HWPX: the HWP format has no place for the page number, so saving to .hwp is refused with the reason and the alternatives (save as .hwpx, or delete the optional master page). The desktop app, MCP save_document and CLI convert all behave this way. Optional master pages read from HWP files carry no page number; they can be viewed and saved only. When a section has only an odd or only an even master page, Hangul draws it on those pages only, but RHWP draws it on every page. Document summary, odd / even / first-page-only page borders, picture fill, WordArt and OLE creation are unavailable because RHWP has no API for them. Form objects can be inserted and their properties edited (name, text, font, text / background colour, size, border drawing, enabled, printable, combo list, radio group, read-only / multi-line / numbers-only / maximum length / password character of input boxes; `form.properties`, 0.20). The border fill and text alignment cannot be edited, and a combo box's list is saved to HWPX only (Hangul keeps it in the document script for HWP, which RHWP does not write). The background colour of check boxes and radio buttons is saved but not drawn. Style create / update / delete exist only in RHWP's wasm wrapper, so HwpCraft edits the model directly. Line-based caret motion and selection-rectangle queries are crate-private in RHWP; HwpCraft computes them from caret geometry.
- Tab widths are filled in on save in the body, table cells at any depth, headers, footers, footnotes, endnotes and text boxes (text boxes inside groups and tables inside headers are saved with width 0).
- A body selection runs over sections as in Hangul, and Select All takes the whole document (0.20); deleting such a selection joins the sections it crossed, as deleting a section break does. Copying it copies the text only, and sorting, text to table and block save (`file.save_block`) take a selection within one section. A table-cell selection stays in one cell.
- In a line made tall by an object treated as a character, ↓ may skip the next line and land on the one after it (0.19 did not move at all).

### Verification limits

- Printing makes a PDF with the page range, odd / even, reverse order, collated copies, paper orientation, 2 / 4 / 8 pages per sheet and watermark applied, and sends it to the printer chosen in the print dialog (the OS default unless another is chosen). In 0.19 that PDF was checked option by option on a 44-page report (sheet count and the pages on each sheet against HwpCraft's own PDF; 2 and 8 pages per sheet from landscape originals stack on a portrait sheet). The step after it, to the printer driver, was not checked: the test PC's default printer is a paper printer, and Windows 11's default PDF app (Edge) has no print verb, so HwpCraft only opens the PDF there (copies are set again in that dialog). Microsoft Print to PDF asks for a file name, so it could not be driven automatically.
- Pointer-event regression tests cover table selection, moving and resizing. End-to-end click / drag verification of every feature has not been completed.
- Round-trip saving of complex real-world documents and performance on very large documents and tables have not been tested thoroughly. Long typing was tried in 0.19 by feeding 10,000 characters as IME composition and commit events into the release desktop app (`--view bench-ime`): on the 44-page report and the 381-page handbook memory stayed flat thanks to the 40-step undo limit (about 370 MB / 830 MB), and the report saved and reopened with all 49 pages identical. Typing through the real Windows IME was not possible in the test environment, which had no interactive desktop. Keep originals and save important documents under a new name, then check them in Hangul.
- Input documents are limited to 128 MB and a single text insertion to 1 MB. Parsing and rendering are synchronous, so the UI may pause on complex documents. PDF export of large documents takes time (about 17 s for a 381-page document; pages are converted on up to 8 threads, using up to about 1.9 GB of memory, adjustable with `RHWP_PDF_THREADS`).

Release notes are in `docs/` (Korean).

## Contributing

Bug reports and pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

- Contributions are provided under the MIT License, and every commit is signed off (`git commit -s`, DCO).
- Editing commands belong in `crates/engine`, shared by the desktop app, CLI and MCP server.
- A failed load, command or save must never destroy the current document or an existing file.
- Unimplemented features and compatibility limits are recorded honestly in this README.
- Use only fonts and icons whose licenses allow redistribution; no Hancom code or resources.
- AI-assisted code is accepted when a human reviews it, takes responsibility for it and signs it off.
- Before pushing, pass `cargo test --locked --workspace` and `cargo clippy --locked --workspace --all-targets -- -D warnings`.

## License

HwpCraft is released under the **MIT License**. The full text is in [LICENSE](LICENSE); the copyright holders are the HwpCraft contributors.

**What the MIT License covers** — everything HwpCraft itself produces in this repository.

- Source code: the Rust code in `crates/` and `apps/`, plus the build / packaging scripts (`HwpCraft.ps1`, `hwpcraft.sh`).
- Documentation and samples: `README.md`, `docs/`, `CONTRIBUTING.md`, and the HWP / HWPX / PDF documents in `samples/`.
- Assets made by HwpCraft: the application icon `assets/icon.svg` and the ribbon icon atlas `assets/toolbar-icons.png` (a composition of Lucide icons).
- Contributions: every change submitted by pull request is provided under MIT and requires a DCO sign-off ([CONTRIBUTING.md](CONTRIBUTING.md)).

**Third-party components** — each keeps its own license and is not relicensed under MIT. Full texts are in `assets/licenses/`; the list is in `THIRD-PARTY-NOTICES.txt`.

| Component | License | Where |
|---|---|---|
| [RHWP](https://github.com/edwardkim/rhwp) document engine (Edward Kim) | MIT | Separate repository (`../rhwp`, v0.8.7), linked into the binaries. HwpCraft's local changes are kept as patch files in `patches/rhwp/` and stay under RHWP's MIT license |
| Code adapted from WordCraft, DeckCraft and DesignCraft (ArtCraft team) | MIT | Marked in the file headers of `ribbon_layout.rs`, `file_access.rs`, `chrome.rs` and others |
| [Lucide](https://lucide.dev) icons | ISC | `assets/icons/lucide/` |
| egui default fonts: Hack, Noto Emoji, Ubuntu, emoji-icon-font | MIT, OFL 1.1, UFL 1.0, MIT | Compiled into the binaries |
| Rust crate dependencies (`Cargo.lock`) | Permissive licenses: MIT, Apache-2.0, BSD, ISC and similar | `THIRD-PARTY-DEPENDENCIES.md` |

Crates offered under a choice of licenses are used under MIT or Apache-2.0. No code distributed only under GPL or AGPL is included.

**What is not included** — no Hancom application code, icons, logos, fonts, templates or clip art exist anywhere in the source or the binaries. "Hangul" (한글), "Hancom", "HWP" and "HWPX" are trademarks of their respective owners; the MIT License grants no trademark rights, and this repository uses the names only to describe file-format compatibility.

When redistributing binaries, ship `LICENSE`, `THIRD-PARTY-NOTICES.txt` and the license texts in `assets/licenses/` alongside them. The packaging script copies them automatically.

## Acknowledgements

- [RHWP](https://github.com/edwardkim/rhwp) (MIT) — HWP / HWPX document engine.
- [Lucide](https://lucide.dev) (ISC) — ribbon icons in `assets/icons/lucide`, built into `assets/toolbar-icons.png`.
- [WordCraft](https://github.com/storytold/wordcraft), DeckCraft and DesignCraft (MIT, ArtCraft team) — crate structure, ribbon width planning, file replacement, MCP structure, dialog draft / commit patterns. Licenses are kept in `assets/licenses/` and listed in `THIRD-PARTY-NOTICES.txt`.
- [egui](https://github.com/emilk/egui) / eframe, [resvg](https://github.com/linebender/resvg), and the other crates in `Cargo.lock`.

Hancom, Hangul (한글) and HWP are trademarks of their respective owners and are used here only to describe file-format compatibility.
