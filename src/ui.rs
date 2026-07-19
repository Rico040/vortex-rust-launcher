// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
#[cfg(not(test))]
use std::time::Duration;
use std::{fs, io};

use eframe::egui;

use crate::config::{join_args, parse_arg_string, LauncherConfig, DEFAULT_CONFIG_FILE};
use crate::download::{DownloadEvent, ManifestVersion};
use crate::launch::{self, LaunchOptions};
use crate::minecraft::LaunchProfile;
use crate::platform::{current_platform_defaults, RuntimeEnvironment, UiLayout};

const LAUNCHER_AUTHOR: &str = "ottie";
const LAUNCHER_VERSION: &str = "1.2.0";

#[derive(Debug)]
pub struct LauncherUi {
    layout: UiLayout,
    state: LauncherUiState,
    config: LauncherConfig,
    persisted_config: LauncherConfig,
    config_path: PathBuf,
    download_rx: Option<mpsc::Receiver<DownloadEvent>>,
    download_handle: Option<thread::JoinHandle<Result<String, String>>>,
    game_rx: Option<mpsc::Receiver<Result<String, String>>>,
    game_handle: Option<thread::JoinHandle<()>>,
    launcher_hidden_for_game: bool,
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
    pub manifest_versions: Vec<ManifestVersion>,
    pub versions_error: Option<String>,
    pub download_running: bool,
    pub download_total: usize,
    pub download_finished: usize,
    pub download_failed: usize,
    pub active_download: Option<String>,
    pub download_error: Option<String>,
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
    ) -> Self {
        let layout = runtime.ui_layout();
        let state = LauncherUiState::from_config(&config, &profile, &runtime);
        Self {
            layout,
            state,
            persisted_config: config.clone(),
            config,
            config_path: PathBuf::from(DEFAULT_CONFIG_FILE),
            download_rx: None,
            download_handle: None,
            game_rx: None,
            game_handle: None,
            launcher_hidden_for_game: false,
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
    fn save_config_if_main_fields_changed(&mut self) -> std::io::Result<()> {
        if self.main_fields_changed() {
            self.save_config()?;
        }
        Ok(())
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

    fn handle_play(&mut self, ctx: egui::Context) -> bool {
        eprintln!("[launcher] GUI Play clicked");
        if self.game_running() {
            self.state.status_message = "Minecraft is already running".to_owned();
            return false;
        }
        if let Err(error) = self.save_config_if_main_fields_changed() {
            eprintln!("[launcher] Could not save launch settings before play: {error}");
            self.state.status_message = format!("Could not save launch settings: {error}");
            return false;
        }
        self.apply_state_to_config();
        let profile = LaunchProfile::from_config(&self.config);
        eprintln!(
            "[launcher] GUI launch profile: version={}, player={}, game_dir={}, memory={}M",
            profile.version_id,
            profile.username,
            profile.game_directory.display(),
            profile.memory_mb
        );
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
                Ok((outcome.child, outcome.should_hide_launcher))
            }) {
            Ok((mut child, hide_launcher)) => {
                let pid = child.id();
                eprintln!("[launcher] GUI launch succeeded: pid={pid}");
                let (tx, rx) = mpsc::channel();
                self.game_rx = Some(rx);
                self.game_handle = Some(thread::spawn(move || {
                    let result = child
                        .wait()
                        .map(|status| format!("Minecraft closed with status {status}"))
                        .map_err(|error| format!("Could not wait for Minecraft: {error}"));
                    let _ = tx.send(result);
                    ctx.request_repaint();
                }));
                self.launcher_hidden_for_game = hide_launcher;
                self.state.status_message = format!("Minecraft running with pid {pid}");
                hide_launcher
            }
            Err(error) => {
                eprintln!("[launcher] GUI launch failed: {error}");
                self.state.status_message = format!("Launch failed: {error}");
                false
            }
        }
    }

    fn game_running(&self) -> bool {
        self.game_handle.is_some()
    }

    fn pump_game_events(&mut self) -> bool {
        let Some(rx) = self.game_rx.take() else {
            return false;
        };

        match rx.try_recv() {
            Ok(Ok(message)) => {
                eprintln!("[launcher] {message}");
                self.state.status_message = message;
                self.finish_game_monitor();
                true
            }
            Ok(Err(error)) => {
                eprintln!("[launcher] {error}");
                self.state.status_message = error;
                self.finish_game_monitor();
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.game_rx = Some(rx);
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let message = "Minecraft process monitor stopped".to_owned();
                eprintln!("[launcher] {message}");
                self.state.status_message = message;
                self.finish_game_monitor();
                true
            }
        }
    }

    fn finish_game_monitor(&mut self) {
        if let Some(handle) = self.game_handle.take() {
            if handle.join().is_err() {
                eprintln!("[launcher] Minecraft process monitor panicked");
            }
        }
        self.game_rx = None;
        self.launcher_hidden_for_game = false;
    }

    fn handle_download(&mut self) {
        if self.state.downloader.download_running {
            eprintln!("[download] GUI download requested while another download is running");
            self.state.status_message = "Download already in progress".to_owned();
            return;
        }
        self.state.main.selected_version = self.state.downloader.selected_version.clone();
        self.apply_state_to_config();
        let config = self.config.clone();
        let version = self.state.main.selected_version.clone();
        eprintln!(
            "[download] GUI download clicked: version={version}, game_dir={}, threads={}, redownload_all={}, snapshots={}",
            config
                .game_directory
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| ".minecraft".to_owned()),
            config.download_threads,
            config.redownload_all_files,
            config.show_all_versions
        );
        let (tx, rx) = mpsc::channel();
        self.download_rx = Some(rx);
        self.download_handle = Some(thread::spawn(move || {
            crate::download::download_selected_version(&config, tx)
                .map_err(|error| error.to_string())
        }));
        self.state.downloader.download_running = true;
        self.state.downloader.download_total = 0;
        self.state.downloader.download_finished = 0;
        self.state.downloader.download_failed = 0;
        self.state.downloader.active_download = None;
        self.state.downloader.download_error = None;
        self.state.status_message = format!("Downloading Minecraft {version}...");
    }

    fn pump_download_events(&mut self) {
        if let Some(rx) = self.download_rx.take() {
            for event in rx.try_iter() {
                self.apply_download_event(event);
            }
            self.download_rx = Some(rx);
        }

        let finished = self
            .download_handle
            .as_ref()
            .map(|handle| handle.is_finished())
            .unwrap_or(false);
        if !finished {
            return;
        }
        if let Some(handle) = self.download_handle.take() {
            self.state.downloader.download_running = false;
            match handle.join() {
                Ok(Ok(version)) => {
                    eprintln!("[download] GUI download worker finished: version={version}");
                    self.state.status_message = format!("Downloaded Minecraft {version}");
                    self.state.downloader.active_download = None;
                    self.refresh_installed_versions();
                }
                Ok(Err(error)) => {
                    eprintln!("[download] GUI download worker failed: {error}");
                    self.state.status_message = format!("Download failed: {error}");
                    if self.state.downloader.download_error.is_none() {
                        self.state.downloader.download_error = Some(error);
                    }
                }
                Err(_) => {
                    let error = "download worker panicked".to_owned();
                    eprintln!("[download] GUI download worker panicked");
                    self.state.status_message = format!("Download failed: {error}");
                    self.state.downloader.download_error = Some(error);
                }
            }
            self.download_rx = None;
        }
    }

    fn apply_download_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Started { total } => {
                self.state.downloader.download_total = total;
                self.state.downloader.download_finished = 0;
                self.state.downloader.download_failed = 0;
            }
            DownloadEvent::JobStarted { label, .. } => {
                self.state.downloader.active_download = Some(label);
            }
            DownloadEvent::JobProgress { .. } => {}
            DownloadEvent::JobFinished { .. } => {
                self.state.downloader.download_finished += 1;
            }
            DownloadEvent::JobFailed { label, error, .. } => {
                self.state.downloader.download_finished += 1;
                self.state.downloader.download_failed += 1;
                let detail = format!("{label}: {error}");
                match &mut self.state.downloader.download_error {
                    Some(existing) => {
                        existing.push('\n');
                        existing.push_str(&detail);
                    }
                    None => self.state.downloader.download_error = Some(detail),
                }
            }
            DownloadEvent::Finished { succeeded, failed } => {
                self.state.downloader.download_finished = succeeded + failed;
                self.state.downloader.download_failed = failed;
            }
        }
    }

    fn refresh_installed_versions(&mut self) {
        self.apply_state_to_config();
        let profile = LaunchProfile::from_config(&self.config);
        match installed_versions(&profile.game_directory) {
            Ok(versions) => {
                self.state.main.installed_versions = versions;
                self.state.main.versions_error =
                    self.state.main.installed_versions.is_empty().then(|| {
                        format!(
                            "No installed versions found in {}. Open Downloader to install one.",
                            profile.game_directory.join("versions").display()
                        )
                    });
                if self
                    .state
                    .main
                    .installed_versions
                    .iter()
                    .all(|version| version != &self.state.main.selected_version)
                {
                    if let Some(version) = self.state.main.installed_versions.first() {
                        self.state.main.selected_version = version.clone();
                    }
                }
            }
            Err(error) => {
                self.state.main.versions_error = Some(format!(
                    "Could not read {}: {error}",
                    profile.game_directory.join("versions").display()
                ));
            }
        }
    }

    fn handle_save(&mut self) {
        eprintln!(
            "[launcher] Saving GUI settings to {}",
            self.config_path.display()
        );
        match self.save_config() {
            Ok(()) => {
                eprintln!("[launcher] GUI settings saved");
                self.state.status_message =
                    format!("Saved settings to {}", self.config_path.display())
            }
            Err(error) => {
                eprintln!("[launcher] GUI settings save failed: {error}");
                self.state.status_message = format!("Save failed: {error}");
            }
        }
    }
}

struct LauncherApp {
    ui: LauncherUi,
}

impl LauncherApp {
    fn show_downloader_viewport(&mut self, ctx: &egui::Context) {
        if !self.ui.state.downloader.open {
            return;
        }

        let size = egui::vec2(
            self.ui.layout.downloader_window.0 as f32,
            self.ui.layout.downloader_window.1 as f32,
        );
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("client_downloader"),
            egui::ViewportBuilder::default()
                .with_title("Client Downloader")
                .with_inner_size(size)
                .with_min_inner_size(size),
            |ctx, class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    self.ui.state.downloader.open = false;
                    return;
                }
                if matches!(class, egui::ViewportClass::Embedded) {
                    let mut open = self.ui.state.downloader.open;
                    egui::Window::new("Client Downloader")
                        .open(&mut open)
                        .default_size(size)
                        .show(ctx, |ui| self.downloader_contents(ui));
                    self.ui.state.downloader.open = open;
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.downloader_contents(ui);
                    });
                }
            },
        );
    }

    fn downloader_contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Version:");
            if self.ui.state.downloader.available_versions.is_empty() {
                ui.label("No versions available");
            } else {
                egui::ComboBox::from_id_source("download_version")
                    .selected_text(&self.ui.state.downloader.selected_version)
                    .show_ui(ui, |ui| {
                        for version in &self.ui.state.downloader.available_versions {
                            ui.selectable_value(
                                &mut self.ui.state.downloader.selected_version,
                                version.clone(),
                                version,
                            );
                        }
                    });
            }
        });
        if ui
            .checkbox(
                &mut self.ui.state.downloader.show_all_versions,
                "Show all versions",
            )
            .changed()
        {
            self.ui.state.downloader.sync_available_versions();
        }
        if let Some(error) = &self.ui.state.downloader.versions_error {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        ui.checkbox(
            &mut self.ui.state.downloader.redownload_all_files,
            "Redownload all files",
        );
        if ui
            .add_enabled(
                !self.ui.state.downloader.download_running,
                egui::Button::new("Download"),
            )
            .clicked()
        {
            self.ui.handle_download();
        }
        if self.ui.state.downloader.download_running || self.ui.state.downloader.download_total > 0
        {
            let total = self.ui.state.downloader.download_total.max(1);
            let finished = self.ui.state.downloader.download_finished.min(total);
            let files_remaining = self
                .ui
                .state
                .downloader
                .download_total
                .saturating_sub(self.ui.state.downloader.download_finished);
            ui.separator();
            ui.label(format!("Files remaining: {files_remaining}"));
            ui.add(
                egui::ProgressBar::new(finished as f32 / total as f32)
                    .desired_width(ui.available_width()),
            );
            if let Some(label) = &self.ui.state.downloader.active_download {
                ui.label(label);
            }
        }
        if let Some(error) = &self.ui.state.downloader.download_error {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
    }

    fn show_settings_viewport(&mut self, ctx: &egui::Context) {
        if !self.ui.state.settings.open {
            return;
        }

        let size = egui::vec2(
            self.ui.layout.settings_window.0 as f32,
            self.ui.layout.settings_window.1 as f32,
        );
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("launcher_settings"),
            egui::ViewportBuilder::default()
                .with_title("Vortex Launcher Settings")
                .with_inner_size(size)
                .with_min_inner_size(size),
            |ctx, class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    self.ui.state.settings.open = false;
                    return;
                }
                if matches!(class, egui::ViewportClass::Embedded) {
                    let mut open = self.ui.state.settings.open;
                    egui::Window::new("Vortex Launcher Settings")
                        .open(&mut open)
                        .default_size(size)
                        .show(ctx, |ui| self.settings_contents(ui));
                    self.ui.state.settings.open = open;
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.settings_contents(ui);
                    });
                }
            },
        );
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui) {
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
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui.pump_download_events();
        let launcher_was_hidden = self.ui.launcher_hidden_for_game;
        if self.ui.pump_game_events() && launcher_was_hidden {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if self.ui.state.downloader.download_running || self.ui.game_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

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
                    && self.ui.handle_play(ctx.clone())
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
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
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("by {LAUNCHER_AUTHOR}"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("v{LAUNCHER_VERSION}"));
                    });
                });
            });
        });

        self.show_downloader_viewport(ctx);
        self.show_settings_viewport(ctx);
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
        let (manifest_versions, downloader_versions_error) =
            available_manifest_versions(&profile.game_directory)
                .map(|versions| (versions, None))
                .unwrap_or_else(|error| {
                    (
                        fallback_manifest_versions(&selected_version),
                        Some(format!("Could not load Minecraft versions: {error}")),
                    )
                });
        let available_versions =
            filter_manifest_versions(&manifest_versions, config.show_all_versions);
        let downloader_selected_version = if available_versions
            .iter()
            .any(|version| version == &selected_version)
        {
            selected_version.clone()
        } else {
            available_versions
                .first()
                .cloned()
                .unwrap_or_else(|| selected_version.clone())
        };
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
                selected_version: downloader_selected_version,
                show_all_versions: config.show_all_versions,
                redownload_all_files: config.redownload_all_files,
                available_versions,
                manifest_versions,
                versions_error: downloader_versions_error,
                download_running: false,
                download_total: 0,
                download_finished: 0,
                download_failed: 0,
                active_download: None,
                download_error: None,
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
}

impl DownloaderWindowState {
    fn sync_available_versions(&mut self) {
        self.available_versions =
            filter_manifest_versions(&self.manifest_versions, self.show_all_versions);
        if self
            .available_versions
            .iter()
            .all(|version| version != &self.selected_version)
        {
            if let Some(version) = self.available_versions.first() {
                self.selected_version = version.clone();
            }
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

fn available_manifest_versions(game_directory: &Path) -> io::Result<Vec<ManifestVersion>> {
    let manifest_path = game_directory.join("version_manifest_v2.json");
    if let Ok(manifest) = fs::read_to_string(&manifest_path) {
        let versions = crate::download::parse_versions_manifest(&manifest, true)?;
        if !versions.is_empty() {
            return Ok(versions);
        }
    }

    #[cfg(not(test))]
    {
        let manifest = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .get(crate::download::VERSION_MANIFEST_URL)
            .call()
            .map_err(|error| io::Error::other(error.to_string()))?
            .into_string()
            .map_err(|error| io::Error::other(error.to_string()))?;
        crate::download::parse_versions_manifest(&manifest, true)
    }

    #[cfg(test)]
    {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} not found", manifest_path.display()),
        ))
    }
}

fn filter_manifest_versions(versions: &[ManifestVersion], include_snapshots: bool) -> Vec<String> {
    versions
        .iter()
        .filter(|version| include_snapshots || version.kind == "release")
        .map(|version| version.id.clone())
        .collect()
}

fn fallback_manifest_versions(selected_version: &str) -> Vec<ManifestVersion> {
    vec![ManifestVersion {
        id: selected_version.to_owned(),
        kind: "release".to_owned(),
        url: String::new(),
        sha1: None,
    }]
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
    fn downloader_versions_use_cached_manifest_and_snapshot_toggle() {
        let root = temp_game_dir();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("version_manifest_v2.json"),
            r#"{"versions":[
                {"id":"24w01a","type":"snapshot","url":"https://example/24w01a.json"},
                {"id":"1.21","type":"release","url":"https://example/1.21.json"},
                {"id":"1.20.4","type":"release","url":"https://example/1.20.4.json"}
            ]}"#,
        )
        .unwrap();
        let config = LauncherConfig::default();
        let mut profile = LaunchProfile::from_config(&config);
        profile.game_directory = root.clone();

        let mut state =
            LauncherUiState::from_config(&config, &profile, &RuntimeEnvironment::detect());

        assert_eq!(state.downloader.available_versions, vec!["1.21", "1.20.4"]);
        state.downloader.show_all_versions = true;
        state.downloader.sync_available_versions();
        assert_eq!(
            state.downloader.available_versions,
            vec!["24w01a", "1.21", "1.20.4"]
        );
        assert!(state.downloader.versions_error.is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
