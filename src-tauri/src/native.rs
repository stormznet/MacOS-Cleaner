use serde_json::Value;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

extern "C" {
    fn mc_trash(path: *const libc::c_char) -> *mut libc::c_char;
    fn mc_pick_folder() -> *mut libc::c_char;
    fn mc_app_info(path: *const libc::c_char) -> *mut libc::c_char;
    fn mc_app_for_bundle(identifier: *const libc::c_char) -> *mut libc::c_char;
    fn mc_is_running(identifier: *const libc::c_char) -> libc::c_int;
    fn mc_reveal(path: *const libc::c_char) -> libc::c_int;
    fn mc_open_trash() -> libc::c_int;
    fn mc_open_update() -> libc::c_int;
    fn mc_open_privacy_settings(full_disk_access: libc::c_int) -> libc::c_int;
}

fn c_path(path: &Path) -> Result<CString, String> {
    CString::new(
        path.to_str()
            .ok_or("This path cannot be represented as UTF-8")?,
    )
    .map_err(|e| e.to_string())
}

unsafe fn response(ptr: *mut libc::c_char) -> Result<Value, String> {
    if ptr.is_null() {
        return Err("macOS returned no response".into());
    }
    let bytes = CStr::from_ptr(ptr).to_bytes().to_vec();
    libc::free(ptr.cast());
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    Ok(value)
}

pub fn trash(path: &Path) -> Result<PathBuf, String> {
    let path = c_path(path)?;
    let value = unsafe { response(mc_trash(path.as_ptr()))? };
    let destination = value["path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("macOS moved the item but returned no Trash path; check Trash")?;
    Ok(PathBuf::from(destination))
}

pub fn choose_folder() -> Result<Option<PathBuf>, String> {
    let value = unsafe { response(mc_pick_folder())? };
    Ok(value["path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from))
}

pub fn app_info(path: &Path) -> Result<Value, String> {
    let path = c_path(path)?;
    unsafe { response(mc_app_info(path.as_ptr())) }
}

pub fn app_name_for_bundle(identifier: &str) -> Option<String> {
    let bundle = CString::new(identifier).ok()?;
    let value = unsafe { response(mc_app_for_bundle(bundle.as_ptr())).ok()? };
    let path = Path::new(value["path"].as_str()?);
    if !path.is_absolute()
        || path.extension().is_none_or(|ext| ext != "app")
        || path
            .components()
            .any(|part| part.as_os_str() == ".Trash" || part.as_os_str() == ".Trashes")
    {
        return None;
    }
    let info = app_info(path).ok()?;
    if info["bundleId"].as_str() != Some(identifier) {
        return None;
    }
    info["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| path.file_stem()?.to_str())
        .map(str::to_owned)
}

pub fn running(identifier: &str) -> bool {
    CString::new(identifier).is_ok_and(|s| unsafe { mc_is_running(s.as_ptr()) != 0 })
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let path = c_path(path)?;
    unsafe {
        mc_reveal(path.as_ptr());
    }
    Ok(())
}

pub fn open_trash() -> Result<(), String> {
    if unsafe { mc_open_trash() } == 0 {
        Err("Could not open Trash in Finder".into())
    } else {
        Ok(())
    }
}

pub fn open_privacy_settings(full_disk_access: bool) -> Result<(), String> {
    if unsafe { mc_open_privacy_settings(i32::from(full_disk_access)) } == 0 {
        Err("Could not open System Settings. Open Privacy & Security from the Apple menu.".into())
    } else {
        Ok(())
    }
}

pub fn open_update() -> Result<(), String> {
    if unsafe { mc_open_update() } == 0 {
        Err("Could not open the update download in your browser. Try again.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_registered_apps_and_handles_missing_metadata() {
        assert!(app_name_for_bundle("com.apple.finder").is_some());
        assert!(app_name_for_bundle("invalid.maccleaner.fixture-not-installed").is_none());
        assert!(app_name_for_bundle("bad\0identifier").is_none());
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("Contents")).unwrap();
        std::fs::write(temp.path().join("Contents/Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>invalid.fixture</string><key>CFBundleDisplayName</key><string></string><key>CFBundleName</key><string>Fixture App</string></dict></plist>"#).unwrap();
        let info = app_info(temp.path()).unwrap();
        assert_eq!(info["name"], "Fixture App");
        assert_eq!(info["bundleName"], "Fixture App");
    }
}
