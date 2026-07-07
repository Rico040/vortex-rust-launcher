// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOs,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEnvironment {
    pub os: OperatingSystem,
    pub home_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiLayout {
    pub main_window: (u32, u32),
    pub downloader_window: (u32, u32),
    pub settings_window: (u32, u32),
}

impl RuntimeEnvironment {
    pub fn detect() -> Self {
        Self {
            os: OperatingSystem::current(),
            home_dir: home_dir(),
        }
    }

    pub fn minecraft_directory(&self) -> Option<PathBuf> {
        let home = self.home_dir.as_ref()?;
        Some(match self.os {
            OperatingSystem::Windows => env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                .join(".minecraft"),
            OperatingSystem::Linux | OperatingSystem::Other => home.join(".minecraft"),
            OperatingSystem::MacOs => home
                .join("Library")
                .join("Application Support")
                .join("minecraft"),
        })
    }

    pub fn ui_layout(&self) -> UiLayout {
        match self.os {
            OperatingSystem::Windows => UiLayout {
                main_window: (350, 255),
                downloader_window: (200, 120),
                settings_window: (335, 255),
            },
            OperatingSystem::Linux | OperatingSystem::Other => UiLayout {
                main_window: (350, 280),
                downloader_window: (300, 160),
                settings_window: (350, 315),
            },
            OperatingSystem::MacOs => UiLayout {
                main_window: (350, 245),
                downloader_window: (250, 140),
                settings_window: (350, 315),
            },
        }
    }

    pub fn find_java(&self) -> Option<PathBuf> {
        if let Some(java_home) = env::var_os("JAVA_HOME") {
            let executable = if self.os == OperatingSystem::Windows {
                "java.exe"
            } else {
                "java"
            };
            let candidate = PathBuf::from(java_home).join("bin").join(executable);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        find_on_path(if self.os == OperatingSystem::Windows {
            "java.exe"
        } else {
            "java"
        })
    }
}

impl OperatingSystem {
    pub fn current() -> Self {
        match env::consts::OS {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::MacOs,
            _ => Self::Other,
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(if cfg!(windows) { ';' } else { ':' })
        .map(|entry| PathBuf::from(entry).join(binary))
        .find(|candidate| candidate.exists())
}
