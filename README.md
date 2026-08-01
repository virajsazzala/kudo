# kudo

Stream a video from an Android phone to a Linux laptop over Bluetooth, with no
network, no cell service, and no internet. The phone sends, the laptop plays it
back as the bytes arrive.

kudo is a from-scratch systems project: its own wire protocol over Bluetooth
RFCOMM, a Rust receiver on Linux and a Kotlin sender on Android that implement
the same protocol independently, with credit-based flow control and streaming
integrity verification. The protocol and design are documented in [docs/](docs/).

## How it works

The phone picks a video and offers it. The laptop connects over Bluetooth,
accepts, and the phone streams the file in chunks while the laptop plays it
through `mpv` as it arrives (progressive playback, not download-then-watch) and
verifies a SHA-256 of the whole file at the end.

Bluetooth is the constraint that shapes everything: usable throughput is around
1.5 Mbps at close range, so kudo is built for low-bitrate video (480p and
below). Measured numbers are in [docs/benchmarks.md](docs/benchmarks.md).

## Requirements

Laptop (receiver):
- Linux with BlueZ and a working Bluetooth adapter
- [mpv](https://mpv.io/) for playback (optional if you only save)
- Rust toolchain (to build)

Phone (sender):
- Android 8.0 (API 26) or newer
- Bluetooth Classic (any normal Android phone; iOS is not supported, as it
  does not allow third-party Bluetooth Classic)

## Build and install

Receiver (laptop), from the repo root:

    cd linux
    cargo install --path .

This installs a `kudo` binary to `~/.cargo/bin/`.

Sender (phone): open the `android/` project in Android Studio and run it on your
phone, or from the command line with the phone connected over USB:

    cd android
    ./gradlew installDebug

## Usage

On the phone: open the kudo app, grant the Bluetooth permission, and tap
**Pick a video**. It then waits for the laptop to connect.

On the laptop:

    kudo --name Void                          # find the phone by Bluetooth name, play
    kudo --mac 3C:B0:ED:80:01:5B              # or by MAC address
    kudo --name Void --save clip.mp4          # play and keep a copy
    kudo --name Void --save clip.mp4 --no-play  # keep only, no playback
    kudo --help                               # all options

First run: the two devices need to pair. Keep the phone's Bluetooth settings
screen open while the laptop discovers it, and confirm the pairing prompt on
both if one appears.

For smooth progressive playback of MP4, the file should be "faststart" (its
index at the front). Phone recordings often are not; re-mux with:

    ffmpeg -i input.mp4 -c copy -movflags +faststart output.mp4

## Repository layout

    docs/       protocol spec, shared test vectors, benchmark numbers
    linux/      Rust receiver (BlueZ via bluer, mpv playback)
    android/    Kotlin sender (Android Bluetooth SDK)

The protocol lives in [docs/protocol.md](docs/protocol.md). Both sides implement
it independently and are kept in sync by shared byte-level test vectors
([docs/test-vectors.md](docs/test-vectors.md)); the codec test suites on each
side assert the same vectors.

## Current limitations

- One direction only: phone to laptop. Laptop to phone is planned.
- Low-bitrate video only, by nature of Bluetooth bandwidth.

## License

MIT. See [LICENSE](LICENSE).
