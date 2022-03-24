use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};

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

/// Checks a file for captures with the given regex and returns them all.
fn get_captures<'a>(
    entry: &'a DirEntry,
    pattern: &'a Regex,
) -> Result<impl Iterator<Item = (usize, String)> + 'a> {
    Ok(BufReader::new(File::open(entry.path())?)
        .lines()
        .enumerate()
        .filter_map(|(lineno, r)| match r {
            Ok(r) => Some((lineno + 1, r)),
            _ => None,
        })
        .map(|(lineno, line)| {
            pattern
                .captures(&line)?
                .get(1)
                .map(|m| (lineno, m.as_str().to_owned()))
        })
        .flatten()
        .into_iter())
}

/// Returns an iterator over all python files in the given path.
fn make_walker(path: &Path) -> Result<impl Iterator<Item = DirEntry>> {
    Ok(WalkDir::new(path.canonicalize()?)
        .into_iter()
        .filter_entry(is_useful)
        .filter_map(|e| e.ok())
        .filter(is_python_file))
}

/// Test and dunder methods are allowed to be "unused."
fn should_consider_function(name: &String) -> bool {
    !name.contains("test_") && !name.contains("__")
}

fn counts_to_functions(counts: HashMap<&Function, usize>) -> Vec<&Function> {
    let mut unused_functions = counts
        .into_iter()
        .filter(|(_, count)| *count < 2)
        .map(|k| k.0)
        .collect::<Vec<&Function>>();
    unused_functions.sort_by_key(|function| &function.location);
    unused_functions
}

fn main() -> Result<()> {
    let args = Args::parse();

    let function_pattern = Regex::new(r"^[^#]*def (\S.*)\s*\(.*$")?;
    let mut functions = HashSet::new();

    let walker = make_walker(&args.path)?;
    for entry in walker {
        functions.extend(
            get_captures(&entry, &function_pattern)?
                .filter(|(_, name)| should_consider_function(name))
                .map(|(lineno, s)| Function {
                    name: s,
                    location: (entry.path().to_owned(), lineno),
                }),
        );
    }

    let mut counts = HashMap::new();

    let walker = make_walker(&args.path)?;
    for entry in walker {
        BufReader::new(File::open(entry.path())?)
            .lines()
            .filter_map(|r| r.ok())
            .for_each(|line| {
                functions
                    .iter()
                    .map(|function| (function, line.matches(&function.name).count()))
                    .for_each(|(function, count)| *counts.entry(function).or_insert(0) += count);
            })
    }

    let unused_functions = counts_to_functions(counts);
    let should_fail = unused_functions.len() > 0;

    let root = args.path.canonicalize()?;
    for function in unused_functions {
        eprintln!(
            "{}:{} - function \"{}\" may be unused",
            function
                .location
                .0
                .strip_prefix(&root)
                .map(|path| path.to_string_lossy())?,
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
