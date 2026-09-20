# renamer

**Batch file renamer with regex capture groups, collision detection and an undo script — dry-run by default.**

[![CI](https://github.com/efealtiparmakoglu/renamer/actions/workflows/ci.yml/badge.svg)](https://github.com/efealtiparmakoglu/renamer/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

```
$ renamer 'IMG_(\d{4})' 'holiday_$1' *.jpg
IMG_0001.jpg -> holiday_0001.jpg
IMG_0002.jpg -> holiday_0002.jpg
renamer: 2 rename(s) planned (dry-run; use --execute to apply)
```

`renamer` renames file *base names* by regex, keeps directories intact, and
is built around one rule: **nothing happens unless you ask, and everything
that happens can be undone.**

## Why renamer

- **Dry-run is the default.** The plan prints; `--execute` applies it.
- **Collisions abort everything.** Two files mapping to the same target, or
  a target that already exists on disk, stops the whole plan before a single
  rename — you see the exact collision instead of a mangled directory.
- **Undo script.** With `--undo-file`, an executable `renamer-undo.sh` is
  written that reverses every rename, in reverse order, with proper shell
  quoting.
- **Replace-time validation.** The regex crate silently expands unknown
  `$group` references to empty strings (a deleted filename waiting to
  happen); renamer validates references against the pattern's groups and
  refuses them.
- **The parts that touch your files are small and tested.** Pattern matching
  goes through the `regex` crate; the planning, safety rules, undo
  generation and CLI are hand-written and unit-tested.

## Install

```sh
cargo install --path .
```

Requires Rust 1.70+.

## Usage

```sh
renamer 'IMG_(\d{4})' 'holiday_$1' *.jpg       # preview (dry-run)
renamer -x 'IMG_(\d{4})' 'holiday_$1' *.jpg    # apply
renamer -x -o ' ' '_' *.mp3                    # apply + write renamer-undo.sh
renamer -i '(?i)\.JPEG$' '.jpg' *.jpeg         # case-insensitive
find . -name '*.log' | renamer -x '\.log$' '.txt'   # paths on stdin
```

| Option | Meaning |
|---|---|
| `-x, --execute` | actually rename (default: dry-run) |
| `-o, --undo-file` | write `renamer-undo.sh` after executing |
| `-i, --ignore-case` | case-insensitive matching |

Replacement syntax: `$1`, `${name}`, `$0` (whole match), `$$` (literal `$`).
Unknown or out-of-range group references are a hard error.

## Safety rules

1. Only base names change; directory prefixes are preserved.
2. Renames that do not change the name (or would empty it) are skipped.
3. Two inputs targeting the same path abort the entire plan.
4. A target that already exists on disk aborts the entire plan.
5. Existence is re-checked immediately before each rename at execution time.
6. `renamer-undo.sh` restores the previous names in reverse order.

## Development

```sh
cargo test
cargo clippy --all-targets
```

The planner is pure logic with no I/O, so every safety rule is a unit test:
capture groups, directory preservation, no-op skipping, duplicate targets,
existing targets, chained renames, undo ordering and shell quoting.

## License

MIT — see [LICENSE](LICENSE).
