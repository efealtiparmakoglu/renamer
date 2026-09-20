//! renamer — batch file renamer with regex captures, collision detection
//! and an undo script.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use regex::Regex;

use renamer::plan::{plan, undo_script, validate_replacement, PlanError};
use renamer::{apply, exists_on_disk, gather_inputs};

const VERSION: &str = "1.0.0";

const USAGE: &str = "\
renamer VERSION - batch file renamer with regex captures

USAGE:
    renamer [options] <pattern> <replacement> [files...]
    ... | renamer [options] <pattern> <replacement>    (paths on stdin)

With no files given, paths are read one per line from stdin (xargs style).
Dry-run is the DEFAULT: nothing is renamed without --execute.

OPTIONS:
    -x, --execute        actually perform the renames
    -o, --undo-file      write renamer-undo.sh after executing
    -i, --ignore-case    case-insensitive pattern matching
    -V, --version        version
    -h, --help           this help

REPLACEMENT:
    $1, $2 ... ${name}   capture groups (regex crate syntax)
    $0                   the whole match

SAFETY:
    - no-op renames are skipped
    - two files renaming to the same target abort the whole plan
    - targets that already exist on disk abort the whole plan
    - after --execute an undo script restores every rename in reverse

EXAMPLES:
    renamer -n 'IMG_(\\d{4})' 'holiday_$1' *.jpg      # preview
    renamer -x ' ' '_' *.mp3                          # spaces to underscores
    renamer -x -i '(?i)\\.JPEG$' '.jpg' *.jpeg
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run(args: &[String]) -> Result<(), ExitCode> {
    let mut execute = false;
    let mut undo_file = false;
    let mut ignore_case = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!("{}", USAGE.replace("VERSION", VERSION));
                return Ok(());
            }
            "-V" | "--version" => {
                println!("renamer {VERSION}");
                return Ok(());
            }
            "-x" | "--execute" => execute = true,
            "-o" | "--undo-file" => undo_file = true,
            "-i" | "--ignore-case" => ignore_case = true,
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if positional.len() < 2 {
        eprintln!("renamer: need <pattern> and <replacement> (try --help)");
        return Err(ExitCode::from(2));
    }
    let pattern = positional.remove(0);
    let replacement = positional.remove(0);

    let regex_source = if ignore_case { format!("(?i){pattern}") } else { pattern };
    let re = match Regex::new(&regex_source) {
        Ok(re) => re,
        Err(e) => {
            eprintln!("renamer: bad pattern: {e}");
            return Err(ExitCode::from(2));
        }
    };
    if let Err(e) = validate_replacement(&re, &replacement) {
        eprintln!("renamer: {e}");
        return Err(ExitCode::from(2));
    }

    let mut stdin = String::new();
    if positional.is_empty() && !std::io::stdin().is_terminal() {
        std::io::stdin().lock().read_to_string(&mut stdin).ok();
    }
    let inputs = gather_inputs(
        &positional.iter().map(PathBuf::from).collect::<Vec<_>>(),
        stdin.lines().map(str::to_string).collect(),
    );
    if inputs.is_empty() {
        eprintln!("renamer: no input files");
        return Err(ExitCode::from(2));
    }

    let (renames, errors) = match plan(&re, &replacement, &inputs, &exists_on_disk) {
        Ok(renames) => (renames, None),
        Err((err, partial)) => (partial, Some(err)),
    };

    if !execute {
        for r in &renames {
            println!("{} -> {}", r.from.display(), r.to.display());
        }
        if errors.is_some() {
            print_error(&errors.unwrap());
            eprintln!("renamer: dry-run found the problem above; nothing renamed");
            return Err(ExitCode::from(1));
        }
        println!(
            "renamer: {} rename(s) planned (dry-run; use --execute to apply)",
            renames.len()
        );
        return Ok(());
    }

    if let Some(err) = errors {
        print_error(&err);
        return Err(ExitCode::from(1));
    }

    match apply(&renames) {
        Ok(count) => {
            println!("renamed {count} file(s)");
            if undo_file {
                let script = undo_script(&renames);
                std::fs::write("renamer-undo.sh", script).map_err(|e| {
                    eprintln!("renamer: undo script write failed: {e}");
                    ExitCode::FAILURE
                })?;
                println!("undo script written: renamer-undo.sh (run with: sh renamer-undo.sh)");
            }
            Ok(())
        }
        Err((path, msg)) => {
            eprintln!("renamer: {}: {msg}", path.display());
            Err(ExitCode::FAILURE)
        }
    }
}

fn print_error(err: &PlanError) {
    match err {
        PlanError::DuplicateTarget { target, first, second } => {
            eprintln!("renamer: collision: {first:?} and {second:?} both map to {target:?}");
        }
        PlanError::TargetExists { target } => {
            eprintln!("renamer: target already exists: {target:?}");
        }
        PlanError::Noop => {}
    }
}
