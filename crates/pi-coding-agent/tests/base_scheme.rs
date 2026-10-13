//! Cleartext base URLs to non-local hosts are refused (audit finding).

use std::process::{Command, Stdio};

#[test]
fn cleartext_base_urls_to_remote_hosts_are_refused() {
    let dir = std::env::temp_dir().join(format!("pi-scheme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--quiet",
            "--root",
            dir.to_str().unwrap(),
            "-m",
            "custom:http://example.com/v1:gpt-x",
            "hi",
        ])
        .stdin(Stdio::null())
        .output().expect("run");
    let t = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "must fail: {t}");
    assert!(t.contains("cleartext") || t.contains("refusing"), "clear message: {t}");
    let _ = std::fs::remove_dir_all(&dir);
}
