use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use regex::Regex;
use tempfile::tempfile;
use walkdir::{DirEntry, WalkDir};

/// Find potentially unused functions in a python source tree.
#[derive(Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(validator = path_exists)]
    path: PathBuf,
}

#[derive(PartialEq, Eq, Hash, Debug)]
struct Function {
    name: String,
    location: (PathBuf, usize),
}

/// Validates that a path exists.
fn path_exists(s: &str) -> Result<()> {
    let path = PathBuf::from(s);

    if !path.exists() {
        return Err(anyhow!("path does not exist"));
    }

    Ok(())
}

/// We only consider non-hidden entries.
fn is_useful(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| !s.starts_with("."))
        .unwrap_or(false)
}

/// Assumes anything ending with `.py` is a python source file.
fn is_python_file(entry: &DirEntry) -> bool {
    entry.file_type().is_file()
        && entry
            .file_name()
            .to_str()
            .map(|s| s.ends_with(".py"))
            .unwrap_or(false)
}

/// Returns an iterator over all python files in the given path.
fn make_walker(path: &Path) -> Result<impl Iterator<Item = DirEntry>> {
    Ok(WalkDir::new(path)
        .into_iter()
        .filter_entry(is_useful)
        .filter_map(|e| e.ok())
        .filter(is_python_file))
}

/// Test and dunder methods are allowed to be "unused."
fn should_consider_function(name: &String) -> bool {
    !name.contains("test_") && !name.contains("__")
}

/// Return all functions that are only mentioned once.
fn find_unused_functions(counts: HashMap<&Function, usize>) -> Vec<&Function> {
    let mut unused_functions = counts
        .into_iter()
        .filter(|(_, count)| *count < 2)
        .map(|k| k.0)
        .collect::<Vec<&Function>>();
    unused_functions.sort_by_key(|function| &function.location);

    unused_functions
}

/// Walk the given path, finding all declared functions.
/// Also populates the haystack file used later for counting references.
fn scan_path(path: &Path, haystack_file: &File) -> Result<HashSet<Function>> {
    let function_pattern = Regex::new(r"^[^#]*def (\S.*)\s*\(.*$")?;
    let mut functions = HashSet::new();

    let mut haystack_writer = BufWriter::new(haystack_file);

    let walker = make_walker(path)?;
    for entry in walker {
        for (lineno, line) in BufReader::new(File::open(entry.path())?)
            .lines()
            .enumerate()
        {
            let line = line?;
            if let Some(name) = function_pattern
                .captures(&line)
                .map(|c| c.get(1).unwrap().as_str().to_owned())
            {
                if should_consider_function(&name) {
                    let location = (entry.path().to_owned(), lineno + 1);
                    functions.insert(Function { name, location });
                }
            }
            haystack_writer.write_all(line.as_bytes())?;
        }
    }

    Ok(functions)
}

/// Scan the haystack file to find functions that are only mentioned once.
fn scan_for_unused_functions<'a>(
    haystack: &File,
    functions: &'a HashSet<Function>,
) -> Result<Vec<&'a Function>> {
    let mut counts = HashMap::new();

    for line in BufReader::new(haystack).lines() {
        let line = line?;
        for function in functions {
            *counts.entry(function).or_insert(0) += line.matches(&function.name).count();
        }
    }

    Ok(find_unused_functions(counts))
}

/// Find potentially unused functions in a python source tree.
fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.path.canonicalize()?;

    let mut haystack = tempfile()?;
    let functions = scan_path(&root, &haystack)?;

    haystack.seek(SeekFrom::Start(0))?;
    let unused_functions = scan_for_unused_functions(&haystack, &functions)?;

    let should_fail = unused_functions.len() > 0;

    for function in unused_functions {
        eprintln!(
            "{}:{} - function \"{}\" may be unused",
            args.path
                .join(function.location.0.strip_prefix(&root)?)
                .display(),
            function.location.1,
            function.name
        )
    }

    if should_fail {
        Err(anyhow!("possible unused functions were found"))
    } else {
        Ok(())
    }
}
