//! `bbindex`: checks, builds and signs the BloogBase mod index.
//!
//! ```text
//! bbindex check [--download [--links]] [--cache DIR]
//!                                                 validate index/mods/*.yaml
//! bbindex build --out DIR [--key FILE|--unsigned] [--generated-at SECS]
//! bbindex keygen --out DIR                        new minisign key pair
//! ```
//!
//! `build` reads the secret key from `--key` or the `INDEX_MINISIGN_KEY`
//! environment variable (the key file's contents, as stored in CI).

mod archive;
mod entry;
mod publish;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use entry::Entry;

const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

struct Args(Vec<String>);

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        let found = self.0.iter().position(|a| a == name);
        found.map(|i| self.0.remove(i)).is_some()
    }

    fn value(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(i) = self.0.iter().position(|a| a == name) else {
            return Ok(None);
        };
        if i + 1 >= self.0.len() {
            return Err(format!("{name} needs a value"));
        }
        self.0.remove(i);
        Ok(Some(self.0.remove(i)))
    }

    fn path(&mut self, name: &str, default: &str) -> Result<PathBuf, String> {
        Ok(self
            .value(name)?
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(CRATE_DIR).join(default)))
    }

    fn done(self) -> Result<(), String> {
        match self.0.first() {
            Some(a) => Err(format!("unexpected argument {a}")),
            None => Ok(()),
        }
    }
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: bbindex check|build|keygen (see src/main.rs)");
        return ExitCode::FAILURE;
    }
    let command = args.remove(0);
    let args = Args(args);
    let result = match command.as_str() {
        "check" => check(args),
        "build" => build(args),
        "keygen" => keygen(args),
        other => Err(format!("unknown command {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Reads and checks all entries; fails with every problem found.
fn load(args: &mut Args) -> Result<Vec<Entry>, String> {
    let mods = args.path("--mods", "../mods")?;
    let catalog =
        entry::load_catalog(&args.path("--catalog", "../../src-tauri/catalog/games.json")?)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&mods)
        .map_err(|e| format!("{}: {e}", mods.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    files.sort();

    let mut entries = Vec::new();
    let mut problems = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let stem = name.trim_end_matches(".yaml");
        let text = std::fs::read_to_string(file).map_err(|e| format!("{name}: {e}"))?;
        match entry::parse(&text) {
            Ok(e) => {
                problems.extend(
                    entry::problems(&e, stem, &catalog)
                        .into_iter()
                        .map(|p| format!("{name}: {p}")),
                );
                entries.push(e);
            }
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    problems.extend(entry::extends_problems(&entries));
    problems.extend(entry::provides_problems(&entries));
    if problems.is_empty() {
        Ok(entries)
    } else {
        Err(format!(
            "{} problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        ))
    }
}

fn check(mut args: Args) -> Result<(), String> {
    let download = args.flag("--download");
    // Also ask the servers about files that are cached already.
    let links = args.flag("--links");
    let cache = args.path("--cache", "../../.local/mods/index-cache")?;
    let entries = load(&mut args)?;
    args.done()?;
    eprintln!("{} entries are valid", entries.len());
    if !download {
        return Ok(());
    }

    let mut problems = Vec::new();
    for e in &entries {
        eprintln!("{}", e.id);
        for d in &e.downloads {
            if links && let Err(err) = archive::reachable(d) {
                problems.push(format!("{}: {err}", e.id));
            }
            let found = archive::fetch(d, &cache).and_then(|path| {
                std::fs::read(&path).map_err(|err| format!("{}: {err}", path.display()))
            });
            // The recipe may run files of the mod it's played over.
            let inherited = match base_files(e, &entries, &cache) {
                Ok(names) => names,
                Err(err) => {
                    problems.push(format!("{}: {err}", e.id));
                    continue;
                }
            };
            match found {
                Ok(data) => problems.extend(
                    archive::problems(e, d, &data, &inherited)
                        .into_iter()
                        .map(|p| format!("{}: {p}", e.id)),
                ),
                Err(err) => problems.push(format!("{}: {err}", e.id)),
            }
        }
    }
    if problems.is_empty() {
        eprintln!("all downloads are as expected");
        Ok(())
    } else {
        Err(format!(
            "{} problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        ))
    }
}

/// The file names in the newest download of the entry `e` extends.
fn base_files(e: &Entry, entries: &[Entry], cache: &Path) -> Result<Vec<String>, String> {
    let Some(base) = e
        .extends
        .as_ref()
        .and_then(|id| entries.iter().find(|b| b.id == *id))
    else {
        return Ok(Vec::new());
    };
    let download = base
        .downloads
        .first()
        .ok_or("the mod it extends has no download")?;
    let path = archive::fetch(download, cache)?;
    let data = std::fs::read(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    archive::file_names(&data).map_err(|err| format!("{}: {err}", download.url))
}

fn build(mut args: Args) -> Result<(), String> {
    let out = PathBuf::from(args.value("--out")?.ok_or("--out is required")?);
    let unsigned = args.flag("--unsigned");
    let key = match args.value("--key")? {
        Some(file) => Some(std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?),
        None => std::env::var("INDEX_MINISIGN_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty()),
    };
    let generated_at = match args.value("--generated-at")? {
        Some(s) => s.parse().map_err(|_| "--generated-at needs Unix seconds")?,
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let entries = load(&mut args)?;
    args.done()?;

    let json = publish::index_json(&entries, generated_at);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let target = out.join("index.json");
    std::fs::write(&target, &json).map_err(|e| e.to_string())?;
    match (key, unsigned) {
        (Some(key), _) => {
            let sig = publish::sign(json.as_bytes(), &key, generated_at)?;
            std::fs::write(out.join("index.json.minisig"), sig).map_err(|e| e.to_string())?;
            eprintln!("wrote signed {} ({} mods)", target.display(), entries.len());
        }
        (None, true) => eprintln!(
            "wrote UNSIGNED {} ({} mods)",
            target.display(),
            entries.len()
        ),
        (None, false) => {
            return Err(
                "no signing key: pass --key, set INDEX_MINISIGN_KEY or use --unsigned".into(),
            );
        }
    }
    Ok(())
}

fn keygen(mut args: Args) -> Result<(), String> {
    let out = PathBuf::from(args.value("--out")?.ok_or("--out is required")?);
    args.done()?;
    let secret_file = out.join("minisign.key");
    let public_file = out.join("minisign.pub");
    if secret_file.exists() || public_file.exists() {
        return Err(format!(
            "{} already holds a key; not overwriting it",
            out.display()
        ));
    }
    let (secret, public, base64) = publish::keygen()?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(&secret_file, secret).map_err(|e| e.to_string())?;
    std::fs::write(&public_file, public).map_err(|e| e.to_string())?;
    eprintln!("secret key: {} (keep it private)", secret_file.display());
    eprintln!("public key: {}", public_file.display());
    println!("{base64}");
    Ok(())
}
