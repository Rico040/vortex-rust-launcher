// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use crate::config::LauncherConfig;
use crate::download::{DownloadMode, DownloadOptions, DownloadPlan};
use crate::minecraft::{LaunchProfile, VersionMetadata};
use crate::platform::{current_platform_defaults, RuntimeEnvironment, UiLayout};

const LAUNCHER_AUTHOR: &str = "Kron4ek";
const LAUNCHER_VERSION: &str = "1.1.20";

#[derive(Debug)]
pub struct LauncherUi {
    runtime: RuntimeEnvironment,
    layout: UiLayout,
    state: LauncherUiState,
    download_plan: DownloadPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherUiState {
    pub main: MainWindowState,
    pub downloader: DownloaderWindowState,
    pub settings: SettingsWindowState,
    pub status_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainWindowState {
    pub player_name: String,
    pub ram_mb: String,
    pub selected_version: String,
    pub installed_versions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloaderWindowState {
    pub open: bool,
    pub selected_version: String,
    pub show_all_versions: bool,
    pub redownload_all_files: bool,
    pub available_versions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsWindowState {
    pub open: bool,
    pub download_missing_libraries: bool,
    pub async_download: bool,
    pub download_threads: String,
    pub use_custom_java: bool,
    pub custom_java_path: String,
    pub use_custom_jvm_parameters: bool,
    pub custom_jvm_parameters: String,
    pub save_launch_string: bool,
    pub keep_launcher_open: bool,
}

impl LauncherUi {
    pub fn new(
        runtime: RuntimeEnvironment,
        config: LauncherConfig,
        profile: LaunchProfile,
        download_plan: DownloadPlan,
    ) -> Self {
        let layout = runtime.ui_layout();
        let state = LauncherUiState::from_config(&config, &profile, &runtime);
        Self {
            runtime,
            layout,
            state,
            download_plan,
        }
    }

    pub fn run(&self) {
        // The current crate intentionally keeps the GUI backend swappable while the
        // launcher logic is ported.  This renderer mirrors the PureBasic windows and
        // callbacks as a deterministic text presentation, so the non-UI modules own
        // config persistence, download planning, Java lookup, and launch arguments.
        println!("{}", self.render_main_window());
        println!("Queued download tasks: {}", self.download_queue_len());
        println!("Download options: {:?}", self.downloader_options());
        println!("Launch preview args: {:?}", self.play());
        if self.state.downloader.open {
            println!("\n{}", self.render_downloader_window());
        }
        if self.state.settings.open {
            println!("\n{}", self.render_settings_window());
        }
    }

    pub fn render_main_window(&self) -> String {
        format!(
            "Vortex Minecraft Launcher ({:?}, {}x{})\nPlayer name: [{}]\nRAM (MB): [{}]\nMinecraft version: [{}]\n[Play]\n[Downloader]\n[Settings]\nby {}                                                    v{}\n{}",
            self.runtime.os,
            self.layout.main_window.0,
            self.layout.main_window.1,
            self.state.main.player_name,
            self.state.main.ram_mb,
            self.state.main.selected_version,
            LAUNCHER_AUTHOR,
            LAUNCHER_VERSION,
            self.state.status_message,
        )
    }

    pub fn render_downloader_window(&self) -> String {
        format!(
            "Client Downloader ({}x{})\nVersion: [{}]\n[{}] Show all versions\n[{}] Redownload all files\n[Download]",
            self.layout.downloader_window.0,
            self.layout.downloader_window.1,
            self.state.downloader.selected_version,
            checkbox(self.state.downloader.show_all_versions),
            checkbox(self.state.downloader.redownload_all_files),
        )
    }

    pub fn render_settings_window(&self) -> String {
        format!(
            "Vortex Launcher Settings ({}x{})\nLaunch parameters: [{}]{}\nPath to Java binary: [{}]{}\nDownload threads: [{}]{}\n[{}] Fast multithreaded downloading\n[{}] Download missing libraries on game start\n[{}] Save the launch string to a file\n[{}] Use custom Java\n[{}] Use custom launch parameters\n[{}] Keep the launcher open\n[Save and apply]",
            self.layout.settings_window.0,
            self.layout.settings_window.1,
            self.state.settings.custom_jvm_parameters,
            disabled(!self.state.settings.use_custom_jvm_parameters),
            self.state.settings.custom_java_path,
            disabled(!self.state.settings.use_custom_java),
            self.state.settings.download_threads,
            disabled(!self.state.settings.async_download),
            checkbox(self.state.settings.async_download),
            checkbox(self.state.settings.download_missing_libraries),
            checkbox(self.state.settings.save_launch_string),
            checkbox(self.state.settings.use_custom_java),
            checkbox(self.state.settings.use_custom_jvm_parameters),
            checkbox(self.state.settings.keep_launcher_open),
        )
    }

    pub fn play(&self) -> Vec<String> {
        let profile = self.state.to_launch_profile(&self.runtime);
        profile.launch_arguments(&VersionMetadata::minimal(&profile.version_id))
    }

    pub fn downloader_options(&self) -> DownloadOptions {
        DownloadOptions {
            mode: if self.state.downloader.redownload_all_files {
                DownloadMode::AllFiles
            } else {
                DownloadMode::MissingLibraries
            },
            include_snapshots: self.state.downloader.show_all_versions,
            max_parallel_downloads: self.state.settings.download_threads.parse().unwrap_or(5),
            async_download: self.state.settings.async_download,
        }
    }

    pub fn download_queue_len(&self) -> usize {
        self.download_plan.tasks.len()
    }
}

impl LauncherUiState {
    fn from_config(
        config: &LauncherConfig,
        profile: &LaunchProfile,
        runtime: &RuntimeEnvironment,
    ) -> Self {
        let installed_versions = config
            .selected_version
            .clone()
            .into_iter()
            .chain([profile.version_id.clone()])
            .collect();
        let java_path = config
            .java_path
            .clone()
            .or_else(|| runtime.find_java())
            .unwrap_or_else(|| current_platform_defaults().default_java_executable_path);
        Self {
            main: MainWindowState {
                player_name: profile.username.clone(),
                ram_mb: profile.memory_mb.to_string(),
                selected_version: profile.version_id.clone(),
                installed_versions,
            },
            downloader: DownloaderWindowState {
                open: false,
                selected_version: profile.version_id.clone(),
                show_all_versions: config.show_all_versions,
                redownload_all_files: config.redownload_all_files,
                available_versions: vec![profile.version_id.clone()],
            },
            settings: SettingsWindowState {
                open: false,
                download_missing_libraries: config.download_missing_libraries,
                async_download: config.async_download,
                download_threads: config.download_threads.to_string(),
                use_custom_java: config.use_custom_java,
                custom_java_path: java_path.display().to_string(),
                use_custom_jvm_parameters: config.use_custom_jvm_parameters,
                custom_jvm_parameters: if config.extra_jvm_args.is_empty() {
                    current_platform_defaults().default_modern_jvm_arguments
                } else {
                    config.extra_jvm_args.join(" ")
                },
                save_launch_string: config.save_launch_string,
                keep_launcher_open: config.keep_launcher_open,
            },
            status_message: "Ready".to_owned(),
        }
    }

    fn to_launch_profile(&self, runtime: &RuntimeEnvironment) -> LaunchProfile {
        LaunchProfile {
            username: self.main.player_name.replace(' ', ""),
            version_id: self.main.selected_version.replace(' ', ""),
            game_directory: runtime
                .minecraft_directory()
                .unwrap_or_else(|| PathBuf::from(".minecraft")),
            java_path: self
                .settings
                .use_custom_java
                .then(|| PathBuf::from(&self.settings.custom_java_path)),
            memory_mb: self.main.ram_mb.parse::<u32>().unwrap_or(2500).max(350),
            jvm_args: self
                .settings
                .use_custom_jvm_parameters
                .then(|| {
                    self.settings
                        .custom_jvm_parameters
                        .split_whitespace()
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            game_args: Vec::new(),
        }
    }
}

fn checkbox(value: bool) -> &'static str {
    if value {
        "x"
    } else {
        " "
    }
}
fn disabled(value: bool) -> &'static str {
    if value {
        " (disabled)"
    } else {
        ""
    }
}
