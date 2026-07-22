#![cfg(target_os = "macos")]

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[test]
fn shared_viewer_runtime_handles_help_and_clean_exit() {
    let root = std::env::temp_dir().join(format!("near-view-pty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("viewer.txt");
    fs::write(&path, b"shared-viewer-content\n").unwrap();
    let mut child = Command::new("/usr/bin/script")
        .args(["-q", "/dev/null"])
        .arg(env!("CARGO_BIN_EXE_near-view"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        input.write_all(b"\x1bOP").unwrap();
        thread::sleep(Duration::from_millis(100));
        input.write_all(b"\x1b").unwrap();
        thread::sleep(Duration::from_millis(100));
        input.write_all(b"\x1b").unwrap();
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(output.status.success(), "script failed: {output:?}");
    assert!(output.stderr.is_empty(), "script stderr: {output:?}");
    assert!(
        output
            .stdout
            .windows(b"Near View Help".len())
            .any(|window| window == b"Near View Help")
    );
    assert!(
        output
            .stdout
            .windows(8)
            .any(|window| window == b"\x1b[?1049h")
    );
    assert!(
        output
            .stdout
            .windows(8)
            .any(|window| window == b"\x1b[?1049l")
    );
    fs::remove_dir_all(root).unwrap();
}
