// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
use std::collections::HashSet;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::LauncherConfig;
use crate::minecraft::{
    rules_apply_with_context, DownloadInfo, LaunchContext, LaunchProfile, LibraryJson,
};
use serde::Deserialize;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const LIBRARIES_BASE_URL: &str = "https://libraries.minecraft.net/";
pub const ASSET_OBJECTS_BASE_URL: &str = "https://resources.download.minecraft.net/";
const MAX_DOWNLOAD_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadKind {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVersion {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLibrary {
    pub artifact: Option<DownloadJob>,
    pub native: Option<DownloadJob>,
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

impl DownloadError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Network { .. } => true,
            Self::HttpStatus { status, .. } => matches!(*status, 429 | 500 | 502 | 503 | 504),
            Self::Io(error) => matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::BrokenPipe
            ),
            Self::SizeMismatch { .. } | Self::Sha1Mismatch { .. } => true,
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

pub fn download_selected_version(
    config: &LauncherConfig,
    sender: mpsc::Sender<DownloadEvent>,
) -> io::Result<String> {
    let profile = LaunchProfile::from_config(config);
    eprintln!(
        "[download] Starting selected-version download: requested_version={}, game_dir={}, threads={}, redownload_all={}, snapshots={}",
        profile.version_id,
        profile.game_directory.display(),
        config.download_threads,
        config.redownload_all_files,
        config.show_all_versions
    );
    let manifest_text = fetch_manifest_document()?;
    let manifest = parse_versions_manifest(&manifest_text, config.show_all_versions)?;
    let version = resolve_selected_version(&manifest, &profile.version_id);
    eprintln!("[download] Resolved version: {version}");
    let game_dir = &profile.game_directory;
    fs::create_dir_all(game_dir)?;
    write_atomic(
        &game_dir.join("version_manifest_v2.json"),
        manifest_text.as_bytes(),
    )?;

    run_jobs(
        vec![
            version_json_job(&manifest, game_dir, &version).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("version '{version}' not found"),
                )
            })?,
        ],
        config,
        &sender,
    )?;

    let version_json = crate::minecraft::MinecraftVersionJson::load_resolved(game_dir, &version)?;
    let mut jobs = Vec::new();
    if let Some(job) = client_jar_job(&version_json, game_dir) {
        jobs.push(job);
    }
    for lib in parse_libraries(&version_json, game_dir) {
        if let Some(job) = lib.artifact {
            jobs.push(job);
        }
        if let Some(job) = lib.native {
            jobs.push(job);
        }
    }
    if let Some(job) = asset_index_job(&version_json, game_dir) {
        jobs.push(job);
    }
    if let Some(job) = logging_config_job(&version_json, game_dir) {
        jobs.push(job);
    }
    eprintln!(
        "[download] Queued core files: {} jobs before assets",
        jobs.len()
    );
    run_jobs(jobs, config, &sender)?;

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
            let assets = asset_jobs_from_index(&fs::read_to_string(index_path)?, game_dir)?;
            eprintln!("[download] Queued asset objects: {} jobs", assets.len());
            run_jobs(assets, config, &sender)?;
        }
    }

    Ok(version)
}

fn run_jobs(
    jobs: Vec<DownloadJob>,
    config: &LauncherConfig,
    sender: &mpsc::Sender<DownloadEvent>,
) -> io::Result<()> {
    let original_count = jobs.len();
    let jobs = dedupe_jobs_by_destination(jobs);
    let deduped_count = jobs.len();
    if deduped_count != original_count {
        eprintln!(
            "[download] Deduplicated jobs by destination: {original_count} -> {deduped_count}"
        );
    }
    let jobs = jobs
        .into_iter()
        .filter(|job| config.redownload_all_files || !existing_file_is_valid(job))
        .collect::<Vec<_>>();
    if jobs.len() != deduped_count {
        eprintln!(
            "[download] Skipped {} files that already passed integrity checks",
            deduped_count - jobs.len()
        );
    }
    if jobs.is_empty() {
        eprintln!("[download] No jobs to run");
        return Ok(());
    }
    let concurrency = if config.async_download {
        config.download_threads.max(1)
    } else {
        1
    };
    eprintln!(
        "[download] Running {} jobs with concurrency {}",
        jobs.len(),
        concurrency
    );
    let summary = execute_downloads(jobs, concurrency, sender.clone());
    eprintln!(
        "[download] Jobs finished: succeeded={}, failed={}",
        summary.succeeded, summary.failed
    );
    if summary.failed == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} downloads failed",
            summary.failed
        )))
    }
}

fn dedupe_jobs_by_destination(jobs: Vec<DownloadJob>) -> Vec<DownloadJob> {
    let mut seen = HashSet::new();
    jobs.into_iter()
        .filter(|job| seen.insert(job.destination.clone()))
        .collect()
}

pub(crate) fn fetch_manifest(include_snapshots: bool) -> io::Result<Vec<ManifestVersion>> {
    let manifest = fetch_manifest_document()?;
    let versions = parse_versions_manifest(&manifest, include_snapshots)?;
    eprintln!("[download] Manifest versions available: {}", versions.len());
    Ok(versions)
}

fn fetch_manifest_document() -> io::Result<String> {
    eprintln!("[download] Fetching Mojang version manifest");
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build()
        .get(VERSION_MANIFEST_URL)
        .call()
        .map_err(|error| io::Error::other(error.to_string()))?
        .into_string()
        .map_err(|error| io::Error::other(error.to_string()))
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
            crate::minecraft::logging_config_path(game_dir, file),
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
    let artifact_info = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
    let artifact_path = artifact_info
        .and_then(|a| a.path.clone())
        .or_else(|| lib.downloads.is_none().then(|| maven_path(&lib.name)));
    let artifact = artifact_path.as_ref().map(|artifact_path| {
        let artifact_url = artifact_info
            .and_then(|a| a.url.clone())
            .or_else(|| {
                lib.url
                    .as_ref()
                    .map(|base| format!("{}{}", ensure_slash(base), artifact_path))
            })
            .unwrap_or_else(|| format!("{LIBRARIES_BASE_URL}{artifact_path}"));
        DownloadJob::new(
            DownloadKind::Library,
            artifact_url,
            libraries_dir.join(artifact_path),
            lib.name.clone(),
        )
        .with_integrity(
            artifact_info.and_then(|a| a.sha1.clone()),
            artifact_info.and_then(|a| a.size),
        )
    });
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
    ResolvedLibrary { artifact, native }
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

fn asset_jobs_from_index(asset_index: &str, game_dir: &Path) -> io::Result<Vec<DownloadJob>> {
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
            let url = format!("{ASSET_OBJECTS_BASE_URL}{prefix}/{}", object.hash);
            DownloadJob::new(
                DownloadKind::Asset,
                url,
                game_dir.join(object_rel),
                format!("asset {name}"),
            )
            .with_integrity(Some(object.hash), object.size)
        })
        .collect())
}

pub fn execute_downloads(
    jobs: Vec<DownloadJob>,
    concurrency: usize,
    sender: mpsc::Sender<DownloadEvent>,
) -> DownloadSummary {
    let total = jobs.len();
    eprintln!(
        "[download] Download executor started: total={total}, concurrency={}",
        concurrency.max(1)
    );
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
            eprintln!(
                "[download] Starting job {}/{}: {} -> {}",
                index + 1,
                total,
                job.label,
                job.destination.display()
            );
            match download_with_retries(&job, index, &sender) {
                Ok(()) => {
                    eprintln!(
                        "[download] Finished job {}/{}: {}",
                        index + 1,
                        total,
                        job.label
                    );
                    let _ = sender.send(DownloadEvent::JobFinished {
                        index,
                        destination: job.destination,
                    });
                    let _ = done_tx.send(true);
                }
                Err(error) => {
                    eprintln!(
                        "[download] Failed job {}/{}: {}: {error}",
                        index + 1,
                        total,
                        job.label
                    );
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
    eprintln!(
        "[download] Download executor finished: succeeded={}, failed={}",
        summary.succeeded, summary.failed
    );
    summary
}

fn download_with_retries(
    job: &DownloadJob,
    index: usize,
    sender: &mpsc::Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    let mut last_error = None;
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        match download_one_attempt(job, index, sender) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < MAX_DOWNLOAD_ATTEMPTS && error.is_retryable() => {
                eprintln!(
                    "[download] Retrying job {} after attempt {attempt}/{MAX_DOWNLOAD_ATTEMPTS}: {error}",
                    job.label
                );
                let _ = fs::remove_file(temporary_download_path(&job.destination));
                last_error = Some(error);
                let backoff_ms = 500 * attempt as u64;
                thread::sleep(Duration::from_millis(backoff_ms));
            }
            Err(error) => {
                let _ = fs::remove_file(temporary_download_path(&job.destination));
                return Err(error);
            }
        }
    }

    let _ = fs::remove_file(temporary_download_path(&job.destination));
    Err(last_error.expect("retry loop must store the last retryable error"))
}

fn download_one_attempt(
    job: &DownloadJob,
    index: usize,
    sender: &mpsc::Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    if let Some(parent) = job.destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = temporary_download_path(&job.destination);
    let _ = fs::remove_file(&tmp);
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
        let response = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .get(&job.url)
            .call()
            .map_err(|error| match error {
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
    replace_file(&tmp, &job.destination)?;
    Ok(())
}

fn temporary_download_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".download");
    PathBuf::from(name)
}

fn write_atomic(destination: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temporary_download_path(destination);
    let _ = fs::remove_file(&tmp);
    let mut file = fs::File::create(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    replace_file(&tmp, destination)
}

fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() {
        // Windows does not replace an existing file with rename, which made
        // "redownload all" fail after the new file had already been verified.
        fs::remove_file(destination)?;
    }
    fs::rename(source, destination)
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

#[cfg(test)]
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
    use std::time::SystemTime;

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
    fn duplicate_download_destinations_are_scheduled_once() {
        let destination = PathBuf::from(".minecraft/libraries/example.jar");
        let jobs = vec![
            DownloadJob::new(
                DownloadKind::Library,
                "https://example/first.jar",
                destination.clone(),
                "first",
            ),
            DownloadJob::new(
                DownloadKind::Library,
                "https://example/second.jar",
                destination.clone(),
                "second",
            ),
        ];

        let deduped = dedupe_jobs_by_destination(jobs);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].url, "https://example/first.jar");
        assert_eq!(deduped[0].destination, destination);
    }

    #[test]
    fn classifier_only_library_does_not_invent_artifact_download() {
        let native_classifier = minecraft_native_classifier();
        let version: crate::minecraft::MinecraftVersionJson = serde_json::from_str(&format!(
            r#"{{
                "id":"1.0",
                "libraries":[{{
                    "name":"net.java.jinput:jinput-platform:2.0.5",
                    "natives":{{"windows":"natives-windows","linux":"natives-linux","osx":"natives-osx"}},
                    "downloads":{{"classifiers":{{"{native_classifier}":{{
                        "path":"net/java/jinput/jinput-platform/2.0.5/jinput-platform-2.0.5-{native_classifier}.jar",
                        "url":"https://example/native.jar"
                    }}}}}}
                }}]
            }}"#
        ))
        .unwrap();

        let libraries = parse_libraries(&version, Path::new(".minecraft"));

        assert_eq!(libraries.len(), 1);
        assert!(libraries[0].artifact.is_none());
        assert!(libraries[0].native.is_some());
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
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"old").unwrap();
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

        download_with_retries(&job, 7, &tx).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"abc");
        assert!(!temporary_download_path(&destination).exists());
        assert!(rx.try_iter().any(|event| {
            event
                == DownloadEvent::JobProgress {
                    index: 7,
                    downloaded: 3,
                    total: Some(3),
                }
        }));

        let (skip_tx, skip_rx) = mpsc::channel();
        run_jobs(vec![job], &LauncherConfig::default(), &skip_tx).unwrap();
        assert!(skip_rx.try_iter().next().is_none());

        let _ = fs::remove_dir_all(root);
    }
}
