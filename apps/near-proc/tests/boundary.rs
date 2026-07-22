#[test]
fn process_application_has_no_file_or_backend_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    assert!(!manifest.contains("near-fm"));
    assert!(!manifest.contains("near-local-fs"));
    assert!(!manifest.contains("ratatui"));
    assert!(!manifest.contains("crossterm"));
    let source = include_str!("../src/main.rs");
    assert!(!source.contains("PathBuf"));
    assert!(!source.contains("file://"));
}
