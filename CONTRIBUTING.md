# Contributing

## Before starting

Open an issue before implementing a change. Use it to agree on the problem, expected behaviour, and scope.
Open a pull request afterwards and link the issue it resolves.

Keep discussions respectful and focused on technical evidence.

## Pull requests

- Keep each PR focused on one problem. Avoid unrelated refactoring.
- Use Gitflow branches (`feature/...`, `fix/...`, `hotfix/...`) and Conventional Commits (for example, `feat: add group creation` or `fix(ui): adjust spacing between menu items`).
- Follow the existing Rust structure and naming. Write comments only for non-obvious constraints or reasoning.
- Add or update tests for behavioural changes. Include screenshots for UI changes.
- Update translations and documentation when the user-facing behaviour changes.
- Do not commit build output or unrelated files.
- Verify the pinned Horizon and mixin revisions when changing compatibility code.

Run the relevant checks before submitting:

```sh
cargo test --offline
cargo horizon-mod check
cargo horizon-mod build
```

State clearly in the PR if something remains untested in the running game.

## AI-assisted development

AI tools are allowed. The human author is fully responsible for the code,
its correctness, security, licensing, and verification.

Do not credit AI tools as co-authors. Do not submit unreviewed generated output,
fabricated verification, placeholder logic, unrelated rewrites, or other low-quality
generated content. Review every change as if you wrote it yourself.
