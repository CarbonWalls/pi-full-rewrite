//! `pi --install-package <file>`: install a pi package (a tarball of
//! `.pi/pi.json`, `skills/`, `templates/`, …) into `<root>/.pi/`.
//! Zip-slip-safe: every entry path is validated before extraction.

use std::path::Path;

const ALLOWED_ENTRIES: [&str; 5] = [
    ".pi/pi.json",
    ".pi/skills/",
    ".pi/templates/",
    ".pi/themes/",
    ".pi/extensions/",
];

/// A path is safe when it has no absolute/`..` components and matches an
/// allowed top-level `.pi/` entry (exact for files, prefix for dirs).
fn entry_safe(entry: &str) -> bool {
    if entry.starts_with('/') || entry.split('/').any(|c| c == "..") {
        return false;
    }
    ALLOWED_ENTRIES.iter().any(|p| {
        if p.ends_with('/') {
            // the bare dir itself (`.pi/skills`) or anything under it
            entry == &p[..p.len() - 1] || entry.starts_with(p)
        } else {
            entry == *p
        }
    })
}

/// Validate + extract `pkg` (tar/tgz) into `root`.
pub fn install(pkg: &Path, root: &Path) -> Result<String, String> {
    if !pkg.is_file() {
        return Err(format!("package not found: {}", pkg.display()));
    }
    let spec = pkg.to_str().unwrap_or("");
    // -tvf: verbose listing so we can REJECT non regular-file/dir entries.
    // A symlink entry is a classic tar-slip vector (writing THROUGH a link
    // can escape <root> even when the name itself looks safe).
    let listing = std::process::Command::new("tar")
        .args(["-tvf", spec])
        .output()
        .map_err(|e| format!("tar list failed: {e}"))?;
    if !listing.status.success() {
        return Err(format!(
            "tar list failed: {}",
            String::from_utf8_lossy(&listing.stderr)
        ));
    }
    let entries = String::from_utf8_lossy(&listing.stdout);
    let mut count = 0usize;
    for line in entries.lines() {
        // verbose tar line: "<type>rwxr-xr-x owner/group size date path"
        let mut fields = line.split_whitespace();
        let Some(mode) = fields.next() else { continue };
        let tp = mode.chars().next().unwrap_or(' ');
        match tp {
            '-' | 'd' => {}                          // regular file / directory
            'l' => return Err("package contains a symlink (not supported)".into()),
            other => return Err(format!("package contains a special entry ({other})")),
        }
        let e = fields
            .by_ref()
            .last()
            .map(|p| p.trim_end_matches('/').to_string())
            .unwrap_or_default();
        if e.is_empty() {
            continue;
        }
        if !entry_safe(&e) {
            return Err(format!("package contains unsafe path: {e}"));
        }
        count += 1;
    }
    if count == 0 {
        return Err("package is empty".into());
    }

    let out = std::process::Command::new("tar")
        .args(["-xf", spec, "-C", root.to_str().unwrap_or("")])
        .output()
        .map_err(|e| format!("tar extract failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "tar extract failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(format!("installed {count} entries from {}", pkg.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_predicate() {
        assert!(entry_safe(".pi/pi.json"));
        assert!(entry_safe(".pi/skills/foo.md"));
        assert!(entry_safe(".pi/skills/"));
        assert!(!entry_safe("../evil"));
        assert!(!entry_safe("/etc/passwd"));
        assert!(!entry_safe(".pi/pi.json-evil"));
        assert!(!entry_safe(".pi/other/secret"));
        assert!(!entry_safe("pi.json"));
    }

    #[test]
    fn install_and_reject() {
        let dir = std::env::temp_dir().join(format!("pi-pkg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("root");
        std::fs::create_dir_all(root.join(".pi/skills")).unwrap();
        std::fs::write(root.join(".pi/pi.json"), "{}").unwrap();
        std::fs::write(root.join(".pi/skills/a.md"), "skill").unwrap();

        // build a package with system tar (like scripts/package.sh)
        let pkg = dir.join("pkg.tar.gz");
        let st = std::process::Command::new("tar")
            .args([
                "-czf",
                pkg.to_str().unwrap(),
                "-C",
                root.to_str().unwrap(),
                ".pi/pi.json",
                ".pi/skills",
            ])
            .status()
            .unwrap();
        assert!(st.success());

        // install into a fresh root
        let root2 = dir.join("root2");
        std::fs::create_dir_all(&root2).unwrap();
        let msg = install(&pkg, &root2).unwrap();
        assert!(msg.contains("installed"), "{msg}");
        assert_eq!(
            std::fs::read_to_string(root2.join(".pi/skills/a.md")).unwrap(),
            "skill"
        );
        assert_eq!(std::fs::read_to_string(root2.join(".pi/pi.json")).unwrap(), "{}");

        // a hostile archive is rejected before extraction
        let evil = dir.join("evil.tar");
        let st = std::process::Command::new("tar")
            .args(["-cf", evil.to_str().unwrap(), "-C", "/", "etc/hostname"])
            .status()
            .unwrap();
        assert!(st.success());
        let e = install(&evil, &root2).unwrap_err();
        assert!(e.contains("unsafe path"), "{e}");



        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn symlink_entries_are_rejected() {
        // classic tar-slip vector: a symlink named like an allowed path must
        // be refused even though its NAME is "safe" (writing through it
        // would escape <root>).
        let d = std::env::temp_dir().join(format!("pi-pkg-sym-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::create_dir_all(d.join(".pi/skills")).unwrap();
        std::fs::write(d.join(".pi/pi.json"), "{}").unwrap();
        let tar = d.join("pkg.tar");
        // create .pi/pi.json AND a symlink .pi/skills -> / (via tar --dereference
        // is NOT used; we craft a link entry with tar's -h off and --warning)
        std::fs::create_dir_all(d.join(".pi/skills")).unwrap();
        let st = std::process::Command::new("tar")
            .args(["-cf", tar.to_str().unwrap(), "-C", d.to_str().unwrap(), ".pi/pi.json", ".pi/skills"])
            .status().unwrap();
        assert!(st.success());
        // now replace skills dir with a symlink and re-add via a temp copy
        let d2 = std::env::temp_dir().join(format!("pi-pkg-sym2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d2);
        std::fs::create_dir_all(&d2).unwrap();
        std::fs::create_dir_all(d2.join(".pi")).unwrap();
        std::fs::write(d2.join(".pi/pi.json"), "{}").unwrap();
        std::os::unix::fs::symlink("/etc", d2.join(".pi/skills")).unwrap();
        let tar2 = d2.join("evil.tar");
        let st2 = std::process::Command::new("tar")
            .args(["-cf", tar2.to_str().unwrap(), "-C", d2.to_str().unwrap(), ".pi/pi.json", ".pi/skills"])
            .status().unwrap();
        assert!(st2.success());
        let out = install(&tar2, &d);
        assert!(out.is_err(), "symlink entry must be rejected: {out:?}");
        let e = out.unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&d2);
    }
}
