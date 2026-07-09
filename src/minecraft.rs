// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::LauncherConfig;
use crate::platform;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchContext {
    pub os: String,
    pub arch: String,
    pub demo_mode: bool,
    pub resolution: Option<(u32, u32)>,
    pub quick_play_support: bool,
    pub quick_play_singleplayer: bool,
    pub quick_play_multiplayer: bool,
    pub quick_play_realms: bool,
    pub custom_features: BTreeMap<String, bool>,
}

impl Default for LaunchContext {
    fn default() -> Self {
        Self::current()
    }
}

impl LaunchContext {
    pub fn current() -> Self {
        Self {
            os: current_minecraft_os_name().to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            demo_mode: false,
            resolution: None,
            quick_play_support: false,
            quick_play_singleplayer: false,
            quick_play_multiplayer: false,
            quick_play_realms: false,
            custom_features: BTreeMap::new(),
        }
    }

    fn feature_enabled(&self, feature: &str) -> bool {
        match feature {
            "is_demo_user" => self.demo_mode,
            "has_custom_resolution" => self.resolution.is_some(),
            "has_quick_plays_support" => self.quick_play_support,
            "is_quick_play_singleplayer" => self.quick_play_singleplayer,
            "is_quick_play_multiplayer" => self.quick_play_multiplayer,
            "is_quick_play_realms" => self.quick_play_realms,
            feature => self.custom_features.get(feature).copied().unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftVersionJson {
    pub id: Option<String>,
    pub java_version: Option<JavaVersion>,
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
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: Option<String>,
    pub major_version: Option<u32>,
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
    pub id: Option<String>,
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
    pub natives: Option<BTreeMap<String, String>>,
    pub extract: Option<ExtractRules>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadInfo>,
    pub classifiers: Option<BTreeMap<String, DownloadInfo>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExtractRules {
    #[serde(default)]
    pub exclude: Vec<String>,
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
        self.java_version = child.java_version.or(self.java_version);
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
        self.effective_libraries_with_context(game_directory, &LaunchContext::current())
    }

    pub fn effective_libraries_with_context(
        &self,
        game_directory: impl AsRef<Path>,
        context: &LaunchContext,
    ) -> Vec<Library> {
        let libraries_dir = game_directory.as_ref().join("libraries");
        self.libraries
            .iter()
            .filter(|lib| rules_apply_with_context(&lib.rules, context))
            .filter_map(|lib| {
                let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
                let path = artifact
                    .and_then(|a| a.path.clone())
                    .or_else(|| lib.downloads.is_none().then(|| maven_path(&lib.name)))?;
                let url = lib
                    .downloads
                    .as_ref()
                    .and_then(|d| d.artifact.as_ref())
                    .and_then(|a| a.url.clone())
                    .or_else(|| lib.url.clone());
                Some(Library {
                    name: lib.name.clone(),
                    path: libraries_dir.join(path),
                    url,
                })
            })
            .collect()
    }

    pub fn build_launch_command(
        &self,
        profile: &LaunchProfile,
        save_launch_string: bool,
    ) -> LaunchCommand {
        self.build_launch_command_with_context(
            profile,
            save_launch_string,
            &LaunchContext::current(),
        )
    }

    pub fn build_launch_command_with_context(
        &self,
        profile: &LaunchProfile,
        save_launch_string: bool,
        context: &LaunchContext,
    ) -> LaunchCommand {
        eprintln!(
            "[launcher] Building launch command: profile_version={}, metadata_id={}, required_java={}",
            profile.version_id,
            self.id.as_deref().unwrap_or("<missing>"),
            self.required_java_major()
        );
        let java = profile
            .java_path
            .clone()
            .or_else(|| platform::java_executable_for_major(self.required_java_major()))
            .unwrap_or_else(|| PathBuf::from("java"));
        let version_id = self.id.as_deref().unwrap_or(&profile.version_id);
        let jar_id = self.jar.as_deref().unwrap_or(version_id);
        let version_jar = profile
            .game_directory
            .join("versions")
            .join(jar_id)
            .join(format!("{jar_id}.jar"));
        let classpath = build_classpath(
            self.effective_libraries_with_context(&profile.game_directory, context),
            &version_jar,
        );
        let classpath_entries = classpath.split(classpath_separator()).count();
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
        eprintln!(
            "[launcher] Launch metadata: version_id={version_id}, jar_id={jar_id}, main_class={}, assets_index={assets_index}, classpath_entries={classpath_entries}",
            if main_class.is_empty() {
                "<missing>"
            } else {
                &main_class
            }
        );

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
        replacements.insert("user_properties", "{}".to_owned());
        replacements.insert("user_property_map", "{}".to_owned());
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
        if let Some((width, height)) = context.resolution {
            replacements.insert("resolution_width", width.to_string());
            replacements.insert("resolution_height", height.to_string());
        }

        let mut args = vec![format!("-Xmx{}M", profile.memory_mb)];
        args.extend(profile.jvm_args.clone());
        if let Some(arguments) = &self.arguments {
            args.extend(expand_arguments(&arguments.jvm, &replacements, context));
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
            if let Some(client) = logging.client.as_ref() {
                if let Some(file) = client.file.as_ref() {
                    replacements.insert(
                        "path",
                        logging_config_path(&profile.game_directory, file)
                            .display()
                            .to_string(),
                    );
                }
                if let Some(argument) = client.argument.as_ref() {
                    args.push(replace_placeholders(argument, &replacements));
                }
            }
        }
        if !main_class.is_empty() {
            args.push(main_class);
        }
        if let Some(arguments) = &self.arguments {
            args.extend(expand_arguments(&arguments.game, &replacements, context));
        } else if let Some(legacy) = &self.minecraft_arguments {
            args.extend(
                legacy
                    .split_whitespace()
                    .map(|a| replace_placeholders(a, &replacements)),
            );
        }
        args.extend(profile.game_args.clone());
        let launch_string = save_launch_string.then(|| command_to_string(&java, &args));
        eprintln!(
            "[launcher] Launch arguments built: jvm_args={}, total_args={}, save_launch_string={save_launch_string}",
            profile.jvm_args.len(),
            args.len()
        );
        LaunchCommand {
            executable: java,
            args,
            working_directory: profile.game_directory.clone(),
            launch_string,
        }
    }

    pub fn required_java_major(&self) -> u32 {
        self.java_version
            .as_ref()
            .and_then(|java| java.major_version)
            .or_else(|| {
                self.id
                    .as_deref()
                    .and_then(infer_java_major_from_minecraft_version)
            })
            .unwrap_or(8)
    }
}

impl MinecraftVersionJson {
    pub fn extract_native_libraries(&self, profile: &LaunchProfile) -> io::Result<()> {
        self.extract_native_libraries_with_context(profile, &LaunchContext::current())
    }

    pub fn extract_native_libraries_with_context(
        &self,
        profile: &LaunchProfile,
        context: &LaunchContext,
    ) -> io::Result<()> {
        let version_id = self.id.as_deref().unwrap_or(&profile.version_id);
        let natives_dir = profile
            .game_directory
            .join("versions")
            .join(version_id)
            .join("natives");
        eprintln!(
            "[launcher] Extracting native libraries for version {version_id} into {}",
            natives_dir.display()
        );
        fs::create_dir_all(&natives_dir)?;
        let libraries_dir = profile.game_directory.join("libraries");
        let mut extracted = 0_usize;
        let mut missing = 0_usize;
        for library in self
            .libraries
            .iter()
            .filter(|lib| rules_apply_with_context(&lib.rules, context))
        {
            let Some(classifier) = native_classifier_for_context(library, context) else {
                continue;
            };
            let native_path = library
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&classifier))
                .and_then(|info| info.path.clone())
                .unwrap_or_else(|| classifier_path(&library.name, &classifier));
            let archive = libraries_dir.join(native_path);
            if archive.exists() {
                extract_native_archive(&archive, &natives_dir, library.extract.as_ref())?;
                extracted += 1;
            } else {
                missing += 1;
                eprintln!(
                    "[launcher] Native archive missing, skipping extraction: {}",
                    archive.display()
                );
            }
        }
        eprintln!(
            "[launcher] Native extraction finished: extracted={extracted}, missing={missing}"
        );
        Ok(())
    }
}

fn native_classifier_for_context(library: &LibraryJson, context: &LaunchContext) -> Option<String> {
    if let Some(natives) = &library.natives {
        natives
            .get(&context.os)
            .map(|classifier| classifier.replace("${arch}", native_arch_bits(&context.arch)))
    } else {
        let classifier = minecraft_native_classifier_for_os(&context.os);
        library
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.contains_key(classifier).then(|| classifier.to_owned()))
    }
}

fn native_arch_bits(arch: &str) -> &'static str {
    if arch.contains("64") {
        "64"
    } else {
        "32"
    }
}

fn minecraft_native_classifier_for_os(os: &str) -> &'static str {
    match os {
        "windows" => "natives-windows",
        "osx" => "natives-osx",
        "linux" => "natives-linux",
        _ => "natives-linux",
    }
}

pub fn extract_native_archive(
    archive: &Path,
    natives_dir: &Path,
    extract: Option<&ExtractRules>,
) -> io::Result<()> {
    let file = fs::File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().replace('\\', "/");
        let excluded = extract
            .map(|rules| rules.exclude.iter().any(|prefix| name.starts_with(prefix)))
            .unwrap_or(false);
        let safe_path = safe_zip_entry_path(&name);
        let Some(relative) = safe_path else {
            continue;
        };
        if excluded || entry.is_dir() {
            continue;
        }
        let destination = natives_dir.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(destination)?;
        io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn safe_zip_entry_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => clean.push(part),
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

fn zip_error(error: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
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
            java_path: config
                .use_custom_java
                .then(|| config.java_path.clone())
                .flatten(),
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
        eprintln!(
            "[launcher] Loading version metadata: version={}, game_dir={}",
            self.version_id,
            self.game_directory.display()
        );
        let metadata = MinecraftVersionJson::load_resolved(&self.game_directory, &self.version_id)?;
        eprintln!(
            "[launcher] Loaded version metadata: id={}, libraries={}",
            metadata.id.as_deref().unwrap_or("<missing>"),
            metadata.libraries.len()
        );
        metadata.extract_native_libraries(self)?;
        Ok(metadata.build_launch_command(self, save_launch_string))
    }
}

fn infer_java_major_from_minecraft_version(version: &str) -> Option<u32> {
    let numeric = version.split(['-', '_']).next().unwrap_or(version);
    let parts = numeric
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let [major, rest @ ..] = parts.as_slice() else {
        return None;
    };
    if *major >= 26 {
        return Some(25);
    }
    if *major != 1 {
        return None;
    }
    let minor = *rest.first()?;
    match minor {
        0..=16 => Some(8),
        17..=19 => Some(17),
        20 => {
            let patch = rest.get(1).copied().unwrap_or(0);
            Some(if patch <= 4 { 17 } else { 21 })
        }
        21..=25 => Some(21),
        _ => Some(25),
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
    context: &LaunchContext,
) -> Vec<String> {
    values
        .iter()
        .filter(|value| match value {
            ArgumentValue::String(_) => true,
            ArgumentValue::Ruled { rules, .. } => rules_apply_with_context(rules, context),
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
    rules_apply_with_context(rules, &LaunchContext::current())
}

pub(crate) fn rules_apply_with_context(rules: &[Rule], context: &LaunchContext) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if os_rule_matches(rule.os.as_ref(), context) && features_match(&rule.features, context) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn features_match(features: &BTreeMap<String, bool>, context: &LaunchContext) -> bool {
    features
        .iter()
        .all(|(feature, expected)| context.feature_enabled(feature) == *expected)
}

fn os_rule_matches(rule: Option<&OsRule>, context: &LaunchContext) -> bool {
    let Some(rule) = rule else {
        return true;
    };
    if let Some(name) = &rule.name {
        if name != &context.os {
            return false;
        }
    }
    if let Some(arch) = &rule.arch {
        if arch != &context.arch {
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

fn classifier_path(name: &str, classifier: &str) -> String {
    let base = name.split(':').take(3).collect::<Vec<_>>().join(":");
    let mut path = maven_path(&base);
    path.truncate(path.len().saturating_sub(4));
    format!("{path}-{classifier}.jar")
}

fn build_classpath(libraries: Vec<Library>, version_jar: &Path) -> String {
    let mut entries: Vec<String> = libraries
        .into_iter()
        .map(|lib| lib.path.display().to_string())
        .collect();
    entries.push(version_jar.display().to_string());
    entries.join(classpath_separator())
}

pub fn logging_config_path(game_directory: impl AsRef<Path>, file: &DownloadInfo) -> PathBuf {
    game_directory
        .as_ref()
        .join("assets")
        .join("log_configs")
        .join(
            file.id
                .as_deref()
                .or_else(|| file.url.as_deref().and_then(|url| url.rsplit('/').next()))
                .unwrap_or("client-logging.xml"),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn linux_context() -> LaunchContext {
        LaunchContext {
            os: "linux".to_owned(),
            arch: "x86_64".to_owned(),
            demo_mode: false,
            resolution: None,
            quick_play_support: false,
            quick_play_singleplayer: false,
            quick_play_multiplayer: false,
            quick_play_realms: false,
            custom_features: BTreeMap::new(),
        }
    }

    fn profile() -> LaunchProfile {
        LaunchProfile {
            username: "Player".to_owned(),
            version_id: "1.20.4".to_owned(),
            game_directory: PathBuf::from(".minecraft"),
            java_path: None,
            memory_mb: 2048,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
        }
    }

    #[test]
    fn modern_version_json_expands_representative_argument_rules() {
        let version: MinecraftVersionJson = serde_json::from_str(
            r#"{
                "id": "1.20.4",
                "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": { "id": "12", "url": "https://example/assets.json" },
                "arguments": {
                    "jvm": [
                        "-Djava.library.path=${natives_directory}",
                        { "rules": [{ "action": "allow", "os": { "name": "linux" } }], "value": "-XstartOnLinux" },
                        { "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }], "value": ["--width", "${resolution_width}"] }
                    ],
                    "game": [
                        "--username", "${auth_player_name}",
                        { "rules": [{ "action": "allow", "features": { "is_demo_user": true } }], "value": "--demo" }
                    ]
                }
            }"#,
        )
        .unwrap();

        let command =
            version.build_launch_command_with_context(&profile(), false, &linux_context());

        assert!(command.args.contains(&"-XstartOnLinux".to_owned()));
        assert!(command.args.contains(&"--username".to_owned()));
        assert!(!command.args.contains(&"--demo".to_owned()));
        assert!(!command.args.contains(&"--width".to_owned()));
    }

    #[test]
    fn legacy_1_8_9_arguments_do_not_keep_unresolved_placeholders() {
        let version: MinecraftVersionJson = serde_json::from_str(
            r#"{
                "id": "1.8.9",
                "javaVersion": { "component": "jre-legacy", "majorVersion": 8 },
                "assets": "1.8",
                "assetIndex": { "id": "1.8", "url": "https://example/assets.json" },
                "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userProperties ${user_properties} --userType ${user_type}",
                "logging": {
                    "client": {
                        "argument": "-Dlog4j.configurationFile=${path}",
                        "file": {
                            "id": "client-1.7.xml",
                            "url": "https://launcher.mojang.com/v1/objects/hash/client-1.7.xml"
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let command =
            version.build_launch_command_with_context(&profile(), false, &linux_context());

        assert!(
            command.args.iter().all(|arg| !arg.contains("${")),
            "{:?}",
            command.args
        );
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair == ["--userProperties", "{}"]));
        assert!(command.args.contains(&format!(
            "-Dlog4j.configurationFile={}",
            PathBuf::from(".minecraft")
                .join("assets")
                .join("log_configs")
                .join("client-1.7.xml")
                .display()
        )));
    }

    #[test]
    fn java_requirement_uses_metadata_or_version_fallback() {
        let modern: MinecraftVersionJson = serde_json::from_str(
            r#"{"id":"1.20.6","javaVersion":{"component":"java-runtime-delta","majorVersion":21}}"#,
        )
        .unwrap();
        assert_eq!(modern.required_java_major(), 21);

        let legacy: MinecraftVersionJson = serde_json::from_str(r#"{"id":"1.12.2"}"#).unwrap();
        assert_eq!(legacy.required_java_major(), 8);

        assert_eq!(infer_java_major_from_minecraft_version("1.17.1"), Some(17));
        assert_eq!(infer_java_major_from_minecraft_version("1.20.4"), Some(17));
        assert_eq!(infer_java_major_from_minecraft_version("1.20.5"), Some(21));
        assert_eq!(infer_java_major_from_minecraft_version("26.2"), Some(25));
    }

    #[test]
    fn custom_java_is_only_used_when_enabled() {
        let mut config = LauncherConfig::default();
        config.java_path = Some(PathBuf::from("C:/Java/bin/javaw.exe"));
        config.use_custom_java = false;
        assert!(LaunchProfile::from_config(&config).java_path.is_none());

        config.use_custom_java = true;
        assert_eq!(
            LaunchProfile::from_config(&config).java_path,
            Some(PathBuf::from("C:/Java/bin/javaw.exe"))
        );
    }

    #[test]
    fn rule_denial_overrides_previous_allow() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[
                { "action": "allow" },
                { "action": "disallow", "os": { "name": "linux" } }
            ]"#,
        )
        .unwrap();

        assert!(!rules_apply_with_context(&rules, &linux_context()));
    }

    #[test]
    fn os_specific_allow_matches_launch_context() {
        let rules: Vec<Rule> =
            serde_json::from_str(r#"[{ "action": "allow", "os": { "name": "osx" } }]"#).unwrap();
        let mut context = linux_context();
        assert!(!rules_apply_with_context(&rules, &context));

        context.os = "osx".to_owned();
        assert!(rules_apply_with_context(&rules, &context));
    }

    #[test]
    fn feature_specific_allow_uses_launch_context() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[{ "action": "allow", "features": { "is_demo_user": true } }]"#,
        )
        .unwrap();
        let mut context = linux_context();
        assert!(!rules_apply_with_context(&rules, &context));

        context.demo_mode = true;
        assert!(rules_apply_with_context(&rules, &context));
    }

    #[test]
    fn classifier_only_libraries_are_not_added_to_classpath() {
        let version: MinecraftVersionJson = serde_json::from_str(
            r#"{
                "id":"1.0",
                "libraries":[{
                    "name":"net.java.jinput:jinput-platform:2.0.5",
                    "natives":{"windows":"natives-windows","linux":"natives-linux","osx":"natives-osx"},
                    "downloads":{"classifiers":{"natives-linux":{
                        "path":"net/java/jinput/jinput-platform/2.0.5/jinput-platform-2.0.5-natives-linux.jar",
                        "url":"https://example/native.jar"
                    }}}
                }]
            }"#,
        )
        .unwrap();

        assert!(version
            .effective_libraries_with_context(".minecraft", &linux_context())
            .is_empty());
    }

    #[test]
    fn extracts_native_zip_respecting_exclusions_and_safe_paths() {
        let root = std::env::temp_dir().join(format!(
            "vortex-native-test-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("libraries/org/example/native/1.0")).unwrap();
        fs::create_dir_all(root.join("versions/1.0")).unwrap();
        let archive = root.join("libraries/org/example/native/1.0/native-1.0-natives-linux.jar");
        fs::write(
            &archive,
            test_zip(
                &[
                    ("libnative.so", b"native" as &[u8]),
                    ("META-INF/MANIFEST.MF", b"manifest"),
                    ("nested/helper.so", b"helper"),
                    ("../escape.so", b"escape"),
                ],
                zip::CompressionMethod::Stored,
            ),
        )
        .unwrap();
        let version: MinecraftVersionJson = serde_json::from_str(
            r#"{
                "id":"1.0",
                "libraries":[{
                    "name":"org.example:native:1.0",
                    "natives":{"linux":"natives-linux"},
                    "extract":{"exclude":["META-INF/"]},
                    "downloads":{"classifiers":{"natives-linux":{
                        "path":"org/example/native/1.0/native-1.0-natives-linux.jar",
                        "url":"https://example/native.jar"
                    }}}
                }]
            }"#,
        )
        .unwrap();
        let mut context = linux_context();
        context.arch = "x86_64".to_owned();
        let mut profile = profile();
        profile.version_id = "1.0".to_owned();
        profile.game_directory = root.clone();

        version
            .extract_native_libraries_with_context(&profile, &context)
            .unwrap();

        let natives = root.join("versions/1.0/natives");
        assert_eq!(fs::read(natives.join("libnative.so")).unwrap(), b"native");
        assert_eq!(
            fs::read(natives.join("nested/helper.so")).unwrap(),
            b"helper"
        );
        assert!(!natives.join("META-INF/MANIFEST.MF").exists());
        assert!(!root.join("versions/1.0/escape.so").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_deflated_native_zip_entries() {
        let root = std::env::temp_dir().join(format!(
            "vortex-native-deflate-test-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("native.jar");
        fs::write(
            &archive,
            test_zip(
                &[("OpenAL32.dll", b"openal" as &[u8])],
                zip::CompressionMethod::Deflated,
            ),
        )
        .unwrap();
        let natives = root.join("natives");

        extract_native_archive(&archive, &natives, None).unwrap();

        assert_eq!(fs::read(natives.join("OpenAL32.dll")).unwrap(), b"openal");
        let _ = fs::remove_dir_all(root);
    }

    fn test_zip(entries: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default().compression_method(method);
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            std::io::Write::write_all(&mut writer, contents).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
}
