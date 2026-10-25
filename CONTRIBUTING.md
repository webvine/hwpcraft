# Contributing to HwpCraft

Thank you for helping. Please read this page before opening a pull request.

## Ground rules

- HwpCraft is MIT-licensed. By contributing you agree that your contribution is licensed under the MIT License in `LICENSE`.
- Do not bundle fonts or icons unless their license allows redistribution (OFL, MIT, ISC, Apache-2.0 or similar) and the license text is added under `assets/licenses/` and listed in `THIRD-PARTY-NOTICES.txt`.
- Editing commands belong in `crates/engine` and are shared by the UI, CLI and MCP server. Text offsets count Unicode scalar values.
- A failed load, command or save must never destroy the current document or an existing file.
- Record unimplemented features and compatibility limits honestly in `README.md`.

## Developer Certificate of Origin

Every commit must be signed off (`git commit -s`), which adds a line such as

```text
Signed-off-by: Your Name <you@example.com>
```

By signing off you certify the [Developer Certificate of Origin 1.1](https://developercertificate.org/): that you wrote the change or have the right to submit it under the project license, and that you understand the contribution is public and will be kept with your sign-off.

## AI-assisted contributions

Code written with the help of AI tools is welcome when a human contributor reviews it, takes responsibility for it, and signs it off. Say so in the pull request description. Do not paste in output that reproduces third-party code or assets whose license you cannot verify.

## Before you push

```text
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt -p hwpcraft -p hwpcraft-cli -p hwpcraft-doc -p hwpcraft-formats -p hwpcraft-render -p hwpcraft-engine -p hwpcraft-mcp -p hwpcraft-ui-egui
```

Do not run `cargo fmt --all`: it follows the path dependency into RHWP. For page, IME or UI changes, run the desktop app and attach a screenshot (`--screenshot out.png --exit-after-screenshot`).

## 한국어 요약

- 기여물은 MIT 라이선스로 제공됩니다.
- 글꼴·아이콘은 재배포가 허용된 라이선스(OFL, MIT, ISC, Apache-2.0 등)만 쓰고 `assets/licenses/`에 라이선스 전문을 추가하세요.
- 모든 커밋은 `git commit -s`로 서명(DCO)합니다.
- AI 도구로 작성한 코드는 사람이 검토하고 책임지며 서명한 경우에만 받습니다. PR 설명에 표시해 주세요.
