// Vortex Minecraft Launcher - Rust scaffold
// Copyright (C) 2026 Vortex Minecraft Launcher contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

mod config;
mod download;
mod minecraft;
mod platform;
mod ui;

fn main() {
    let runtime = platform::RuntimeEnvironment::detect();
    let config = config::LauncherConfig::load_default().unwrap_or_default();

    let profile = minecraft::LaunchProfile::from_config(&config);
    let download_plan = download::DownloadPlan::for_profile(&profile);

    let mut ui = ui::LauncherUi::new(runtime, config, profile, download_plan);
    ui.run();
}
