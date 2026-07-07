// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::config::{LauncherConfig, DEFAULT_CONFIG_FILE};
use crate::download::{DownloadMode, DownloadOptions, DownloadPlan};
use crate::launch::{self, LaunchOptions};
use crate::minecraft::{LaunchProfile, VersionMetadata};
use crate::platform::{current_platform_defaults, RuntimeEnvironment, UiLayout};

const LAUNCHER_AUTHOR: &str = "Kron4ek";
const LAUNCHER_VERSION: &str = "1.1.20";

#[derive(Debug)]
pub struct LauncherUi {
    runtime: RuntimeEnvironment,
    layout: UiLayout,
    state: LauncherUiState,
    config: LauncherConfig,
    config_path: PathBuf,
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
            config,
            config_path: PathBuf::from(DEFAULT_CONFIG_FILE),
            download_plan,
        }
    }

    pub fn run(self) -> eframe::Result<()> {
        let size = egui::vec2(
            self.layout.main_window.0 as f32,
            self.layout.main_window.1 as f32,
        );
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(size)
                .with_min_inner_size(size),
            ..Default::default()
        };
        eframe::run_native(
            "Vortex Minecraft Launcher",
            options,
            Box::new(|_cc| Ok(Box::new(LauncherApp { ui: self }))),
        )
    }

    pub fn save_config(&mut self) -> std::io::Result<()> {
        self.apply_state_to_config();
        self.config.save(&self.config_path)
    }
    pub fn save_config_to(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        self.apply_state_to_config();
        self.config.save(path)
    }

    fn apply_state_to_config(&mut self) {
        self.config.username = non_empty(self.state.main.player_name.replace(' ', ""));
        self.config.selected_version = non_empty(self.state.main.selected_version.replace(' ', ""));
        self.config.memory_mb = self.state.main.ram_mb.parse::<u32>().ok();
        self.config.download_missing_libraries = self.state.settings.download_missing_libraries;
        self.config.async_download = self.state.settings.async_download;
        self.config.download_threads = self
            .state
            .settings
            .download_threads
            .parse()
            .unwrap_or(self.config.download_threads);
        self.config.use_custom_java = self.state.settings.use_custom_java;
        self.config.java_path =
            non_empty(self.state.settings.custom_java_path.clone()).map(PathBuf::from);
        self.config.use_custom_jvm_parameters = self.state.settings.use_custom_jvm_parameters;
        self.config.extra_jvm_args = self
            .state
            .settings
            .custom_jvm_parameters
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect();
        self.config.save_launch_string = self.state.settings.save_launch_string;
        self.config.keep_launcher_open = self.state.settings.keep_launcher_open;
        self.config.show_all_versions = self.state.downloader.show_all_versions;
        self.config.redownload_all_files = self.state.downloader.redownload_all_files;
    }

    fn handle_play(&mut self) {
        self.apply_state_to_config();
        let profile = self.state.to_launch_profile(&self.runtime);
        match profile
            .launch_command(self.config.save_launch_string)
            .and_then(|command| {
                let outcome = launch::launch_minecraft(
                    &command,
                    LaunchOptions {
                        keep_launcher_open: self.config.keep_launcher_open,
                        save_launch_string: self.config.save_launch_string,
                    },
                )?;
                if let Some(display) = outcome.display_command {
                    std::fs::write("launch_string.txt", display)?;
                }
                Ok(outcome.child.id())
            }) {
            Ok(pid) => self.state.status_message = format!("Launched Minecraft with pid {pid}"),
            Err(error) => self.state.status_message = format!("Launch failed: {error}"),
        }
    }

    fn handle_download(&mut self) {
        self.state.main.selected_version = self.state.downloader.selected_version.clone();
        self.apply_state_to_config();
        if let Err(error) = self.save_config() {
            self.state.status_message = format!("Could not save download settings: {error}");
            return;
        }

        match std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe)
                .arg("download")
                .arg(&self.state.main.selected_version)
                .spawn()
        }) {
            Ok(child) => {
                self.state.status_message = format!(
                    "Started download process {} for {} with {:?}.",
                    child.id(),
                    self.state.main.selected_version,
                    self.downloader_options()
                );
            }
            Err(error) => self.state.status_message = format!("Download failed to start: {error}"),
        }
    }

    fn handle_save(&mut self) {
        match self.save_config() {
            Ok(()) => {
                self.state.status_message =
                    format!("Saved settings to {}", self.config_path.display())
            }
            Err(error) => self.state.status_message = format!("Save failed: {error}"),
        }
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

struct LauncherApp {
    ui: LauncherUi,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Vortex Minecraft Launcher");
            ui.horizontal(|ui| {
                ui.label("Player name:");
                ui.text_edit_singleline(&mut self.ui.state.main.player_name);
            });
            ui.horizontal(|ui| {
                ui.label("RAM (MB):");
                ui.text_edit_singleline(&mut self.ui.state.main.ram_mb);
            });
            ui.horizontal(|ui| {
                ui.label("Minecraft version:");
                egui::ComboBox::from_id_source("version")
                    .selected_text(&self.ui.state.main.selected_version)
                    .show_ui(ui, |ui| {
                        for version in &self.ui.state.main.installed_versions {
                            ui.selectable_value(
                                &mut self.ui.state.main.selected_version,
                                version.clone(),
                                version,
                            );
                        }
                    });
                ui.text_edit_singleline(&mut self.ui.state.main.selected_version);
            });
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    self.ui.handle_play();
                }
                if ui.button("Downloader").clicked() {
                    self.ui.state.downloader.open = true;
                }
                if ui.button("Settings").clicked() {
                    self.ui.state.settings.open = true;
                }
            });
            ui.separator();
            ui.label(&self.ui.state.status_message);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("by {LAUNCHER_AUTHOR} v{LAUNCHER_VERSION}"))
            });
        });

        let mut downloader_open = self.ui.state.downloader.open;
        egui::Window::new("Client Downloader")
            .open(&mut downloader_open)
            .default_size([
                self.ui.layout.downloader_window.0 as f32,
                self.ui.layout.downloader_window.1 as f32,
            ])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Version:");
                    ui.text_edit_singleline(&mut self.ui.state.downloader.selected_version);
                });
                ui.checkbox(
                    &mut self.ui.state.downloader.show_all_versions,
                    "Show all versions",
                );
                ui.checkbox(
                    &mut self.ui.state.downloader.redownload_all_files,
                    "Redownload all files",
                );
                if ui.button("Download").clicked() {
                    self.ui.handle_download();
                }
            });
        self.ui.state.downloader.open = downloader_open;

        let mut settings_open = self.ui.state.settings.open;
        egui::Window::new("Vortex Launcher Settings")
            .open(&mut settings_open)
            .default_size([
                self.ui.layout.settings_window.0 as f32,
                self.ui.layout.settings_window.1 as f32,
            ])
            .show(ctx, |ui| {
                ui.checkbox(
                    &mut self.ui.state.settings.async_download,
                    "Fast multithreaded downloading",
                );
                ui.add_enabled_ui(self.ui.state.settings.async_download, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Download threads:");
                        ui.text_edit_singleline(&mut self.ui.state.settings.download_threads);
                    });
                });
                ui.checkbox(
                    &mut self.ui.state.settings.download_missing_libraries,
                    "Download missing libraries on game start",
                );
                ui.checkbox(
                    &mut self.ui.state.settings.save_launch_string,
                    "Save the launch string to a file",
                );
                ui.checkbox(
                    &mut self.ui.state.settings.use_custom_java,
                    "Use custom Java",
                );
                ui.add_enabled_ui(self.ui.state.settings.use_custom_java, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path to Java binary:");
                        ui.text_edit_singleline(&mut self.ui.state.settings.custom_java_path);
                    });
                });
                ui.checkbox(
                    &mut self.ui.state.settings.use_custom_jvm_parameters,
                    "Use custom launch parameters",
                );
                ui.add_enabled_ui(self.ui.state.settings.use_custom_jvm_parameters, |ui| {
                    ui.label("Launch parameters:");
                    ui.text_edit_multiline(&mut self.ui.state.settings.custom_jvm_parameters);
                });
                ui.checkbox(
                    &mut self.ui.state.settings.keep_launcher_open,
                    "Keep the launcher open",
                );
                if ui.button("Save and apply").clicked() {
                    self.ui.handle_save();
                }
            });
        self.ui.state.settings.open = settings_open;
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

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
