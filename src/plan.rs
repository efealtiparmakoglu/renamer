//! Rename planning: pure logic, fully unit-testable, no I/O.
//!
//! A plan maps every input path to its target path. The planner enforces
//! the safety rules: targets must be unique, must not collide with files on
//! disk, and no-op renames are skipped.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use regex::Regex;

/// Validates capture references in a replacement string against the
/// pattern's groups. The regex crate silently expands unknown groups to an
/// empty string — a deleted filename waiting to happen — so this returns an
/// error instead. `$0` and `$$` are always fine.
pub fn validate_replacement(re: &Regex, replacement: &str) -> Result<(), String> {
    let mut refs: Vec<String> = Vec::new();
    let bytes = replacement.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                i += 2;
                continue;
            }
            let mut j = i + 1;
            let braced = bytes[j] == b'{';
            if braced {
                j += 1;
            }
            let start = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if braced && (j >= bytes.len() || bytes[j] != b'}') {
                return Err(format!("unclosed ${{...}} in replacement at byte {i}"));
            }
            if j > start {
                refs.push(replacement[start..j].to_string());
            } else if braced {
                return Err(format!("empty ${{}} reference at byte {i}"));
            }
            i = if braced { j + 1 } else { j };
            continue;
        }
        i += 1;
    }
    let has_groups = re.captures_len() > 1; // group 0 is the whole match
    if !has_groups && !refs.is_empty() {
        return Err(format!(
            "replacement references {refs:?} but the pattern has no capture groups"
        ));
    }
    for r in refs {
        if r == "0" {
            continue;
        }
        let known = re.capture_names().any(|n| n == Some(r.as_str()))
            || r.parse::<usize>()
                .map(|ix| ix > 0 && ix < re.captures_len())
                .unwrap_or(false);
        if !known {
            return Err(format!("replacement references unknown capture group ${r}"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug)]
pub enum PlanError {
    /// Two inputs rename to the same target path.
    DuplicateTarget { target: PathBuf, first: PathBuf, second: PathBuf },
    /// A target path already exists on disk (and is not the source itself).
    TargetExists { target: PathBuf },
    /// The pattern produced no change for this file (harmless; skipped).
    Noop,
}

/// Computes the new base name for one file. Returns None when unchanged.
pub fn new_basename(re: &Regex, replacement: &str, path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    // regex::Regex::replace honors $1 / ${name} capture references
    let replaced = re.replace(name, replacement);
    let replaced = replaced.into_owned();
    if replaced == name || replaced.is_empty() {
        return None;
    }
    Some(replaced)
}

/// Builds the full plan. `exists_on_disk` checks whether a target path is
/// already taken by something else (case-sensitivity is the caller's
/// filesystem's business; here we compare exact paths only).
pub fn plan(
    re: &Regex,
    replacement: &str,
    inputs: &[PathBuf],
    exists_on_disk: &dyn Fn(&Path) -> bool,
) -> Result<Vec<Rename>, (PlanError, Vec<Rename>)> {
    let mut renames: Vec<Rename> = Vec::new();
    for input in inputs {
        let Some(new_name) = new_basename(re, replacement, input) else {
            continue; // no-op renames are skipped silently
        };
        let to = match input.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(new_name),
            _ => PathBuf::from(new_name),
        };
        if to == *input {
            continue;
        }
        renames.push(Rename { from: input.clone(), to });
    }

    // duplicate targets among the plan itself
    let mut seen: HashMap<PathBuf, PathBuf> = HashMap::new();
    for r in &renames {
        if let Some(first) = seen.get(&r.to) {
            let err = PlanError::DuplicateTarget {
                target: r.to.clone(),
                first: first.clone(),
                second: r.from.clone(),
            };
            return Err((err, std::mem::take(&mut renames)));
        }
        seen.insert(r.to.clone(), r.from.clone());
    }

    // targets taken on disk (by files that are not being renamed away)
    let moving: std::collections::HashSet<&Path> = renames.iter().map(|r| r.from.as_path()).collect();
    for r in &renames {
        if exists_on_disk(&r.to) && !moving.contains(r.to.as_path()) {
            let err = PlanError::TargetExists { target: r.to.clone() };
            return Err((err, std::mem::take(&mut renames)));
        }
    }
    Ok(renames)
}

/// Renders an undo shell script that restores every rename in reverse.
pub fn undo_script(renames: &[Rename]) -> String {
    let mut out = String::from("#!/bin/sh\n# Auto-generated by renamer — restores the previous names.\nset -e\n");
    for r in renames.iter().rev() {
        out.push_str(&format!(
            "mv -- {} {}\n",
            shell_quote(&r.to),
            shell_quote(&r.from)
        ));
    }
    out
}

/// POSIX single-quote shell quoting.
pub fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exists_factory<'a>(taken: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |p: &Path| taken.iter().any(|t| Path::new(t) == p)
    }

    #[test]
    fn capture_groups() {
        let re = Regex::new(r"IMG_(\d{4})\.jpg").unwrap();
        let got = plan(&re, "holiday_$1.jpg", &[PathBuf::from("IMG_0001.jpg")], &exists_factory(&[])).unwrap();
        assert_eq!(got, vec![Rename { from: "IMG_0001.jpg".into(), to: "holiday_0001.jpg".into() }]);
    }

    #[test]
    fn named_and_multi_captures() {
        let re = Regex::new(r"(?P<artist>\w+)-(?P<song>\d+)\.flac").unwrap();
        let got = plan(
            &re,
            "${song}_${artist}.flac",
            &[PathBuf::from("beatles-1965.flac")],
            &exists_factory(&[]),
        )
        .unwrap();
        assert_eq!(got[0].to, PathBuf::from("1965_beatles.flac"));
    }

    #[test]
    fn keeps_directory_prefix() {
        let re = Regex::new(r"\.jpeg$").unwrap();
        let got = plan(
            &re,
            ".jpg",
            &[PathBuf::from("pics/a/b.jpeg")],
            &exists_factory(&[]),
        )
        .unwrap();
        assert_eq!(got[0].to, PathBuf::from("pics/a/b.jpg"));
    }

    #[test]
    fn noops_are_skipped() {
        let re = Regex::new(r"^[a-z]+\.txt$").unwrap();
        let got = plan(&re, "$0", &[PathBuf::from("keep.txt")], &exists_factory(&[])).unwrap();
        assert!(got.is_empty());
        // empty replacement names are refused (would delete the name)
        let got = plan(&re, "", &[PathBuf::from("keep.txt")], &exists_factory(&[])).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn duplicate_targets_abort() {
        let re = Regex::new(r"^img\d+\.png$").unwrap();
        let err = plan(
            &re,
            "image.png",
            &[PathBuf::from("img1.png"), PathBuf::from("img2.png")],
            &exists_factory(&[]),
        );
        let Err((PlanError::DuplicateTarget { target, first, second }, _)) = err else {
            panic!("expected duplicate target");
        };
        assert_eq!(target, PathBuf::from("image.png"));
        assert_eq!(first, PathBuf::from("img1.png"));
        assert_eq!(second, PathBuf::from("img2.png"));
    }

    #[test]
    fn existing_target_aborts() {
        let re = Regex::new(r"^a\.txt$").unwrap();
        let err = plan(
            &re,
            "b.txt",
            &[PathBuf::from("a.txt")],
            &exists_factory(&["b.txt"]),
        );
        match err {
            Err((PlanError::TargetExists { target }, _)) => {
                assert_eq!(target, PathBuf::from("b.txt"))
            }
            _ => panic!("expected target-exists"),
        }
    }

    #[test]
    fn rename_chain_is_allowed_when_ordered() {
        // a.txt -> b.txt while b.txt itself is renamed to c.txt
        let re = Regex::new(r"^(a|b)\.txt$").unwrap();
        let got = plan(
            &re,
            "${1}x.txt",
            &[PathBuf::from("a.txt"), PathBuf::from("b.txt")],
            &exists_factory(&[]),
        )
        .unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn undo_script_reverses_in_order() {
        let re = Regex::new(r"^(\d+)_").unwrap();
        let got = plan(
            &re,
            "pre${1}_",
            &[PathBuf::from("1_a.txt"), PathBuf::from("2_b.txt")],
            &exists_factory(&[]),
        )
        .unwrap();
        let script = undo_script(&got);
        let lines: Vec<&str> = script.lines().filter(|l| l.starts_with("mv ")).collect();
        assert_eq!(lines.len(), 2);
        // reverse order: the LAST rename is undone first
        assert!(lines[0].contains("pre2_b.txt") && lines[0].ends_with("'2_b.txt'"));
        assert!(lines[1].contains("pre1_a.txt") && lines[1].ends_with("'1_a.txt'"));
    }

    #[test]
    fn replacement_validation() {
        let re = Regex::new(r"(?<year>\d{4})-(?<month>\d{2})").unwrap();
        assert!(validate_replacement(&re, "${year}_${month}").is_ok());
        assert!(validate_replacement(&re, "$0 $1").is_ok());
        assert!(validate_replacement(&re, "$$ literal").is_ok());
        assert!(validate_replacement(&re, "$3").is_err());       // out of range
        assert!(validate_replacement(&re, "${day}").is_err());   // unknown name
        assert!(validate_replacement(&re, "${").is_err());       // unclosed

        let plain = Regex::new(r"\.jpeg$").unwrap();
        assert!(validate_replacement(&plain, ".jpg").is_ok());
        assert!(validate_replacement(&plain, "$1").is_err());    // no groups
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote(Path::new("it's file.txt")), "'it'\\''s file.txt'");
        assert_eq!(shell_quote(Path::new("plain.txt")), "'plain.txt'");
    }

    #[test]
    fn dump_undo() {
        let re = Regex::new(r"^(\d+)_").unwrap();
        let got = plan(
            &re,
            "pre${1}_",
            &[PathBuf::from("1_a.txt"), PathBuf::from("2_b.txt")],
            &exists_factory(&[]),
        )
        .unwrap();
        for r in &got {
            println!("RENAME {:?} -> {:?}", r.from, r.to);
        }
        println!("SCRIPT:\n{}", undo_script(&got));
    }

}

