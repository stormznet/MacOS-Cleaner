use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessCheck {
    name: String,
    path: PathBuf,
    status: &'static str,
    message: String,
}

fn check(name: &str, path: PathBuf) -> AccessCheck {
    // Opening the directory lets macOS request consent; never infer access from metadata alone.
    let result = fs::read_dir(&path).and_then(|mut entries| entries.next().transpose());
    let (status, message) = match result {
        Ok(_) => ("success", "Folder can be listed.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            ("unavailable", "Folder does not exist on this Mac.".into())
        }
        Err(error) => ("failed", error.to_string()),
    };
    AccessCheck {
        name: name.into(),
        path,
        status,
        message,
    }
}

pub fn check_access(home: &Path, roots: &[PathBuf]) -> Vec<AccessCheck> {
    let mut checks: Vec<_> = [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("App caches", "Library/Caches"),
        ("Trash", ".Trash"),
    ]
    .into_iter()
    .map(|(name, relative)| check(name, home.join(relative)))
    .collect();
    checks.extend(
        roots
            .iter()
            .map(|path| check("Project folder", path.clone())),
    );
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_failed_access_and_succeeds_after_permissions_are_restored() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o000)).unwrap();
        let denied = check("Denied fixture", directory.path().into());
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(denied.status, "failed");
        assert!(!denied.message.is_empty());
        assert_eq!(
            check("Allowed fixture", directory.path().into()).status,
            "success"
        );
    }

    #[test]
    fn checks_real_directory_access_and_distinguishes_missing_and_invalid_paths() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join("Desktop")).unwrap();
        fs::write(home.path().join("Downloads"), "not a directory").unwrap();
        let projects = home.path().join("projects");
        fs::create_dir(&projects).unwrap();
        fs::write(projects.join("keep.txt"), "keep").unwrap();
        let checks = check_access(home.path(), std::slice::from_ref(&projects));
        assert_eq!(checks.len(), 6);
        assert_eq!(checks[0].status, "success");
        assert_eq!(checks[1].status, "unavailable");
        assert_eq!(checks[2].status, "failed");
        assert!(!checks[2].message.is_empty());
        assert_eq!(checks[5].path, projects);
        assert_eq!(checks[5].status, "success");
        assert_eq!(
            fs::read_to_string(projects.join("keep.txt")).unwrap(),
            "keep"
        );
    }
}
