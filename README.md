# kudo

Kudo lets you stream a video from an Android phone to a Linux-based laptop over Bluetooth, with no "internet". The phone sends, the laptop plays it.

This is a wire protocol over Bluetooth RFCOMM, a Rust receiver on Linux and a Kotlin sender on Android that implement said protocol, with integrity verification built in. The protocol and design are documented in [docs/](docs/).

> Note: This is an experimental system, it's not production grade. It's not security-tested.

## How it works

The phone picks a video and offers it up. The laptop connects over Bluetooth,
accepts, and the phone starts streaming the file in chunks. The laptop plays
it through `mpv` as the bytes arrive — no download-then-watch — and checks a
SHA-256 of the whole file once it's done.

Bluetooth is the constraint that shapes everything here. You get around 1.5
Mbps at close range, so kudo's built for low-bitrate video (480p and below).
Actual numbers are in [docs/benchmarks.md](docs/benchmarks.md).

## Requirements

Laptop (receiver):
- Linux with BlueZ and a working Bluetooth adapter
- [mpv](https://mpv.io/) for playback (optional if you only want to save)
- Rust toolchain (to build)

Phone (sender):
- Android 8.0 (API 26) or newer
- Bluetooth Classic (any normal Android phone works; iOS isn't supported — it
  doesn't allow third-party Bluetooth Classic)

## Build and install

Receiver (laptop), from the repo root:

    cd linux
    cargo install --path .

This installs a `kudo` binary to `~/.cargo/bin/`.

Sender (phone): open the `android/` project in Android Studio and run it on
your phone, or from the command line with the phone connected over USB:

    cd android
    ./gradlew installDebug

## Usage

On the phone: open the kudo app, grant it Bluetooth permission, and tap
**Pick a video**. It'll then wait for the laptop to connect.

On the laptop:

    kudo --name Void                          # find the phone by Bluetooth name, play
    kudo --mac 3C:B0:ED:80:01:5B              # or by MAC address
    kudo --name Void --save clip.mp4          # play and keep a copy
    kudo --name Void --save clip.mp4 --no-play  # keep only, no playback
    kudo --help                               # all options

First run, the two devices need to pair. Keep the phone's Bluetooth settings
screen open while the laptop finds it, and confirm the pairing prompt on both
sides if one pops up.

For smooth progressive playback, an MP4 needs to be "faststart" (index at the
front). Phone recordings usually aren't, so re-mux it with:

    ffmpeg -i input.mp4 -c copy -movflags +faststart output.mp4

## Repository layout

    docs/       protocol spec, shared test vectors, benchmark numbers
    linux/      Rust receiver (BlueZ via bluer, mpv playback)
    android/    Kotlin sender (Android Bluetooth SDK)

The protocol itself lives in [docs/protocol.md](docs/protocol.md). Both sides
implement it independently, kept in sync by shared byte-level test vectors
([docs/test-vectors.md](docs/test-vectors.md)) — the codec test suites on
each side assert against the same vectors.

## Current limitations

- One direction only, phone to laptop. Laptop to phone is planned.
- Low-bitrate video only, by nature of Bluetooth bandwidth.

## License

MIT. See [LICENSE](LICENSE).
