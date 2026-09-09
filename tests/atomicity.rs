use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[test]
fn sigint_cleans_the_partial_destination() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    let large = fs::File::create(source.join("large.bin")).unwrap();
    large.set_len(2 * 1024 * 1024 * 1024).unwrap();

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("cow"))
        .args([
            "clone",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            "--strategy",
            "copy",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut saw_private = false;
    for _ in 0..500 {
        saw_private = fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".cow-tmp-")
        });
        if saw_private || child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        saw_private,
        "copy completed before its private destination could be observed"
    );
    // SAFETY: `child.id()` is the live process spawned directly above.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cancelled"));
    assert!(!destination.exists());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".cow-tmp-")
    }));
    assert!(source.exists());
}
