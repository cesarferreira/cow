# CoW MVP Design

## Purpose

`cow` is a Rust library and command-line tool that creates independent copies of complete directory trees. It uses filesystem Copy-on-Write primitives when available and safely falls back to a regular recursive copy unless the caller requires CoW.

The MVP supports macOS and Linux. It treats Git repositories as ordinary directories, keeps no persistent state, and does not manage clone lifecycles.

## Package Structure

The MVP uses one Cargo package with both a library and binary:

```text
src/
├── lib.rs
├── error.rs
├── metadata.rs
├── platform/
│   ├── mod.rs
│   ├── macos.rs
│   └── linux.rs
└── main.rs
```

Platform modules remain private. The CLI depends only on the public library API. A multi-crate workspace can be introduced later only if independent publication or versioning becomes necessary.

## Public Library API

The public API provides:

```rust
pub fn clone_dir(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: CloneOptions,
) -> Result<CloneResult, CowError>;

pub fn inspect(path: impl AsRef<Path>) -> Result<FilesystemInfo, CowError>;
```

`CloneOptions` contains a `StrategyPreference` with `Auto`, `Cow`, and `Copy` variants. `CloneResult` reports canonical source and destination paths, the actual `CloneStrategy`, whether CoW was used, logical bytes, file count, and elapsed duration. `FilesystemInfo` reports the canonical path, platform, filesystem name when discoverable, whether CoW is supported, and the preferred strategy.

`CowError` has structured variants for invalid or missing sources, non-directory sources, existing destinations, conflicting options, unsupported CoW, permissions, cross-device operations, insufficient space where the operating system exposes it, and other I/O failures.

## CLI Contract

The command surface is:

```text
cow clone SOURCE DESTINATION [--strategy auto|cow|copy] [--require-cow] [--json] [-v]
cow info PATH [--json]
```

`--require-cow` is an alias for `--strategy cow`. Combining it with `--strategy copy` is an invalid argument. `doctor` is deferred because the PRD marks it lower priority.

Human success output is concise. Verbose diagnostic output and human errors use stderr where appropriate. JSON success output contains no decoration and is written to stdout. With `--json`, failures are serialized to stderr as a stable object containing an error code and message. Clap argument failures retain exit code 2; operational failures use exit code 1.

## Clone Flow

`clone_dir` performs these steps:

1. Resolve and validate the source without following a source symlink as a directory.
2. Reject a destination that already exists.
3. Validate that the source and destination cannot overlap recursively.
4. Create a uniquely named private sibling of the destination on the destination filesystem.
5. Execute the selected strategy into that private path.
6. Measure logical bytes and file count during traversal where traversal is required, or with a post-clone walk for a whole-tree native clone.
7. Rename the completed private path to the requested destination.
8. Return the actual strategy and measurements.

A cleanup guard removes only the private path created by the current operation when an error or interrupt unwinds the operation. It is disarmed only after the final rename. The source is opened read-only and is never modified.

## Strategy Selection

`Copy` always uses the portable recursive copier.

`Cow` attempts the platform CoW backend and returns `CowUnsupported` if any required operation cannot use CoW. It never silently mixes regular copies into a result reported as CoW.

`Auto` attempts the CoW backend first. If the backend reports a capability limitation such as unsupported ioctl, unsupported filesystem, or cross-device cloning, the incomplete private tree is removed and the entire operation restarts with the regular-copy backend. Permission, corruption, invalid-path, and resource-exhaustion errors do not trigger fallback because copying would either be unsafe or predictably fail as well.

## Platform Backends

On macOS, the backend calls the native `clonefile(2)` API. It first attempts a whole-tree clone. If the API or filesystem requires per-entry handling, the backend uses native file cloning during a controlled recursive traversal. Unsupported and cross-volume results are mapped distinctly from other failures.

On Linux, the backend recursively creates directories and symlinks, opens regular source files read-only, creates destination files exclusively, and invokes the `FICLONE` ioctl for each regular file. Capability is determined by actually attempting the operation rather than by a filesystem-name allowlist.

The portable backend recursively copies regular-file bytes and recreates symlinks without following them. All backends include hidden files and `.git`, preserve executable bits and permission modes, and restore directory and file modification times. Broken symlinks remain broken symlinks with the same target text. Sparse-file preservation is best effort for the MVP and is not reported as a guarantee.

Special files such as sockets, devices, and FIFOs are rejected with a structured unsupported-file-type error. This avoids blocking on FIFOs or reproducing privileged device nodes while keeping behavior explicit.

## Filesystem Inspection

`inspect` canonicalizes an existing path and reports the compile-time platform plus the mounted filesystem name using native metadata APIs. CoW capability is probed using temporary files created in the inspected directory when it is writable. Probe artifacts use exclusive creation and are removed before returning.

If the path is not writable, inspection reports capability from non-mutating platform metadata when reliable; otherwise it reports capability as unknown rather than incorrectly claiming support. The CLI renders unknown distinctly from unavailable. The public representation therefore uses a tri-state capability (`Supported`, `Unavailable`, `Unknown`).

## Metadata and Consistency

The clone is an ordinary independent directory after creation. CoW block sharing is an implementation property, not an ongoing dependency on `cow`.

The operation does not promise an atomic snapshot of a changing source. Atomicity applies only to destination visibility: where same-filesystem rename semantics permit, the final destination name appears only after cloning succeeds. Concurrent creation of the destination causes the final rename to fail without replacing existing data.

## Dependencies

The generated `anyhow` dependency is retained for the CLI boundary, while the library exposes a typed error using `thiserror`. Clap provides argument parsing. `serde` and `serde_json` provide stable machine-readable output. `libc` supplies macOS and Linux native calls. `walkdir`, `filetime`, and a temporary-name facility are used only where they materially simplify safe traversal, metadata restoration, and private destination creation.

Dependencies will be added only as implementation requires them; native cloning will not shell out to external programs.

## Testing

Tests follow red-green-refactor and are divided into:

- Unit tests for option validation, strategy and fallback decisions, error mapping, and JSON serialization.
- Portable integration tests for nested and hidden files, empty and large files, Unicode and spaced names, `.git`, symlinks and broken symlinks, executable modes, timestamps, destination rejection, source/destination overlap, cleanup after failure, and independence after modifications.
- Platform capability tests that assert the actual reported native strategy and clone independence when the current filesystem supports CoW. Unsupported environments report an explicit skip instead of a passing CoW result.
- CLI integration tests for human output, JSON-only stdout, structured JSON errors, and exit codes.

Verification runs formatting, Clippy with warnings denied, the full test suite, and a release build. The generated CI remains the base and is adjusted only where needed to exercise both macOS and Linux.

## Scope Boundaries

The MVP does not add force deletion, exclusions, progress bars, individual-file cloning, lifecycle tracking, Git behavior, remote cloning, Windows support, shell completions, benchmarking commands, or `doctor`. No state is written under the user's home directory or inside cloned trees.

## Acceptance Criteria

The MVP is accepted when the public Rust API and both CLI commands work on macOS and Linux; native CoW is selected and truthfully reported when supported; auto mode safely falls back; required-CoW mode fails rather than copying; complete supported directory trees remain equivalent and independent; existing destinations are never overwritten; failed operations do not expose partial destinations; and all declared verification commands pass.
