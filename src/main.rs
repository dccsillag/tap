use std::{
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::{Context, Error, Result};
use clap::{AppSettings, ArgEnum, Parser};
use colored::Colorize;

#[derive(Parser)]
#[clap(version, author, about)]
struct Opts {
    /// Which build system to use. If unspecified, then this is automatically deduced
    #[clap(arg_enum, short, long)]
    build_system: Option<BuildSystem>,

    /// Which build mode to use
    #[clap(arg_enum, short = 'm', long, default_value = "debug", global = true)]
    build_mode: BuildMode,

    /// How many jobs to use for compilation.
    /// Defaults to the number of available threads
    #[clap(short, long, global = true)]
    n_jobs: Option<usize>,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
#[clap(setting(AppSettings::InferSubcommands))]
enum Subcommand {
    /// Build the project
    Build,

    /// Run an executable compiled by the project
    Run {
        /// Which executable to run
        executable: String,
        /// Arguments to be passed to the executable
        args: Vec<String>,
    },

    /// Clean build files
    Clean,

    /// Install built binaries
    Install {
        /// Where to install to.
        /// If run as root, this defaults to `/usr/local`. Otherwise, defaults to the parent
        /// directory of where executables should be installed for your user (usually `~/.local`)
        #[clap(long)]
        prefix: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ArgEnum)]
pub enum BuildSystem {
    Make,
    CMake,
    Meson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ArgEnum)]
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
            Self::ChangeDirectory { path } => path.to_string_lossy().into(),
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
    shell_words::join(std::iter::once(&command).chain(args.iter()))
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

fn perform_subcommand(
    subcommand: &Subcommand,
    build_system: BuildSystem,
    build_mode: BuildMode,
    n_jobs: usize,
) -> Result<()> {
    let build_dir = std::env::current_dir()
        .with_context(|| "Couldn't get current directory")?
        .join(match build_mode {
            BuildMode::Debug => ".tap_build_debug",
            BuildMode::Release => ".tap_build_release",
        });
    let build_dir_str = &build_dir.to_string_lossy().into_owned();

    match subcommand {
        Subcommand::Build => match build_system {
            BuildSystem::Make => match build_mode {
                BuildMode::Debug => run_command("make", &["-j", &n_jobs.to_string()]),
                BuildMode::Release => {
                    run_command("make", &["CFLAGS=-O3", "-j", &n_jobs.to_string()])
                }
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
                                match build_mode {
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

                run_command(
                    "meson",
                    &["compile", "-C", build_dir_str, "-j", &n_jobs.to_string()],
                )
            }
        },
        Subcommand::Run { executable, args } => {
            perform_subcommand(&Subcommand::Build, build_system, build_mode, n_jobs)
                .with_context(|| "While building the binary")?;

            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let args = args.as_slice();
            match build_system {
                BuildSystem::Make => run_command(executable, args),
                BuildSystem::CMake => todo!(),
                BuildSystem::Meson => {
                    run_command(&build_dir.join(executable).to_string_lossy(), args)
                }
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
            perform_subcommand(&Subcommand::Build, build_system, build_mode, n_jobs)
                .with_context(|| "While building the binary")?;

            if build_mode == BuildMode::Debug {
                println!("No build mode set, defaulting to debug mode.");
                println!("It is usually a good idea to install executables in release mode (by passing `-m release`).");
                println!("Are you sure you want to continue, in debug mode?");
                if !dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .interact()?
                {
                    return Err(Error::msg("Aborted."));
                }
            }

            let prefix = match prefix.as_ref() {
                Some(x) => x.to_owned(),
                None => {
                    if nix::unistd::getuid().is_root() {
                        PathBuf::from_str("/usr/local/").unwrap()
                    } else {
                        dirs::executable_dir()
                            .with_context(|| "Couldn't get the user's executable directory")?
                            .parent()
                            .ok_or_else(|| {
                                Error::msg(
                                    "Couldn't get the parent of the user's executable directory",
                                )
                            })?
                            .to_path_buf()
                    }
                }
            };

            match build_system {
                BuildSystem::Make => run_command(
                    "make",
                    &["install", &format!("PREFIX={}", prefix.to_string_lossy())],
                ),
                BuildSystem::CMake => todo!(),
                BuildSystem::Meson => {
                    run_command(
                        "meson",
                        &[
                            "configure",
                            "-D",
                            &format!("prefix={}", prefix.to_string_lossy()),
                            build_dir_str,
                        ],
                    )?;
                    run_command("meson", &["install", "-C", build_dir_str])
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    let build_system = match opts.build_system {
        Some(x) => x,
        None => BuildSystem::detect()
            .with_context(|| "Couldn't detect the build system")?
            .ok_or_else(|| Error::msg("Could not detect the build system"))?,
    };

    let n_jobs = match opts.n_jobs {
        Some(n) => n,
        None => num_cpus::get(),
    };

    perform_subcommand(&opts.subcommand, build_system, opts.build_mode, n_jobs)
}
