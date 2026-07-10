# wakeup

A tiny, auditable, cross-platform `caffeinate`-compatible keep-awake CLI.

`wakeup` talks to each OS's native power APIs directly instead of shelling out to a bundled helper: IOKit on macOS, `systemd-logind` on Linux, and the Power Request API on Windows.
That makes it useful as a small standalone binary and as a foundation for higher-level automation.

This repo is organized as two crates:

- `crates/wakeup-core` - the engine. Platform-neutral request/handle types, the per-OS backends, and the shared timer/PID-wait/command-mode logic.
- `crates/wakeup` - the thin CLI that parses flags and drives `wakeup-core`.

The optional Herdr integration now lives in the separate `herdr-wakeup` repo, which consumes the `wakeup` binary rather than duplicating any of this logic.

## Platform support

| Platform | Backend | Supported today | Not yet supported |
| --- | --- | --- | --- |
| macOS | IOKit `IOPMAssertionCreateWithName` | `-d`, `-i`, `-m`, `-s`, `-u` | - |
| Linux | `systemd-logind` inhibitor locks (via `systemd-inhibit`) | `-i`, `-s`, timeout, PID wait, command mode | `-d`, `-m`, `-u` (desktop-specific, no logind equivalent) |
| Windows | `PowerCreateRequest` / `PowerSetRequest` / `PowerClearRequest` | `-d`, `-i`, `-s` | `-m`, `-u` (no matching Power Request type) |

Requesting an unsupported flag on a given platform fails fast with a clear error naming the flag and the platform, instead of silently doing nothing.

## Build and install

```bash
make install                     # build + copy wakeup to ~/.local/bin
make install PREFIX=/opt/homebrew # or anywhere already on your PATH
```

Or just build and copy the binaries yourself:

```bash
cargo build --release
cp target/release/wakeup /somewhere/on/your/PATH
```

## Usage

Flags mirror the common `caffeinate` interface:

```
wakeup [-dimsu] [-t seconds] [-w pid] [command [args...]]

  -d            Prevent the display from sleeping        (caffeinate -d)
  -i            Prevent the system from idle sleeping     (caffeinate -i) [default]
  -m            Prevent the disk from idle sleeping       (caffeinate -m)
  -s            Prevent system sleep entirely (AC power only)
  -u            Declare the user active and wake the display (defaults to 5s without -t)
  -t <seconds>  Release after N seconds, then exit
  -w <pid>      Release when process <pid> exits
  -h, --help    Show help
  -V, --version Show version
```

With no flags, `-i` is assumed (same default as `caffeinate`).
If a command is given, `wakeup` holds the assertion while it runs and exits with the command's status.
Press Ctrl-C to release when running interactively - the kernel also releases the assertion automatically if the process dies.

### Examples

```bash
wakeup                  # keep the system awake until Ctrl-C
wakeup -d               # also keep the display awake  (the caffeinate -d alternative)
wakeup -m               # prevent disk idle sleep
wakeup -u               # wake the display and declare user activity for 5 seconds
wakeup -t 3600          # stay awake for one hour, then release
wakeup -w 12345         # stay awake until process 12345 exits
wakeup -di make build   # keep system + display awake while `make build` runs
```

## Verifying it works

**macOS** - ask the OS what is holding sleep open:

```bash
pmset -g assertions | grep wakeup
```

You should see a `PreventUserIdleSystemSleep` (and/or `PreventUserIdleDisplaySleep`) assertion named `wakeup: ...` while the tool is running.

**Linux** - list active `systemd-logind` inhibitors:

```bash
systemd-inhibit --list | grep wakeup
```

You should see an entry with `who` set to `wakeup` while the tool is running.

**Windows** - list active power requests:

```powershell
powercfg /requests
```

You should see an entry under `SYSTEM` and/or `DISPLAY` while the tool is running.

## Herdr integration

Herdr-specific keep-awake behavior is intentionally kept out of this repo.
Use the separate `herdr-wakeup` plugin repo for Herdr agent-aware automation.

## Notes and limitations

- `-i` prevents idle sleep only, exactly like `caffeinate -i`.
  On macOS, closing the lid still triggers clamshell sleep unless the machine is on AC power with an external display.
- `-u` follows `caffeinate` by using a 5 second default when no timeout, process, or command is supplied. Not supported on Linux or Windows yet (see the platform support table above).
- On Linux, `-i` and `-s` require `systemd-logind` (present on effectively every modern distribution using systemd). Desktop-specific display/idle handling for `-d` and `-m` is a tracked follow-up, not a fundamental limitation.
- On Windows, `-m` (disk idle) and `-u` (user active) have no direct Power Request equivalent and are reported as unsupported rather than silently ignored.
- Not every platform's assertion semantics are identical even where a flag is "supported" - see the per-OS notes above before relying on exact timing behavior.
