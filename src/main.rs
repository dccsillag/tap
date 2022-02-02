use std::{borrow::BorrowMut, path::PathBuf, process::Command};

use anyhow::{Context, Error, Result};
use clap::{ArgEnum, Parser};

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

impl BuildSystem {
    pub fn detect() -> Option<Self> {
        todo!("build system detection")
    }
}

fn run_command(mut command: impl BorrowMut<Command>) -> Result<()> {
    let command: &mut Command = command.borrow_mut();

    {
        println!("   {:?}", command);

        let exit_status = command
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
    .with_context(|| format!("While running command {:?}", command))
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    let build_system = match opts.build_system {
        Some(x) => x,
        None => BuildSystem::detect().ok_or(Error::msg("Could not detect the build system"))?,
    };

    match opts.subcommand {
        Subcommand::Build => match build_system {
            BuildSystem::Make => match opts.build_mode {
                BuildMode::Debug => run_command(Command::new("make")),
                BuildMode::Release => run_command(Command::new("make").arg("CFLAGS=-O3")),
            },
            BuildSystem::CMake => todo!(),
            BuildSystem::Meson => todo!(),
        },
        Subcommand::Run { executable, args } => match build_system {
            BuildSystem::Make => run_command(Command::new(executable).args(args)),
            BuildSystem::CMake => todo!(),
            BuildSystem::Meson => todo!(),
        },
        Subcommand::Clean => match build_system {
            BuildSystem::Make => run_command(Command::new("make").arg("clean")),
            BuildSystem::CMake => todo!(),
            BuildSystem::Meson => todo!(),
        },
        Subcommand::Install { prefix } => {
            let prefix = match prefix {
                Some(x) => x,
                None => todo!("get default prefix based on user"),
            };

            match build_system {
                BuildSystem::Make => run_command(
                    Command::new("make")
                        .arg("install")
                        .arg(format!("PREFIX={:?}", prefix)),
                ),
                BuildSystem::CMake => todo!(),
                BuildSystem::Meson => todo!(),
            }
        }
    }
}
