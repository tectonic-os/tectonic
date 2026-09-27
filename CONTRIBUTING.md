# Contributing

The [tectonic-os contributing guide](https://github.com/tectonic-os/.github/blob/main/CONTRIBUTING.md)
sets the rules every repository shares: one feature per pull request, the typed
title, the sign-off, the AI use policy, the code of conduct and where to report
a vulnerability.

## Checks

A pull request merges after these pass, beside GitHub's CodeQL analysis:

- `lint` runs `./lint.sh`: shellcheck, shfmt, rustfmt, the boundary greps and
  the tests. The tests are goldens: `UPDATE_GOLDEN=1 cargo test` regenerates
  them, and the diff is the review.
- `msrv` runs `cargo test --locked` on the `rust-version` that `Cargo.toml`
  states.
- `typed-title` checks the pull request title.

Run `./lint.sh --fix` to format, and `./lint.sh` before you push. It is the
check CI runs.
