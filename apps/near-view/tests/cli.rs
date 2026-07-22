use std::{
    io::Write,
    process::{Command, Stdio},
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_near-view")
}

#[test]
fn file_path_and_file_uri_emit_exact_plain_bytes_when_piped() {
    let root = std::env::temp_dir().join(format!("near-view-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("sample.bin");
    let bytes = b"alpha\0beta\n\x1b[not-control-from-viewer";
    std::fs::write(&path, bytes).unwrap();

    let path_output = Command::new(binary()).arg(&path).output().unwrap();
    assert!(path_output.status.success());
    assert_eq!(path_output.stdout, bytes);

    let uri = near_local_fs::LocalFileProvider::location(&path);
    let uri_output = Command::new(binary()).arg(uri.as_str()).output().unwrap();
    assert!(uri_output.status.success());
    assert_eq!(uri_output.stdout, bytes);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stdin_emits_exact_plain_bytes_when_piped() {
    let bytes = b"stdin\0payload\n";
    let mut child = Command::new(binary())
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, bytes);
}

#[test]
fn non_filesystem_provider_uri_emits_plain_descriptor() {
    let output = Command::new(binary())
        .arg("plugin://catalog/near.archive")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("id: near.archive"));
    assert!(text.contains("name: Archive Provider"));
}

#[test]
fn manifest_has_no_file_manager_or_backend_dependency() {
    let manifest = include_str!("../Cargo.toml");
    assert!(!manifest.contains("near-fm"));
    assert!(!manifest.contains("ratatui"));
    assert!(!manifest.contains("crossterm"));
}
