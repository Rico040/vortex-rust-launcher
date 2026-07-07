## Vortex Minecraft Launcher

Fast, lightweight and easy to use Minecraft launcher. Natively available for Windows and Linux, with Rust platform support for Windows, Linux, and macOS paths.

---

## Features

* Lightweight and fast
* Open-Source
* Cross-Platform (available for Windows and Linux)
* Supports all Minecraft versions
* Supports Forge and other APIs
* Downloads all Minecraft versions
* Downloads missing libraries
* Doesn't require Minecraft account
* Doesn't require Java to work
* Can work fully offline after the selected version and assets are installed

---

## Download

Check the [**Releases**](https://github.com/Kron4ek/minecraft-vortex-launcher/releases) page to download the latest launcher version.

---

## Screenshots

![settings](https://i.imgur.com/dkiweug.png)
![main window](https://i.imgur.com/pd2tnnK.png)
![client downloader](https://i.imgur.com/1QTjiDw.png)

---

## Rust launcher

This repository includes a Rust binary crate for the launcher at the repository root. It keeps the existing GPLv3 license in `LICENSE.txt` and separates launcher responsibilities into Rust modules under `src/`:

* `config` reads and writes `vortex_launcher.conf` settings compatible with the legacy launcher keys.
* `platform` handles Windows, Linux, and macOS Minecraft paths, UI dimensions, and Java discovery through `JAVA_HOME` and `PATH`.
* `minecraft` parses version metadata, inherited profiles, launch arguments, libraries, assets, and launch commands.
* `download` discovers Mojang versions and downloads manifests, client jars, libraries, native libraries, asset indexes, asset objects, and logging configs.
* `launch` validates Java and starts the Minecraft process.
* `ui` renders the launcher, downloader, and settings states while the command interface provides runnable interactions.

### Rust build prerequisites

Install the stable Rust toolchain with Cargo. The recommended installation method is [rustup](https://rustup.rs/).

Build from the repository root:

```sh
cargo build
```

Open the interactive GUI from the repository root:

```sh
cargo run
```

The GUI uses `egui`/`eframe` native windows. It provides editable launcher state, Play, Downloader, Settings, Save, and Download controls backed by the same config, launch, and download modules as the CLI.

### GUI smoke checks

Manual smoke check on supported desktop platforms (Windows, Linux, and macOS):

```sh
cargo run
```

Verify that the main launcher window opens, the Downloader and Settings buttons open child windows, Save and apply writes `vortex_launcher.conf`, Download starts the download command for the selected version, and Play attempts to validate Java and launch the configured Minecraft profile.

Automated non-window checks:

```sh
cargo test
```

### Command interface

The Rust launcher supports real user interactions from the command line:

```sh
cargo run -- versions
cargo run -- set name Player
cargo run -- set ram 2048
cargo run -- set version 1.21.1
cargo run -- download 1.21.1
cargo run -- launch
```

Settings are persisted to `vortex_launcher.conf`. The `download` command installs the selected Minecraft version metadata, client jar, libraries, native libraries, asset index, assets, and logging configuration into the configured game directory. The `launch` command builds the version-aware Java command and starts Minecraft with the saved settings.

### Platform support

The Rust launcher detects Windows, Linux, and macOS defaults. It includes platform-specific defaults for the Minecraft directory and Java executable discovery through `JAVA_HOME` and `PATH`.

### Feature parity audit

See [`docs/feature-parity.md`](docs/feature-parity.md) for the checklist comparing this Rust implementation against `vlauncher_windows.pb`, `vlauncher_linux.pb`, and `vlauncher_macos.pb`.

---

## License

[GPLv3](https://github.com/Kron4ek/minecraft-vortex-launcher/blob/master/LICENSE.txt)

---

### Mirrors

Mirror on GitLab: https://gitlab.com/Kron4ek/vortex-minecraft-launcher
