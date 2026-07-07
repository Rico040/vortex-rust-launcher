// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::minecraft::{
    rules_apply_with_context, DownloadInfo, LaunchContext, LaunchProfile, LibraryJson,
};
use serde::Deserialize;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const LIBRARIES_BASE_URL: &str = "https://libraries.minecraft.net/";
pub const ASSET_OBJECTS_BASE_URL: &str = "https://resources.download.minecraft.net/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadKind {
    VersionManifest,
    VersionJson,
    ClientJar,
    Library,
    NativeLibrary,
    AssetIndex,
    Asset,
    LogConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadJob {
    pub kind: DownloadKind,
    pub url: String,
    pub destination: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    pub label: String,
}

pub type DownloadTask = DownloadJob;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadPlan {
    pub tasks: Vec<DownloadJob>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVersion {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLibrary {
    pub name: String,
    pub classpath_path: Option<PathBuf>,
    pub artifact: Option<DownloadJob>,
    pub native: Option<DownloadJob>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetResource {
    pub name: String,
    pub hash: String,
    pub object_path: PathBuf,
    pub resource_path: PathBuf,
    pub download: DownloadJob,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEvent {
    Started {
        total: usize,
    },
    JobStarted {
        index: usize,
        label: String,
    },
    JobProgress {
        index: usize,
        downloaded: u64,
        total: Option<u64>,
    },
    JobFinished {
        index: usize,
        destination: PathBuf,
    },
    JobFailed {
        index: usize,
        label: String,
        error: String,
    },
    Finished {
        succeeded: usize,
        failed: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSummary {
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug)]
pub enum DownloadError {
    Io(io::Error),
    HttpStatus { url: String, status: u16 },
    Network { url: String, message: String },
    SizeMismatch { expected: u64, actual: u64 },
    Sha1Mismatch { expected: String, actual: String },
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::HttpStatus { url, status } => {
                write!(f, "HTTP request to {url} failed with status {status}")
            }
            Self::Network { url, message } => {
                write!(f, "network request to {url} failed: {message}")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(f, "size mismatch: expected {expected}, got {actual}")
            }
            Self::Sha1Mismatch { expected, actual } => {
                write!(f, "sha1 mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for DownloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DownloadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl DownloadJob {
    pub fn new(
        kind: DownloadKind,
        url: impl Into<String>,
        destination: PathBuf,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            url: url.into(),
            destination,
            expected_sha1: None,
            expected_size: None,
            label: label.into(),
        }
    }

    pub fn with_integrity(mut self, sha1: Option<String>, size: Option<u64>) -> Self {
        self.expected_sha1 = sha1;
        self.expected_size = size;
        self
    }
}

impl DownloadPlan {
    pub fn for_profile(profile: &LaunchProfile) -> Self {
        Self {
            tasks: vec![DownloadJob::new(
                DownloadKind::VersionManifest,
                VERSION_MANIFEST_URL,
                profile.game_directory.join("version_manifest_v2.json"),
                "version manifest",
            )],
            max_parallel_downloads: 4,
        }
    }

    pub fn from_config(profile: &LaunchProfile, options: DownloadOptions) -> Self {
        let game_dir = &profile.game_directory;
        let mut plan = Self {
            tasks: Vec::new(),
            max_parallel_downloads: options.max_parallel_downloads.max(1),
        };
        let manifest_path = game_dir.join("version_manifest_v2.json");
        plan.push_if_needed(
            DownloadJob::new(
                DownloadKind::VersionManifest,
                VERSION_MANIFEST_URL,
                manifest_path.clone(),
                "version manifest",
            ),
            options.mode,
            true,
        );

        let Ok(manifest_text) = fs::read_to_string(&manifest_path) else {
            return plan;
        };
        let Ok(manifest) = parse_versions_manifest(&manifest_text, options.include_snapshots)
        else {
            return plan;
        };
        let version = resolve_selected_version(&manifest, &profile.version_id);
        let Some(version_job) = version_json_job(&manifest, game_dir, &version) else {
            return plan;
        };
        plan.push_if_needed(version_job, options.mode, false);

        let Ok(version_json) =
            crate::minecraft::MinecraftVersionJson::load_resolved(game_dir, &version)
        else {
            return plan;
        };
        if let Some(job) = client_jar_job(&version_json, game_dir) {
            plan.push_if_needed(job, options.mode, false);
        }
        for lib in parse_libraries(&version_json, game_dir) {
            if let Some(job) = lib.artifact {
                plan.push_if_needed(job, options.mode, false);
            }
            if let Some(job) = lib.native {
                plan.push_if_needed(job, options.mode, false);
            }
        }
        if let Some(job) = asset_index_job(&version_json, game_dir) {
            let index_path = job.destination.clone();
            plan.push_if_needed(job, options.mode, false);
            if let Ok(index_json) = fs::read_to_string(index_path) {
                if let Ok(assets) = assets_to_resources(&index_json, game_dir) {
                    for asset in assets {
                        plan.push_if_needed(asset.download, options.mode, false);
                    }
                }
            }
        }
        if let Some(job) = logging_config_job(&version_json, game_dir) {
            plan.push_if_needed(job, options.mode, false);
        }
        plan
    }

    fn push_if_needed(&mut self, job: DownloadJob, mode: DownloadMode, stale_manifest: bool) {
        let should_download = if mode == DownloadMode::AllFiles {
            true
        } else if stale_manifest {
            file_is_missing_or_older_than(&job.destination, Duration::from_secs(60 * 60 * 24))
        } else {
            !existing_file_is_valid(&job)
        };
        if should_download {
            self.tasks.push(job);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct VersionManifestJson {
    versions: Vec<VersionManifestEntry>,
}
#[derive(Debug, Deserialize)]
struct VersionManifestEntry {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    sha1: Option<String>,
}

pub fn parse_versions_manifest(
    manifest: &str,
    include_snapshots: bool,
) -> io::Result<Vec<ManifestVersion>> {
    let parsed: VersionManifestJson = serde_json::from_str(manifest).map_err(invalid_data)?;
    Ok(parsed
        .versions
        .into_iter()
        .filter(|v| include_snapshots || v.kind == "release")
        .map(|v| ManifestVersion {
            id: v.id,
            kind: v.kind,
            url: v.url,
            sha1: v.sha1,
        })
        .collect())
}

pub fn version_json_job(
    manifest: &[ManifestVersion],
    game_dir: &Path,
    version: &str,
) -> Option<DownloadJob> {
    manifest.iter().find(|v| v.id == version).map(|v| {
        DownloadJob::new(
            DownloadKind::VersionJson,
            &v.url,
            game_dir
                .join("versions")
                .join(version)
                .join(format!("{version}.json")),
            format!("version {version} json"),
        )
        .with_integrity(v.sha1.clone(), None)
    })
}

pub fn client_jar_job(
    version: &crate::minecraft::MinecraftVersionJson,
    game_dir: &Path,
) -> Option<DownloadJob> {
    let id = version.jar.as_deref().or(version.id.as_deref())?;
    let client = version.downloads.as_ref()?.client.as_ref()?;
    download_info_job(
        DownloadKind::ClientJar,
        client,
        game_dir.join("versions").join(id).join(format!("{id}.jar")),
        format!("client jar {id}"),
    )
}

pub fn asset_index_job(
    version: &crate::minecraft::MinecraftVersionJson,
    game_dir: &Path,
) -> Option<DownloadJob> {
    let index = version.asset_index.as_ref()?;
    let id = index.id.as_deref().or(version.assets.as_deref())?;
    let url = index.url.as_ref()?;
    Some(
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
    )
}

pub fn logging_config_job(
    version: &crate::minecraft::MinecraftVersionJson,
    game_dir: &Path,
) -> Option<DownloadJob> {
    let file = version.logging.as_ref()?.client.as_ref()?.file.as_ref()?;
    let url = file.url.as_ref()?;
    Some(
        DownloadJob::new(
            DownloadKind::LogConfig,
            url,
            game_dir
                .join("assets")
                .join("log_configs")
                .join(url.rsplit('/').next().unwrap_or("client-logging.xml")),
            "client logging config",
        )
        .with_integrity(file.sha1.clone(), file.size),
    )
}

pub fn parse_libraries(
    version: &crate::minecraft::MinecraftVersionJson,
    game_dir: &Path,
) -> Vec<ResolvedLibrary> {
    let libraries_dir = game_dir.join("libraries");
    version
        .libraries
        .iter()
        .filter(|lib| rules_apply_with_context(&lib.rules, &LaunchContext::current()))
        .map(|lib| resolve_library(lib, &libraries_dir))
        .collect()
}

fn resolve_library(lib: &LibraryJson, libraries_dir: &Path) -> ResolvedLibrary {
    let artifact_path = lib
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .and_then(|a| a.path.clone())
        .unwrap_or_else(|| maven_path(&lib.name));
    let artifact_url = lib
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .and_then(|a| a.url.clone())
        .or_else(|| {
            lib.url
                .as_ref()
                .map(|base| format!("{}{}", ensure_slash(base), artifact_path))
        })
        .unwrap_or_else(|| format!("{LIBRARIES_BASE_URL}{artifact_path}"));
    let artifact_info = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
    let artifact = Some(
        DownloadJob::new(
            DownloadKind::Library,
            artifact_url,
            libraries_dir.join(&artifact_path),
            lib.name.clone(),
        )
        .with_integrity(
            artifact_info.and_then(|a| a.sha1.clone()),
            artifact_info.and_then(|a| a.size),
        ),
    );
    let native_classifier = native_classifier_for_library(lib);
    let native = native_classifier.as_deref().and_then(|native_classifier| {
        lib.downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.get(native_classifier))
            .and_then(|info| {
                let path = info
                    .path
                    .clone()
                    .unwrap_or_else(|| classifier_path(&lib.name, native_classifier));
                download_info_job(
                    DownloadKind::NativeLibrary,
                    info,
                    libraries_dir.join(path),
                    format!("{} {native_classifier}", lib.name),
                )
            })
    });
    ResolvedLibrary {
        name: lib.name.clone(),
        classpath_path: Some(libraries_dir.join(artifact_path)),
        artifact,
        native,
    }
}

#[derive(Debug, Deserialize)]
struct AssetIndexJson {
    objects: BTreeMap<String, AssetObjectJson>,
}
#[derive(Debug, Deserialize)]
struct AssetObjectJson {
    hash: String,
    size: Option<u64>,
}

pub fn assets_to_resources(asset_index: &str, game_dir: &Path) -> io::Result<Vec<AssetResource>> {
    let parsed: AssetIndexJson = serde_json::from_str(asset_index).map_err(invalid_data)?;
    Ok(parsed
        .objects
        .into_iter()
        .map(|(name, object)| {
            let prefix = object.hash.get(0..2).unwrap_or_default();
            let object_rel = PathBuf::from("assets")
                .join("objects")
                .join(prefix)
                .join(&object.hash);
            let object_path = game_dir.join(&object_rel);
            let resource_path = game_dir.join("resources").join(&name);
            let url = format!("{ASSET_OBJECTS_BASE_URL}{prefix}/{}", object.hash);
            let download = DownloadJob::new(
                DownloadKind::Asset,
                url,
                object_path.clone(),
                format!("asset {name}"),
            )
            .with_integrity(Some(object.hash.clone()), object.size);
            AssetResource {
                name,
                hash: object.hash,
                object_path,
                resource_path,
                download,
            }
        })
        .collect())
}

pub fn copy_assets_to_resources(resources: &[AssetResource]) -> io::Result<()> {
    for resource in resources {
        if resource
            .resource_path
            .metadata()
            .map(|m| Some(m.len()) == resource.download.expected_size)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(parent) = resource.resource_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&resource.object_path, &resource.resource_path)?;
    }
    Ok(())
}

pub fn run_downloads(
    plan: DownloadPlan,
    options: DownloadOptions,
    sender: mpsc::Sender<DownloadEvent>,
) -> Option<thread::JoinHandle<DownloadSummary>> {
    let concurrency = options.max_parallel_downloads.max(1);
    if options.async_download {
        Some(thread::spawn(move || {
            execute_downloads(plan.tasks, concurrency, sender)
        }))
    } else {
        let _ = execute_downloads(plan.tasks, concurrency, sender);
        None
    }
}

pub fn execute_downloads(
    jobs: Vec<DownloadJob>,
    concurrency: usize,
    sender: mpsc::Sender<DownloadEvent>,
) -> DownloadSummary {
    let total = jobs.len();
    let _ = sender.send(DownloadEvent::Started { total });
    let queue = Arc::new(Mutex::new(
        jobs.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (done_tx, done_rx) = mpsc::channel();
    for _ in 0..concurrency.max(1) {
        let queue = Arc::clone(&queue);
        let sender = sender.clone();
        let done_tx = done_tx.clone();
        thread::spawn(move || loop {
            let Some((index, job)) = queue.lock().expect("download queue poisoned").pop_front()
            else {
                break;
            };
            let _ = sender.send(DownloadEvent::JobStarted {
                index,
                label: job.label.clone(),
            });
            match download_one(&job, index, &sender) {
                Ok(()) => {
                    let _ = sender.send(DownloadEvent::JobFinished {
                        index,
                        destination: job.destination,
                    });
                    let _ = done_tx.send(true);
                }
                Err(error) => {
                    let _ = sender.send(DownloadEvent::JobFailed {
                        index,
                        label: job.label,
                        error: error.to_string(),
                    });
                    let _ = done_tx.send(false);
                }
            }
        });
    }
    drop(done_tx);
    let mut summary = DownloadSummary {
        succeeded: 0,
        failed: 0,
    };
    for ok in done_rx {
        if ok {
            summary.succeeded += 1
        } else {
            summary.failed += 1
        }
    }
    let _ = sender.send(DownloadEvent::Finished {
        succeeded: summary.succeeded,
        failed: summary.failed,
    });
    summary
}

fn download_one(
    job: &DownloadJob,
    index: usize,
    sender: &mpsc::Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    if let Some(parent) = job.destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = job.destination.with_extension("download");
    let mut file = fs::File::create(&tmp)?;
    let mut downloaded = 0_u64;
    let total = job.expected_size;
    let mut sha1 = Sha1::new();

    if let Some(path) = job.url.strip_prefix("file://") {
        let mut input = fs::File::open(path)?;
        stream_to_file(
            &mut input,
            &mut file,
            &mut downloaded,
            &mut sha1,
            index,
            total,
            sender,
        )?;
    } else {
        let response = ureq::get(&job.url).call().map_err(|error| match error {
            ureq::Error::Status(status, _) => DownloadError::HttpStatus {
                url: job.url.clone(),
                status,
            },
            ureq::Error::Transport(error) => DownloadError::Network {
                url: job.url.clone(),
                message: error.to_string(),
            },
        })?;
        let total = response
            .header("content-length")
            .and_then(|value| value.parse::<u64>().ok())
            .or(job.expected_size);
        let mut input = response.into_reader();
        stream_to_file(
            &mut input,
            &mut file,
            &mut downloaded,
            &mut sha1,
            index,
            total,
            sender,
        )?;
    }
    file.sync_all()?;

    if let Some(expected) = job.expected_size {
        if downloaded != expected {
            return Err(DownloadError::SizeMismatch {
                expected,
                actual: downloaded,
            });
        }
    }
    if let Some(expected) = &job.expected_sha1 {
        let actual = sha1.finalize_hex();
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(DownloadError::Sha1Mismatch {
                expected: expected.clone(),
                actual,
            });
        }
    }
    fs::rename(tmp, &job.destination)?;
    Ok(())
}

fn stream_to_file(
    input: &mut dyn Read,
    output: &mut fs::File,
    downloaded: &mut u64,
    sha1: &mut Sha1,
    index: usize,
    total: Option<u64>,
    sender: &mpsc::Sender<DownloadEvent>,
) -> io::Result<()> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
        *downloaded += read as u64;
        let _ = sender.send(DownloadEvent::JobProgress {
            index,
            downloaded: *downloaded,
            total,
        });
    }
    Ok(())
}

fn sha1_hex_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut sha1 = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
    }
    Ok(sha1.finalize_hex())
}

fn sha1_hex(data: &[u8]) -> String {
    let mut sha1 = Sha1::new();
    sha1.update(data);
    sha1.finalize_hex()
}

#[derive(Debug, Clone)]
struct Sha1 {
    h0: u32,
    h1: u32,
    h2: u32,
    h3: u32,
    h4: u32,
    len_bytes: u64,
    buffer: Vec<u8>,
}

impl Sha1 {
    fn new() -> Self {
        Self {
            h0: 0x67452301,
            h1: 0xefcdab89,
            h2: 0x98badcfe,
            h3: 0x10325476,
            h4: 0xc3d2e1f0,
            len_bytes: 0,
            buffer: Vec::with_capacity(64),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.len_bytes = self.len_bytes.wrapping_add(data.len() as u64);
        let mut input = data;
        if !self.buffer.is_empty() {
            let needed = 64 - self.buffer.len();
            let take = needed.min(input.len());
            self.buffer.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.buffer.len() == 64 {
                let chunk = self.buffer.clone();
                self.process_chunk(&chunk);
                self.buffer.clear();
            }
        }
        let mut chunks = input.chunks_exact(64);
        for chunk in chunks.by_ref() {
            self.process_chunk(chunk);
        }
        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            self.buffer.extend_from_slice(remainder);
        }
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.len_bytes.wrapping_mul(8);
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        let buffer = std::mem::take(&mut self.buffer);
        for chunk in buffer.chunks(64) {
            self.process_chunk(chunk);
        }
        format!(
            "{:08x}{:08x}{:08x}{:08x}{:08x}",
            self.h0, self.h1, self.h2, self.h3, self.h4
        )
    }

    fn process_chunk(&mut self, chunk: &[u8]) {
        debug_assert_eq!(chunk.len(), 64);
        let mut w = [0_u32; 80];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (self.h0, self.h1, self.h2, self.h3, self.h4);
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        self.h0 = self.h0.wrapping_add(a);
        self.h1 = self.h1.wrapping_add(b);
        self.h2 = self.h2.wrapping_add(c);
        self.h3 = self.h3.wrapping_add(d);
        self.h4 = self.h4.wrapping_add(e);
    }
}

fn download_info_job(
    kind: DownloadKind,
    info: &DownloadInfo,
    dest: PathBuf,
    label: String,
) -> Option<DownloadJob> {
    Some(
        DownloadJob::new(kind, info.url.as_ref()?, dest, label)
            .with_integrity(info.sha1.clone(), info.size),
    )
}
fn minecraft_native_classifier() -> &'static str {
    match std::env::consts::OS {
        "windows" => "natives-windows",
        "macos" => "natives-osx",
        "linux" => "natives-linux",
        _ => "natives-linux",
    }
}
fn native_classifier_for_library(lib: &LibraryJson) -> Option<String> {
    if let Some(natives) = &lib.natives {
        let os = match std::env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        natives.get(os).map(|classifier| {
            let bits = if std::env::consts::ARCH.contains("64") {
                "64"
            } else {
                "32"
            };
            classifier.replace("${arch}", bits)
        })
    } else {
        Some(minecraft_native_classifier().to_owned())
    }
}
fn maven_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return name.replace(':', "/");
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar")
}
fn classifier_path(name: &str, classifier: &str) -> String {
    let base = name.split(':').take(3).collect::<Vec<_>>().join(":");
    let mut path = maven_path(&base);
    path.truncate(path.len().saturating_sub(4));
    format!("{path}-{classifier}.jar")
}
fn ensure_slash(value: &str) -> String {
    if value.ends_with('/') {
        value.to_owned()
    } else {
        format!("{value}/")
    }
}
fn existing_file_is_valid(job: &DownloadJob) -> bool {
    let Ok(metadata) = job.destination.metadata() else {
        return false;
    };
    if let Some(expected_size) = job.expected_size {
        if metadata.len() != expected_size {
            return false;
        }
    }
    if let Some(expected_sha1) = &job.expected_sha1 {
        let Ok(actual_sha1) = sha1_hex_file(&job.destination) else {
            return false;
        };
        if !expected_sha1.eq_ignore_ascii_case(&actual_sha1) {
            return false;
        }
    }
    true
}

fn file_is_missing_or_older_than(path: &Path, max_age: Duration) -> bool {
    let Ok(modified) = path.metadata().and_then(|m| m.modified()) else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age > max_age)
        .unwrap_or(false)
}

fn resolve_selected_version(manifest: &[ManifestVersion], selected: &str) -> String {
    if selected == "latest" {
        manifest
            .first()
            .map(|v| v.id.clone())
            .unwrap_or_else(|| selected.to_owned())
    } else {
        selected.to_owned()
    }
}

fn invalid_data(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parser_filters_snapshots_like_original_downloader() {
        let manifest = r#"{
            "versions": [
                { "id": "1.20.4", "type": "release", "url": "https://example/1.20.4.json" },
                { "id": "24w01a", "type": "snapshot", "url": "https://example/24w01a.json" }
            ]
        }"#;

        let releases = parse_versions_manifest(manifest, false).unwrap();
        assert_eq!(
            releases.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
            vec!["1.20.4"]
        );

        let all = parse_versions_manifest(manifest, true).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn assets_map_hashes_to_objects_and_resources() {
        let index = r#"{
            "objects": {
                "icons/icon_16x16.png": {
                    "hash": "abcdef0123456789abcdef0123456789abcdef01",
                    "size": 12
                }
            }
        }"#;
        let resources = assets_to_resources(index, Path::new(".minecraft")).unwrap();
        let resource = &resources[0];

        assert_eq!(
            resource.object_path,
            PathBuf::from(".minecraft/assets/objects/ab/abcdef0123456789abcdef0123456789abcdef01")
        );
        assert_eq!(
            resource.resource_path,
            PathBuf::from(".minecraft/resources/icons/icon_16x16.png")
        );
        assert_eq!(resource.download.expected_size, Some(12));
    }

    #[test]
    fn plan_from_config_expands_local_metadata_and_skips_valid_missing_mode() {
        let root = std::env::temp_dir().join(format!(
            "vortex-plan-test-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("versions/1.0")).unwrap();
        fs::create_dir_all(root.join("assets/indexes")).unwrap();
        fs::write(
            root.join("version_manifest_v2.json"),
            r#"{"versions":[{"id":"1.0","type":"release","url":"https://example/1.0.json"}]}"#,
        )
        .unwrap();
        let native_classifier = minecraft_native_classifier();
        fs::write(
            root.join("versions/1.0/1.0.json"),
            format!(
                r#"{{
                "id":"1.0",
                "downloads":{{"client":{{"url":"https://example/client.jar","sha1":"a9993e364706816aba3e25717850c26c9cd0d89d","size":3}}}},
                "libraries":[{{"name":"org.example:lib:1.0","downloads":{{"artifact":{{"path":"org/example/lib/1.0/lib-1.0.jar","url":"https://example/lib.jar","sha1":"a9993e364706816aba3e25717850c26c9cd0d89d","size":3}},"classifiers":{{"{native_classifier}":{{"path":"org/example/lib/1.0/lib-1.0-{native_classifier}.jar","url":"https://example/native.jar"}}}}}}}}}}],
                "assetIndex":{{"id":"1","url":"https://example/assets.json"}},
                "logging":{{"client":{{"file":{{"url":"https://example/log.xml"}}}}}}
            }}"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("assets/indexes/1.json"),
            r#"{"objects":{"icons/icon.png":{"hash":"abcdef0123456789abcdef0123456789abcdef01","size":12}}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("libraries/org/example/lib/1.0")).unwrap();
        fs::write(
            root.join("libraries/org/example/lib/1.0/lib-1.0.jar"),
            b"abc",
        )
        .unwrap();

        let profile = LaunchProfile {
            username: "Player".to_owned(),
            version_id: "1.0".to_owned(),
            game_directory: root.clone(),
            java_path: None,
            memory_mb: 2048,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
        };
        let options = DownloadOptions {
            mode: DownloadMode::MissingLibraries,
            include_snapshots: false,
            max_parallel_downloads: 2,
            async_download: false,
        };
        let plan = DownloadPlan::from_config(&profile, options);
        let kinds = plan.tasks.iter().map(|job| &job.kind).collect::<Vec<_>>();

        assert_eq!(plan.max_parallel_downloads, 2);
        assert!(!kinds.contains(&&DownloadKind::Library));
        assert!(kinds.contains(&&DownloadKind::ClientJar));
        assert!(kinds.contains(&&DownloadKind::NativeLibrary));
        assert!(kinds.contains(&&DownloadKind::AssetIndex));
        assert!(kinds.contains(&&DownloadKind::Asset));
        assert!(kinds.contains(&&DownloadKind::LogConfig));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_sha1_matches_known_vector() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn download_streams_file_url_with_progress_and_atomic_destination() {
        let root = std::env::temp_dir().join(format!(
            "vortex-download-test-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let destination = root.join("nested/destination.bin");
        fs::write(&source, b"abc").unwrap();
        let (tx, rx) = mpsc::channel();
        let job = DownloadJob::new(
            DownloadKind::Asset,
            format!("file://{}", source.display()),
            destination.clone(),
            "local test asset",
        )
        .with_integrity(
            Some("a9993e364706816aba3e25717850c26c9cd0d89d".to_owned()),
            Some(3),
        );

        download_one(&job, 7, &tx).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"abc");
        assert!(!destination.with_extension("download").exists());
        assert!(rx.try_iter().any(|event| {
            event
                == DownloadEvent::JobProgress {
                    index: 7,
                    downloaded: 3,
                    total: Some(3),
                }
        }));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn large_file_download_emits_progress_for_each_chunk() {
        let root = std::env::temp_dir().join(format!(
            "vortex-large-download-test-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let destination = root.join("nested/destination.bin");
        let bytes = vec![42_u8; (64 * 1024) + 17];
        fs::write(&source, &bytes).unwrap();
        let (tx, rx) = mpsc::channel();
        let job = DownloadJob::new(
            DownloadKind::Asset,
            format!("file://{}", source.display()),
            destination.clone(),
            "large local test asset",
        )
        .with_integrity(Some(sha1_hex(&bytes)), Some(bytes.len() as u64));

        download_one(&job, 3, &tx).unwrap();

        let progress = rx
            .try_iter()
            .filter_map(|event| match event {
                DownloadEvent::JobProgress { downloaded, .. } => Some(downloaded),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert_eq!(progress, vec![64 * 1024, (64 * 1024) + 17]);

        let _ = fs::remove_dir_all(root);
    }
}
