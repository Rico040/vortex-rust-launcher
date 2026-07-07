// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::config::{join_args, parse_arg_string, LauncherConfig, DEFAULT_CONFIG_FILE};
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
    persisted_config: LauncherConfig,
    config_path: PathBuf,
    download_plan: DownloadPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherUiState {
    pub main: MainWindowState,
    pub downloader: DownloaderWindowState,
    pub settings: SettingsWindowState,
    pub settings_dirty: bool,
    pub status_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainWindowState {
    pub player_name: String,
    pub ram_mb: String,
    pub selected_version: String,
    pub installed_versions: Vec<String>,
    pub versions_error: Option<String>,
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
            persisted_config: config.clone(),
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
        self.config.save(&self.config_path)?;
        self.mark_persisted();
        Ok(())
    }
    pub fn save_config_to(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        self.apply_state_to_config();
        self.config.save(path)?;
        self.mark_persisted();
        Ok(())
    }

    fn save_config_if_main_fields_changed(&mut self) -> std::io::Result<()> {
        if self.main_fields_changed() {
            self.save_config()?;
        }
        Ok(())
    }

    fn save_config_if_main_fields_changed_to(
        &mut self,
        path: impl AsRef<Path>,
    ) -> std::io::Result<bool> {
        if self.main_fields_changed() {
            self.save_config_to(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn mark_persisted(&mut self) {
        self.persisted_config = self.config.clone();
        self.state.settings_dirty = false;
    }

    fn projected_config(&self) -> LauncherConfig {
        let mut config = self.config.clone();
        Self::apply_state_to(&self.state, &mut config);
        config
    }

    fn main_fields_changed(&self) -> bool {
        let projected = self.projected_config();
        projected.username != self.persisted_config.username
            || projected.selected_version != self.persisted_config.selected_version
            || projected.memory_mb != self.persisted_config.memory_mb
    }

    fn apply_state_to_config(&mut self) {
        Self::apply_state_to(&self.state, &mut self.config);
    }

    fn apply_state_to(state: &LauncherUiState, config: &mut LauncherConfig) {
        config.username = non_empty(state.main.player_name.replace(' ', ""));
        config.selected_version = non_empty(state.main.selected_version.replace(' ', ""));
        config.memory_mb = state.main.ram_mb.parse::<u32>().ok();
        config.download_missing_libraries = state.settings.download_missing_libraries;
        config.async_download = state.settings.async_download;
        config.download_threads = state
            .settings
            .download_threads
            .parse()
            .unwrap_or(config.download_threads);
        config.use_custom_java = state.settings.use_custom_java;
        config.java_path = non_empty(state.settings.custom_java_path.clone()).map(PathBuf::from);
        config.use_custom_jvm_parameters = state.settings.use_custom_jvm_parameters;
        config.extra_jvm_args = parse_arg_string(&state.settings.custom_jvm_parameters);
        config.save_launch_string = state.settings.save_launch_string;
        config.keep_launcher_open = state.settings.keep_launcher_open;
        config.show_all_versions = state.downloader.show_all_versions;
        config.redownload_all_files = state.downloader.redownload_all_files;
    }

    fn handle_play(&mut self) -> bool {
        if let Err(error) = self.save_config_if_main_fields_changed() {
            self.state.status_message = format!("Could not save launch settings: {error}");
            return false;
        }
        self.apply_state_to_config();
        let profile = LaunchProfile::from_config(&self.config);
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
                Ok((outcome.child.id(), outcome.should_close_launcher))
            }) {
            Ok((pid, should_close_launcher)) => {
                self.state.status_message = format!("Launched Minecraft with pid {pid}");
                should_close_launcher
            }
            Err(error) => {
                self.state.status_message = format!("Launch failed: {error}");
                false
            }
        }
    }

    fn handle_download(&mut self) {
        self.state.main.selected_version = self.state.downloader.selected_version.clone();
        self.apply_state_to_config();

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
                if self.ui.state.main.installed_versions.is_empty() {
                    ui.label("No installed versions");
                } else {
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
                }
            });
            if let Some(error) = &self.ui.state.main.versions_error {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
            ui.horizontal(|ui| {
                let can_play = !self.ui.state.main.installed_versions.is_empty();
                if ui
                    .add_enabled(can_play, egui::Button::new("Play"))
                    .clicked()
                    && self.ui.handle_play()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
                let mut changed = ui
                    .checkbox(
                        &mut self.ui.state.settings.async_download,
                        "Fast multithreaded downloading",
                    )
                    .changed();
                ui.add_enabled_ui(self.ui.state.settings.async_download, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Download threads:");
                        changed |= ui
                            .text_edit_singleline(&mut self.ui.state.settings.download_threads)
                            .changed();
                    });
                });
                changed |= ui
                    .checkbox(
                        &mut self.ui.state.settings.download_missing_libraries,
                        "Download missing libraries on game start",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.ui.state.settings.save_launch_string,
                        "Save the launch string to a file",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.ui.state.settings.use_custom_java,
                        "Use custom Java",
                    )
                    .changed();
                ui.add_enabled_ui(self.ui.state.settings.use_custom_java, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path to Java binary:");
                        changed |= ui
                            .text_edit_singleline(&mut self.ui.state.settings.custom_java_path)
                            .changed();
                    });
                });
                changed |= ui
                    .checkbox(
                        &mut self.ui.state.settings.use_custom_jvm_parameters,
                        "Use custom launch parameters",
                    )
                    .changed();
                ui.add_enabled_ui(self.ui.state.settings.use_custom_jvm_parameters, |ui| {
                    ui.label("Launch parameters:");
                    changed |= ui
                        .text_edit_multiline(&mut self.ui.state.settings.custom_jvm_parameters)
                        .changed();
                });
                changed |= ui
                    .checkbox(
                        &mut self.ui.state.settings.keep_launcher_open,
                        "Keep the launcher open",
                    )
                    .changed();
                if changed {
                    self.ui.state.settings_dirty = true;
                }
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
        let (installed_versions, versions_error) = installed_versions(&profile.game_directory)
            .map_or_else(
                |error| {
                    (
                        Vec::new(),
                        Some(format!(
                            "Could not read {}: {error}. Open Downloader to install a version.",
                            profile.game_directory.join("versions").display()
                        )),
                    )
                },
                |versions| {
                    let versions_error = versions.is_empty().then(|| {
                        format!(
                            "No installed versions found in {}. Open Downloader to install one.",
                            profile.game_directory.join("versions").display()
                        )
                    });
                    (versions, versions_error)
                },
            );
        let selected_version = config
            .selected_version
            .as_ref()
            .filter(|chosen| installed_versions.iter().any(|version| version == *chosen))
            .cloned()
            .or_else(|| installed_versions.first().cloned())
            .unwrap_or_else(|| profile.version_id.clone());
        let java_path = config
            .java_path
            .clone()
            .or_else(|| runtime.find_java())
            .unwrap_or_else(|| current_platform_defaults().default_java_executable_path);
        Self {
            main: MainWindowState {
                player_name: profile.username.clone(),
                ram_mb: profile.memory_mb.to_string(),
                selected_version: selected_version.clone(),
                installed_versions,
                versions_error,
            },
            downloader: DownloaderWindowState {
                open: false,
                selected_version,
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
                    join_args(&config.extra_jvm_args)
                },
                save_launch_string: config.save_launch_string,
                keep_launcher_open: config.keep_launcher_open,
            },
            settings_dirty: false,
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
                .then(|| parse_arg_string(&self.settings.custom_jvm_parameters))
                .unwrap_or_default(),
            game_args: Vec::new(),
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn installed_versions(game_directory: &Path) -> std::io::Result<Vec<String>> {
    let versions_directory = game_directory.join("versions");
    let mut versions = Vec::new();

    if !versions_directory.exists() {
        return Ok(versions);
    }

    for entry in fs::read_dir(&versions_directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let version = entry.file_name().to_string_lossy().into_owned();
        if entry.path().join(format!("{version}.json")).is_file() {
            versions.push(version);
        }
    }

    versions.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_game_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "vortex-ui-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn ui_for_config(config: LauncherConfig, game_directory: PathBuf) -> LauncherUi {
        let mut profile = LaunchProfile::from_config(&config);
        profile.game_directory = game_directory;
        LauncherUi::new(
            RuntimeEnvironment::detect(),
            config,
            profile,
            DownloadPlan::default(),
        )
    }

    #[test]
    fn scans_only_version_directories_with_matching_json() {
        let root = temp_game_dir();
        fs::create_dir_all(root.join("versions/1.20.4")).unwrap();
        fs::write(root.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        fs::create_dir_all(root.join("versions/1.19.4")).unwrap();
        fs::write(root.join("versions/1.19.4/wrong.json"), "{}").unwrap();
        fs::write(root.join("versions/readme.txt"), "ignored").unwrap();

        assert_eq!(installed_versions(&root).unwrap(), vec!["1.20.4"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prefers_configured_chosen_version_when_installed() {
        let root = temp_game_dir();
        for version in ["1.20.4", "1.21"] {
            fs::create_dir_all(root.join("versions").join(version)).unwrap();
            fs::write(
                root.join("versions")
                    .join(version)
                    .join(format!("{version}.json")),
                "{}",
            )
            .unwrap();
        }
        let mut config = LauncherConfig::default();
        config.selected_version = Some("1.21".to_owned());
        let mut profile = LaunchProfile::from_config(&config);
        profile.game_directory = root.clone();

        let state = LauncherUiState::from_config(&config, &profile, &RuntimeEnvironment::detect());

        assert_eq!(state.main.installed_versions, vec!["1.20.4", "1.21"]);
        assert_eq!(state.main.selected_version, "1.21");
        assert!(state.main.versions_error.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn surfaces_empty_state_when_no_versions_are_installed() {
        let root = temp_game_dir();
        fs::create_dir_all(root.join("versions")).unwrap();
        let config = LauncherConfig::default();
        let mut profile = LaunchProfile::from_config(&config);
        profile.game_directory = root.clone();

        let state = LauncherUiState::from_config(&config, &profile, &RuntimeEnvironment::detect());

        assert!(state.main.installed_versions.is_empty());
        assert!(state
            .main
            .versions_error
            .unwrap()
            .contains("Open Downloader"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_main_fields_do_not_write_config() {
        let root = temp_game_dir();
        fs::create_dir_all(root.join("versions/1.20.4")).unwrap();
        fs::write(root.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        let config_path = root.join("vortex_launcher.conf");
        fs::write(&config_path, "original config").unwrap();

        let mut config = LauncherConfig::default();
        config.username = Some("Player".to_owned());
        config.selected_version = Some("1.20.4".to_owned());
        config.memory_mb = Some(2048);
        let mut ui = ui_for_config(config, root.clone());

        assert!(!ui
            .save_config_if_main_fields_changed_to(&config_path)
            .unwrap());
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "original config");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_main_fields_write_config_on_launch_save_path() {
        let root = temp_game_dir();
        fs::create_dir_all(root.join("versions/1.20.4")).unwrap();
        fs::write(root.join("versions/1.20.4/1.20.4.json"), "{}").unwrap();
        let config_path = root.join("vortex_launcher.conf");
        fs::write(&config_path, "original config").unwrap();

        let mut config = LauncherConfig::default();
        config.username = Some("Player".to_owned());
        config.selected_version = Some("1.20.4".to_owned());
        config.memory_mb = Some(2048);
        let mut ui = ui_for_config(config, root.clone());
        ui.state.main.player_name = "Edited Player".to_owned();

        assert!(ui
            .save_config_if_main_fields_changed_to(&config_path)
            .unwrap());
        let saved = fs::read_to_string(&config_path).unwrap();
        assert!(saved.contains("Name=EditedPlayer"));
        assert!(!ui.state.settings_dirty);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_apply_clears_settings_dirty() {
        let root = temp_game_dir();
        let config_path = root.join("vortex_launcher.conf");
        fs::create_dir_all(&root).unwrap();
        let mut ui = ui_for_config(LauncherConfig::default(), root.clone());
        ui.state.settings_dirty = true;
        ui.state.settings.keep_launcher_open = true;

        ui.save_config_to(&config_path).unwrap();

        assert!(!ui.state.settings_dirty);
        assert!(fs::read_to_string(&config_path)
            .unwrap()
            .contains("KeepLauncherOpen=true"));
        fs::remove_dir_all(root).unwrap();
    }
}
