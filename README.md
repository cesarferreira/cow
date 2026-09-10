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

Sockets and FIFOs cannot be reproduced meaningfully, so they are skipped and
reported in the `skipped` count rather than aborting the clone. This matters
for live Git repositories, which keep a `.git/fsmonitor--daemon.ipc` socket
when `core.fsmonitor` is enabled. Device nodes remain a hard error.

<a id="performance"></a>
## Performance: CoW vs copy vs Git worktree

**`cow` buys disk space, not wall-clock time.** On a monorepo-shaped fixture on
XFS — 252,206 files totalling 3.05 GiB, of which 100,001 are tracked — cloning
the complete working tree cost **141 MiB** of new allocation instead of the
**3.63 GiB** a physical copy needs. It took about as long as `cp`, because
Linux reflinks file by file and per-file syscalls dominate at this file count.

| Method | Median time | Apparent output | New XFS allocation | Filesystem state | Git metadata |
|---|---:|---:|---:|---|---|
| `cow clone --require-cow` | 23.1 s | 3.05 GiB | **141 MiB** | Complete | Independent `.git/` |
| `cp -R` (GNU 9.4, reflinks by default) | 21.9 s | 3.05 GiB | 151 MiB | Complete | Independent `.git/` |
| `cp -a --reflink=never` | 24.8 s | 3.05 GiB | 3.63 GiB | Complete | Independent `.git/` |
| `git worktree add --detach` | **7.0 s** | 1.00 GiB | 1.21 GiB | Tracked checkout only | Linked to source repository |

A Git worktree is the fastest option here, but it is not a directory copy and
it is not free. It writes every tracked file, so its cost scales with the
tracked file count — and with Git LFS smudging or `post-checkout` hooks it gets
substantially slower on real repositories. It also omits untracked and ignored
files such as `node_modules/`, `target/`, `.gradle/`, build outputs, and local
configuration: above, 1.00 GiB and 100,002 files versus the full 3.05 GiB and
252,206. Its Git metadata stays linked to the source repository.

`cow` instead reproduces the current filesystem state, including an independent
`.git/`. Choose it when you want the whole working directory — build caches and
all — without paying for it twice on disk. Choose a worktree when a clean
tracked checkout is all you need.

On a private ~160,000-file, 40 GiB monorepo working tree on the same hardware,
`cow clone --require-cow` finished in 17.2 s with 0.11 GiB of new allocation
and skipped one `fsmonitor` socket.

<details>
<summary><strong>Benchmark methodology</strong></summary>

Measured on Ubuntu 24.04.4, `x86_64`, Linux 6.17, XFS, Git 2.55.0, GNU
coreutils 9.4, and Rust 1.98.1. The release binary was built with
`cargo build --release`.

The disposable fixture was shaped to match a large Android/Bazel monorepo,
since file count rather than byte count dominates a per-file reflink clone:

- 99,000 small tracked source files spread across a module and package tree;
- 1,000 tracked binary assets of 200–600 KB;
- 50,000 ignored build artifacts under `build-out/`; and
- a `.git/` directory holding the resulting loose objects.

That totals 252,206 files and 3.05 GiB apparent, of which 100,001 files are
tracked. `core.fsmonitor` was disabled in the fixture so no daemon socket
appeared mid-run, and it uses neither Git LFS nor hooks — a real repository
with either will make the worktree column slower.

Each method ran five times on the same volume with caches left warm, deleting
the destination between runs. The table reports medians. Commands:

```bash
target/release/cow clone SOURCE DESTINATION --require-cow
cp -R SOURCE DESTINATION
cp -a --reflink=never SOURCE DESTINATION
git -C SOURCE worktree add --detach DESTINATION HEAD
```

Apparent size came from `du --apparent-size -sk`. Incremental allocation is the
median change in filesystem-used blocks from `df -kP` immediately before and
after creation. That figure is approximate and can include unrelated filesystem
activity; it shows the initial order of magnitude, not a universal storage
guarantee. `cp -a --reflink=never` was the noisiest method, ranging from 18.6 s
to 133.7 s across runs. Times and allocation vary with hardware, filesystem,
source shape, and cache state. Note that GNU `cp -R` reflinks by default on XFS
and Btrfs, which is why its allocation matches `cow`; use `cp --reflink=never`
when you want a genuine physical copy.

macOS was not re-measured at this scale. There `clonefile(2)` clones the whole
tree in a single call rather than per file, so its time should scale differently
— but that is untested for a monorepo-sized source, and no macOS timings are
claimed here.

</details>

### What happens after cloning?

CoW clones initially share storage blocks. Changes to either directory allocate
new blocks only where they diverge; over time, a heavily modified clone can use
as much physical space as a regular copy. Deleting either directory with normal
filesystem tools does not affect the other.

## License

MIT
