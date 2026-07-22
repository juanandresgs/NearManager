#![cfg(target_os = "macos")]

use std::{
    fmt::Write as _,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("near-fm-handoff-{}-{id}", std::process::id()));
        fs::create_dir_all(path.join("home")).unwrap();
        fs::create_dir_all(path.join("work")).unwrap();
        fs::write(path.join("work/file.txt"), b"round trip\n").unwrap();
        Self(path)
    }

    fn editor_wrapper(&self, editor: &Path, arguments: &[&str]) -> PathBuf {
        let wrapper = self.0.join("editor-wrapper");
        let arguments = arguments
            .iter()
            .fold(String::new(), |mut output, argument| {
                write!(output, " '{argument}'").unwrap();
                output
            });
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexec \"{}\"{arguments} -- \"$@\"\n",
                editor.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&wrapper, permissions).unwrap();
        wrapper
    }

    fn handler_document(&self, wrapper: &Path) -> PathBuf {
        let document = self.0.join("handlers.toml");
        let program = wrapper
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        fs::write(
            &document,
            format!(
                r#"schema = 1

[[handlers]]
id = "near.test.editor"
actions = ["edit"]

[handlers.predicate]
schema_version = 1
hidden = "include"
ignore = "none"

[handlers.invocation]
mode = "argv"
program = "{program}"
arguments = [{{ value = "native-path" }}]
current_directory = {{ value = "native-parent" }}
"#
            ),
        )
        .unwrap();
        document
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn find(program: &str) -> Option<PathBuf> {
    let output = Command::new("/usr/bin/which").arg(program).output().ok()?;
    output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}

fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|value| *value == needle)
        .count()
}

fn assert_editor_round_trip(editor: &Path, arguments: &[&str]) {
    let fixture = Fixture::new();
    let wrapper = fixture.editor_wrapper(editor, arguments);
    let handlers = fixture.handler_document(&wrapper);
    let mut child = Command::new("/usr/bin/script")
        .args(["-q", "/dev/null"])
        .arg(env!("CARGO_BIN_EXE_near-fm"))
        .current_dir(fixture.0.join("work"))
        .env("HOME", fixture.0.join("home"))
        .env("NEAR_HANDLERS", handlers)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\x1b[B\x1b[1;3S\x1b[21~")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "script failed: {output:?}");
    assert!(output.stderr.is_empty(), "script stderr: {output:?}");
    assert!(occurrences(&output.stdout, b"file.txt") >= 2);
    assert!(occurrences(&output.stdout, b"\x1b[?1049h") >= 2);
    assert!(occurrences(&output.stdout, b"\x1b[?1049l") >= 2);
    assert!(output.stdout.windows(8).any(|value| value == b"External"));
    assert!(output.stdout.windows(6).any(|value| value == b"status"));
}

#[test]
fn vim_round_trip_restores_the_workspace() {
    let vim = find("vim").expect("macOS must provide Vim for the M1 handoff test");
    assert_editor_round_trip(&vim, &["-Nu", "NONE", "-n", "-c", "qa!"]);
}

#[test]
fn neovim_round_trip_restores_the_workspace_when_installed() {
    let Some(neovim) = find("nvim") else {
        eprintln!("Neovim is not installed; skipping optional local compatibility evidence");
        return;
    };
    assert_editor_round_trip(&neovim, &["--clean", "-n", "-c", "qa!"]);
}
