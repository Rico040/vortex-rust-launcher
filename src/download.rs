// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::path::PathBuf;

use crate::minecraft::LaunchProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadKind {
    VersionManifest,
    ClientJar,
    Library,
    Asset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    pub kind: DownloadKind,
    pub url: String,
    pub destination: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadPlan {
    pub tasks: Vec<DownloadTask>,
    pub max_parallel_downloads: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadMode {
    MissingLibraries,
    AllFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOptions {
    pub mode: DownloadMode,
    pub include_snapshots: bool,
    pub max_parallel_downloads: usize,
    pub async_download: bool,
}

impl DownloadPlan {
    pub fn for_profile(profile: &LaunchProfile) -> Self {
        Self {
            tasks: vec![DownloadTask {
                kind: DownloadKind::VersionManifest,
                url: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json".to_owned(),
                destination: profile.game_directory.join("version_manifest_v2.json"),
            }],
            max_parallel_downloads: 4,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}
