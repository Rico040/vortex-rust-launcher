# Windows smoke test log

This log records Windows validation performed against the Rust launcher. It is a partial smoke test, not a full replacement-release sign-off.

## 2026-07-08: Windows 10 partial smoke

Environment:

- OS: Microsoft Windows NT 10.0.19045.0
- PowerShell: 7.6.0
- Rust: `rustc 1.96.1`, `cargo 1.96.1`
- Java on `PATH`: Eclipse Temurin OpenJDK 21.0.8 LTS
- Installed Adoptium runtimes found: Java 8, 11, 17, 21, and 25

Validated:

- `cargo test` passed after downloader retry hardening: 36 tests passed, 0 failed.
- `cargo build --release` produced `target/release/vortex-rust-launcher.exe`.
- Release executable printed command help with `target/release/vortex-rust-launcher.exe help`.
- Release executable reached Mojang's version manifest with `target/release/vortex-rust-launcher.exe versions`.
- Native GUI startup smoke passed: the release executable stayed alive after startup, accepted a normal close request, and exited with code 0.
- Visual GUI smoke passed on Windows 10: the main launcher window, Settings window, and Client Downloader window opened at the same time.
- Settings save smoke passed visually: the main window reported `Saved settings to vortex_launcher.conf`.
- Empty installed-version state displayed correctly: the main window reported no installed versions and directed the user to open Downloader.
- Downloader version selector populated from the manifest and showed the latest available release.
- Settings window displayed downloader, Java, JVM parameter, save-launch-string, and keep-open controls.
- Full `1.12.2` download completed on Windows after retry hardening with exit code 0 and no failed-job lines.
- The completed `1.12.2` install includes `target/release/versions/1.12.2/1.12.2.jar`.
- No stale `.download` scratch files remained after the successful download.

Observed and fixed during smoke:

- A first GUI download attempt for `1.12.2` failed late in the asset phase with several `peer closed connection without sending TLS close_notify` network errors.
- The Rust downloader now retries retryable network, server, and integrity failures up to five times with backoff, and removes failed temporary files between attempts and after final failure.

Observed warnings:

- The Rust build still reports dead-code warnings for helper/test-support APIs in `platform` and `ui`.

Still pending before Windows replacement-release sign-off:

- Settings reload through the GUI on Windows.
- Java auto-selection launch validation across required Java versions, including Java 8 and Java 21+.
- Native extraction and actual Minecraft process launch on Windows.
- Offline relaunch after required files are downloaded.
- At least one inherited/modded profile validation, such as Forge or Fabric.
