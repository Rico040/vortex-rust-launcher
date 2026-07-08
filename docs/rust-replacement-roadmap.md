# Rust-first replacement roadmap

This roadmap makes the Rust launcher the active implementation path for Vortex Minecraft Launcher. The PureBasic launchers remain in the repository as legacy reference material until the first validated Rust replacement release ships.

The current Rust implementation already covers the main launcher flow: `vortex_launcher.conf` compatibility, version discovery, full client downloads, Mojang metadata parsing, native extraction, Java selection, launch command generation, native-window GUI views, and command-line operations. The current automated baseline is `cargo test`, which should pass before each roadmap milestone advances.

## Product decisions

- Rust is the future primary launcher implementation.
- PureBasic is not removed immediately; it remains a reference until the Rust replacement release is validated.
- The first replacement release targets Windows and Linux. macOS remains best-effort until separately validated.
- Microsoft/Minecraft authentication is post-v1 optional and does not block replacing the PureBasic launcher.
- Java installation remains the user's responsibility for v1. Rust discovers compatible installed runtimes or uses a configured custom Java path.
- Offline-style launcher behavior remains supported after required version files, libraries, assets, native archives, and logging configs have already been downloaded.

## Milestone 1: Rust primary candidate

- Keep all existing Rust launcher capabilities passing under `cargo test`.
- Resolve dead-code warnings where the code is no longer needed, or document helper APIs that intentionally exist for test/support hooks.
- Keep `vortex_launcher.conf` read/write compatibility with legacy keys such as `Name`, `Ram`, `ChosenVer`, `JavaPath`, `CustomParams`, downloader flags, save-launch-string, and keep-open settings.
- Document manual smoke checks for GUI startup, settings save/load, version manifest loading, full version download, download progress, Java validation, native extraction, launch command generation, and actual process launch.
- Document known working vanilla coverage with at least one Java 8-era version and one modern Java 21+ version after successful launch validation.

## Milestone 2: Windows and Linux replacement release

- Produce documented Rust build artifacts for Windows and Linux.
- Validate fresh install, existing config migration, version download, offline relaunch after download, and game launch on both platforms.
- Validate Java auto-selection and custom Java override on Windows and Linux.
- Validate at least one inherited/modded profile, such as Forge or Fabric, before documenting it as known working.
- Update release documentation so Rust artifacts are the default recommended downloads once validation is complete.
- Treat macOS as best-effort until a separate smoke-tested release path exists.

Current validation notes:

- Windows has a partial smoke record in [`windows-smoke-test.md`](windows-smoke-test.md): unit tests, release build, CLI help, manifest access, and GUI startup/close pass on Windows 10.
- Windows `1.12.2` full download now succeeds after retry hardening for transient TLS/network failures.
- Windows full replacement sign-off still requires settings reload, Java-version launch validation, native-extraction, game-launch, offline-relaunch, and modded-profile validation.

## Milestone 3: PureBasic retirement

- Stop advertising PureBasic as the stable or default launcher once the Windows and Linux Rust replacement release is validated.
- Keep PureBasic source as historical reference for one release cycle after the Rust replacement release.
- After that release cycle, move PureBasic references out of the active user path. Use an archive directory, a dedicated legacy branch/tag, or release notes to preserve historical access.
- Replace legacy screenshots and user-facing examples with Rust screenshots, Rust commands, and Rust release artifacts.

## Minimum validation checklist

- `cargo test` passes.
- Fresh Rust GUI starts on the target platform.
- Settings save to and reload from `vortex_launcher.conf`.
- Downloader loads the Mojang version manifest.
- Downloader installs a selected Minecraft version, including client jar, libraries, native archives, asset index, asset objects, and logging config.
- Download progress and failed-job details are visible in the GUI.
- Downloaded versions refresh into the main version selector.
- Java auto-selection chooses a compatible installed runtime, and custom Java override still works.
- Launch path validates Java, extracts natives, generates the launch command, and starts Minecraft.
- Offline relaunch works after all required files are already downloaded.

## Known release blockers

- Packaged Rust artifacts for Windows and Linux are not yet documented as complete.
- End-to-end download and launch validation has not yet been documented across the replacement target platforms.
- Legacy GUI parity is not yet claimed for every dialog, workflow, and platform-specific polish detail.
- Authentication is not implemented, by decision, and remains a later optional feature.
