use std::{
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::{Context, Error, Result};
use clap::{ArgEnum, Parser};
use colored::Colorize;

#[derive(Parser)]
struct Opts {
    #[clap(arg_enum, short, long)]
    build_system: Option<BuildSystem>,

    #[clap(arg_enum, short('m'), long, default_value("debug"))]
    build_mode: BuildMode,

    // TODO dry run
    // TODO number of jobs
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
    Build,
    Run {
        executable: String,
        args: Vec<String>,
    },
    Clean,
    Install {
        #[clap(long)]
        prefix: Option<PathBuf>,
    },
    // TODO test
    // TODO benchmarks
}

#[derive(Debug, Clone, ArgEnum)]
pub enum BuildSystem {
    Make,
    CMake,
    Meson,
}

#[derive(Debug, Clone, ArgEnum)]
pub enum BuildMode {
    Debug,
    Release,
}

enum Tap<'a> {
    ChangeDirectory {
        path: &'a Path,
    },
    RunCommand {
        command: &'a str,
        args: &'a [&'a str],
    },
}

impl<'a> Tap<'a> {
    fn get_prefix(&self) -> (&str, &str) {
        match self {
            Self::ChangeDirectory { .. } => ("cd", "yellow"),
            Self::RunCommand { .. } => ("run", "purple"),
        }
    }

    fn get_message(&self) -> String {
        match self {
            Self::ChangeDirectory { path } => format!("{:?}", path),
            Self::RunCommand { command, args } => command_to_string(command, args),
        }
    }

    pub fn print(self) {
        let (prefix, color) = self.get_prefix();
        let icon = " ".bold();
        println!(
            " {} {} {}",
            icon,
            format!(" {} ", prefix).bold().reversed().color(color),
            self.get_message().bold()
        );
    }
}

impl BuildSystem {
    fn detect_in_dir(path: &Path) -> Option<Self> {
        if path.join("CMakeLists.txt").exists() {
            Some(Self::CMake)
        } else if path.join("Makefile").exists() || path.join("makefile").exists() {
            Some(Self::Make)
        } else if path.join("meson.build").exists() {
            Some(Self::Meson)
        } else {
            None
        }
    }

    pub fn detect() -> Result<Option<Self>, std::io::Error> {
        let path = std::env::current_dir()?;
        let mut path = path.as_path();
        while path.parent().is_some() {
            if let Some(build_system) = Self::detect_in_dir(path) {
                Tap::ChangeDirectory { path }.print();
                std::env::set_current_dir(path)?;
                return Ok(Some(build_system));
            }
            path = path.parent().unwrap();
        }
        Ok(None)
    }
}

fn command_to_string(command: &str, args: &[&str]) -> String {
    shell_words::join(std::iter::once(&command).chain(args.into_iter()))
}

fn run_command(command: &str, args: &[&str]) -> Result<()> {
    {
        Tap::RunCommand { command, args }.print();

        let exit_status = Command::new(command)
            .args(args)
            .spawn()
            .with_context(|| "Couldn't spawn process")?
            .wait()
            .with_context(|| "Couldn't wait for process to finish")?;

        if exit_status.success() {
            Ok(())
        } else {
            Err(Error::msg(match exit_status.code() {
                Some(code) => format!("Process exited with exit code {}", code),
                None => "Process was killed".to_string(),
            }))
        }
    }
    .with_context(|| format!("While running command {}", command_to_string(command, args)))
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    let build_system = match opts.build_system {
        Some(x) => x,
        None => BuildSystem::detect()
            .with_context(|| "Couldn't detect the build system")?
            .ok_or(Error::msg("Could not detect the build system"))?,
    };

    let build_dir = std::env::current_dir()
        .with_context(|| "Couldn't get current directory")?
        .join(match opts.build_mode {
            BuildMode::Debug => ".tap_build_debug",
            BuildMode::Release => ".tap_build_release",
        });
    let build_dir_str = build_dir
        .to_str()
        .expect("Path couldn't be converted to str");

    match opts.subcommand {
        Subcommand::Build => match build_system {
            BuildSystem::Make => match opts.build_mode {
                BuildMode::Debug => run_command("make", &[]),
                BuildMode::Release => run_command("make", &["CFLAGS=-O3"]),
            },
            BuildSystem::CMake => todo!(),
            BuildSystem::Meson => {
                if !build_dir.exists() {
                    match run_command(
                        "meson",
                        &[
                            "setup",
                            &format!(
                                "--buildtype={}",
                                match opts.build_mode {
                                    BuildMode::Debug => "debug",
                                    BuildMode::Release => "release",
                                },
                            ),
                            build_dir_str,
                        ],
                    ) {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            if build_dir.exists() {
                                std::fs::remove_dir_all(&build_dir)
                                    .with_context(|| "Couldn't clean up partial build directory")?;
                            }
                            Err(e)
                        }
                    }?;
                }

                run_command("meson", &["compile", "-C", build_dir_str])
            }
        },
        Subcommand::Run { executable, args } => {
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let args = args.as_slice();
            match build_system {
                BuildSystem::Make => run_command(&executable, args),
                BuildSystem::CMake => todo!(),
                BuildSystem::Meson => run_command(
                    build_dir
                        .join(executable)
                        .to_str()
                        .expect("Couldn't convert executable path to string"),
                    args,
                ),
            }
        }
        Subcommand::Clean => match build_system {
            BuildSystem::Make => run_command("make", &["clean"]),
            BuildSystem::CMake => todo!(),
            BuildSystem::Meson => {
                run_command("meson", &["compile", "-C", build_dir_str, "--clean"])
            }
        },
        Subcommand::Install { prefix } => {
            let prefix = match prefix {
                Some(x) => x,
                None => {
                    if nix::unistd::getuid().is_root() {
                        PathBuf::from_str("/usr/local/").unwrap()
                    } else {
                        dirs::executable_dir()
                            .with_context(|| "Couldn't get the user's executable directory")?
                    }
                }
            };

            match build_system {
                BuildSystem::Make => {
                    run_command("make", &["install", &format!("PREFIX={:?}", prefix)])
                }
                BuildSystem::CMake => todo!(),
                BuildSystem::Meson => {
                    run_command(
                        "meson",
                        &[
                            "configure",
                            "-D",
                            &format!("prefix={:?}", prefix),
                            build_dir_str,
                        ],
                    )?;
                    run_command("meson", &["install", "-C", build_dir_str])
                }
            }
        }
    }
}
