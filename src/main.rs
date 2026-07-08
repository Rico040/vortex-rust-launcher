// Vortex Minecraft Launcher
// Copyright (C) 2026 Vortex Minecraft Launcher contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

mod config;
mod download;
mod launch;
mod minecraft;
mod platform;
mod ui;

use std::fs;
use std::io;
use std::process::Command;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let mut config = config::LauncherConfig::load_default().unwrap_or_default();
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("versions") => list_versions(config.show_all_versions),
        Some("download") => {
            let version = args
                .next()
                .or_else(|| config.selected_version.clone())
                .unwrap_or_else(|| "latest".to_owned());
            config.selected_version =
                Some(resolve_latest_alias(&version, config.show_all_versions)?);
            download_selected_version(&config)
        }
        Some("set") => {
            apply_setting(&mut config, args.collect::<Vec<_>>())?;
            config.save_default()
        }
        Some("launch") => launch_selected_version(&config),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command '{other}'. Run with 'help' for usage."),
        )),
        None => {
            let runtime = platform::RuntimeEnvironment::detect();
            let profile = minecraft::LaunchProfile::from_config(&config);
            let download_plan = download::DownloadPlan::for_profile(&profile);
            let ui = ui::LauncherUi::new(runtime, config, profile, download_plan);
            ui.run()
                .map_err(|error| io::Error::other(error.to_string()))
        }
    }
}

fn print_help() {
    println!(
        "\nCommands:\n  versions                 Discover Minecraft versions\n  download [version]       Download a version, libraries, assets, and logging files\n  set <key> <value>        Configure name, ram, version, java, threads, snapshots, async-download, download-missing-libs, save-launch-string, keep-open\n  launch                   Launch the configured Minecraft version"
    );
}

fn list_versions(include_snapshots: bool) -> io::Result<()> {
    for version in fetch_manifest(include_snapshots)? {
        println!("{}\t{}", version.id, version.kind);
    }
    Ok(())
}

fn download_selected_version(config: &config::LauncherConfig) -> io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let result = download::download_selected_version(config, tx);
    for event in rx.try_iter() {
        println!("{event:?}");
    }
    let version = result?;
    let game_dir = minecraft::LaunchProfile::from_config(config).game_directory;
    println!("Downloaded Minecraft {version} into {}", game_dir.display());
    Ok(())
}

fn launch_selected_version(config: &config::LauncherConfig) -> io::Result<()> {
    let profile = minecraft::LaunchProfile::from_config(config);
    let command = profile.launch_command(config.save_launch_string)?;
    let outcome = launch::launch_minecraft(
        &command,
        launch::LaunchOptions {
            keep_launcher_open: config.keep_launcher_open,
            save_launch_string: config.save_launch_string,
        },
    )?;
    if let Some(display) = outcome.display_command {
        fs::write("launch_string.txt", display)?;
    }
    println!("Launched Minecraft with pid {}", outcome.child.id());
    Ok(())
}

fn apply_setting(config: &mut config::LauncherConfig, args: Vec<String>) -> io::Result<()> {
    if args.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: set <key> <value>",
        ));
    }
    match args[0].as_str() {
        "name" => config.username = Some(args[1].clone()),
        "ram" => config.memory_mb = args[1].parse().ok(),
        "version" => config.selected_version = Some(args[1].clone()),
        "java" => {
            config.java_path = Some(args[1].clone().into());
            config.use_custom_java = true;
        }
        "threads" => config.download_threads = args[1].parse().unwrap_or(config.download_threads),
        "snapshots" => config.show_all_versions = parse_bool(&args[1]),
        "async-download" => config.async_download = parse_bool(&args[1]),
        "download-missing-libs" => config.download_missing_libraries = parse_bool(&args[1]),
        "save-launch-string" => config.save_launch_string = parse_bool(&args[1]),
        "keep-open" => config.keep_launcher_open = parse_bool(&args[1]),
        key => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown setting '{key}'"),
            ))
        }
    }
    Ok(())
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes" | "on")
}

fn resolve_latest_alias(version: &str, include_snapshots: bool) -> io::Result<String> {
    if version != "latest" {
        return Ok(version.to_owned());
    }
    fetch_manifest(include_snapshots)?
        .into_iter()
        .next()
        .map(|v| v.id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no versions in manifest"))
}

fn fetch_manifest(include_snapshots: bool) -> io::Result<Vec<download::ManifestVersion>> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            download::VERSION_MANIFEST_URL,
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    download::parse_versions_manifest(&String::from_utf8_lossy(&output.stdout), include_snapshots)
}
