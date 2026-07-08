# Rust replacement and feature-parity checklist

This document compares the Rust rewrite with the legacy PureBasic launchers and tracks the remaining work needed to retire PureBasic from the active user path:

* `vlauncher_windows.pb`
* `vlauncher_linux.pb`
* `vlauncher_macos.pb`

The checklist is intentionally conservative. A checked item means the Rust code has an implementation path for that capability; it does not mean every legacy edge case, UI workflow, packaged release, or third-party/modded profile has been proven equivalent.

The active roadmap is maintained in [`rust-replacement-roadmap.md`](rust-replacement-roadmap.md). Rust is the intended successor implementation. PureBasic remains reference material until the first validated Rust replacement release ships.

## Legend

- [x] Implemented in the Rust rewrite.
- [ ] Not implemented, incomplete, or not yet claimed as complete.
- **Legacy-only/unsupported** means the behavior may exist in the legacy launcher or project history, but is not currently a Rust rewrite guarantee.

## Retirement stages

### Stage 1: Rust primary candidate

- [x] Rust binary crate builds from the repository root with Cargo.
- [x] Core config, download, metadata parsing, native extraction, launch, and GUI modules are present.
- [x] Automated Rust unit tests cover representative config, download, launch-command, native-extraction, Java-selection, and UI-state behavior.
- [x] Rust reads and writes legacy `vortex_launcher.conf` style settings.
- [ ] Dead-code warnings are either fixed or documented as intentional test/support hooks.
- [ ] Manual smoke checks are documented for GUI launch, downloader, settings save/load, Java validation, native extraction, and launch.
- [ ] Known working vanilla coverage is documented for at least one Java 8-era version and one modern Java 21+ version.

### Stage 2: Rust replacement release

- [x] Windows Rust artifact builds with `cargo build --release`.
- [x] Windows release executable passes basic CLI and GUI-startup smoke checks. See [`windows-smoke-test.md`](windows-smoke-test.md).
- [x] Windows GUI opens the main launcher, Downloader, and Settings windows, and visually confirms settings save feedback.
- [x] Windows full `1.12.2` version download succeeds after retry hardening, including client jar and asset objects.
- [ ] Windows full replacement smoke is complete, including settings reload, Java-version launch validation, native-extraction, game-launch, offline-relaunch, and modded-profile validation.
- [ ] Linux Rust artifact is built, documented, and smoke-tested.
- [ ] Fresh install, config migration, full version download, offline relaunch after download, and game launch are validated on Windows and Linux.
- [ ] Java auto-selection and custom Java override are validated on Windows and Linux.
- [ ] At least one inherited/modded profile, such as Forge or Fabric, is validated before being documented as known working.
- [ ] README download guidance points users to Rust artifacts as the default recommended release path.
- [ ] macOS status is documented as validated or best-effort.

### Stage 3: PureBasic archived

- [ ] PureBasic is no longer advertised as the stable/default launcher after the Rust replacement release.
- [ ] PureBasic source remains available as historical reference for one release cycle.
- [ ] Legacy screenshots and user-facing examples are replaced with Rust screenshots, Rust commands, and Rust release artifacts.
- [ ] PureBasic references are moved out of the active user path through an archive directory, dedicated legacy branch/tag, or release notes.

## Current Rust rewrite status

- [x] Binary crate builds from the repository root with Cargo.
- [x] Command interface exists for version discovery, settings updates, downloads, and launch attempts.
- [x] Native-window GUI exists using `egui`/`eframe`.
- [x] Core config, download, metadata parsing, native extraction, and launch modules are present.
- [x] Downloader and Settings open as separate native OS windows where the backend supports multiple viewports.
- [x] Automatic Java selection exists for installed runtimes, with custom Java override support.
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
- [x] Avoid adding classifier-only native libraries to the classpath as nonexistent artifacts.
- [x] Extract downloaded native-library archives into the selected version's natives directory before launch.
- [x] Extract deflated and stored native-library ZIP/JAR entries while respecting exclusion and safe-path rules.
- [x] Select an installed Java runtime from Minecraft `javaVersion.majorVersion` metadata or fallback Minecraft version rules.
- [x] Validate Java with `java -version` before starting Minecraft.
- [x] Launch Minecraft as a child process using the selected Java binary.
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
- [x] Deduplicate repeated download destinations before parallel downloads.
- [x] Surface GUI download progress, files remaining, active item labels, and failed-job details.
- [x] Refresh the main installed-version selector after a successful GUI download.
- [ ] Real download success still depends on network access, Mojang service availability, and metadata formats the rewrite understands.
- [ ] The Rust rewrite does not install Java or other external runtime dependencies.
- [ ] Offline operation is only expected after required version files, libraries, assets, native archives, and logging configs are already present.

## GUI and user interaction parity

- [x] Configure player name.
- [x] Configure RAM allocation.
- [x] Configure selected Minecraft version.
- [x] Configure custom Java binary and automatic Java discovery/selection.
- [x] Configure custom JVM parameters.
- [x] Configure download thread count.
- [x] Configure release-only versus all-version discovery.
- [x] Configure redownload/download-missing-library behavior.
- [x] Configure save-launch-string and keep-launcher-open behavior.
- [x] Provide command interactions for version discovery, downloading, settings changes, and launching.
- [x] Provide a Rust GUI with launcher, downloader, and settings windows.
- [x] Open Downloader and Settings as separate native OS windows on desktop backends that support multiple viewports.
- [x] Present GUI download progress with a progress bar and remaining-file count.
- [ ] Legacy GUI parity is incomplete; layout, polish, dialogs, and some workflows may differ.
- [ ] GUI behavior has not been documented as fully smoke-tested on every supported desktop platform.
- [ ] Packaged Rust GUI releases are not currently claimed.

## Platform support

- [x] Windows Minecraft directory defaults and Java executable naming.
- [x] Windows Eclipse Adoptium and Java install-folder discovery for matching Java major versions.
- [x] Linux Minecraft directory defaults and Java executable naming.
- [x] macOS Minecraft directory defaults and Java executable naming.
- [x] OS-specific native-library classifier selection.
- [x] OS-specific classpath separators.
- [ ] Platform support currently means code paths/defaults exist; it does not guarantee release-quality validation on every OS.
- [ ] Unsupported host operating systems fall back to generic defaults and are not release targets.

## Known limitations

* **GUI:** The Rust GUI is useful for smoke testing launcher, downloader, and settings flows and uses separate native windows for Downloader and Settings where supported, but it should not be described as a complete packaged legacy UI replacement.
* **Downloads:** The downloader uses Mojang metadata, deduplicates repeated destinations, and validates hashes/sizes when present. It may still fail on network errors, unavailable upstream files, unsupported metadata changes, or profiles that require files outside the implemented download plan.
* **Launching:** The launcher selects and validates Java, extracts native archives, builds the command, and starts a child process. Actual game startup can still fail if Java is missing, files are incomplete, metadata is unsupported, or a modded profile needs additional setup.
* **Java requirement:** Minecraft launch requires a compatible Java runtime. The Rust rewrite can select installed Java from metadata/fallback rules or use custom Java, but does not bundle or install Java.
* **Platform support:** Windows, Linux, and macOS path/classifier logic exists. Cross-platform release packaging and exhaustive validation are still incomplete.
* **Authentication:** Microsoft/Minecraft login and ownership verification are not implemented.

## Known follow-up items

- [ ] Improve GUI parity, dialogs, and platform-specific polish.
- [ ] Add release packaging documentation for Rust artifacts when available.
- [ ] Add documented end-to-end smoke tests for download and launch flows on Windows and Linux.
- [ ] Expand mod-loader/profile compatibility testing and document known working profiles.
- [ ] Revisit Microsoft/Minecraft authentication after the Rust replacement release; it is post-v1 optional and not a PureBasic retirement blocker.
