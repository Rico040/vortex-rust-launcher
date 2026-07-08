## Vortex Minecraft Launcher

Vortex Minecraft Launcher is an open-source Minecraft launcher project. The original launcher is the legacy PureBasic implementation; the Rust rewrite is now the active successor path.

> **Rust rewrite status:** usable for development and smoke testing, with working client downloads, launch command generation, Java selection, and native-window launcher/downloader/settings UI. Rust is intended to become the next primary launcher after Windows and Linux packaging plus release validation are complete. See the [Rust replacement roadmap](docs/rust-replacement-roadmap.md).

---

## Legacy launcher status

The legacy PureBasic launcher is retained as reference material while Rust reaches replacement-release quality. It has historically provided these capabilities on its supported platforms:

* Lightweight launcher UI for Windows and Linux releases.
* Player name, RAM, Java path, custom JVM arguments, and launcher behavior settings.
* Minecraft version discovery and client downloading.
* Library, native-library, asset, and logging-config downloads.
* Launch argument generation for Mojang metadata, including inherited/modded profiles such as Forge.
* Optional offline use after a selected version, libraries, assets, and required files are already installed.

These bullets describe the legacy launcher reference point. New development should target Rust unless a change is explicitly about preserving or auditing legacy behavior.

---

## Rust rewrite status checklist

Current Rust implementation status:

- [x] Reads and writes `vortex_launcher.conf`-style settings.
- [x] Provides a command interface for version listing, setting values, downloading, and launching.
- [x] Detects default Minecraft paths on Windows, Linux, and macOS.
- [x] Resolves Mojang version metadata, inherited profiles, launch arguments, libraries, assets, and classpaths.
- [x] Downloads version JSON files, client jars, Java libraries, native-library archives, asset indexes, asset objects, and logging configs.
- [x] Deduplicates duplicate download destinations and supports Mojang asset downloads for legacy versions such as 1.12.2.
- [x] Selects a matching installed Java runtime from Minecraft metadata or version fallback rules, with custom Java override support.
- [x] Validates the selected Java executable before launch.
- [x] Extracts downloaded native-library archives before launch.
- [x] Starts Minecraft as a child process when the selected profile and Java installation are valid.
- [x] Includes an `egui`/`eframe` UI with separate native launcher, downloader, and settings windows where the backend supports multiple viewports.
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

For current stable legacy launcher builds, check the [**Releases**](https://github.com/Kron4ek/minecraft-vortex-launcher/releases) page.

The Rust rewrite is built from source unless a release explicitly says it contains Rust artifacts. The roadmap target is to make Windows and Linux Rust artifacts the default recommended downloads after validation.

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
* `platform` handles Windows, Linux, and macOS Minecraft paths, UI dimensions, and Java discovery through configured paths, `JAVA_HOME`, `PATH`, and Windows JDK install folders such as Eclipse Adoptium.
* `minecraft` parses version metadata, Java runtime requirements, inherited profiles, launch arguments, libraries, assets, native extraction rules, and launch commands.
* `download` discovers Mojang versions and downloads manifests, client jars, libraries, native-library archives, asset indexes, asset objects, and logging configs.
* `launch` validates Java and starts the Minecraft process.
* `ui` renders the experimental Rust launcher, downloader, and settings windows while the command interface provides scriptable interactions.

### Rust build prerequisites

Install the stable Rust toolchain with Cargo. The recommended installation method is [rustup](https://rustup.rs/).

For Minecraft runtime Java, [Adoptium Temurin](https://adoptium.net/) is the recommended JDK distribution. Install the Java versions needed by the Minecraft versions you want to run.

Build from the repository root:

```sh
cargo build
```

Run the experimental native-window UI from the repository root:

```sh
cargo run
```

The GUI uses `egui`/`eframe` native windows. It exposes editable launcher state, Play, Downloader, Settings, Save, and Download controls backed by the same config, launch, and download modules as the CLI. Downloader and Settings open as separate native OS windows when supported by the desktop backend. The GUI is part of the Rust replacement path, but it is not documented as a packaged legacy UI replacement until Windows and Linux release validation is complete.

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

Settings are persisted to `vortex_launcher.conf`. The `download` command downloads the selected Minecraft version metadata, client jar, libraries, native-library archives, asset index, assets, and logging configuration into the configured game directory. The downloader deduplicates repeated destinations from Mojang metadata before parallel downloads. The `launch` command extracts native libraries, selects a compatible installed Java runtime unless custom Java is enabled, builds the version-aware Java command, validates Java, and starts Minecraft with the saved settings.

### Known limitations

* **GUI:** The Rust GUI exists and now uses separate native windows for Downloader and Settings when possible, but it may still differ from the legacy launcher's layout, dialogs, workflows, error handling, or release packaging.
* **Downloads:** Download code handles Mojang metadata, duplicate destinations, and file integrity where metadata provides hashes/sizes, but real network downloads can still fail because of connectivity, upstream changes, or unsupported metadata edge cases.
* **Launching:** The Rust launcher builds and starts the Java command, but successful gameplay depends on a compatible installed Java runtime, complete downloaded files, platform-native libraries, and the selected profile's metadata.
* **Java requirement:** The Rust rewrite requires Java to launch Minecraft. It can select installed Java runtimes by Minecraft metadata or fallback version rules and can search Windows Eclipse Adoptium installs, `JAVA_HOME`, `PATH`, or a configured custom Java path, but it does not install Java for you.
* **Platform support:** Path defaults and Java discovery are implemented for Windows, Linux, and macOS. Routine development and release validation may still vary by platform, and packaged Rust releases are not yet claimed here.

### Smoke checks

Manual smoke check on a desktop platform:

```sh
cargo run
```

Verify that the launcher window opens, the Downloader and Settings windows open as separate OS windows, Save writes `vortex_launcher.conf`, Download attempts the selected version download with progress, the downloaded version appears in the main selector, and Play attempts Java selection, Java validation, native extraction, and process launch for the configured Minecraft profile.

Before declaring a Rust replacement release, perform the manual smoke check on Windows and Linux with at least one Java 8-era Minecraft version, one modern Java 21+ version, and one inherited/modded profile such as Forge or Fabric.

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
