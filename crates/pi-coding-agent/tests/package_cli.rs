//! `pi --install-package` CLI contract: good archives extract and exit 0;
//! hostile/invalid archives must exit NON-zero (a CI-visible failure).

use std::process::{Command, Stdio};

#[test]
fn package_cli_exit_codes() {
    let dir = std::env::temp_dir().join(format!("pi-pkgcli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // good archive
    let src = dir.join("src");
    std::fs::create_dir_all(src.join(".pi/skills")).unwrap();
    std::fs::write(src.join(".pi/pi.json"), "{}").unwrap();
    std::fs::write(src.join(".pi/skills/a.md"), "x").unwrap();
    let good = dir.join("good.tar.gz");
    let st = Command::new("tar")
        .args(["-czf", good.to_str().unwrap(), "-C", src.to_str().unwrap(), ".pi/pi.json", ".pi/skills"])
        .status().unwrap();
    assert!(st.success());

    let b = dir.join("b");
    std::fs::create_dir_all(&b).unwrap();
    let ok = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--install-package", good.to_str().unwrap(), "--root", b.to_str().unwrap()])
        .stdin(Stdio::null())
        .output().unwrap();
    assert!(ok.status.success(), "good package exits 0: {ok:?}");
    assert!(b.join(".pi/skills/a.md").exists(), "extracted");

    // hostile archive (absolute-ish path via a tarball entry that escapes)
    let evil_src = dir.join("evil");
    std::fs::create_dir_all(&evil_src).unwrap();
    let evil = dir.join("evil.tar");
    // build an archive whose entry is rejected (non-.pi path)
    Command::new("tar")
        .args(["-cf", evil.to_str().unwrap(), "-C", "/", "etc/hostname"])
        .status().unwrap();
    let bad = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--install-package", evil.to_str().unwrap(), "--root", b.to_str().unwrap()])
        .stdin(Stdio::null())
        .output().unwrap();
    assert!(
        !bad.status.success(),
        "hostile package must exit non-zero, got {:?}",
        bad
    );

    // missing file -> non-zero too
    let missing = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["--install-package", dir.join("nope.tar.gz").to_str().unwrap(), "--root", b.to_str().unwrap()])
        .stdin(Stdio::null())
        .output().unwrap();
    assert!(!missing.status.success(), "missing package exits non-zero");

    let _ = std::fs::remove_dir_all(&dir);
}
