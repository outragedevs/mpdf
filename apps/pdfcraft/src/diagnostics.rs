//! The diagnostics log: a file on the user's Desktop that says how a run went, for a
//! performance report from a machine we can't measure on (a terminal server without a
//! graphics processor). What goes into it and what never does: `logging`.
//!
//! This build keeps it by default on Windows, where those machines are;
//! `PDFCRAFT_DIAGNOSTICS=0` turns it off and `1` turns it on elsewhere. It needs no rights
//! beyond the user's own: the Desktop is theirs, and when it can't be written the file goes
//! beside the ordinary log.

use std::path::{Path, PathBuf};

/// The file's name on the Desktop.
pub const FILE: &str = "mpdf.log";
/// The file's name beside the ordinary log, which is `mpdf.log` there.
pub const FALLBACK_FILE: &str = "mpdf-diagnostics.log";

/// Environment variables that change how the app draws, reported at the start when set.
const SWITCHES: [&str; 11] = [
    "PDFCRAFT_RENDERER",
    "PDFCRAFT_REDUCED_EFFECTS",
    "PDFCRAFT_MAX_FPS",
    "PDFCRAFT_SCROLL_FPS",
    "PDFCRAFT_RENDER_THREADS",
    "PDFCRAFT_SYSTEM_FONTS",
    "WGPU_BACKEND",
    "WGPU_POWER_PREF",
    "WGPU_ADAPTER_NAME",
    "WGPU_DX12_PRESENTATION_SYSTEM",
    "RUST_LOG",
];

/// What the machine says about itself, reported at the start when set. None of it names the
/// user.
const MACHINE: [&str; 5] = ["NUMBER_OF_PROCESSORS", "PROCESSOR_IDENTIFIER", "PROCESSOR_ARCHITECTURE", "OS", "SESSIONNAME"];

/// `PDFCRAFT_DIAGNOSTICS`: `1` keeps the diagnostics log, `0` doesn't; anything else leaves
/// it to `default`.
pub fn enabled(value: Option<&str>, default: bool) -> bool {
    match value.map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

/// Whether this build keeps the diagnostics log unless told otherwise.
pub const fn by_default() -> bool {
    cfg!(target_os = "windows")
}

/// The user's Desktop folder, wherever the system keeps it (a redirected or roaming profile
/// moves it); `None` when there is none.
pub fn desktop() -> Option<PathBuf> {
    desktop_candidates().into_iter().find(|dir| dir.is_dir())
}

#[cfg(target_os = "windows")]
fn desktop_candidates() -> Vec<PathBuf> {
    let known = known_folders::get_known_folder_path(known_folders::KnownFolder::Desktop);
    let profile = std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Desktop"));
    known.into_iter().chain(profile).collect()
}

#[cfg(not(target_os = "windows"))]
fn desktop_candidates() -> Vec<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop")).into_iter().collect()
}

/// Where the diagnostics log may go, best first: the Desktop, then beside the ordinary log.
pub fn candidates(desktop: Option<&Path>, logs: Option<&Path>) -> Vec<PathBuf> {
    desktop.map(|dir| dir.join(FILE)).into_iter().chain(logs.map(|dir| dir.join(FALLBACK_FILE))).collect()
}

/// The lines that open a run in the diagnostics log: the build, the machine and the switches
/// in force. `var` reads an environment variable.
pub fn header(version: &str, processors: usize, var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |names: &[&str]| -> String {
        let pairs: Vec<String> = names.iter().filter_map(|name| var(name).map(|value| format!("{name}={}", value.trim()))).collect();
        if pairs.is_empty() { "none".to_owned() } else { pairs.join(", ") }
    };
    vec![
        format!("==== mPDF {version} started ({} {}) ====", std::env::consts::OS, std::env::consts::ARCH),
        format!("processors: {processors} usable; machine: {}", set(&MACHINE)),
        format!("switches: {}", set(&SWITCHES)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_turns_the_log_on_or_off_and_else_the_build_decides() {
        for default in [true, false] {
            assert!(enabled(Some("1"), default));
            assert!(enabled(Some(" 1 "), default));
            assert!(!enabled(Some("0"), default));
            for other in [None, Some(""), Some("yes"), Some("2")] {
                assert_eq!(enabled(other, default), default, "{other:?}");
            }
        }
        assert_eq!(by_default(), cfg!(target_os = "windows"));
    }

    #[test]
    fn the_log_goes_to_the_desktop_and_else_beside_the_ordinary_log() {
        let (desktop, logs) = (Path::new("/home/u/Desktop"), Path::new("/data/logs"));
        assert_eq!(candidates(Some(desktop), Some(logs)), [desktop.join("mpdf.log"), logs.join("mpdf-diagnostics.log")]);
        assert_eq!(candidates(None, Some(logs)), [logs.join("mpdf-diagnostics.log")]);
        assert!(candidates(None, None).is_empty());
    }

    #[test]
    fn the_header_names_the_build_the_machine_and_the_switches_in_force() {
        let var = |name: &str| match name {
            "NUMBER_OF_PROCESSORS" => Some("8".to_owned()),
            "SESSIONNAME" => Some("RDP-Tcp#3".to_owned()),
            "PDFCRAFT_MAX_FPS" => Some(" 20 ".to_owned()),
            // Never asked for: nothing in the header names the user.
            "USERNAME" | "USERPROFILE" | "HOME" => Some("someone".to_owned()),
            _ => None,
        };
        let lines = header("0.21.1 (build-1)", 8, var);
        assert!(lines[0].starts_with("==== mPDF 0.21.1 (build-1) started ("), "{lines:?}");
        assert_eq!(lines[1], "processors: 8 usable; machine: NUMBER_OF_PROCESSORS=8, SESSIONNAME=RDP-Tcp#3");
        assert_eq!(lines[2], "switches: PDFCRAFT_MAX_FPS=20");
        assert!(!lines.join("\n").contains("someone"));
        assert_eq!(header("1", 2, |_| None)[2], "switches: none");
    }
}
