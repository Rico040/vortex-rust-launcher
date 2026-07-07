# Rust feature-parity checklist

This audit compares the Rust implementation with the legacy PureBasic launchers:

* `vlauncher_windows.pb`
* `vlauncher_linux.pb`
* `vlauncher_macos.pb`

## Core launcher flow

- [x] Load and save `vortex_launcher.conf` style settings (`Name`, `Ram`, `ChosenVer`, Java path, custom parameters, downloader flags, and launcher behavior flags).
- [x] Discover installed/configured versions for the main launcher state.
- [x] Build Mojang-version-aware launch arguments from modern `arguments` metadata and legacy `minecraftArguments` metadata.
- [x] Resolve inherited version JSON files used by Forge and other modded profiles.
- [x] Build a classpath from libraries plus the selected client jar.
- [x] Validate Java with `java -version` before starting Minecraft.
- [x] Launch Minecraft as a child process using the configured Java binary.
- [x] Optionally save the generated launch string to `launch_string.txt`.
- [x] Honor the keep-launcher-open setting when reporting launch behavior.

## Downloader parity

- [x] Download Mojang's version manifest.
- [x] Filter release-only versus all-version/snapshot lists.
- [x] Resolve a selected version to its version JSON URL.
- [x] Download selected version JSON files.
- [x] Download client jars.
- [x] Download Java libraries.
- [x] Download OS-specific native libraries.
- [x] Download asset indexes.
- [x] Download asset objects into the Mojang hashed object tree.
- [x] Download client logging configuration files.
- [x] Verify file sizes and SHA-1 hashes when metadata provides them.
- [x] Support configurable parallel download counts.
- [x] Preserve synchronous/asynchronous downloader plumbing for UI use.

## Settings and user interaction parity

- [x] Configure player name.
- [x] Configure RAM allocation.
- [x] Configure selected Minecraft version.
- [x] Configure custom Java binary and automatic Java discovery.
- [x] Configure custom JVM parameters.
- [x] Configure download thread count.
- [x] Configure release-only versus all-version discovery.
- [x] Configure redownload/download-missing-library behavior.
- [x] Configure save-launch-string and keep-launcher-open behavior.
- [x] Provide command interactions for version discovery, downloading, settings changes, and launching.

## Platform parity

- [x] Windows Minecraft directory defaults and Java executable naming.
- [x] Linux Minecraft directory defaults and Java executable naming.
- [x] macOS Minecraft directory defaults and Java executable naming.
- [x] OS-specific native-library classifier selection.
- [x] OS-specific classpath separators.

## Known follow-up items

- [ ] Replace the deterministic text UI renderer with a native GUI backend while keeping the current launcher/download/config/launch logic.
- [ ] Add native extraction of downloaded native libraries before process launch.
- [ ] Add richer progress presentation for the command interface and future GUI backend.
