use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_the_two_mvp_commands() {
    Command::cargo_bin("cow")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("info"));
}
