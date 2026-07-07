// Vortex Minecraft Launcher - Rust scaffold
// SPDX-License-Identifier: GPL-3.0-only

use crate::config::LauncherConfig;
use crate::download::DownloadPlan;
use crate::minecraft::LaunchProfile;
use crate::platform::RuntimeEnvironment;

#[derive(Debug)]
pub struct LauncherUi {
    runtime: RuntimeEnvironment,
    config: LauncherConfig,
    profile: LaunchProfile,
    download_plan: DownloadPlan,
}

impl LauncherUi {
    pub fn new(
        runtime: RuntimeEnvironment,
        config: LauncherConfig,
        profile: LaunchProfile,
        download_plan: DownloadPlan,
    ) -> Self {
        Self {
            runtime,
            config,
            profile,
            download_plan,
        }
    }

    pub fn run(&self) {
        println!("Vortex Minecraft Launcher Rust scaffold");
        println!("Detected platform: {:?}", self.runtime.os);
        println!("Selected version: {}", self.profile.version_id);
        println!("Player name: {}", self.profile.username);
        println!(
            "Configured Java: {}",
            self.profile
                .java_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "not found".to_owned())
        );
        println!("Queued download tasks: {}", self.download_plan.tasks.len());
        if let Some(game_dir) = self.runtime.minecraft_directory() {
            println!("Default Minecraft directory: {}", game_dir.display());
        }
        if self.config.selected_version.is_none() {
            println!("No version configured yet; GUI implementation will prompt for one.");
        }
    }
}
