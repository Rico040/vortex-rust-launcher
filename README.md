## Vortex Minecraft Launcher

Vortex Minecraft Launcher is an open-source Minecraft launcher project. The original launcher is the legacy PureBasic implementation; this repository also contains an in-progress Rust rewrite.

> **Rust rewrite status:** usable for development and smoke testing, but not a complete replacement for the legacy launcher. Treat the Rust binary as experimental until the checklist below is complete.

---

## Legacy launcher capabilities

The legacy PureBasic launcher has historically provided these capabilities on its supported platforms:

* Lightweight launcher UI for Windows and Linux releases.
* Player name, RAM, Java path, custom JVM arguments, and launcher behavior settings.
* Minecraft version discovery and client downloading.
* Library, native-library, asset, and logging-config downloads.
* Launch argument generation for Mojang metadata, including inherited/modded profiles such as Forge.
* Optional offline use after a selected version, libraries, assets, and required files are already installed.

These bullets describe the legacy launcher, not guaranteed Rust rewrite parity.

---

## Rust rewrite status checklist

Current Rust implementation status:

- [x] Reads and writes `vortex_launcher.conf`-style settings.
- [x] Provides a command interface for version listing, setting values, downloading, and launching.
- [x] Detects default Minecraft paths on Windows, Linux, and macOS.
- [x] Resolves Mojang version metadata, inherited profiles, launch arguments, libraries, assets, and classpaths.
- [x] Downloads version JSON files, client jars, Java libraries, native-library archives, asset indexes, asset objects, and logging configs.
- [x] Validates the configured Java executable before launch.
- [x] Extracts downloaded native-library archives before launch.
- [x] Starts Minecraft as a child process when the selected profile and Java installation are valid.
- [x] Includes an `egui`/`eframe` native-window UI with launcher, downloader, and settings controls.
- [ ] The Rust GUI is not yet claimed to match all legacy UI behavior, polish, or packaging.
- [ ] Release packaging/installers for the Rust rewrite are not documented as complete.
- [ ] Automated end-to-end tests for real Mojang downloads and real game launching are not provided.
- [ ] Authenticated Microsoft/Minecraft account flows are not implemented.

See [`docs/feature-parity.md`](docs/feature-parity.md) for a more detailed parity and limitation checklist.

---

## Unsupported or incomplete in the Rust rewrite

The Rust rewrite currently does **not** claim support for:

* Microsoft/Minecraft account authentication or ownership checks.
* A fully polished replacement for the legacy PureBasic GUI.
* Packaged desktop releases equivalent to the legacy launcher downloads.
* Guaranteed launch success for every Minecraft, Forge, Fabric, NeoForge, or other mod-loader profile.
* Automatic Java installation. A compatible Java runtime must already be installed or configured.
* Fully offline first-run behavior. Offline use only applies after the required game files have already been downloaded.

---

## Download

For stable legacy launcher builds, check the [**Releases**](https://github.com/Kron4ek/minecraft-vortex-launcher/releases) page.

The Rust rewrite is built from source unless a release explicitly says it contains Rust rewrite artifacts.

---

## Screenshots

These screenshots show the legacy launcher UI and should not be treated as exact Rust GUI screenshots:

![settings](https://i.imgur.com/dkiweug.png)
![main window](https://i.imgur.com/pd2tnnK.png)
![client downloader](https://i.imgur.com/1QTjiDw.png)

---

## Rust launcher

The Rust binary crate lives at the repository root. It keeps the existing GPLv3 license in `LICENSE.txt` and separates launcher responsibilities into Rust modules under `src/`:

* `config` reads and writes `vortex_launcher.conf` settings compatible with legacy launcher keys.
* `platform` handles Windows, Linux, and macOS Minecraft paths, UI dimensions, and Java discovery through `JAVA_HOME` and `PATH`.
* `minecraft` parses version metadata, inherited profiles, launch arguments, libraries, assets, native extraction rules, and launch commands.
* `download` discovers Mojang versions and downloads manifests, client jars, libraries, native-library archives, asset indexes, asset objects, and logging configs.
* `launch` validates Java and starts the Minecraft process.
* `ui` renders the experimental Rust launcher, downloader, and settings windows while the command interface provides scriptable interactions.

### Rust build prerequisites

Install the stable Rust toolchain with Cargo. The recommended installation method is [rustup](https://rustup.rs/).

Build from the repository root:

```sh
cargo build
```

Run the experimental native-window UI from the repository root:

```sh
cargo run
```

The GUI uses `egui`/`eframe` native windows. It exposes editable launcher state, Play, Downloader, Settings, Save, and Download controls backed by the same config, launch, and download modules as the CLI. It is still part of the rewrite work and is not documented as a complete legacy UI replacement.

### Command interface

The Rust launcher supports these command-line interactions:

```sh
cargo run -- versions
cargo run -- set name Player
cargo run -- set ram 2048
cargo run -- set version 1.21.1
cargo run -- download 1.21.1
cargo run -- launch
```

Settings are persisted to `vortex_launcher.conf`. The `download` command downloads the selected Minecraft version metadata, client jar, libraries, native-library archives, asset index, assets, and logging configuration into the configured game directory. The `launch` command extracts native libraries, builds the version-aware Java command, validates Java, and starts Minecraft with the saved settings.

### Known limitations

* **GUI:** The Rust GUI exists, but it is experimental and may not match the legacy launcher's layout, workflows, error handling, or release packaging.
* **Downloads:** Download code handles Mojang metadata and file integrity where metadata provides hashes/sizes, but real network downloads can still fail because of connectivity, upstream changes, or unsupported metadata edge cases.
* **Launching:** The Rust launcher builds and starts the Java command, but successful gameplay depends on a compatible installed Java runtime, complete downloaded files, platform-native libraries, and the selected profile's metadata.
* **Java requirement:** The Rust rewrite requires Java to launch Minecraft. It can discover Java through `JAVA_HOME` and `PATH`, or use a configured custom Java path, but it does not install Java for you.
* **Platform support:** Path defaults and Java discovery are implemented for Windows, Linux, and macOS. Routine development and release validation may still vary by platform, and packaged Rust releases are not yet claimed here.

### Smoke checks

Manual smoke check on a desktop platform:

```sh
cargo run
```

Verify that the launcher window opens, the Downloader and Settings windows can be opened, Save writes `vortex_launcher.conf`, Download attempts the selected version download, and Play attempts Java validation plus process launch for the configured Minecraft profile.

Automated non-window checks:

```sh
cargo test
```

---

## License

[GPLv3](https://github.com/Kron4ek/minecraft-vortex-launcher/blob/master/LICENSE.txt)

---

### Mirrors

Mirror on GitLab: https://gitlab.com/Kron4ek/vortex-minecraft-launcher
