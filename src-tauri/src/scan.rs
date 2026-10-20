use crate::{model::*, native};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use std::time::Instant;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScanEvent {
    Item { item: Box<Item> },
    Phase { message: String },
    Warning { message: String },
    Progress { processed: usize, total: usize },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub elapsed_ms: u128,
    pub cancelled: bool,
    pub count: usize,
    pub warnings: Vec<String>,
}

fn placeholder(
    path: PathBuf,
    root: PathBuf,
    category: &str,
    description: &str,
    consequence: &str,
) -> Result<Item, String> {
    clean_ancestry(&path)?;
    let identity = path_identity(&path)?;
    Ok(Item {
        id: operation_id(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        path,
        root,
        category: category.into(),
        bytes: 0,
        modified: identity.modified.max(0) as u64,
        description: description.into(),
        consequence: consequence.into(),
        complete: false,
        selectable: true,
        issue: None,
        bundle_id: None,
        app_parent: None,
        version: None,
        identity,
    })
}

fn children(path: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|e| match e {
                Ok(e) => Some(e.path()),
                Err(e) => {
                    warnings.push(format!("{}: {e}", path.display()));
                    None
                }
            })
            .collect(),
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warnings.push(format!("{}: {e}", path.display()));
            }
            vec![]
        }
    }
}

fn hidden_or_bundle(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.starts_with('.')
        || name.ends_with(".app")
        || name.ends_with(".photoslibrary")
        || name.ends_with(".bundle")
}

fn valid_bundle_id(bundle: &str) -> bool {
    bundle.contains('.')
        && bundle.split('.').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

struct CacheApp {
    id: String,
    name: String,
    aliases: Vec<String>,
}

fn cache_owner(folder: &str, apps: &HashMap<String, CacheApp>) -> Option<(String, String)> {
    for bundle in [Some(folder), folder.strip_suffix(".ShipIt")]
        .into_iter()
        .flatten()
    {
        if let Some(app) = apps.get(bundle) {
            return Some((bundle.into(), app.name.clone()));
        }
    }
    let mut matches = apps.iter().filter(|(_, app)| {
        app.aliases
            .iter()
            .any(|alias| !alias.is_empty() && alias.eq_ignore_ascii_case(folder))
    });
    let (bundle, app) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some((bundle.clone(), app.name.clone()))
}

fn discover_app_data(
    home: &Path,
    system_library: &Path,
    apps: &HashMap<String, CacheApp>,
    warnings: &mut Vec<String>,
) -> Vec<Item> {
    let user_library = home.join("Library");
    let mut items = vec![];
    for library in [user_library.as_path(), system_library] {
        for relative in [
            "Application Support",
            "Caches",
            "Preferences",
            "Preferences/ByHost",
            "Saved Application State",
            "Logs",
            "Containers",
            "Application Scripts",
            "HTTPStorages",
            "WebKit",
            "LaunchAgents",
            "LaunchDaemons",
        ] {
            // User caches keep one candidate shared by the cache and uninstall screens.
            if relative == "Caches" && library == user_library {
                continue;
            }
            let root = library.join(relative);
            for path in children(&root, warnings) {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let owner = match relative {
                    "Application Support" | "Logs" | "Caches" => cache_owner(&name, apps),
                    _ => {
                        let key = match relative {
                            "Preferences" | "LaunchAgents" | "LaunchDaemons" => {
                                name.strip_suffix(".plist")
                            }
                            "Saved Application State" => name.strip_suffix(".savedState"),
                            "Preferences/ByHost" => name.strip_suffix(".plist").and_then(|stem| {
                                let (bundle, host) = stem.rsplit_once('.')?;
                                (host.len() == 36
                                    && host.bytes().enumerate().all(|(i, c)| {
                                        if [8, 13, 18, 23].contains(&i) {
                                            c == b'-'
                                        } else {
                                            c.is_ascii_hexdigit()
                                        }
                                    }))
                                .then_some(bundle)
                            }),
                            "HTTPStorages" => {
                                Some(name.strip_suffix(".binarycookies").unwrap_or(&name))
                            }
                            _ => Some(name.as_ref()),
                        };
                        key.and_then(|bundle| {
                            apps.get(bundle)
                                .map(|app| (bundle.into(), app.name.clone()))
                        })
                    }
                };
                let Some((bundle, name)) = owner else {
                    continue;
                };
                if bundle.starts_with("com.apple.") || !valid_bundle_id(&bundle) {
                    continue;
                }
                match placeholder(path, root.clone(), "apps",
                    &format!("{relative} for {name} ({bundle}), matched by bundle identifier or unique app name. May include local documents, settings, or sign-in data."),
                    "Selected for uninstall. Uncheck this item to keep its app data; otherwise it will move to Trash.") {
                    Ok(mut item) => {
                        item.app_parent = Some(apps[&bundle].id.clone());
                        item.bundle_id = Some(bundle);
                        items.push(item);
                    }
                    Err(error) => warnings.push(error),
                }
            }
        }
    }
    items
}

pub fn discover_nodes(
    root: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
    warnings: &mut Vec<String>,
) -> Vec<PathBuf> {
    let mut found = vec![];
    let mut dirs = vec![root.to_path_buf()];
    let device = match path_identity(root) {
        Ok(i) => i.dev,
        Err(e) => {
            warnings.push(e);
            return found;
        }
    };
    while let Some(dir) = dirs.pop() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        for path in children(&dir, warnings) {
            let m = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    warnings.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            if !m.is_dir()
                || m.file_type().is_symlink()
                || m.dev() != device
                || m.st_flags() & 0x40000000 != 0
            {
                continue;
            }
            if path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(is_node_modules_name)
            {
                found.push(path);
            } else if !hidden_or_bundle(&path) {
                dirs.push(path);
            }
        }
    }
    found
}

pub fn measure(path: &Path, cancel: &std::sync::atomic::AtomicBool) -> Result<u64, String> {
    let device = path_identity(path)?.dev;
    let mut stack = vec![path.to_path_buf()];
    let mut seen = HashSet::new();
    let mut bytes = 0u64;
    while let Some(p) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Scan cancelled; rescan to measure this item".into());
        }
        let m = fs::symlink_metadata(&p)
            .map_err(|e| format!("Could not measure {}: {e}", p.display()))?;
        if m.dev() != device {
            return Err("Contains a mounted volume; excluded from cleanup".into());
        }
        if m.st_flags() & 0x40000000 != 0 {
            return Err("Contains cloud placeholders; excluded from cleanup".into());
        }
        if seen.insert((m.dev(), m.ino())) {
            bytes = bytes.saturating_add(m.blocks().saturating_mul(512));
        }
        if m.is_dir() && !m.file_type().is_symlink() {
            for entry in
                fs::read_dir(&p).map_err(|e| format!("Access needed for {}: {e}", p.display()))?
            {
                stack.push(entry.map_err(|e| e.to_string())?.path());
            }
        }
    }
    Ok(bytes)
}

fn measure_items(
    state: &AppState,
    work: &[Item],
    emit: &(impl Fn(ScanEvent) + Sync),
) -> Result<Vec<String>, String> {
    emit(ScanEvent::Progress {
        processed: 0,
        total: work.len(),
    });
    let next = AtomicUsize::new(0);
    let processed = AtomicUsize::new(0);
    let errors = Mutex::new(vec![]);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let (next, processed, errors) = (&next, &processed, &errors);
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(candidate) = work.get(index) else {
                    break;
                };
                let mut item = candidate.clone();
                match measure(&item.path, &state.cancel) {
                    Ok(bytes) => {
                        item.bytes = bytes;
                        match path_identity(&item.path) {
                            Ok(identity) if identity == item.identity => {
                                item.complete = true;
                            }
                            _ => {
                                item.selectable = false;
                                item.issue = Some("Item changed while scanning; scan again".into());
                            }
                        }
                    }
                    Err(e) => {
                        item.selectable = false;
                        item.issue = Some(e.clone());
                        errors
                            .lock()
                            .unwrap()
                            .push(format!("{}: {e}", item.path.display()));
                    }
                }
                if let Ok(mut store) = state.store.lock() {
                    store.items.insert(item.id.clone(), item.clone());
                }
                emit(ScanEvent::Item {
                    item: Box::new(item),
                });
                if !state.cancel.load(Ordering::Relaxed) {
                    emit(ScanEvent::Progress {
                        processed: processed.fetch_add(1, Ordering::Relaxed) + 1,
                        total: work.len(),
                    });
                }
            });
        }
    });
    errors.into_inner().map_err(|e| e.to_string())
}

pub fn run(state: &AppState, emit: impl Fn(ScanEvent) + Sync) -> Result<Summary, String> {
    let started = Instant::now();
    let roots = {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        store.items.clear();
        store.saved.settings.roots.clone()
    };
    let mut warnings = vec![];
    if roots.is_empty() {
        warnings.push("node_modules was not scanned: add a project folder in Settings or node_modules, then scan again.".into());
    }
    let mut work = vec![];
    let mut paths = HashSet::new();
    let mut add = |item: Item| {
        if paths.insert(item.path.clone()) {
            emit(ScanEvent::Item {
                item: Box::new(item.clone()),
            });
            work.push(item);
        }
    };
    emit(ScanEvent::Phase {
        message: "Finding Xcode build data and package caches…".into(),
    });
    let derived = state.home.join("Library/Developer/Xcode/DerivedData");
    for path in children(&derived, &mut warnings) {
        if let Ok(mut item) = placeholder(
            path,
            derived.clone(),
            "xcode",
            "Xcode build products and project indexes. Your source code stays in place.",
            "Xcode will rebuild and reindex this project.",
        ) {
            item.bundle_id = Some("com.apple.dt.Xcode".into());
            add(item);
        }
    }
    for (rel, label) in [
        (".npm/_cacache", "npm download cache"),
        ("Library/Caches/Yarn", "Yarn download cache"),
        ("Library/Caches/pnpm", "pnpm download cache"),
        (".bun/install/cache", "Bun download cache"),
    ] {
        let path = state.home.join(rel);
        if path.exists() {
            match placeholder(path.clone(), path.parent().unwrap().into(), "packages", "Downloaded package archives. Project files and installed dependencies stay in place.", "Packages may need to be downloaded again. Stop package installations first.") {
                Ok(mut item) => { item.name = label.into(); add(item); }, Err(e) => warnings.push(e)
            }
        }
    }
    emit(ScanEvent::Phase {
        message: "Identifying installed applications…".into(),
    });
    let mut known_apps = HashMap::new();
    for app_root in [
        PathBuf::from("/Applications"),
        state.home.join("Applications"),
    ] {
        let mut dirs = vec![app_root.clone()];
        while let Some(dir) = dirs.pop() {
            if state.cancel.load(Ordering::Relaxed) {
                break;
            }
            for path in children(&dir, &mut warnings) {
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    continue;
                }
                if path.extension().is_some_and(|e| e == "app") {
                    let Ok(info) = native::app_info(&path) else {
                        continue;
                    };
                    let bundle = info["bundleId"].as_str().unwrap_or_default();
                    if bundle.is_empty() || bundle == "com.maccleaner.desktop" {
                        continue;
                    }
                    let Ok(mut item) = placeholder(path.clone(), app_root.clone(), "apps", "Installed application bundle. Selecting this app also selects its detected associated data. Review the files below it and uncheck anything to keep. Apps with helpers or extensions may require their own uninstaller.", "The application will no longer be available until restored or reinstalled.") else { continue; };
                    item.bundle_id = Some(bundle.into());
                    item.version = info["version"].as_str().map(str::to_owned);
                    if let Some(name) = info["name"].as_str().filter(|s| !s.is_empty()) {
                        item.name = name.into();
                    }
                    if bundle.starts_with("com.apple.") {
                        item.selectable = false;
                        item.issue =
                            Some("Apple applications are inspection-only in this version".into());
                    }
                    known_apps.insert(
                        bundle.to_string(),
                        CacheApp {
                            id: item.id.clone(),
                            name: item.name.clone(),
                            aliases: vec![
                                item.name.clone(),
                                info["bundleName"].as_str().unwrap_or_default().into(),
                                path.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into(),
                            ],
                        },
                    );
                    add(item);
                } else if !hidden_or_bundle(&path)
                    && path.components().count() < app_root.components().count() + 3
                {
                    dirs.push(path);
                }
            }
        }
    }
    for item in discover_app_data(
        &state.home,
        Path::new("/Library"),
        &known_apps,
        &mut warnings,
    ) {
        add(item);
    }
    emit(ScanEvent::Phase {
        message: "Checking application caches…".into(),
    });
    let cache_root = state.home.join("Library/Caches");
    for path in children(&cache_root, &mut warnings) {
        if state.cancel.load(Ordering::Relaxed) {
            break;
        }
        let folder_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let is_xcode = folder_name == "com.apple.dt.Xcode";
        let Ok(mut item) = placeholder(
            path,
            cache_root.clone(),
            if is_xcode { "xcode" } else { "caches" },
            "Cached application data. The owning app may rebuild or download this data again.",
            "Quit the app before cleanup. Its next launch may be slower.",
        ) else {
            continue;
        };
        let bundle = folder_name.strip_suffix(".ShipIt").unwrap_or(&folder_name);
        let owner = cache_owner(&folder_name, &known_apps).or_else(|| {
            if valid_bundle_id(bundle) && bundle != "com.maccleaner.desktop" {
                native::app_name_for_bundle(bundle).map(|name| (bundle.into(), name))
            } else {
                None
            }
        });
        if let Some((bundle, name)) = owner {
            item.name = format!("{name} cache");
            item.description = format!("Cached data for {name} ({bundle}). The app may rebuild or download this data again.");
            item.app_parent = known_apps
                .get(&bundle)
                .filter(|_| !bundle.starts_with("com.apple."))
                .map(|app| app.id.clone());
            item.bundle_id = Some(bundle);
            if folder_name.ends_with(".ShipIt") {
                item.name = format!("{name} updater cache");
                item.consequence = "Quit the app and wait for any update to finish before cleanup. Updates may need to be downloaded again.".into();
            }
        } else if is_xcode {
            item.bundle_id = Some("com.apple.dt.Xcode".into());
        } else if bundle == "com.maccleaner.desktop" {
            item.selectable = false;
            item.issue = Some("Mac Cleaner's own cache is excluded from cleanup".into());
        } else {
            item.selectable = false;
            item.issue = Some("No matching installed app; review in Finder".into());
            item.description = "This cache could belong to a removed app, a shared tool, or a macOS service. Its owner could not be confirmed, so cleanup is disabled.".into();
        }
        add(item);
    }
    for root in roots {
        if state.cancel.load(Ordering::Relaxed) {
            break;
        }
        match validate_root(&root, &state.home) {
            Ok(root) => {
                emit(ScanEvent::Phase {
                    message: format!("Finding dependencies in {}…", root.display()),
                });
                for path in discover_nodes(&root, &state.cancel, &mut warnings) {
                    if let Ok(mut item) = placeholder(path.clone(), root.clone(), "node", "Installed JavaScript dependencies. Source code, package.json, and lockfiles stay in place.", "Reinstall dependencies before using this project. Local changes inside node_modules will be removed. Stop builds and dev servers first.") {
                        item.name = path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().into();
                        let folder = path.file_name().unwrap_or_default().to_string_lossy();
                        if folder != "node_modules" {
                            item.name = format!("{} / {folder}", item.name);
                            item.description = "Renamed dependency folder, possibly returned from Trash or kept as a copy. Review its contents before removing it. Source files, package.json, and lockfiles outside this folder stay in place.".into();
                            item.consequence = "Only this renamed folder will be moved to Trash. Local changes inside it will be removed. Stop any process using it first.".into();
                        }
                        if !path.parent().is_some_and(|p| p.join("package.json").is_file()) {
                            item.description = "Dependency folder without a parent package.json, possibly left by a backup or removed project. Review its contents before removing it.".into();
                            item.consequence = "Only this folder will be moved to Trash, including any local changes inside it. Reinstalling may require recovering the missing package.json. Stop any process using it first.".into();
                        }
                        add(item);
                    }
                }
            }
            Err(e) => warnings.push(format!("{}: {e}", root.display())),
        }
    }
    emit(ScanEvent::Phase {
        message: "Measuring storage on disk…".into(),
    });
    warnings.extend(measure_items(state, &work, &emit)?);
    for message in &warnings {
        emit(ScanEvent::Warning {
            message: message.clone(),
        });
    }
    Ok(Summary {
        elapsed_ms: started.elapsed().as_millis(),
        cancelled: state.cancel.load(Ordering::Relaxed),
        count: work.len(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn finds_app_data_in_both_libraries_without_matching_shared_or_similar_names() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let home = root.join("home");
        let system_library = root.join("Library");
        let apps = HashMap::from([
            (
                "com.example.Editor".into(),
                CacheApp {
                    id: "editor".into(),
                    name: "Editor Pro".into(),
                    aliases: vec!["Editor Pro".into(), "Editor".into(), "Shared".into()],
                },
            ),
            (
                "com.example.Other".into(),
                CacheApp {
                    id: "other".into(),
                    name: "Other".into(),
                    aliases: vec!["Other".into(), "Shared".into()],
                },
            ),
            (
                "com.apple.Fixture".into(),
                CacheApp {
                    id: "apple".into(),
                    name: "Apple".into(),
                    aliases: vec![],
                },
            ),
        ]);
        let expected = [
            "Application Support/Editor",
            "Application Support/com.example.Editor",
            "Preferences/com.example.Editor.plist",
            "Preferences/ByHost/com.example.Editor.01234567-89AB-CDEF-0123-456789ABCDEF.plist",
            "Saved Application State/com.example.Editor.savedState",
            "Logs/editor pro",
            "Containers/com.example.Editor",
            "Application Scripts/com.example.Editor",
            "HTTPStorages/com.example.Editor",
            "HTTPStorages/com.example.Editor.binarycookies",
            "WebKit/com.example.Editor",
            "LaunchAgents/com.example.Editor.plist",
            "LaunchDaemons/com.example.Editor.plist",
        ];
        let excluded = [
            "Application Support/Shared",
            "Application Support/com.example",
            "Application Support/Editor Helper",
            "Application Support/com.apple.Fixture",
            "Group Containers/com.example.Editor",
            "Containers/Editor",
            "Preferences/com.example.Editor.Helper.plist",
            "Preferences/ByHost/com.example.Editor.Helper.plist",
            "Application Support/Linked Editor",
        ];
        for library in [home.join("Library"), system_library.clone()] {
            for relative in expected.iter().chain(excluded[..excluded.len() - 1].iter()) {
                let path = library.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                if path
                    .extension()
                    .is_some_and(|ext| ext == "plist" || ext == "binarycookies")
                {
                    fs::write(path, "fixture").unwrap();
                } else {
                    fs::create_dir(path).unwrap();
                }
            }
            std::os::unix::fs::symlink(&root, library.join("Application Support/Linked Editor"))
                .unwrap();
        }
        // A matching name still cannot authorize traversing a symlink.
        let mut apps = apps;
        apps.get_mut("com.example.Editor")
            .unwrap()
            .aliases
            .push("Linked Editor".into());
        fs::create_dir_all(system_library.join("Caches/com.example.Editor.ShipIt")).unwrap();
        fs::create_dir_all(home.join("Library/Caches/com.example.Editor")).unwrap();
        let mut warnings = vec![];
        let items = discover_app_data(&home, &system_library, &apps, &mut warnings);
        let paths: HashSet<_> = items.iter().map(|item| item.path.clone()).collect();
        assert_eq!(paths.len(), expected.len() * 2 + 1);
        for library in [home.join("Library"), system_library.clone()] {
            for relative in expected {
                assert!(paths.contains(&library.join(relative)), "{relative}");
            }
            for relative in excluded {
                assert!(!paths.contains(&library.join(relative)), "{relative}");
            }
        }
        assert!(paths.contains(&system_library.join("Caches/com.example.Editor.ShipIt")));
        assert!(!paths.contains(&home.join("Library/Caches/com.example.Editor")));
        assert!(items
            .iter()
            .all(|item| item.app_parent.as_deref() == Some("editor")
                && item.bundle_id.as_deref() == Some("com.example.Editor")));
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn cache_owners_match_names_and_updaters_without_guessing_shared_owners() {
        let apps = HashMap::from([
            (
                "com.example.Editor".into(),
                CacheApp {
                    id: "editor".into(),
                    name: "Editor Pro".into(),
                    aliases: vec!["Editor Pro".into(), "Editor".into(), "Shared".into()],
                },
            ),
            (
                "com.example.Other".into(),
                CacheApp {
                    id: "other".into(),
                    name: "Other".into(),
                    aliases: vec!["Other".into(), "shared".into()],
                },
            ),
        ]);
        for folder in [
            "com.example.Editor",
            "com.example.Editor.ShipIt",
            "Editor Pro",
            "editor",
        ] {
            assert_eq!(
                cache_owner(folder, &apps),
                Some(("com.example.Editor".into(), "Editor Pro".into())),
                "{folder}"
            );
        }
        for folder in [
            "Shared",
            "SHARED",
            "com.example",
            "com.example.Editor.Helper",
            "com.example.Editor.ShipIt.ShipIt",
            "Editor.ShipIt",
            "Uninstalled",
            "",
            "Google",
        ] {
            assert_eq!(cache_owner(folder, &apps), None, "{folder}");
        }
    }

    #[test]
    fn progress_counts_failed_checks_and_stops_advancing_after_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let state = AppState::new(home.clone(), home.join("state")).unwrap();
        let mut work = vec![];
        for name in ["readable", "missing"] {
            let path = home.join(name);
            fs::write(&path, "fixture").unwrap();
            work.push(placeholder(path, home.clone(), "node", "fixture", "fixture").unwrap());
        }
        fs::remove_file(&work[1].path).unwrap();
        let progress = Mutex::new(vec![]);
        let warnings = measure_items(&state, &work, &|event| {
            if let ScanEvent::Progress { processed, total } = event {
                progress.lock().unwrap().push((processed, total));
            }
        })
        .unwrap();
        let mut counts = progress.into_inner().unwrap();
        counts.sort();
        assert_eq!(counts, vec![(0, 2), (1, 2), (2, 2)]);
        assert_eq!(warnings.len(), 1);
        {
            let store = state.store.lock().unwrap();
            assert!(store.items[&work[0].id].complete);
            assert!(!store.items[&work[1].id].complete);
            assert!(!store.items[&work[1].id].selectable);
        }
        state.cancel.store(true, Ordering::SeqCst);
        let events = Mutex::new(vec![]);
        measure_items(&state, &work[..1], &|event| {
            if let ScanEvent::Progress { .. } = event {
                events
                    .lock()
                    .unwrap()
                    .push(serde_json::to_value(event).unwrap());
            }
        })
        .unwrap();
        assert_eq!(
            events.into_inner().unwrap(),
            vec![serde_json::json!({ "kind": "progress", "processed": 0, "total": 1 })]
        );
    }

    #[test]
    fn discovers_dependencies_without_manifests_and_never_follows_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("project/node_modules/a/node_modules")).unwrap();
        fs::write(root.join("project/package.json"), "{}").unwrap();
        fs::write(root.join("project/node_modules/a/file"), "data").unwrap();
        fs::create_dir_all(root.join(".git/node_modules")).unwrap();
        fs::create_dir_all(root.join("orphan/node_modules")).unwrap();
        std::os::unix::fs::symlink(root.join("project"), root.join("linked")).unwrap();
        let mut warnings = vec![];
        let cancel = AtomicBool::new(false);
        let mut found = discover_nodes(root, &cancel, &mut warnings);
        found.sort();
        assert_eq!(
            found,
            vec![
                root.join("orphan/node_modules"),
                root.join("project/node_modules")
            ]
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(measure(&root.join("project/node_modules"), &cancel).unwrap() > 0);
        cancel.store(true, Ordering::Relaxed);
        assert!(measure(&root.join("project"), &cancel).is_err());
    }

    #[test]
    fn discovers_renamed_dependencies_without_nested_or_unrelated_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let root = home.join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("package-lock.json"), "keep").unwrap();
        let names = [
            "node_modules",
            "node_modules 2",
            "node_modules 10-58-12-759",
        ];
        for name in names {
            assert!(is_node_modules_name(name));
            let folder = root.join(name);
            fs::create_dir_all(folder.join("package/node_modules")).unwrap();
            fs::write(folder.join("package/package.json"), "{}").unwrap();
            assert!(validate_root(&folder, &home).is_err());
            assert!(validate_root(&folder.join("package"), &home).is_err());
        }
        for name in [
            "node_modules-old",
            "node_modules notes",
            "node_modules ",
            "node_modules 2 notes",
            "node_modules 10-58-12-759-extra",
            "node_modules 24-00-00-000",
            "node_modules 10-60-00-000",
            "node_modules 10-00-60-000",
            "node_modules 10-58-12",
            "node_modules 1-58-12-759",
            "node_modules 10-58-12-75",
            "node_modules ２",
        ] {
            assert!(!is_node_modules_name(name), "{name}");
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::create_dir_all(root.join("orphan/node_modules 10-58-12-759/package/node_modules"))
            .unwrap();
        fs::write(
            root.join("orphan/node_modules 10-58-12-759/package/package.json"),
            "{}",
        )
        .unwrap();
        fs::create_dir_all(root.join(".hidden/node_modules 3")).unwrap();
        std::os::unix::fs::symlink(root.join("node_modules"), root.join("node_modules 4")).unwrap();
        let mut warnings = vec![];
        let mut found = discover_nodes(&root, &AtomicBool::new(false), &mut warnings);
        found.sort();
        let mut expected: Vec<_> = names.map(|name| root.join(name)).into();
        expected.push(root.join("orphan/node_modules 10-58-12-759"));
        expected.sort();
        assert_eq!(found, expected);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(validate_root(&root, &home).is_ok());
        assert_eq!(
            fs::read_to_string(root.join("package-lock.json")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn rejects_bundle_identifiers_that_could_escape_related_data_paths() {
        for bundle in [
            "",
            ".",
            "..",
            "com..example",
            "com.example/../../Library",
            "/Applications",
            "com.example.",
        ] {
            assert!(!valid_bundle_id(bundle));
        }
        assert!(valid_bundle_id("com.example.My-App"));
        assert!(
            clean_ancestry(Path::new("/Users/example/Library/Application Support/..")).is_err()
        );
        assert!(clean_ancestry(Path::new("relative/path")).is_err());
    }

    #[test]
    #[ignore = "Read-only performance smoke test against the current Mac; prints aggregate results only"]
    fn local_read_only_scan() {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let data = fs::canonicalize(temp.path()).unwrap().join("scan-state");
        let state = AppState::new(home, data).unwrap();
        let started = Instant::now();
        let marks = Mutex::new(std::collections::BTreeMap::new());
        let summary = run(&state, |event| {
            if let ScanEvent::Item { item } = event {
                if item.complete {
                    let mut marks = marks.lock().unwrap();
                    let mark = marks.entry(item.category).or_insert((0usize, 0u128));
                    mark.0 += 1;
                    mark.1 = started.elapsed().as_millis();
                }
            }
        })
        .unwrap();
        println!(
            "Read-only scan: {} candidates, {} ms, {} notices",
            summary.count,
            summary.elapsed_ms,
            summary.warnings.len()
        );
        assert!(!summary.cancelled);
        println!(
            "Category completion (count, ms): {:?}",
            marks.into_inner().unwrap()
        );
        assert!(state.store.lock().unwrap().saved.history.is_empty());
        let store = state.store.lock().unwrap();
        let caches: Vec<_> = store
            .items
            .values()
            .filter(|item| item.category == "caches")
            .collect();
        println!(
            "App cache ownership: {} recognized, {} unresolved or excluded",
            caches
                .iter()
                .filter(|item| item.bundle_id.is_some())
                .count(),
            caches
                .iter()
                .filter(|item| item.bundle_id.is_none())
                .count()
        );
    }
}
