// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::LauncherConfig;

const LAUNCHER_NAME: &str = "vortex-rust-launcher";
const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionMetadata {
    pub id: String,
    pub main_class: Option<String>,
    pub libraries: Vec<Library>,
    pub assets_index: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub name: String,
    pub path: PathBuf,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProfile {
    pub username: String,
    pub version_id: String,
    pub game_directory: PathBuf,
    pub java_path: Option<PathBuf>,
    pub memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub launch_string: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftVersionJson {
    pub id: Option<String>,
    pub main_class: Option<String>,
    pub assets: Option<String>,
    pub asset_index: Option<AssetIndex>,
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub libraries: Vec<LibraryJson>,
    pub arguments: Option<ArgumentsJson>,
    pub minecraft_arguments: Option<String>,
    pub inherits_from: Option<String>,
    pub jar: Option<String>,
    pub logging: Option<LoggingJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub id: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    #[serde(rename = "totalSize")]
    pub total_size: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: Option<DownloadInfo>,
    pub server: Option<DownloadInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadInfo {
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryJson {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadInfo>,
    pub classifiers: Option<BTreeMap<String, DownloadInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArgumentsJson {
    #[serde(default)]
    pub game: Vec<ArgumentValue>,
    #[serde(default)]
    pub jvm: Vec<ArgumentValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    String(String),
    Ruled {
        rules: Vec<Rule>,
        value: ArgumentValueList,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValueList {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingJson {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: Option<String>,
    pub file: Option<DownloadInfo>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

impl VersionMetadata {
    pub fn minimal(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            main_class: None,
            libraries: Vec::new(),
            assets_index: None,
        }
    }
}

impl MinecraftVersionJson {
    pub fn load(game_directory: impl AsRef<Path>, version: &str) -> io::Result<Self> {
        let path = version_json_path(game_directory, version);
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    pub fn load_resolved(game_directory: impl AsRef<Path>, version: &str) -> io::Result<Self> {
        let game_directory = game_directory.as_ref();
        let child = Self::load(game_directory, version)?;
        if let Some(parent_id) = child.inherits_from.clone() {
            let parent = Self::load_resolved(game_directory, &parent_id)?;
            Ok(parent.inherit(child))
        } else {
            Ok(child)
        }
    }

    pub fn inherit(mut self, child: Self) -> Self {
        let inherited_jar = child
            .jar
            .clone()
            .or_else(|| self.jar.clone())
            .or_else(|| child.inherits_from.clone());
        self.id = child.id.or(self.id);
        self.main_class = child.main_class.or(self.main_class);
        self.assets = child.assets.or(self.assets);
        self.asset_index = child.asset_index.or(self.asset_index);
        self.downloads = child.downloads.or(self.downloads);
        self.arguments = child.arguments.or(self.arguments);
        self.minecraft_arguments = child.minecraft_arguments.or(self.minecraft_arguments);
        self.inherits_from = child.inherits_from;
        self.jar = inherited_jar;
        self.logging = child.logging.or(self.logging);
        self.libraries.extend(child.libraries);
        self
    }

    pub fn to_metadata(&self, game_directory: impl AsRef<Path>) -> VersionMetadata {
        VersionMetadata {
            id: self.id.clone().unwrap_or_default(),
            main_class: self.main_class.clone(),
            libraries: self.effective_libraries(game_directory),
            assets_index: self
                .asset_index
                .as_ref()
                .and_then(|a| a.id.clone())
                .or_else(|| self.assets.clone()),
        }
    }

    pub fn effective_libraries(&self, game_directory: impl AsRef<Path>) -> Vec<Library> {
        let libraries_dir = game_directory.as_ref().join("libraries");
        self.libraries
            .iter()
            .filter(|lib| rules_apply(&lib.rules))
            .map(|lib| {
                let path = lib
                    .downloads
                    .as_ref()
                    .and_then(|d| d.artifact.as_ref())
                    .and_then(|a| a.path.clone())
                    .unwrap_or_else(|| maven_path(&lib.name));
                let url = lib
                    .downloads
                    .as_ref()
                    .and_then(|d| d.artifact.as_ref())
                    .and_then(|a| a.url.clone())
                    .or_else(|| lib.url.clone());
                Library {
                    name: lib.name.clone(),
                    path: libraries_dir.join(path),
                    url,
                }
            })
            .collect()
    }

    pub fn build_launch_command(
        &self,
        profile: &LaunchProfile,
        save_launch_string: bool,
    ) -> LaunchCommand {
        let java = profile
            .java_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("java"));
        let version_id = self.id.as_deref().unwrap_or(&profile.version_id);
        let jar_id = self.jar.as_deref().unwrap_or(version_id);
        let version_jar = profile
            .game_directory
            .join("versions")
            .join(jar_id)
            .join(format!("{jar_id}.jar"));
        let classpath = build_classpath(
            self.effective_libraries(&profile.game_directory),
            &version_jar,
        );
        let natives_dir = profile
            .game_directory
            .join("versions")
            .join(version_id)
            .join("natives");
        let assets_index = self
            .asset_index
            .as_ref()
            .and_then(|a| a.id.as_deref())
            .or(self.assets.as_deref())
            .unwrap_or(version_id);
        let main_class = self.main_class.clone().unwrap_or_default();

        let mut replacements = BTreeMap::new();
        replacements.insert("auth_player_name", profile.username.clone());
        replacements.insert("version_name", version_id.to_owned());
        replacements.insert(
            "game_directory",
            profile.game_directory.display().to_string(),
        );
        replacements.insert(
            "assets_root",
            profile.game_directory.join("assets").display().to_string(),
        );
        replacements.insert("assets_index_name", assets_index.to_owned());
        replacements.insert("auth_uuid", offline_uuid(&profile.username));
        replacements.insert("auth_access_token", "0".to_owned());
        replacements.insert("user_type", "legacy".to_owned());
        replacements.insert("version_type", "release".to_owned());
        replacements.insert("launcher_name", LAUNCHER_NAME.to_owned());
        replacements.insert("launcher_version", LAUNCHER_VERSION.to_owned());
        replacements.insert("classpath", classpath);
        replacements.insert("natives_directory", natives_dir.display().to_string());
        replacements.insert(
            "library_directory",
            profile
                .game_directory
                .join("libraries")
                .display()
                .to_string(),
        );
        replacements.insert("classpath_separator", classpath_separator().to_owned());

        let mut args = vec![format!("-Xmx{}M", profile.memory_mb)];
        args.extend(profile.jvm_args.clone());
        if let Some(arguments) = &self.arguments {
            args.extend(expand_arguments(&arguments.jvm, &replacements));
        } else {
            args.extend(
                [
                    "-Djava.library.path=${natives_directory}".to_owned(),
                    "-cp".to_owned(),
                    "${classpath}".to_owned(),
                ]
                .into_iter()
                .map(|a| replace_placeholders(&a, &replacements)),
            );
        }
        if let Some(logging) = &self.logging {
            if let Some(argument) = logging.client.as_ref().and_then(|c| c.argument.as_ref()) {
                args.push(replace_placeholders(argument, &replacements));
            }
        }
        if !main_class.is_empty() {
            args.push(main_class);
        }
        if let Some(arguments) = &self.arguments {
            args.extend(expand_arguments(&arguments.game, &replacements));
        } else if let Some(legacy) = &self.minecraft_arguments {
            args.extend(
                legacy
                    .split_whitespace()
                    .map(|a| replace_placeholders(a, &replacements)),
            );
        }
        args.extend(profile.game_args.clone());
        let launch_string = save_launch_string.then(|| command_to_string(&java, &args));
        LaunchCommand {
            executable: java,
            args,
            working_directory: profile.game_directory.clone(),
            launch_string,
        }
    }
}

impl LaunchProfile {
    pub fn from_config(config: &LauncherConfig) -> Self {
        Self {
            username: config
                .username
                .clone()
                .unwrap_or_else(|| "Player".to_owned()),
            version_id: config
                .selected_version
                .clone()
                .unwrap_or_else(|| "latest".to_owned()),
            game_directory: config
                .game_directory
                .clone()
                .unwrap_or_else(|| PathBuf::from(".minecraft")),
            java_path: config.java_path.clone(),
            memory_mb: config.memory_mb.unwrap_or(2048),
            jvm_args: config.extra_jvm_args.clone(),
            game_args: config.extra_game_args.clone(),
        }
    }

    pub fn launch_arguments(&self, metadata: &VersionMetadata) -> Vec<String> {
        let mut args = vec![format!("-Xmx{}M", self.memory_mb)];
        args.extend(self.jvm_args.clone());
        if let Some(main_class) = &metadata.main_class {
            args.push(main_class.clone());
        }
        args.extend([
            "--username".to_owned(),
            self.username.clone(),
            "--version".to_owned(),
            self.version_id.clone(),
            "--gameDir".to_owned(),
            self.game_directory.display().to_string(),
        ]);
        args.extend(self.game_args.clone());
        args
    }

    pub fn launch_command(&self, save_launch_string: bool) -> io::Result<LaunchCommand> {
        let metadata = MinecraftVersionJson::load_resolved(&self.game_directory, &self.version_id)?;
        Ok(metadata.build_launch_command(self, save_launch_string))
    }
}

pub fn version_json_path(game_directory: impl AsRef<Path>, version: &str) -> PathBuf {
    game_directory
        .as_ref()
        .join("versions")
        .join(version)
        .join(format!("{version}.json"))
}

fn expand_arguments(
    values: &[ArgumentValue],
    replacements: &BTreeMap<&str, String>,
) -> Vec<String> {
    values
        .iter()
        .filter(|value| match value {
            ArgumentValue::String(_) => true,
            ArgumentValue::Ruled { rules, .. } => rules_apply(rules),
        })
        .flat_map(|value| match value {
            ArgumentValue::String(value) => vec![replace_placeholders(value, replacements)],
            ArgumentValue::Ruled { value, .. } => match value {
                ArgumentValueList::One(value) => vec![replace_placeholders(value, replacements)],
                ArgumentValueList::Many(values) => values
                    .iter()
                    .map(|v| replace_placeholders(v, replacements))
                    .collect(),
            },
        })
        .collect()
}

fn replace_placeholders(value: &str, replacements: &BTreeMap<&str, String>) -> String {
    let mut output = value.to_owned();
    for (key, replacement) in replacements {
        output = output.replace(&format!("${{{key}}}"), replacement);
    }
    output
}

fn rules_apply(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if os_rule_matches(rule.os.as_ref()) && features_match(&rule.features) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn features_match(features: &BTreeMap<String, bool>) -> bool {
    // Launcher-controlled optional Minecraft features (demo mode, quick play,
    // custom resolution) default to disabled until the UI exposes them.
    features.values().all(|enabled| !enabled)
}

fn os_rule_matches(rule: Option<&OsRule>) -> bool {
    let Some(rule) = rule else {
        return true;
    };
    if let Some(name) = &rule.name {
        if name != current_minecraft_os_name() {
            return false;
        }
    }
    if let Some(arch) = &rule.arch {
        if arch != std::env::consts::ARCH {
            return false;
        }
    }
    true
}

fn current_minecraft_os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        _ => "unknown",
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

fn build_classpath(libraries: Vec<Library>, version_jar: &Path) -> String {
    let mut entries: Vec<String> = libraries
        .into_iter()
        .map(|lib| lib.path.display().to_string())
        .collect();
    entries.push(version_jar.display().to_string());
    entries.join(classpath_separator())
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

fn offline_uuid(username: &str) -> String {
    // Deterministic offline UUID surrogate. It is not a full RFC-4122 MD5 UUID,
    // but it is stable and avoids pulling process-launch concerns into parsing.
    let mut hash = 0xcbf29ce484222325_u128;
    for byte in format!("OfflinePlayer:{username}").bytes() {
        hash ^= byte as u128;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:032x}")
}

fn command_to_string(executable: &Path, args: &[String]) -> String {
    std::iter::once(executable.display().to_string())
        .chain(args.iter().cloned())
        .map(|part| quote_arg(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_arg(value: &str) -> String {
    if value.chars().all(|c| !c.is_whitespace() && c != '"') {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}
