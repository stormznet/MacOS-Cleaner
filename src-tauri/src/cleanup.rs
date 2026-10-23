use crate::{model::*, native};
use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

fn validate(item: &Item, home: &Path) -> Result<(), String> {
    if !item.selectable || !item.complete {
        return Err("This item is not eligible for cleanup".into());
    }
    if item.path == item.root || !item.path.starts_with(&item.root) {
        return Err("Item is outside its scan scope".into());
    }
    clean_ancestry(&item.path)?;
    if path_identity(&item.path)? != item.identity {
        return Err("Item changed since the scan; scan again".into());
    }
    if item.identity.dev != path_identity(home)?.dev {
        return Err("Items on other volumes are not supported yet".into());
    }
    if let Some(bundle) = &item.bundle_id {
        if native::running(bundle) {
            return Err("Quit the owning app before moving this item to Trash".into());
        }
    }
    Ok(())
}

pub fn quit_selected(state: &AppState, ids: &[String]) -> Result<u64, String> {
    if ids.is_empty() || ids.len() > 10_000 {
        return Err("Select between 1 and 10,000 items".into());
    }
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let mut bundles = HashSet::new();
    for id in ids {
        let item = store
            .items
            .get(id)
            .ok_or("A selected item is no longer in the scan; scan again")?;
        if !item.selectable || !item.complete {
            return Err("This item is not eligible for cleanup".into());
        }
        if let Some(bundle) = &item.bundle_id {
            bundles.insert(bundle);
        }
    }
    let mut requested = 0;
    for bundle in bundles {
        requested += native::quit(bundle)?;
    }
    Ok(requested)
}

pub fn trash_selected(state: &AppState, ids: &[String]) -> Result<Vec<Record>, String> {
    if ids.is_empty() || ids.len() > 10_000 {
        return Err("Select between 1 and 10,000 items".into());
    }
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let mut unique = HashSet::new();
    let mut items = vec![];
    for id in ids {
        if !unique.insert(id) {
            continue;
        }
        let item = store
            .items
            .get(id)
            .ok_or("A selected item is no longer in the scan; scan again")?;
        validate(item, &state.home)?;
        items.push(item.clone());
    }
    for (i, item) in items.iter().enumerate() {
        for other in items.iter().skip(i + 1) {
            if item.path.starts_with(&other.path) || other.path.starts_with(&item.path) {
                return Err(
                    "Selection contains overlapping items; select only the parent or its children"
                        .into(),
                );
            }
        }
        if let Some(parent) = &item.app_parent {
            if item.category == "apps" && !ids.contains(parent) {
                return Err("Select the app bundle together with its associated files".into());
            }
        }
    }
    items.sort_by_key(|item| item.app_parent.is_some());
    let operation = operation_id();
    let mut failed_apps = HashSet::new();
    for item in items {
        let mut record = Record {
            id: operation_id(),
            operation: operation.clone(),
            name: item.name.clone(),
            original: item.path.clone(),
            trash_path: None,
            identity: item.identity.clone(),
            bytes: item.bytes,
            timestamp: now(),
            status: "pending".into(),
            error: None,
            bundle_id: item.bundle_id.clone(),
        };
        store.saved.history.push(record.clone());
        // A durable intent lets startup locate a moved item by identity after an interrupted write.
        state.save(&store.saved)?;
        let outcome = if item
            .app_parent
            .as_ref()
            .is_some_and(|id| failed_apps.contains(id))
        {
            Err("App bundle could not be moved; associated data was kept".into())
        } else {
            validate(&item, &state.home).and_then(|_| native::trash(&item.path))
        };
        match outcome {
            Ok(path) => {
                record.trash_path = Some(path.clone());
                if path_identity(&path).is_ok_and(|i| i.same_file(&item.identity)) {
                    record.status = "trashed".into();
                    store.items.remove(&item.id);
                } else {
                    record.status = "uncertain".into();
                    record.error =
                        Some("Moved item needs inspection in Trash before restoration".into());
                }
            }
            Err(error) => {
                record.status =
                    if path_identity(&item.path).is_ok_and(|i| i.same_file(&item.identity)) {
                        "failed"
                    } else {
                        "uncertain"
                    }
                    .into();
                record.error = Some(error);
                failed_apps.insert(item.id.clone());
            }
        }
        if record.status != "trashed" {
            failed_apps.insert(item.id.clone());
        }
        *store.saved.history.last_mut().unwrap() = record;
        state.save(&store.saved).map_err(|e| {
            format!(
                "Stopped after a history write failed. Check History or Trash before retrying: {e}"
            )
        })?;
    }
    Ok(store.saved.history.clone())
}

pub fn reconcile(history: &mut [Record], home: &Path) {
    let trash = home.join(".Trash");
    for record in history {
        if record.status == "restored" || record.status == "failed" {
            continue;
        }
        let original_matches =
            path_identity(&record.original).is_ok_and(|i| i.same_file(&record.identity));
        if record.status == "restoring" && original_matches {
            record.status = "restored".into();
            record.error = None;
            continue;
        }
        let existing = record
            .trash_path
            .as_ref()
            .filter(|p| p.starts_with(&trash))
            .filter(|p| path_identity(p).is_ok_and(|i| i.same_file(&record.identity)))
            .cloned();
        let found = existing.or_else(|| {
            fs::read_dir(&trash)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| path_identity(p).is_ok_and(|i| i.same_file(&record.identity)))
        });
        if let Some(path) = found {
            record.trash_path = Some(path);
            record.status = "trashed".into();
            record.error = None;
        } else if record.status == "pending" && original_matches {
            record.status = "failed".into();
            record.error = Some("Interrupted before the item was moved".into());
        } else {
            record.status = "unavailable".into();
            record.error = Some(
                "Item was moved, Trash was emptied, or Trash access is unavailable. Check Finder."
                    .into(),
            );
        }
    }
}

fn restore_file(record: &Record, home: &Path) -> Result<(), String> {
    let source = record
        .trash_path
        .as_ref()
        .ok_or("No Trash location is recorded")?;
    if !source.starts_with(home.join(".Trash")) || source == &home.join(".Trash") {
        return Err("Cannot validate this Trash location; restore it in Finder".into());
    }
    clean_ancestry(source)?;
    if !path_identity(source)?.same_file(&record.identity) {
        return Err("The item in Trash changed; inspect it in Finder".into());
    }
    let parent = record
        .original
        .parent()
        .ok_or("Original folder is unavailable")?;
    clean_ancestry(parent)?;
    if record.original.starts_with(home.join(".Trash")) {
        return Err("Invalid original destination".into());
    }
    if fs::symlink_metadata(&record.original).is_ok() {
        return Err(
            "An item already exists at the original path. It will not be overwritten.".into(),
        );
    }
    if let Some(bundle) = &record.bundle_id {
        if native::running(bundle) {
            return Err("Quit the owning app before restoring its files".into());
        }
    }
    let from = CString::new(source.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let to = CString::new(record.original.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // RENAME_EXCL makes destination collision checks atomic, including races with other apps.
    if unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) } != 0 {
        return Err(format!(
            "Could not restore: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub fn restore(state: &AppState, id: &str) -> Result<Vec<Record>, String> {
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    reconcile(&mut store.saved.history, &state.home);
    let index = store
        .saved
        .history
        .iter()
        .position(|r| r.id == id)
        .ok_or("History item not found")?;
    if store.saved.history[index].status != "trashed" {
        return Err("This item is not available in Trash".into());
    }
    store.saved.history[index].status = "restoring".into();
    state.save(&store.saved)?;
    let result = restore_file(&store.saved.history[index], &state.home);
    store.saved.history[index].status = if result.is_ok() {
        "restored"
    } else {
        "trashed"
    }
    .into();
    store.saved.history[index].error = result.err();
    state.save(&store.saved)?;
    Ok(store.saved.history.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::measure;
    use std::sync::atomic::AtomicBool;

    fn candidate(path: &Path, root: &Path) -> Item {
        Item {
            id: "selected".into(),
            name: "Fixture".into(),
            path: path.into(),
            root: root.into(),
            category: "node".into(),
            bytes: measure(path, &AtomicBool::new(false)).unwrap(),
            modified: 0,
            description: "test".into(),
            consequence: "test".into(),
            complete: true,
            selectable: true,
            issue: None,
            bundle_id: None,
            app_parent: None,
            version: None,
            identity: path_identity(path).unwrap(),
        }
    }

    #[test]
    fn quit_validates_the_entire_selection_and_preserves_scan_and_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let path = root.join("cache");
        fs::create_dir(&path).unwrap();
        let mut item = candidate(&path, &root);
        item.bundle_id = Some("invalid.maccleaner.fixture-not-installed".into());
        let state = AppState::new(root.clone(), root.join("state")).unwrap();
        state
            .store
            .lock()
            .unwrap()
            .items
            .insert(item.id.clone(), item.clone());
        assert!(quit_selected(&state, &[]).is_err());
        assert!(quit_selected(&state, &vec![item.id.clone(); 10_001]).is_err());
        assert_eq!(
            quit_selected(&state, &[item.id.clone(), item.id.clone()]).unwrap(),
            0
        );
        for (complete, selectable) in [(false, true), (true, false)] {
            item.complete = complete;
            item.selectable = selectable;
            state
                .store
                .lock()
                .unwrap()
                .items
                .insert(item.id.clone(), item.clone());
            assert!(quit_selected(&state, &[item.id.clone()])
                .unwrap_err()
                .contains("not eligible"));
        }
        item.complete = true;
        item.selectable = true;
        item.bundle_id = Some("com.apple.finder".into());
        state
            .store
            .lock()
            .unwrap()
            .items
            .insert(item.id.clone(), item.clone());
        // Invalid IDs must fail before any native quit request, even for earlier entries.
        assert!(quit_selected(&state, &[item.id.clone(), "missing".into()])
            .unwrap_err()
            .contains("no longer in the scan"));
        assert!(quit_selected(&state, &[item.id.clone()])
            .unwrap_err()
            .contains("cannot be quit"));
        let store = state.store.lock().unwrap();
        assert_eq!(store.items.len(), 1);
        assert!(store.saved.history.is_empty());
        assert_eq!(path_identity(&path).unwrap(), item.identity);
    }

    #[test]
    fn rejects_changed_items_symlinks_and_overlapping_selections() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir(root.join("dependencies")).unwrap();
        let item = candidate(&root.join("dependencies"), &root);
        fs::rename(&item.path, root.join("old")).unwrap();
        fs::create_dir(&item.path).unwrap();
        assert!(validate(&item, &root).is_err());
        fs::remove_dir(&item.path).unwrap();
        std::os::unix::fs::symlink(root.join("old"), &item.path).unwrap();
        assert!(validate(&item, &root).is_err());
        assert!(clean_ancestry(&item.path).is_err());
        let parent = candidate(&root.join("old"), &root);
        fs::write(root.join("old/file"), "keep").unwrap();
        let child = candidate(&root.join("old/file"), &root);
        let mut parent = parent;
        parent.id = "parent".into();
        parent.identity = path_identity(&parent.path).unwrap();
        let state = AppState::new(root.clone(), root.join("state")).unwrap();
        {
            let mut store = state.store.lock().unwrap();
            store.items.insert(parent.id.clone(), parent);
            store.items.insert(child.id.clone(), child);
        }
        assert!(
            trash_selected(&state, &["parent".into(), "selected".into()])
                .unwrap_err()
                .contains("overlapping")
        );
        assert!(root.join("old/file").exists());
    }

    #[test]
    fn restores_without_overwrite_and_reconciles_interrupted_moves() {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir(home.join(".Trash")).unwrap();
        let source = home.join("node_modules");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("dependency.txt"), "original").unwrap();
        let identity = path_identity(&source).unwrap();
        let destination = home.join(".Trash/node_modules 10-58-12-759");
        fs::rename(&source, &destination).unwrap();
        let mut record = Record {
            id: "r".into(),
            operation: "o".into(),
            name: "fixture".into(),
            original: source.clone(),
            trash_path: None,
            identity,
            bytes: 8,
            timestamp: now(),
            status: "pending".into(),
            error: None,
            bundle_id: None,
        };
        reconcile(std::slice::from_mut(&mut record), &home);
        assert_eq!(record.status, "trashed");
        assert_eq!(record.trash_path.as_ref(), Some(&destination));
        fs::create_dir(&source).unwrap();
        fs::write(source.join("dependency.txt"), "new data").unwrap();
        assert!(restore_file(&record, &home).is_err());
        assert_eq!(
            fs::read_to_string(source.join("dependency.txt")).unwrap(),
            "new data"
        );
        assert_eq!(
            fs::read_to_string(destination.join("dependency.txt")).unwrap(),
            "original"
        );
        fs::remove_dir_all(&source).unwrap();
        restore_file(&record, &home).unwrap();
        assert_eq!(
            fs::read_to_string(source.join("dependency.txt")).unwrap(),
            "original"
        );
        assert!(!destination.exists());
        assert!(!home.join("node_modules 10-58-12-759").exists());
        record.status = "restoring".into();
        reconcile(std::slice::from_mut(&mut record), &home);
        assert_eq!(record.status, "restored");
    }

    #[test]
    #[ignore = "Opt-in integration test moves only temporary app/data fixtures through macOS Trash"]
    fn native_uninstall_roundtrip_includes_data_and_keeps_cache_cleanup_independent() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let home = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let state = AppState::new(home, root.join("history")).unwrap();
        for (id, relative, category) in [
            ("app", "Applications/Fixture.app", "apps"),
            ("support", "Library/Application Support/Fixture", "apps"),
            ("cache", "Library/Caches/Fixture", "caches"),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("fixture.txt"), id).unwrap();
            let mut item = candidate(&path, path.parent().unwrap());
            item.id = id.into();
            item.category = category.into();
            item.bundle_id = Some("invalid.maccleaner.uninstall-fixture".into());
            if id != "app" {
                item.app_parent = Some("app".into());
            }
            state.store.lock().unwrap().items.insert(id.into(), item);
        }
        let keep = root.join("Library/Application Support/Shared Vendor");
        fs::create_dir_all(&keep).unwrap();
        fs::write(keep.join("keep.txt"), "unselected").unwrap();
        assert!(trash_selected(&state, &["support".into()])
            .unwrap_err()
            .contains("Select the app"));
        let history = trash_selected(&state, &["cache".into()]).unwrap();
        assert_eq!(history[0].status, "trashed", "{:?}", history[0].error);
        restore(&state, &history[0].id).unwrap();
        let path = root.join("Library/Caches/Fixture");
        let mut cache = candidate(&path, path.parent().unwrap());
        cache.id = "cache".into();
        cache.category = "caches".into();
        cache.app_parent = Some("app".into());
        state
            .store
            .lock()
            .unwrap()
            .items
            .insert("cache".into(), cache);
        let history =
            trash_selected(&state, &["support".into(), "cache".into(), "app".into()]).unwrap();
        let removed = &history[1..];
        assert_eq!(removed.len(), 3);
        assert!(removed[0].original.ends_with("Fixture.app"));
        for record in removed {
            assert_eq!(record.status, "trashed", "{:?}", record.error);
            assert!(!record.original.exists());
            restore(&state, &record.id).unwrap();
            assert!(record.original.join("fixture.txt").exists());
        }
        assert_eq!(
            fs::read_to_string(keep.join("keep.txt")).unwrap(),
            "unselected"
        );
    }

    #[test]
    #[ignore = "Opt-in integration test moves only its own temporary fixture through macOS Trash"]
    fn native_trash_roundtrip_keeps_unselected_files() {
        let home = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(
            root.join("node_modules/dependency.txt"),
            "fixture dependency",
        )
        .unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("package-lock.json"), "{}").unwrap();
        let renamed = root.join("node_modules 10-58-12-759");
        fs::create_dir(&renamed).unwrap();
        fs::write(renamed.join("dependency.txt"), "restored copy").unwrap();
        let found = crate::scan::discover_nodes(&root, &AtomicBool::new(false), &mut vec![]);
        assert!(found.contains(&renamed));
        let item = candidate(&renamed, &root);
        let state = AppState::new(home, root.join("history")).unwrap();
        state
            .store
            .lock()
            .unwrap()
            .items
            .insert(item.id.clone(), item);
        let history = trash_selected(&state, &["selected".into()]).unwrap();
        assert_eq!(history[0].status, "trashed", "{:?}", history[0].error);
        assert!(!renamed.exists());
        assert_eq!(
            fs::read_to_string(root.join("node_modules/dependency.txt")).unwrap(),
            "fixture dependency"
        );
        assert!(root.join("package.json").exists());
        assert!(root.join("package-lock.json").exists());
        let history = restore(&state, &history[0].id).unwrap();
        assert_eq!(history[0].status, "restored", "{:?}", history[0].error);
        assert_eq!(
            fs::read_to_string(renamed.join("dependency.txt")).unwrap(),
            "restored copy"
        );
    }
}
