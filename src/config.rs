// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_FILE: &str = "vortex_launcher.conf";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LauncherConfig {
    pub username: Option<String>,
    pub game_directory: Option<PathBuf>,
    pub java_path: Option<PathBuf>,
    pub selected_version: Option<String>,
    pub memory_mb: Option<u32>,
    pub extra_jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
}

impl LauncherConfig {
    pub fn load_default() -> io::Result<Self> {
        Self::load_from(DEFAULT_CONFIG_FILE)
    }

    pub fn load_from(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        Self::from_str(&fs::read_to_string(path)?)
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(self.to_config_string().as_bytes())
    }

    pub fn from_str(contents: &str) -> io::Result<Self> {
        let mut values = BTreeMap::new();
        for line in contents.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }

        Ok(Self {
            username: values.remove("username").filter(|value| !value.is_empty()),
            game_directory: values.remove("game_directory").map(PathBuf::from),
            java_path: values.remove("java_path").map(PathBuf::from),
            selected_version: values
                .remove("selected_version")
                .filter(|value| !value.is_empty()),
            memory_mb: values
                .remove("memory_mb")
                .and_then(|value| value.parse::<u32>().ok()),
            extra_jvm_args: split_args(values.remove("extra_jvm_args")),
            extra_game_args: split_args(values.remove("extra_game_args")),
        })
    }

    pub fn to_config_string(&self) -> String {
        let mut lines = vec!["# Vortex Minecraft Launcher configuration".to_owned()];
        push_optional(&mut lines, "username", self.username.as_deref());
        push_optional_path(&mut lines, "game_directory", self.game_directory.as_deref());
        push_optional_path(&mut lines, "java_path", self.java_path.as_deref());
        push_optional(
            &mut lines,
            "selected_version",
            self.selected_version.as_deref(),
        );
        if let Some(memory_mb) = self.memory_mb {
            lines.push(format!("memory_mb={memory_mb}"));
        }
        if !self.extra_jvm_args.is_empty() {
            lines.push(format!("extra_jvm_args={}", self.extra_jvm_args.join(" ")));
        }
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
