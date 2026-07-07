// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use crate::minecraft::{DownloadInfo, LaunchProfile, LibraryJson, Rule, RuleAction};
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
        let mut plan = Self::for_profile(profile);
        plan.max_parallel_downloads = options.max_parallel_downloads.max(1);
        plan
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
    let id = version.id.as_deref()?;
    let client = version.downloads.as_ref()?.client.as_ref()?;
    download_info_job(
        DownloadKind::ClientJar,
        client,
        game_dir.join("versions").join(id).join(format!("{id}.jar")),
        format!("client jar {id}"),
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
        .filter(|lib| rules_apply(&lib.rules))
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
    let native_classifier = minecraft_native_classifier();
    let native = lib
        .downloads
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
) -> io::Result<()> {
    if let Some(parent) = job.destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = job.destination.with_extension("download");
    let mut bytes = Vec::new();
    if let Some(path) = job.url.strip_prefix("file://") {
        let mut input = fs::File::open(path)?;
        let mut file = fs::File::create(&tmp)?;
        input.read_to_end(&mut bytes)?;
        file.write_all(&bytes)?;
    } else {
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(&tmp)
            .arg(&job.url)
            .status()?;
        if !status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("curl exited with status {status}"),
            ));
        }
        fs::File::open(&tmp)?.read_to_end(&mut bytes)?;
    }
    let downloaded = bytes.len() as u64;
    let _ = sender.send(DownloadEvent::JobProgress {
        index,
        downloaded,
        total: job.expected_size,
    });

    if let Some(expected) = job.expected_size {
        if downloaded != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("size mismatch: expected {expected}, got {downloaded}"),
            ));
        }
    }
    if let Some(expected) = &job.expected_sha1 {
        let actual = sha1_hex(&bytes);
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("sha1 mismatch: expected {expected}, got {actual}"),
            ));
        }
    }
    fs::rename(tmp, &job.destination)?;
    Ok(())
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xefcdab89;
    let mut h2: u32 = 0x98badcfe;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xc3d2e1f0;
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
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
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
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
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    format!("{h0:08x}{h1:08x}{h2:08x}{h3:08x}{h4:08x}")
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
fn rules_apply(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if os_rule_matches(rule.os.as_ref()) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}
fn os_rule_matches(rule: Option<&crate::minecraft::OsRule>) -> bool {
    let Some(rule) = rule else {
        return true;
    };
    rule.name
        .as_deref()
        .map(|n| n == current_minecraft_os_name())
        .unwrap_or(true)
        && rule
            .arch
            .as_deref()
            .map(|a| a == std::env::consts::ARCH)
            .unwrap_or(true)
}
fn current_minecraft_os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        _ => "unknown",
    }
}
fn minecraft_native_classifier() -> &'static str {
    match std::env::consts::OS {
        "windows" => "natives-windows",
        "macos" => "natives-osx",
        "linux" => "natives-linux",
        _ => "natives-linux",
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
    fn bundled_sha1_matches_known_vector() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
