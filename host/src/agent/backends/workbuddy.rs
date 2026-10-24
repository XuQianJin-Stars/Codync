//! `WorkBuddy`'s ACP CLI lives inside the desktop app and is also named `codebuddy`.
//! Looking it up on `PATH` would take `CodeBuddy` Code's binary, so it is found only
//! under a `WorkBuddy` install.

use std::path::{Path, PathBuf};

const CLI_DIR_MAC: &str = "Contents/Resources/app.asar.unpacked/cli/bin";
const CLI_DIR_OTHER: &str = "resources/app.asar.unpacked/cli/bin";

/// The desktop app ships only `codebuddy-lite-wb.mjs`. `--acp` alone loads the
/// headless bundle, which is not in the app, so the process exits immediately.
pub(super) const PROCESS_ENV: &[(&str, &str)] = &[("CODEBUDDY_FORCE_LITE_WB_BUNDLE", "1")];

/// The bundled CLI, when `WorkBuddy` is installed.
pub(super) fn find() -> Option<PathBuf> {
    find_in(&install_roots())
}

/// `program` is that bundled CLI (a path), not the `codebuddy` on `PATH`.
pub(super) fn is_cli(program: &str) -> bool {
    let path = Path::new(program.trim().trim_matches('"'));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !matches!(name, "codebuddy" | "codebuddy.cmd" | "codebuddy.exe") {
        return false;
    }
    let text = path.to_string_lossy().replace('\\', "/");
    text.contains("/WorkBuddy.app/Contents/Resources/app.asar.unpacked/cli/bin/")
        || text.contains("/WorkBuddy/resources/app.asar.unpacked/cli/bin/")
}

fn cli_dir() -> &'static str {
    if cfg!(target_os = "macos") { CLI_DIR_MAC } else { CLI_DIR_OTHER }
}

fn find_in(roots: &[PathBuf]) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["codebuddy.cmd", "codebuddy.exe", "codebuddy"] } else { &["codebuddy"] };
    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(cli_dir()).join(name)))
        .find(|p| is_executable(p))
}

fn install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Applications/WorkBuddy.app"));
        roots.extend(dirs::home_dir().map(|home| home.join("Applications/WorkBuddy.app")));
    }
    if cfg!(windows) {
        // electron-builder's per-user install, then a machine-wide one.
        if let Some(local) = dirs::data_local_dir() {
            roots.push(local.join("Programs/WorkBuddy"));
            roots.push(local.join("WorkBuddy"));
        }
        roots.extend(std::env::var_os("ProgramFiles").map(|dir| PathBuf::from(dir).join("WorkBuddy")));
    }
    roots
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_bundle_counts_and_a_path_codebuddy_does_not() {
        let dir = std::env::temp_dir().join(format!("codync-workbuddy-{}", uuid::Uuid::new_v4()));
        let root = dir.join(if cfg!(target_os = "macos") { "WorkBuddy.app" } else { "WorkBuddy" });
        let bin = root.join(cli_dir()).join(if cfg!(windows) { "codebuddy.cmd" } else { "codebuddy" });
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(find_in(&[root]).as_deref(), Some(bin.as_path()));
        assert!(is_cli(&bin.to_string_lossy()));
        assert!(!is_cli("/opt/homebrew/bin/codebuddy"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
