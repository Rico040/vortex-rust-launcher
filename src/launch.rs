// Vortex Minecraft Launcher
// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]

use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus};

use crate::minecraft::LaunchCommand;

const JAVA_OPTIONS_ENV: &str = "_JAVA_OPTIONS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchOptions {
    pub keep_launcher_open: bool,
    pub save_launch_string: bool,
}

#[derive(Debug)]
pub struct LaunchOutcome {
    pub child: Child,
    pub display_command: Option<String>,
    pub should_close_launcher: bool,
}

#[derive(Debug)]
pub struct JavaValidation {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn launch_minecraft(
    command: &LaunchCommand,
    options: LaunchOptions,
) -> io::Result<LaunchOutcome> {
    let validation = validate_java(command)?;
    let display_command = options.save_launch_string.then(|| display_command(command));
    log_launch_plan(command, &validation);
    let child = minecraft_process(command).spawn()?;
    eprintln!(
        "[launcher] Minecraft process started with pid {}",
        child.id()
    );

    Ok(LaunchOutcome {
        child,
        display_command,
        should_close_launcher: !options.keep_launcher_open,
    })
}

pub fn validate_java(command: &LaunchCommand) -> io::Result<JavaValidation> {
    let output = java_version_process(command).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Java validation failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(JavaValidation {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn display_command(command: &LaunchCommand) -> String {
    command
        .launch_string
        .clone()
        .unwrap_or_else(|| command_to_string(&command.executable, &command.args))
}

fn java_version_process(command: &LaunchCommand) -> Command {
    let mut process = Command::new(&command.executable);
    process
        .arg("-version")
        .current_dir(&command.working_directory)
        .env_remove(JAVA_OPTIONS_ENV);
    process
}

fn minecraft_process(command: &LaunchCommand) -> Command {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.args)
        .current_dir(&command.working_directory)
        .env_remove(JAVA_OPTIONS_ENV);
    process
}

fn log_launch_plan(command: &LaunchCommand, validation: &JavaValidation) {
    eprintln!("[launcher] Preparing Minecraft launch");
    eprintln!(
        "[launcher] Java executable: {}",
        command.executable.display()
    );
    eprintln!(
        "[launcher] Working directory: {}",
        command.working_directory.display()
    );
    eprintln!("[launcher] Java validation status: {}", validation.status);
    if let Some(version) = first_non_empty_line(&validation.stderr)
        .or_else(|| first_non_empty_line(&validation.stdout))
    {
        eprintln!("[launcher] Java version: {version}");
    }
    eprintln!("[launcher] Launch argument count: {}", command.args.len());
    eprintln!("[launcher] Launch command: {}", display_command(command));
    eprintln!("[launcher] Environment override: {JAVA_OPTIONS_ENV} removed");
}

fn first_non_empty_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
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
    use std::path::PathBuf;

    use super::*;

    fn sample_command() -> LaunchCommand {
        LaunchCommand {
            executable: PathBuf::from("/opt/java bin/java"),
            args: vec![
                "-Xmx2048M".to_owned(),
                "-Dkey=value with spaces".to_owned(),
                "net.minecraft.client.Main".to_owned(),
            ],
            working_directory: PathBuf::from("."),
            launch_string: None,
        }
    }

    #[test]
    fn display_command_quotes_without_changing_process_args() {
        let command = sample_command();

        assert_eq!(
            display_command(&command),
            "\"/opt/java bin/java\" -Xmx2048M \"-Dkey=value with spaces\" net.minecraft.client.Main"
        );
        assert_eq!(command.args[1], "-Dkey=value with spaces");
    }

    #[test]
    fn display_command_uses_saved_launch_string_when_available() {
        let mut command = sample_command();
        command.launch_string = Some("saved java invocation".to_owned());

        assert_eq!(display_command(&command), "saved java invocation");
    }

    #[test]
    fn keep_launcher_open_controls_close_flag() {
        let close_after_spawn = !LaunchOptions {
            keep_launcher_open: false,
            save_launch_string: false,
        }
        .keep_launcher_open;
        let remain_open = !LaunchOptions {
            keep_launcher_open: true,
            save_launch_string: false,
        }
        .keep_launcher_open;

        assert!(close_after_spawn);
        assert!(!remain_open);
    }

    #[test]
    fn process_builders_remove_java_options_and_keep_distinct_args() {
        let command = sample_command();
        let process = minecraft_process(&command);
        let env_overrides: Vec<_> = process.get_envs().collect();

        assert_eq!(
            process
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            command.args
        );
        assert!(env_overrides
            .iter()
            .any(|(key, value)| key == &JAVA_OPTIONS_ENV && value.is_none()));

        let validation = java_version_process(&command);
        assert_eq!(
            validation.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("-version")]
        );
        assert!(validation
            .get_envs()
            .any(|(key, value)| key == JAVA_OPTIONS_ENV && value.is_none()));
    }
}
