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
use std::sync::mpsc;

use download::{DownloadJob, DownloadKind, DownloadMode, DownloadOptions};

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
            let mut ui = ui::LauncherUi::new(runtime, config, profile, download_plan);
            ui.run();
            print_help();
            Ok(())
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
    let profile = minecraft::LaunchProfile::from_config(config);
    let version = resolve_latest_alias(&profile.version_id, config.show_all_versions)?;
    let manifest = fetch_manifest(config.show_all_versions)?;
    let game_dir = &profile.game_directory;
    fs::create_dir_all(game_dir)?;

    let mut jobs = Vec::new();
    jobs.push(DownloadJob::new(
        DownloadKind::VersionManifest,
        download::VERSION_MANIFEST_URL,
        game_dir.join("version_manifest_v2.json"),
        "version manifest",
    ));
    jobs.push(
        download::version_json_job(&manifest, game_dir, &version).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("version '{version}' not found"),
            )
        })?,
    );
    run_jobs(jobs, config)?;

    let version_json = minecraft::MinecraftVersionJson::load_resolved(game_dir, &version)?;
    let mut jobs = Vec::new();
    if let Some(job) = download::client_jar_job(&version_json, game_dir) {
        jobs.push(job);
    }
    for lib in download::parse_libraries(&version_json, game_dir) {
        if let Some(job) = lib.artifact {
            jobs.push(job);
        }
        if let Some(job) = lib.native {
            jobs.push(job);
        }
    }
    if let Some(index) = version_json.asset_index.as_ref() {
        if let (Some(id), Some(url)) = (&index.id, &index.url) {
            jobs.push(
                DownloadJob::new(
                    DownloadKind::AssetIndex,
                    url,
                    game_dir
                        .join("assets")
                        .join("indexes")
                        .join(format!("{id}.json")),
                    format!("asset index {id}"),
                )
                .with_integrity(index.sha1.clone(), index.size),
            );
        }
    }
    if let Some(log) = version_json
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .and_then(|c| c.file.as_ref())
    {
        if let Some(url) = &log.url {
            jobs.push(
                DownloadJob::new(
                    DownloadKind::LogConfig,
                    url,
                    game_dir
                        .join("assets")
                        .join("log_configs")
                        .join(url.rsplit('/').next().unwrap_or("client-logging.xml")),
                    "client logging config",
                )
                .with_integrity(log.sha1.clone(), log.size),
            );
        }
    }
    run_jobs(jobs, config)?;

    if let Some(index_id) = version_json
        .asset_index
        .as_ref()
        .and_then(|i| i.id.as_ref())
    {
        let index_path = game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{index_id}.json"));
        if index_path.exists() {
            let assets = download::assets_to_resources(&fs::read_to_string(index_path)?, game_dir)?;
            run_jobs(assets.into_iter().map(|a| a.download).collect(), config)?;
        }
    }
    println!("Downloaded Minecraft {version} into {}", game_dir.display());
    Ok(())
}

fn run_jobs(jobs: Vec<DownloadJob>, config: &config::LauncherConfig) -> io::Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }
    let (tx, rx) = mpsc::channel();
    let options = DownloadOptions {
        mode: if config.redownload_all_files {
            DownloadMode::AllFiles
        } else {
            DownloadMode::MissingLibraries
        },
        include_snapshots: config.show_all_versions,
        max_parallel_downloads: config.download_threads,
        async_download: false,
    };
    let summary = download::execute_downloads(jobs, options.max_parallel_downloads, tx);
    for event in rx.try_iter() {
        println!("{event:?}");
    }
    if summary.failed == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} downloads failed",
            summary.failed
        )))
    }
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
