# wakeup

A tiny, auditable `caffeinate`-compatible keep-awake utility for macOS.

`wakeup` creates IOKit power assertions directly, which makes it useful as a small standalone binary and as a foundation for higher-level automation.
It is especially handy when you want a self-contained tool instead of shelling out to `/usr/bin/caffeinate`.

This repo contains the standalone `wakeup` binary.
The optional Herdr integration now lives in the separate `herdr-wakeup` repo.

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

Ask macOS what is holding sleep open:

```bash
pmset -g assertions | grep wakeup
```

You should see a `PreventUserIdleSystemSleep` (and/or `PreventUserIdleDisplaySleep`) assertion named `wakeup: ...` while the tool is running.

## Herdr integration

Herdr-specific keep-awake behavior is intentionally kept out of this repo.
Use the separate `herdr-wakeup` plugin repo for Herdr agent-aware automation.

## Notes and limitations

- The shipped power assertion backend is macOS-only today.
  Linux and Windows backends are good candidates for a future cross-platform release.
- `-i` prevents idle sleep only, exactly like `caffeinate -i`.
  Closing the lid still triggers clamshell sleep unless the machine is on AC power with an external display.
- `-u` follows `caffeinate` by using a 5 second default when no timeout, process, or command is supplied.
