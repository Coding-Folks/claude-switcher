## What changes for users

<!-- One or two sentences. Link the issue: Fixes #… -->

## How it was tested

- [ ] `cargo test --lib` (and `-- --skip live_` if Claude Desktop is not installed)
- [ ] `cargo clippy --lib --tests -- -D warnings` and `cargo fmt --check`
- [ ] `npx playwright test` in `tests-ui`
- [ ] Relevant steps of `tests/manual/README.md` (anything touching Windows users, UAC or slot folders)

## Checklist

- [ ] A test that failed before this change now passes
- [ ] UI changes keep keyboard access, visible focus and text for every status
- [ ] New strings are in `ui/strings.js`
- [ ] `CHANGELOG.md` updated under an *Unreleased* heading
