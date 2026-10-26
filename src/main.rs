use std::{
    collections::HashMap,
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType},
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

type Index = HashMap<String, String>;

const WORKSPACE_MARKERS: &[&str] = &[
    ".git",
    ".gradle",
    ".maven",
    "Cargo.toml",
    "package.json",
    "CMakeLists.txt",
    "Makefile",
    "build.gradle.kts",
];

const PRUNED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".cargo",
    ".rustup",
    ".npm",
    ".gradle",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
];

fn index_path(home: &Path) -> PathBuf {
    home.join(".local").join("index.json")
}

fn is_excluded(path: &Path) -> bool {
    ["/proc", "/sys", "/dev", "/run", "/tmp", "/var"]
        .iter()
        .any(|excluded| path.starts_with(excluded))
}

fn is_workspace(path: &Path) -> bool {
    WORKSPACE_MARKERS
        .iter()
        .any(|marker| path.join(marker).exists())
}

fn is_hidden(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
}

fn should_prune(path: &Path) -> bool {
    if is_excluded(path) || is_hidden(path) {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| PRUNED_DIRECTORY_NAMES.contains(&name))
}

fn recurse_and_add_to_index(path: &Path, index: &mut Index) -> io::Result<()> {
    if should_prune(path) {
        return Ok(());
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        let child = entry.path();
        let file_type = entry.file_type()?;

        // Index directories only, and do not follow symlinks (which can create cycles).
        if file_type.is_dir() && !should_prune(&child) {
            let path_string = child.to_string_lossy().into_owned();
            // Keep walking ordinary directories so nested workspaces are found,
            // but only workspace roots become searchable entries.
            if is_workspace(&child) {
                // The full path is the key, so same-named workspaces do not overwrite.
                index.insert(path_string.clone(), path_string);
            }
            recurse_and_add_to_index(&child, index)?;
        }
    }
    Ok(())
}

fn scan_parallel(home: &Path) -> io::Result<Index> {
    let mut roots = Vec::new();
    let entries = match fs::read_dir(home) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(Index::new()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        let path = entry.path();
        if entry.file_type()?.is_dir() && !should_prune(&path) {
            roots.push(path);
        }
    }

    if roots.is_empty() {
        return Ok(Index::new());
    }

    let worker_count = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(roots.len());
    let chunk_size = roots.len().div_ceil(worker_count);

    thread::scope(|scope| {
        let handles: Vec<_> = roots
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    let mut local_index = Index::new();
                    for root in chunk {
                        recurse_and_add_to_index(root, &mut local_index)?;
                    }
                    Ok::<Index, io::Error>(local_index)
                })
            })
            .collect();

        let mut combined = Index::new();
        for handle in handles {
            let local_index = handle
                .join()
                .map_err(|_| io::Error::other("workspace scan worker panicked"))??;
            combined.extend(local_index);
        }
        Ok(combined)
    })
}

fn folder_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn resolve_exact_or_partial(query: &str, index: &Index) -> Option<String> {
    let folded_query = query.to_lowercase();
    let mut matches: Vec<&String> = index
        .values()
        // Count exact and partial case-insensitive matches together. An exact
        // name must not hide another workspace such as `star_core` for `Star`.
        .filter(|path| folder_name(path).to_lowercase().contains(&folded_query))
        .collect();
    matches.sort();
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }
    None
}

fn get_matching_close_folders(query: &str, index: &Index) -> Vec<String> {
    let matcher = SkimMatcherV2::default();
    let folded_query = query.to_lowercase();
    let mut scored: Vec<(i64, String)> = index
        .values()
        .filter_map(|path| {
            let name = folder_name(path).to_lowercase();
            let score = if name == folded_query {
                Some(i64::MAX)
            } else if name.contains(&folded_query) {
                Some(i64::MAX - 1)
            } else {
                matcher.fuzzy_match(folder_name(path), query)
            };
            score.map(|score| (score, path.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, path)| path).take(10).collect()
}

fn interactive_selector(matches: &[String]) -> io::Result<Option<String>> {
    if matches.is_empty() {
        return Ok(None);
    }

    // Keep the selector visible when the selected path is captured from stdout
    // by the shell helper.
    let mut stderr = io::stderr();
    terminal::enable_raw_mode()?;
    execute!(stderr, terminal::EnterAlternateScreen, cursor::Hide)?;

    let selection = (|| -> io::Result<Option<String>> {
        let mut input = String::new();
        loop {
            execute!(stderr, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
            execute!(
                stderr,
                Print("Choose a folder by number and press Enter (Esc cancels):\r\n")
            )?;
            for (number, path) in matches.iter().enumerate() {
                execute!(stderr, Print(format!("{number}: {path}\r\n")))?;
            }
            execute!(stderr, Print(format!("\r\nSelection: {input}")))?;
            stderr.flush()?;

            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char(c) if c.is_ascii_digit() => input.push(c),
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Enter => {
                            if let Ok(number) = input.parse::<usize>() {
                                if let Some(path) = matches.get(number) {
                                    return Ok(Some(path.clone()));
                                }
                            }
                            input.clear();
                        }
                        KeyCode::Esc => return Ok(None),
                        _ => {}
                    }
                }
            }
        }
    })();

    // Always restore the user's terminal, including when drawing or reading input fails.
    let restore = execute!(stderr, terminal::LeaveAlternateScreen, cursor::Show)
        .and_then(|_| terminal::disable_raw_mode());
    match (selection, restore) {
        (Ok(selected), Ok(())) => Ok(selected),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(error),
    }
}

fn load_or_build_index(home: &Path) -> io::Result<Index> {
    let path = index_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut index = match fs::read_to_string(&path) {
        Ok(contents) if !contents.trim().is_empty() => {
            serde_json::from_str::<Index>(&contents).unwrap_or_default()
        }
        Ok(_) => Index::new(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Index::new(),
        Err(error) => return Err(error),
    };

    // Older index files contain every directory. Filter them so the selector
    // only offers visible workspace roots, and discard stale paths.
    let cached_entry_count = index.len();
    index.retain(|_, folder| {
        let folder_path = Path::new(folder);
        folder_path.is_dir() && !is_hidden(folder_path) && is_workspace(folder_path)
    });
    if index.len() != cached_entry_count {
        let json = serde_json::to_vec_pretty(&index)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&path, json)?;
    }

    if index.is_empty() {
        if is_workspace(home) {
            let path_string = home.to_string_lossy().into_owned();
            index.insert(path_string.clone(), path_string);
        }
        index.extend(scan_parallel(home)?);
        let json = serde_json::to_vec_pretty(&index)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(path, json)?;
    }
    Ok(index)
}

fn rebuild_index(home: &Path) -> io::Result<Index> {
    let path = index_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut index = Index::new();
    if is_workspace(home) && !is_hidden(home) {
        let path_string = home.to_string_lossy().into_owned();
        index.insert(path_string.clone(), path_string);
    }
    index.extend(scan_parallel(home)?);

    let json = serde_json::to_vec_pretty(&index)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(path, json)?;
    Ok(index)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 1 && ["--help", "-h", ".help"].contains(&args[0].as_str()) {
        println!(
            "Usage:\n  open_d <folder_name>       Find and select a workspace\n  open_d --list               List workspace names\n  open_d --index              Rebuild the workspace index\n  open_d --help, -h, .help    Show this help\n\nShell helper:\n  source open_d.sh <folder>   Change the current shell directory\n  source open_d.sh -t <folder> Open a new terminal in the workspace\n  source open_d.sh --index    Rebuild the index"
        );
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--index" {
        let home = PathBuf::from(env::var_os("HOME").ok_or("HOME environment variable not set")?);
        let index = rebuild_index(&home)?;
        println!("Indexed {} workspaces.", index.len());
        return Ok(());
    }
    if args.len() != 1 {
        return Err("usage: open_d <folder_name> | open_d --list | open_d --index".into());
    }
    let query = &args[0];
    let list_only = query == "--list";
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME environment variable not set")?);
    let index = load_or_build_index(&home)?;

    if list_only {
        let mut paths: Vec<&String> = index.values().collect();
        paths.sort();
        for path in paths {
            println!("{}", folder_name(path));
        }
        return Ok(());
    }

    let selected = if let Some(path) = resolve_exact_or_partial(&query, &index) {
        path
    } else {
        let matches = get_matching_close_folders(&query, &index);
        if matches.is_empty() {
            return Err(format!("no folder matches '{query}'").into());
        }
        if !io::stdin().is_terminal() {
            return Err(format!(
                "multiple or fuzzy matches for '{query}'; run in a terminal to choose"
            )
            .into());
        }
        interactive_selector(&matches)?.ok_or("selection cancelled")?
    };

    // stdout is a machine-readable path for the shell wrapper.
    println!("{selected}");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("open_d: {error}");
        std::process::exit(1);
    }
}
