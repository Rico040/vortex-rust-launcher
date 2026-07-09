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
    eprintln!("[launcher] Starting Vortex Minecraft Launcher");
    let mut config = match config::LauncherConfig::load_default() {
        Ok(config) => {
            eprintln!(
                "[launcher] Loaded configuration from {}",
                config::DEFAULT_CONFIG_FILE
            );
            config
        }
        Err(error) => {
            eprintln!(
                "[launcher] Could not load {}: {error}; using platform defaults",
                config::DEFAULT_CONFIG_FILE
            );
            config::LauncherConfig::default()
        }
    };
    log_config_summary(&config);
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("versions") => {
            eprintln!("[launcher] Command: versions");
            list_versions(config.show_all_versions)
        }
        Some("download") => {
            let version = args
                .next()
                .or_else(|| config.selected_version.clone())
                .unwrap_or_else(|| "latest".to_owned());
            eprintln!("[launcher] Command: download {version}");
            config.selected_version =
                Some(resolve_latest_alias(&version, config.show_all_versions)?);
            download_selected_version(&config)
        }
        Some("set") => {
            eprintln!("[launcher] Command: set");
            apply_setting(&mut config, args.collect::<Vec<_>>())?;
            config.save_default()
        }
        Some("launch") => {
            eprintln!("[launcher] Command: launch");
            launch_selected_version(&config)
        }
        Some("help") | Some("--help") | Some("-h") => {
            eprintln!("[launcher] Command: help");
            print_help();
            Ok(())
        }
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command '{other}'. Run with 'help' for usage."),
        )),
        None => {
            eprintln!("[launcher] Command: gui");
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
    eprintln!(
        "[launcher] Launch profile: version={}, player={}, game_dir={}, memory={}M",
        profile.version_id,
        profile.username,
        profile.game_directory.display(),
        profile.memory_mb
    );
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
    eprintln!("[launcher] Updated setting {}", args[0]);
    Ok(())
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes" | "on")
}

fn resolve_latest_alias(version: &str, include_snapshots: bool) -> io::Result<String> {
    if version != "latest" {
        return Ok(version.to_owned());
    }
    eprintln!("[launcher] Resolving latest version alias; include_snapshots={include_snapshots}");
    fetch_manifest(include_snapshots)?
        .into_iter()
        .next()
        .map(|v| v.id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no versions in manifest"))
}

fn fetch_manifest(include_snapshots: bool) -> io::Result<Vec<download::ManifestVersion>> {
    eprintln!("[launcher] Fetching version manifest with curl");
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
    let versions = download::parse_versions_manifest(
        &String::from_utf8_lossy(&output.stdout),
        include_snapshots,
    )?;
    eprintln!("[launcher] Parsed {} manifest versions", versions.len());
    Ok(versions)
}

fn log_config_summary(config: &config::LauncherConfig) {
    eprintln!(
        "[launcher] Config summary: version={}, player={}, game_dir={}, memory={}M, custom_java={}, custom_jvm_args={}, extra_game_args={}, download_threads={}, snapshots={}, redownload_all={}",
        config.selected_version.as_deref().unwrap_or("latest"),
        config.username.as_deref().unwrap_or("Player"),
        config
            .game_directory
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".minecraft".to_owned()),
        config.memory_mb.unwrap_or(2048),
        config.use_custom_java,
        config.extra_jvm_args.len(),
        config.extra_game_args.len(),
        config.download_threads,
        config.show_all_versions,
        config.redownload_all_files
    );
}
