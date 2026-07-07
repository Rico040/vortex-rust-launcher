## Vortex Minecraft Launcher

Fast, lightweight and easy to use Minecraft launcher. Natively available for Windows and Linux.

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
* Can work fully offline

---

## Download

Check the [**Releases**](https://github.com/Kron4ek/minecraft-vortex-launcher/releases) page to download the latest launcher version.

---

## Screenshots

![settings](https://i.imgur.com/dkiweug.png)
![main window](https://i.imgur.com/pd2tnnK.png)
![client downloader](https://i.imgur.com/1QTjiDw.png)

---

## Rust application scaffold

This repository now includes an initial Rust binary crate for the launcher at the repository root. The scaffold keeps the existing GPLv3 license in `LICENSE.txt` and separates the launcher responsibilities into Rust modules under `src/`:

* `config` for reading and writing `vortex_launcher.conf`-style settings.
* `platform` for Windows, Linux, and macOS path handling and Java discovery.
* `minecraft` for version metadata, launch profiles, launch arguments, libraries, assets, and profile generation.
* `download` for manifests, client jars, libraries, assets, and future asynchronous or multithreaded download planning.
* `ui` for the launcher user interface layer.

### Rust build prerequisites

Install the stable Rust toolchain with Cargo. The recommended installation method is [rustup](https://rustup.rs/).

Build the scaffold from the repository root:

```sh
cargo build
```

Run it from the repository root:

```sh
cargo run
```

### Platform support

The Rust scaffold currently detects Windows, Linux, and macOS. It includes platform-specific defaults for the Minecraft directory and Java executable discovery through `JAVA_HOME` and `PATH`. The GUI and launcher implementation are placeholders that will be expanded as the PureBasic functionality is migrated.

---

## License

[GPLv3](https://github.com/Kron4ek/minecraft-vortex-launcher/blob/master/LICENSE.txt)

---

### Mirrors

Mirror on GitLab: https://gitlab.com/Kron4ek/vortex-minecraft-launcher
