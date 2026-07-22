#![cfg(unix)]

use std::{thread, time::Duration};

use near_core::CommandInvocation;
use near_pty::ShellProfile;
use near_ui::{FarWorkspace, Keymap, SemanticTheme};

const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
const THEME: &str = include_str!("../../../specs/theme.toml");

fn workspace(policy: &str, startup_command: Option<&str>) -> FarWorkspace {
    let startup = startup_command.map_or(String::new(), |command| {
        format!("startup_command = {command:?}\n")
    });
    let profile = ShellProfile::from_toml(&format!(
        "schema = 1\nprogram = '/bin/sh'\nmode = 'clean'\nclose_policy = '{policy}'\n{startup}"
    ))
    .unwrap();
    FarWorkspace::demo()
        .with_embedded_pty(true)
        .with_shell_profile(profile)
}

fn dispatch(workspace: &mut FarWorkspace, command: &str) {
    workspace.dispatch(&CommandInvocation::new(command));
}

fn frame(workspace: &FarWorkspace) -> String {
    workspace
        .snapshot(
            &SemanticTheme::from_toml(THEME).unwrap(),
            &Keymap::from_toml(KEYMAP).unwrap(),
            100,
            30,
        )
        .join("\n")
}

fn wait_for(workspace: &mut FarWorkspace, needle: &str) -> String {
    for _ in 0..200 {
        workspace.poll_background_tasks();
        let current = frame(workspace);
        if current.contains(needle) {
            return current;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {needle:?}\n{}", frame(workspace));
}

#[test]
fn shell_close_policies_warn_retain_and_close_without_abandonment() {
    let mut warn = workspace("warn", None);
    dispatch(&mut warn, "near.terminal.open");
    wait_for(&mut warn, "close=warn");
    dispatch(&mut warn, "near.terminal.close");
    let warning = wait_for(&mut warn, "Close Running Shell");
    assert!(warning.contains("close=warn"));
    dispatch(&mut warn, "near.terminal.close-confirmed");
    assert!(!wait_for(&mut warn, "Closed user screen").contains("close=warn"));

    let mut keep_open = workspace("keep-open", None);
    dispatch(&mut keep_open, "near.terminal.open");
    wait_for(&mut keep_open, "close=keep-open");
    dispatch(&mut keep_open, "near.terminal.close");
    assert!(!wait_for(&mut keep_open, "Shell kept running").contains("close=keep-open"));
    dispatch(&mut keep_open, "near.terminal.open");
    assert!(wait_for(&mut keep_open, "close=keep-open").contains("[running/"));

    let mut close = workspace("close", Some("printf NEAR_CLOSE_EXIT"));
    dispatch(&mut close, "near.terminal.open");
    let closed = wait_for(&mut close, "Shell exited and the user screen closed");
    assert!(!closed.contains("close=close"));

    let mut close_running = workspace("close", None);
    dispatch(&mut close_running, "near.terminal.open");
    wait_for(&mut close_running, "close=close");
    dispatch(&mut close_running, "near.terminal.close");
    assert!(!wait_for(&mut close_running, "Closed user screen").contains("close=close"));
}
