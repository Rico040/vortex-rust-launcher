// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::platform::{current_platform_defaults, PlatformDefaults};

pub const DEFAULT_CONFIG_FILE: &str = "vortex_launcher.conf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherConfig {
    pub username: Option<String>,
    pub game_directory: Option<PathBuf>,
    pub java_path: Option<PathBuf>,
    pub selected_version: Option<String>,
    pub memory_mb: Option<u32>,
    pub extra_jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
    pub download_missing_libraries: bool,
    pub redownload_all_files: bool,
    pub show_all_versions: bool,
    pub download_threads: usize,
    pub async_download: bool,
    pub use_custom_java: bool,
    pub use_custom_jvm_parameters: bool,
    pub save_launch_string: bool,
    pub keep_launcher_open: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self::with_platform_defaults(&current_platform_defaults())
    }
}

impl LauncherConfig {
    pub fn with_platform_defaults(defaults: &PlatformDefaults) -> Self {
        Self {
            username: None,
            game_directory: Some(defaults.working_directory.clone()),
            java_path: Some(defaults.default_java_executable_path.clone()),
            selected_version: None,
            memory_mb: Some(defaults.default_ram_mb),
            extra_jvm_args: split_args(Some(defaults.default_modern_jvm_arguments.clone())),
            extra_game_args: Vec::new(),
            download_missing_libraries: defaults.default_download_missing_libraries,
            redownload_all_files: false,
            show_all_versions: false,
            download_threads: defaults.default_download_threads,
            async_download: defaults.default_async_download,
            use_custom_java: false,
            use_custom_jvm_parameters: false,
            save_launch_string: false,
            keep_launcher_open: false,
        }
    }

    pub fn load_default() -> io::Result<Self> {
        Self::load(DEFAULT_CONFIG_FILE)
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let defaults = current_platform_defaults();
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::with_platform_defaults(&defaults));
        }

        Self::from_str_with_defaults(&fs::read_to_string(path)?, &defaults)
    }

    pub fn load_from(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::load(path)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(self.to_config_string().as_bytes())
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        self.save(path)
    }

    pub fn from_str(contents: &str) -> io::Result<Self> {
        Self::from_str_with_defaults(contents, &current_platform_defaults())
    }

    pub fn from_str_with_defaults(contents: &str, defaults: &PlatformDefaults) -> io::Result<Self> {
        let mut config = Self::with_platform_defaults(defaults);
        let mut values = BTreeMap::new();
        for line in contents.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }

        config.username = take_string(&mut values, &["Name", "username"]);
        config.game_directory = take_string(&mut values, &["game_directory"]).map(PathBuf::from);
        config.java_path = take_string(&mut values, &["JavaPath", "java_path"]).map(PathBuf::from);
        config.selected_version = take_string(&mut values, &["ChosenVer", "selected_version"]);
        if let Some(value) = take_string(&mut values, &["Ram", "memory_mb"]) {
            config.memory_mb = value.parse::<u32>().ok();
        }
        if let Some(value) = take_string(&mut values, &["CustomParams", "extra_jvm_args"]) {
            config.extra_jvm_args = split_args(Some(value));
        }
        if let Some(value) = take_string(&mut values, &["extra_game_args"]) {
            config.extra_game_args = split_args(Some(value));
        }
        config.download_missing_libraries = take_bool(
            &mut values,
            &["DownloadMissingLibs", "download_missing_libraries"],
            config.download_missing_libraries,
        );
        config.redownload_all_files = take_bool(
            &mut values,
            &["DownloadAllFiles", "redownload_all_files"],
            config.redownload_all_files,
        );
        config.show_all_versions = take_bool(
            &mut values,
            &["VersionsType", "show_all_versions"],
            config.show_all_versions,
        );
        if let Some(value) = take_string(&mut values, &["DownloadThreads", "download_threads"]) {
            config.download_threads = value.parse::<usize>().unwrap_or(config.download_threads);
        }
        config.async_download = take_bool(
            &mut values,
            &["AsyncDownload", "async_download"],
            config.async_download,
        );
        config.use_custom_java = take_bool(
            &mut values,
            &["UseCustomJava", "use_custom_java"],
            config.use_custom_java,
        );
        config.use_custom_jvm_parameters = take_bool(
            &mut values,
            &["UseCustomParams", "use_custom_jvm_parameters"],
            config.use_custom_jvm_parameters,
        );
        config.save_launch_string = take_bool(
            &mut values,
            &["SaveLaunchString", "save_launch_string"],
            config.save_launch_string,
        );
        config.keep_launcher_open = take_bool(
            &mut values,
            &["KeepLauncherOpen", "keep_launcher_open"],
            config.keep_launcher_open,
        );

        Ok(config)
    }

    pub fn to_config_string(&self) -> String {
        let mut lines = vec!["# Vortex Minecraft Launcher configuration".to_owned()];
        push_optional(&mut lines, "Name", self.username.as_deref());
        if let Some(memory_mb) = self.memory_mb {
            lines.push(format!("Ram={memory_mb}"));
        }
        push_optional(&mut lines, "ChosenVer", self.selected_version.as_deref());
        lines.push(format!("DownloadThreads={}", self.download_threads));
        lines.push(format!("AsyncDownload={}", self.async_download));
        lines.push(format!(
            "DownloadMissingLibs={}",
            self.download_missing_libraries
        ));
        lines.push(format!("DownloadAllFiles={}", self.redownload_all_files));
        lines.push(format!("VersionsType={}", self.show_all_versions));
        lines.push(format!("SaveLaunchString={}", self.save_launch_string));
        lines.push(format!("UseCustomJava={}", self.use_custom_java));
        push_optional_path(&mut lines, "JavaPath", self.java_path.as_deref());
        lines.push(format!(
            "UseCustomParams={}",
            self.use_custom_jvm_parameters
        ));
        if !self.extra_jvm_args.is_empty() {
            lines.push(format!("CustomParams={}", self.extra_jvm_args.join(" ")));
        }
        lines.push(format!("KeepLauncherOpen={}", self.keep_launcher_open));
        push_optional_path(&mut lines, "game_directory", self.game_directory.as_deref());
        if !self.extra_game_args.is_empty() {
            lines.push(format!(
                "extra_game_args={}",
                self.extra_game_args.join(" ")
            ));
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

fn split_args(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn take_string(values: &mut BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| values.remove(*key))
        .filter(|value| !value.is_empty())
}

fn take_bool(values: &mut BTreeMap<String, String>, keys: &[&str], default: bool) -> bool {
    take_string(values, keys)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn push_optional(lines: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{key}={value}"));
    }
}

fn push_optional_path(lines: &mut Vec<String>, key: &str, value: Option<&Path>) {
    if let Some(value) = value.and_then(Path::to_str) {
        lines.push(format!("{key}={value}"));
    }
}
