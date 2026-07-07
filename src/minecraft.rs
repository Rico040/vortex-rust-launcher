// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::path::PathBuf;

use crate::config::LauncherConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionMetadata {
    pub id: String,
    pub main_class: Option<String>,
    pub libraries: Vec<Library>,
    pub assets_index: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub name: String,
    pub path: PathBuf,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProfile {
    pub username: String,
    pub version_id: String,
    pub game_directory: PathBuf,
    pub java_path: Option<PathBuf>,
    pub memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

impl LaunchProfile {
    pub fn from_config(config: &LauncherConfig) -> Self {
        Self {
            username: config
                .username
                .clone()
                .unwrap_or_else(|| "Player".to_owned()),
            version_id: config
                .selected_version
                .clone()
                .unwrap_or_else(|| "latest".to_owned()),
            game_directory: config
                .game_directory
                .clone()
                .unwrap_or_else(|| PathBuf::from(".minecraft")),
            java_path: config.java_path.clone(),
            memory_mb: config.memory_mb.unwrap_or(2048),
            jvm_args: config.extra_jvm_args.clone(),
            game_args: config.extra_game_args.clone(),
        }
    }

    pub fn launch_arguments(&self, metadata: &VersionMetadata) -> Vec<String> {
        let mut args = vec![format!("-Xmx{}M", self.memory_mb)];
        args.extend(self.jvm_args.clone());
        if let Some(main_class) = &metadata.main_class {
            args.push(main_class.clone());
        }
        args.extend([
            "--username".to_owned(),
            self.username.clone(),
            "--version".to_owned(),
            self.version_id.clone(),
            "--gameDir".to_owned(),
            self.game_directory.display().to_string(),
        ]);
        args.extend(self.game_args.clone());
        args
    }
}
