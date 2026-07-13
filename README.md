<div align="center">
  <img src="assets/banner.png" alt="wakeup" width="100%" />
  <h1>wakeup</h1>
  <p><strong>A tiny, auditable <code>caffeinate</code>-compatible keep-awake CLI.</strong><br/>
  Cross-platform. Zero dependencies. Talks directly to your OS&rsquo;s power APIs.</p>
  <p>
    <a href="https://github.com/usrivastava92/wakeup/actions/workflows/ci.yml"><img src="https://github.com/usrivastava92/wakeup/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
    <a href="https://github.com/usrivastava92/wakeup/actions/workflows/release.yml"><img src="https://github.com/usrivastava92/wakeup/actions/workflows/release.yml/badge.svg" alt="Release" /></a>
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-111827" alt="Platform" />
    <img src="https://img.shields.io/badge/dependencies-zero-brightgreen" alt="Dependencies" />
    <img src="https://img.shields.io/badge/license-MIT-blue" alt="License" />
  </p>
</div>

---

## Overview

`wakeup` keeps your computer awake.
It replaces platform-specific tools like `caffeinate` (macOS), `systemd-inhibit` (Linux), and custom power scripts (Windows) with a single cross-platform binary that works the same way everywhere.

Instead of shelling out to helpers or bundling runtimes, `wakeup` talks directly to each platform&rsquo;s native power APIs via FFI:
**IOKit** on macOS, **systemd-logind** on Linux, and the **Power Request API** on Windows.
No external crates.
No runtime dependencies.
Just a single static binary you can vendor, audit, and trust.

> **Current status:** `wakeup` is stable and production-ready on macOS (all assertion types), Linux (`-i` and `-s`), and Windows (`-d`, `-i`, `-s`).
> See the platform support table below for per-platform details.

---

## Why wakeup?

- **Zero dependencies** &mdash; Every OS interaction is raw FFI or a direct subprocess call. No `clap`, no `anyhow`, no `libc`, no `winapi`. The entire audit surface is a few hundred lines of Rust plus the platform headers your kernel already trusts.
- **Cross-platform** &mdash; One binary, same flags, same behavior. Write a script that calls `wakeup -t 3600` and it works on macOS, Linux, and Windows without conditional logic.
- **caffeinate-compatible** &mdash; If you know `caffeinate`, you know `wakeup`. Same flags (`-dimsu`), same defaults, same semantics.
- **RAII guarantees** &mdash; Assertions are tied to the process lifetime. Kill `wakeup`, the kernel releases the assertion automatically. No stale locks, no cleanup scripts.
- **Tiny and auditable** &mdash; The entire codebase is ~1,000 lines of Rust across two crates. Read it in an afternoon.

---

## Platform Support

| Platform | Backend | `-d` (Display) | `-i` (Idle) | `-m` (Disk) | `-s` (System) | `-u` (User) |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| **macOS** | IOKit | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Linux** | systemd-logind | &mdash; | ✅ | &mdash; | ✅ | &mdash; |
| **Windows** | Power Request API | ✅ | ✅ | &mdash; | ✅ | &mdash; |

Unsupported combinations fail fast with a clear error message naming the flag and platform &mdash; never silently ignored.

| Flag | macOS | Linux | Windows |
| :--- | :--- | :--- | :--- |
| `-d` | `kIOPMAssertPreventUserIdleDisplaySleep` | Desktop-specific (tracked) | `PowerRequestDisplayRequired` |
| `-i` | `kIOPMAssertPreventUserIdleSystemSleep` | `systemd-inhibit --what=idle` | `PowerRequestSystemRequired` |
| `-m` | `kIOPMAssertPreventDiskIdle` | Desktop-specific (tracked) | No equivalent |
| `-s` | `kIOPMAssertPreventSystemSleep` | `systemd-inhibit --what=sleep` | `PowerRequestSystemRequired` |
| `-u` | `kIOPMAssertUserIsActive` | Desktop-specific (tracked) | No equivalent |

---

## Installation

### Homebrew (macOS &amp; Linux)

```bash
brew install usrivastava92/tap/wakeup
```

Or tap once for future updates:

```bash
brew tap usrivastava92/tap
brew install wakeup
```

### From crates.io

```bash
cargo install wakeup
```

### Prebuilt binaries

Every [release](https://github.com/usrivastava92/wakeup/releases) includes prebuilt static binaries.
No Rust toolchain required.

| Asset | Platform |
| :--- | :--- |
| `wakeup-macos-arm64` | macOS Apple Silicon |
| `wakeup-macos-x86_64` | macOS Intel |
| `wakeup-linux-x86_64` | Linux x86_64 (static musl, runs anywhere) |
| `wakeup-linux-arm64` | Linux arm64 (static musl, runs anywhere) |
| `wakeup-windows-x86_64.exe` | Windows x86_64 |

Download, make executable, and place it on your `PATH`:

```bash
# macOS/Linux
chmod +x wakeup-macos-arm64
mv wakeup-macos-arm64 /usr/local/bin/wakeup
```

### Build from source

```bash
# Requires Rust (install via https://rustup.rs)
git clone https://github.com/usrivastava92/wakeup.git
cd wakeup

make install                     # builds + copies to ~/.local/bin
make install PREFIX=/usr/local   # or anywhere on your PATH
```

Or manually:

```bash
cargo build --release
cp target/release/wakeup /usr/local/bin/
```

### Windows

Prebuilt `.exe` binaries are available on the [releases page](https://github.com/usrivastava92/wakeup/releases).
Native package manager support (winget, Scoop, Chocolatey) is on the roadmap.

### Linux (without Homebrew)

Prebuilt static binaries are available on the [releases page](https://github.com/usrivastava92/wakeup/releases).
Native package manager support (apt, snap, Flatpak) is on the roadmap.

---

## Quick Start

```bash
wakeup                  # hold until Ctrl-C (default: prevent idle sleep)
wakeup -di              # keep display + system awake
wakeup -t 3600          # release after one hour
wakeup -w 12345         # hold until process 12345 exits
wakeup -di make build   # stay awake while `make build` runs
```

With no flags, `-i` is assumed &mdash; the same default as `caffeinate`.

---

## Usage

```
wakeup [-dimsu] [-t seconds] [-w pid] [command [args...]]

  -d            Prevent the display from sleeping
  -i            Prevent the system from idle sleeping        [default]
  -m            Prevent the disk from idle sleeping
  -s            Prevent system sleep entirely (AC power only)
  -u            Declare user active and wake the display
  -t <seconds>  Release after N seconds, then exit
  -w <pid>      Release when process <pid> exits
  -h, --help    Show help
  -V, --version Show version
```

All short flags can be bundled: `-di`, `-dimsu`, `-di -t 300`.

`-u` defaults to a 5-second hold when no timeout, PID, or command is supplied &mdash; matching `caffeinate -u` exactly.

---

## Examples

### Keep the system awake until interrupted

```bash
wakeup
# Press Ctrl-C to release
```

### Keep display and system awake while compiling

```bash
wakeup -di make build
# Assertion held until make exits; wakeup returns make's exit code
```

### Prevent sleep for one hour

```bash
wakeup -t 3600
# Automatically releases after 3600 seconds
```

### Stay awake until a specific process finishes

```bash
wakeup -w 12345
# Releases when PID 12345 exits (polled every second)
```

### Wake the display and simulate user activity

```bash
wakeup -u
# Holds "user active" assertion for 5 seconds (default), waking the display
wakeup -u -t 30
# Holds for 30 seconds instead
```

---

## Verifying It Works

**macOS** &mdash; ask the OS what is holding sleep open:

```bash
pmset -g assertions | grep wakeup
```

You should see a `PreventUserIdleSystemSleep` (and/or `PreventUserIdleDisplaySleep`) assertion named `wakeup: &hellip;` while the tool is running.

**Linux** &mdash; list active systemd-logind inhibitors:

```bash
systemd-inhibit --list | grep wakeup
```

You should see an entry with `who` set to `wakeup`.

**Windows** &mdash; list active power requests:

```powershell
powercfg /requests
```

You should see an entry under `SYSTEM` and/or `DISPLAY` while the tool is running.

---

## Architecture

The repo is organized as two crates:

```
wakeup/
├── crates/
│   ├── wakeup/          # Thin CLI binary (~300 lines)
│   │   └── src/main.rs  # Flag parsing, mode dispatch
│   └── wakeup-core/     # Platform engine (~600 lines)
│       └── src/
│           ├── lib.rs           # Session, Handle, AssertionKind
│           └── backend/
│               ├── macos.rs      # IOKit FFI (all 5 assertion types)
│               ├── linux.rs      # systemd-inhibit (idle + sleep)
│               ├── windows.rs    # kernel32 Power Request API
│               └── unsupported.rs # Clear error for unknown platforms
├── Cargo.toml           # Workspace root (zero external deps)
└── Cargo.lock
```

- **`wakeup-core`** is the engine &mdash; platform-neutral types (`AssertionKind`, `Session`, `Handle`), the per-OS backends, and the shared timer/PID-wait/command-mode logic. It can be consumed as a library without the CLI parser.
- **`wakeup`** is the thin CLI that parses flags and drives `wakeup-core`.

The optional Herdr integration lives in the separate [`herdr-wakeup`](https://github.com/usrivastava92/herdr-wakeup) repo, which vendors the `wakeup` binary rather than duplicating any logic.

---

## Design

### Zero dependencies

Every OS call is a direct FFI binding:

| Platform | What `wakeup` links |
| :--- | :--- |
| macOS | `IOKit.framework` + `CoreFoundation.framework` |
| Linux | `/usr/bin/systemd-inhibit` (subprocess) |
| Windows | `kernel32.dll` |

No `libc` crate, no `winapi`, no `nix`, no dependency tree to audit.
The release profile is tuned for size: `opt-level = "z"`, LTO, stripping, and `panic = "abort"`.
The result is a binary measured in kilobytes, not megabytes.

### RAII for power assertions

Every assertion is wrapped in a Rust `Handle` whose `Drop` implementation releases it.
If `wakeup` is killed (`SIGKILL`), the kernel also releases the assertion automatically &mdash; IOKit and the Power Request API guarantee this, and `systemd-inhibit` exits when its child process dies.
No stale locks, no cleanup scripts, no `atexit` handlers.

### Fail loudly, never silently

If you request an unsupported flag on your platform, `wakeup` prints a clear error and exits non-zero.
It will never silently ignore a flag you passed, leaving you to wonder why your display still went to sleep.

---

## Roadmap

- [ ] Linux: `-d`, `-m`, `-u` support via desktop-environment-specific APIs
- [ ] Windows: explore `-m` via `ExecutionState` API
- [ ] `--json` output mode for scripting and monitoring
- [ ] `wakeup status` to dump active assertions
- [ ] Package manager submissions (Homebrew, Winget, apt)

---

## Contributing

Bug reports, feature requests, and pull requests are welcome.
Please open an issue to discuss significant changes before submitting a PR.

---

## License

MIT &copy; 2026 wakeup contributors.
See [LICENSE](LICENSE) for the full text.
