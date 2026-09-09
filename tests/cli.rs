use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn clone_json_is_machine_readable() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data"), "hello").unwrap();

    let output = Command::cargo_bin("cow")
        .unwrap()
        .args(["clone", source.to_str().unwrap(), destination.to_str().unwrap(), "--strategy", "copy", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["strategy"], "copy");
    assert_eq!(value["cow"], false);
    assert_eq!(value["logical_bytes"], 5);
    assert!(value["duration_ms"].is_number());
}

#[test]
fn info_json_is_machine_readable() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("cow")
        .unwrap()
        .args(["info", root.path().to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["platform"].is_string());
    assert!(value["cow_supported"].is_string());
    assert!(value["preferred_strategy"].is_string());
}

#[test]
fn human_clone_output_names_the_strategy() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data"), "hello").unwrap();
    Command::cargo_bin("cow")
        .unwrap()
        .args(["clone", source.to_str().unwrap(), destination.to_str().unwrap(), "--strategy", "copy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"))
        .stdout(predicate::str::contains("regular copy"));
}

#[test]
fn json_errors_are_structured_on_stderr() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let output = Command::cargo_bin("cow")
        .unwrap()
        .args(["clone", source.to_str().unwrap(), destination.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["code"], "destination_exists");
}

#[test]
fn require_cow_conflicts_with_copy_strategy() {
    Command::cargo_bin("cow")
        .unwrap()
        .args(["clone", "source", "destination", "--require-cow", "--strategy", "copy"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used"));
}
