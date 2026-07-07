// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::path::{Path, PathBuf};

const MODERN_JVM_ARGUMENTS: &str = "-Xss1M -XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";
const OLD_JVM_ARGUMENTS: &str =
    "-XX:+UseConcMarkSweepGC -XX:+CMSIncrementalMode -XX:-UseAdaptiveSizePolicy -Xmn128M";
#[cfg(target_os = "macos")]
const MACOS_MODERN_JVM_ARGUMENTS: &str = "-XstartOnFirstThread -Xss1M -XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";
#[cfg(target_os = "macos")]
const MACOS_OLD_JVM_ARGUMENTS: &str = "-XstartOnFirstThread -XX:+UseConcMarkSweepGC -XX:+CMSIncrementalMode -XX:-UseAdaptiveSizePolicy -Xmn128M";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformDefaults {
    pub working_directory: PathBuf,
    pub default_java_executable_path: PathBuf,
    pub default_ram_mb: u32,
    pub default_download_threads: usize,
    pub default_async_download: bool,
    pub default_download_missing_libraries: bool,
    pub default_modern_jvm_arguments: String,
    pub default_old_jvm_arguments: String,
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

        match self.os {
            OperatingSystem::Windows => discover_windows_java()
                .or_else(|| find_on_path("javaw.exe"))
                .or_else(|| find_on_path("java.exe")),
            OperatingSystem::MacOs => discover_macos_java().or_else(|| find_on_path("java")),
            OperatingSystem::Linux | OperatingSystem::Other => find_on_path("java"),
        }
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

#[cfg(target_os = "windows")]
pub fn current_platform_defaults() -> PlatformDefaults {
    PlatformDefaults {
        working_directory: executable_directory(),
        default_java_executable_path: discover_windows_java()
            .unwrap_or_else(|| PathBuf::from(r"C:\jre8\bin\javaw.exe")),
        default_ram_mb: 2500,
        default_download_threads: 20,
        default_async_download: true,
        default_download_missing_libraries: true,
        default_modern_jvm_arguments: MODERN_JVM_ARGUMENTS.to_owned(),
        default_old_jvm_arguments: OLD_JVM_ARGUMENTS.to_owned(),
    }
}

#[cfg(target_os = "linux")]
pub fn current_platform_defaults() -> PlatformDefaults {
    PlatformDefaults {
        working_directory: executable_directory(),
        default_java_executable_path: configured_java_path()
            .unwrap_or_else(|| PathBuf::from("java")),
        default_ram_mb: 2500,
        default_download_threads: 20,
        default_async_download: true,
        default_download_missing_libraries: true,
        default_modern_jvm_arguments: MODERN_JVM_ARGUMENTS.to_owned(),
        default_old_jvm_arguments: OLD_JVM_ARGUMENTS.to_owned(),
    }
}

#[cfg(target_os = "macos")]
pub fn current_platform_defaults() -> PlatformDefaults {
    PlatformDefaults {
        working_directory: home_dir()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("minecraft_vlauncher")
            })
            .unwrap_or_else(executable_directory),
        default_java_executable_path: configured_java_path()
            .or_else(discover_macos_java)
            .unwrap_or_else(|| PathBuf::from("java")),
        default_ram_mb: 700,
        default_download_threads: 10,
        default_async_download: false,
        default_download_missing_libraries: false,
        default_modern_jvm_arguments: MACOS_MODERN_JVM_ARGUMENTS.to_owned(),
        default_old_jvm_arguments: MACOS_OLD_JVM_ARGUMENTS.to_owned(),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn current_platform_defaults() -> PlatformDefaults {
    PlatformDefaults {
        working_directory: executable_directory(),
        default_java_executable_path: configured_java_path()
            .unwrap_or_else(|| PathBuf::from("java")),
        default_ram_mb: 2500,
        default_download_threads: 20,
        default_async_download: true,
        default_download_missing_libraries: true,
        default_modern_jvm_arguments: MODERN_JVM_ARGUMENTS.to_owned(),
        default_old_jvm_arguments: OLD_JVM_ARGUMENTS.to_owned(),
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn executable_directory() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn configured_java_path() -> Option<PathBuf> {
    env::var_os("VORTEX_JAVA_PATH")
        .or_else(|| env::var_os("JAVA_PATH"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn discover_windows_java() -> Option<PathBuf> {
    let program_files_dirs = [env::var_os("ProgramW6432"), env::var_os("PROGRAMFILES")];
    let java_dirs = ["Java", "Eclipse Adoptium"];

    for program_files_dir in program_files_dirs.into_iter().flatten() {
        let program_files_dir = PathBuf::from(program_files_dir);
        if program_files_dir.as_os_str().is_empty() {
            continue;
        }

        for java_dir in java_dirs {
            let root = program_files_dir.join(java_dir);
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let candidate = entry.path().join("bin").join("javaw.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn discover_macos_java() -> Option<PathBuf> {
    let legacy_plugin = PathBuf::from("/Library")
        .join("Internet Plug-ins")
        .join("JavaAppletPlugin.plugin")
        .join("Contents")
        .join("Home")
        .join("bin")
        .join("java");

    legacy_plugin.is_file().then_some(legacy_plugin)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(if cfg!(windows) { ';' } else { ':' })
        .map(|entry| PathBuf::from(entry).join(binary))
        .find(|candidate| candidate.exists())
}
