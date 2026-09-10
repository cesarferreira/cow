<div align="center">
  <h1>cow</h1>

  <p><strong>Create cheap independent directory clones</strong></p>

  <p>
    <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
    <img alt="Rust" src="https://img.shields.io/badge/rust-1.85%2B-orange">
    <img alt="Edition" src="https://img.shields.io/badge/edition-2024-blue">
    <a href="https://crates.io/crates/cow"><img alt="crates.io" src="https://img.shields.io/crates/v/cow.svg"></a>
  </p>

  <p>
    <a href="#install">Install</a>
    &nbsp;·&nbsp;
    <a href="#quickstart">Quickstart</a>
    &nbsp;·&nbsp;
    <a href="#performance">Performance</a>
  </p>
</div>

---

## Install

Requires [Rust](https://rustup.rs) **1.85+** and `~/.cargo/bin` on your `PATH`.

```bash
cargo install cow
```

Verify:

```bash
cow --help
```

<details>
<summary><strong>Build from source</strong> — for development or unreleased changes</summary>

```bash
git clone https://github.com/cesarferreira/cow.git
cd cow
cargo install --path . --locked
# or
make install-release
```

Debug install (faster compile, larger binary):

```bash
make install
```

Run without installing:

```bash
make build-release
./target/release/cow
```

</details>

<a id="quickstart"></a>
## Quickstart

```bash
cow clone ./project ./experiment
```

On APFS, Btrfs, XFS, and other reflink-capable filesystems, `cow` uses native
Copy-on-Write cloning. Elsewhere it performs a regular recursive copy:

```text
✓ Cloned /code/project → /code/experiment using APFS clone (60 ms)
```

The destination is an ordinary independent directory. It includes `.git`,
ignored dependencies and build outputs, untracked files, hidden files, and
symlinks. It needs no `cow` metadata and remains usable if `cow` is removed.

Require CoW instead of accepting fallback:

```bash
cow clone ./project ./experiment --require-cow
```

Select a strategy or request machine-readable output:

```bash
cow clone ./project ./experiment --strategy auto
cow clone ./project ./physical-copy --strategy copy
cow clone ./project ./experiment --json
cow info .
cow info . --json
```

### Rust library

```rust
use cow::{CloneOptions, clone_dir};

let result = clone_dir("./project", "./experiment", CloneOptions::default())?;
println!("{:?}", result.strategy);
# Ok::<(), cow::CowError>(())
```

### Filesystem behavior

| Environment | Automatic strategy |
|---|---|
| macOS on APFS | Native `clonefile(2)` CoW clone |
| Linux with reflink support | Per-file `FICLONE` reflinks |
| Unsupported or cross-filesystem | Metadata-preserving regular copy |

`--strategy cow` and `--require-cow` fail if every file cannot be cloned with
CoW. `--strategy copy` always creates physical copies. The destination must not
already exist; `cow` never provides a force/overwrite option.

<a id="performance"></a>
## Performance: CoW vs copy vs Git worktree

On the measured APFS fixture, `cow` reproduced the complete 205 MiB working
directory **about 34× faster than `cp -R`**, while initially allocating roughly
7 MiB of additional filesystem space.

| Method | Median creation time | Apparent output size | Approx. new APFS allocation | Current filesystem state | Git metadata |
|---|---:|---:|---:|---|---|
| `cow clone --require-cow` | **60 ms** | 205 MiB | **7.1 MiB** | Complete | Independent `.git/` |
| `cp -R` | 2.06 s | 205 MiB | 216 MiB | Complete | Independent `.git/` |
| `git worktree add --detach` | **50 ms** | 32 MiB | 35.8 MiB | Tracked checkout only | Linked to source repository |

On Linux, CoW is per-file (`FICLONE`) rather than a whole-tree clone. On the
same fixture on XFS it still avoided a full physical copy (**about 2.7 MiB**
new allocation versus **222 MiB** for `cp -a --reflink=never`), but it was not
faster than `cp`: thousands of small files make metadata syscalls dominate.
GitHub-hosted Ubuntu runners typically use ext4 without reflink, so `cow`
falls back to a regular copy there unless you pass `--require-cow`.

| Method | Median creation time | Apparent output size | Approx. new XFS allocation | Current filesystem state | Git metadata |
|---|---:|---:|---:|---|---|
| `cow clone --require-cow` | 520 ms | 200 MiB | **2.7 MiB** | Complete | Independent `.git/` |
| `cp -R` (GNU 9.4, also reflinked) | 429 ms | 200 MiB | **2.7 MiB** | Complete | Independent `.git/` |
| `cp -a --reflink=never` | 378 ms | 200 MiB | 222 MiB | Complete | Independent `.git/` |
| `git worktree add --detach` | **37 ms** | 32 MiB | 32.0 MiB | Tracked checkout only | Linked to source repository |

A Git worktree is quick, but it is not a directory copy: it omits untracked and
ignored files such as `node_modules/`, `target/`, `.gradle/`, build outputs, and
local configuration, and its Git metadata remains linked to the source
repository. `cow` instead starts from the current filesystem state, including
an independent Git repository. Sockets, devices, and FIFOs (including Git's
`fsmonitor` socket under `.git/`) currently abort the clone.

<details>
<summary><strong>Benchmark methodology</strong></summary>

APFS numbers were measured on macOS 26.6.2, Apple Silicon (`arm64`), APFS,
Git 2.55.0, and Rust 1.98.1. Linux numbers were measured on Ubuntu 24.04.4,
`x86_64`, Linux 6.17, XFS, Git 2.55.0, GNU coreutils 9.4, and Rust 1.98.1.
The release binary was built with `cargo build --release`.

The disposable source contained about 5,035 files with a ~200–205 MiB
apparent size:

- 32 MiB of tracked random data and a tracked Rust source file;
- 128 MiB of ignored build output;
- 8 MiB of untracked random data;
- 5,000 ignored dependency-like files; and
- a normal `.git/` directory.

Each method ran five times on the same volume with filesystem caches left
warm. The table reports medians. Commands:

```bash
target/release/cow clone SOURCE DESTINATION --require-cow
cp -R SOURCE DESTINATION
cp -a --reflink=never SOURCE DESTINATION   # Linux physical-copy comparison
git -C SOURCE worktree add --detach DESTINATION HEAD
```

Apparent size came from `du -skA` on macOS and `du --apparent-size -sk` on
Linux. Incremental allocation is the median change in filesystem-used blocks
from `df -kP` immediately before and after creation. That allocation figure is
approximate and can include unrelated filesystem activity; it is included to
show the initial order of magnitude, not as a universal storage guarantee. Times
and allocation will vary with hardware, filesystem, source shape, and cache
state. GNU `cp -R` may itself reflink on XFS and Btrfs; use
`cp --reflink=never` when you need a physical copy.

</details>

### What happens after cloning?

CoW clones initially share storage blocks. Changes to either directory allocate
new blocks only where they diverge; over time, a heavily modified clone can use
as much physical space as a regular copy. Deleting either directory with normal
filesystem tools does not affect the other.

## License

MIT
