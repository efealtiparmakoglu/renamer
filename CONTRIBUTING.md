# Contributing to renamer

Thanks for your interest!

## Ground rules

- **Dry-run stays the default.** Anything that makes renaming easier to
  trigger accidentally will be declined.
- **Plan and I/O are separate.** `src/plan.rs` is pure logic (no filesystem);
  execution lives in `src/lib.rs`. Keep it that way — it is what makes the
  safety rules unit-testable.
- **Collisions abort, always.** Partial renames on collision are a bug, not
  a feature.
- **Match the regex crate's semantics.** Replacement expansion is the crate's
  `$name`/`${name}` syntax; validation must reject exactly what the crate
  would silently mangle.

## Workflow

```sh
cargo test
cargo clippy --all-targets
```

## Pull requests

1. One feature or fix per PR.
2. Safety-adjacent changes (collision rules, undo script) need tests that
   demonstrate the old behavior failing.
3. CLI changes must update the usage text and README.

## Reporting bugs

Open an issue with the exact command line, the file names involved, and the
output. If a rename went wrong, say what you expected to be kept — the undo
script contents help too.
