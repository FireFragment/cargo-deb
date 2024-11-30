use crate::debian_triple_from_rust_triple;
use crate::error::{CDResult, CargoDebError};
use std::path::Path;
use std::process::Command;

/// Resolves the dependencies based on the output of dpkg-shlibdeps on the binary.
pub(crate) fn resolve(path: &Path, rust_target_triple: Option<&str>) -> CDResult<Vec<String>> {
    let temp_folder = tempfile::tempdir()?;
    let debian_folder = temp_folder.path().join("debian");
    let control_file_path = debian_folder.join("control");
    std::fs::create_dir_all(&debian_folder)?;
    // dpkg-shlibdeps requires a (possibly empty) debian/control file to exist in its working
    // directory. The executable location doesn't matter.
    let _ = std::fs::File::create(control_file_path);

    // Print result to stdout instead of a file.
    let mut args = vec!["-O"];
    let libpath_arg;
    // determine library search path from target
    if let Some(rust_target_triple) = rust_target_triple {
        libpath_arg = format!("-l/usr/lib/{}", debian_triple_from_rust_triple(rust_target_triple));
        args.push(&libpath_arg);
    }
    const DPKG_SHLIBDEPS_COMMAND: &str = "dpkg-shlibdeps";
    let output = Command::new(DPKG_SHLIBDEPS_COMMAND)
        .args(args)
        .arg(path)
        .current_dir(temp_folder.path())
        .output()
        .map_err(|e| CargoDebError::CommandFailed(e, DPKG_SHLIBDEPS_COMMAND))?;
    if !output.status.success() {
        return Err(CargoDebError::CommandError(
            DPKG_SHLIBDEPS_COMMAND,
            path.display().to_string(),
            output.stderr,
        ));
    }

    log::debug!("dpkg-shlibdeps for {}: {}", path.display(), String::from_utf8_lossy(&output.stdout));

    let deps = output.stdout.as_slice().split(|&c| c == b'\n')
        .find_map(|line| line.strip_prefix(b"shlibs:Depends="))
        .ok_or(CargoDebError::Str("Failed to find dependency specification."))?
        .split(|&c| c == b',')
        .filter_map(|dep| std::str::from_utf8(dep).ok())
        .map(|dep| dep.trim_matches(|c: char| c.is_ascii_whitespace()))
        // libgcc guaranteed by LSB to always be present
        .filter(|dep| !dep.starts_with("libgcc-") && !dep.starts_with("libgcc1"))
        .map(|dep| dep.to_string())
        .collect();

    Ok(deps)
}

#[test]
#[cfg(target_os = "linux")]
fn resolve_test() {
    let exe = std::env::current_exe().unwrap();
    let deps = resolve(&exe, None).unwrap();
    assert!(deps.iter().any(|d| d.starts_with("libc")));
    assert!(!deps.iter().any(|d| d.starts_with("libgcc")), "{deps:?}");
}
