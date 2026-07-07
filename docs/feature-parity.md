# Rust rewrite status and feature-parity checklist

This document compares the in-progress Rust rewrite with the legacy PureBasic launchers:

* `vlauncher_windows.pb`
* `vlauncher_linux.pb`
* `vlauncher_macos.pb`

The checklist is intentionally conservative. A checked item means the Rust code has an implementation path for that capability; it does not mean every legacy edge case, UI workflow, packaged release, or third-party/modded profile has been proven equivalent.

## Legend

- [x] Implemented in the Rust rewrite.
- [ ] Not implemented, incomplete, or not yet claimed as complete.
- **Legacy-only/unsupported** means the behavior may exist in the legacy launcher or project history, but is not currently a Rust rewrite guarantee.

## Current Rust rewrite status

- [x] Binary crate builds from the repository root with Cargo.
- [x] Command interface exists for version discovery, settings updates, downloads, and launch attempts.
- [x] Native-window GUI exists using `egui`/`eframe`.
- [x] Core config, download, metadata parsing, native extraction, and launch modules are present.
- [ ] Rust GUI parity with the legacy PureBasic interface is not complete or guaranteed.
- [ ] Rust release packaging/installers are not documented as complete.
- [ ] End-to-end release validation across Windows, Linux, and macOS is not documented as complete.
- [ ] Microsoft/Minecraft authentication is not implemented.

## Legacy launcher capabilities versus Rust rewrite

The legacy launcher has historically advertised a lightweight GUI, client downloader, settings, Java selection, launch-string generation, missing-library downloads, and support for many Minecraft/mod-loader profiles. The Rust rewrite is working toward those areas, but documentation should not present all legacy claims as current Rust guarantees.

## Core launcher flow

- [x] Load and save `vortex_launcher.conf` style settings (`Name`, `Ram`, `ChosenVer`, Java path, custom parameters, downloader flags, and launcher behavior flags).
- [x] Discover installed/configured versions for launcher state.
- [x] Build Mojang-version-aware launch arguments from modern `arguments` metadata and legacy `minecraftArguments` metadata.
- [x] Resolve inherited version JSON files used by Forge and other modded profiles.
- [x] Build a classpath from libraries plus the selected client jar.
- [x] Extract downloaded native-library archives into the selected version's natives directory before launch.
- [x] Validate Java with `java -version` before starting Minecraft.
- [x] Launch Minecraft as a child process using the configured Java binary.
- [x] Optionally save the generated launch string to `launch_string.txt`.
- [x] Honor the keep-launcher-open setting when reporting launch behavior.
- [ ] Successful game startup is not guaranteed for every vanilla, Forge, Fabric, NeoForge, or custom inherited profile.
- [ ] Authenticated account/session flows are not implemented.

## Downloader parity

- [x] Download Mojang's version manifest.
- [x] Filter release-only versus all-version/snapshot lists.
- [x] Resolve a selected version to its version JSON URL.
- [x] Download selected version JSON files.
- [x] Download client jars.
- [x] Download Java libraries.
- [x] Download OS-specific native-library archives.
- [x] Download asset indexes.
- [x] Download asset objects into the Mojang hashed object tree.
- [x] Download client logging configuration files.
- [x] Verify file sizes and SHA-1 hashes when metadata provides them.
- [x] Support configurable parallel download counts.
- [x] Preserve synchronous/asynchronous downloader plumbing for UI use.
- [ ] Real download success still depends on network access, Mojang service availability, and metadata formats the rewrite understands.
- [ ] The Rust rewrite does not install Java or other external runtime dependencies.
- [ ] Offline operation is only expected after required version files, libraries, assets, native archives, and logging configs are already present.

## GUI and user interaction parity

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
- [x] Provide an experimental Rust GUI with launcher, downloader, and settings windows.
- [ ] Legacy GUI parity is incomplete; layout, polish, dialogs, progress presentation, and some workflows may differ.
- [ ] GUI behavior has not been documented as fully smoke-tested on every supported desktop platform.
- [ ] Packaged Rust GUI releases are not currently claimed.

## Platform support

- [x] Windows Minecraft directory defaults and Java executable naming.
- [x] Linux Minecraft directory defaults and Java executable naming.
- [x] macOS Minecraft directory defaults and Java executable naming.
- [x] OS-specific native-library classifier selection.
- [x] OS-specific classpath separators.
- [ ] Platform support currently means code paths/defaults exist; it does not guarantee release-quality validation on every OS.
- [ ] Unsupported host operating systems fall back to generic defaults and are not release targets.

## Known limitations

* **GUI:** The Rust GUI is experimental. It is useful for smoke testing launcher, downloader, and settings flows, but it should not be described as a complete legacy UI replacement.
* **Downloads:** The downloader uses Mojang metadata and validates hashes/sizes when present. It may still fail on network errors, unavailable upstream files, unsupported metadata changes, or profiles that require files outside the implemented download plan.
* **Launching:** The launcher validates Java, extracts native archives, builds the command, and starts a child process. Actual game startup can still fail if Java is incompatible, files are incomplete, metadata is unsupported, or a modded profile needs additional setup.
* **Java requirement:** Minecraft launch requires a compatible Java runtime. The Rust rewrite can discover or use Java, but does not bundle or install Java.
* **Platform support:** Windows, Linux, and macOS path/classifier logic exists. Cross-platform release packaging and exhaustive validation are still incomplete.
* **Authentication:** Microsoft/Minecraft login and ownership verification are not implemented.

## Known follow-up items

- [ ] Improve GUI parity, progress reporting, dialogs, and platform-specific polish.
- [ ] Add release packaging documentation for Rust artifacts when available.
- [ ] Add documented end-to-end smoke tests for download and launch flows on Windows, Linux, and macOS.
- [ ] Expand mod-loader/profile compatibility testing and document known working profiles.
- [ ] Decide whether authentication is in scope for the Rust rewrite and document the result.
