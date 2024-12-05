use indoc::indoc;
use libproc::proc_pid::pidpath;
use nix::unistd::getppid;
use std::{env::args, path::PathBuf, process::exit};

const SHELLS: [&str; 4] = ["bash", "fish", "sh", "zsh"];

fn main() {
    if args().count() > 1 {
        exit_with_help();
    }

    let parent_path = get_parent_process_path_or_exit();

    if SHELLS.iter().any(|&shell| parent_path.ends_with(shell)) {
        eprintln!("Error: not to be run directly. Please run from within other programs");
        exit(1);
    }

    forc_telemetry::supervise_parent_process();
}

fn get_parent_process_path_or_exit() -> PathBuf {
    match pidpath(getppid().as_raw()) {
        Ok(parent_path) => PathBuf::from(parent_path),
        _ => {
            // There was an error getting the parent process path, but we want
            // to silently exit here as to not impede the calling process
            exit(1);
        }
    }
}

fn exit_with_help() {
    eprintln!(indoc! {r#"
        usage: forc-telemetry

        This program is not to be run directly and instead should be run from within other programs.

        forc-telemetry when run from another program will collect metrics on the calling program and send
        them to our metrics server. This will allow us to better understand how our programs are used
        meaning we can make better decisions on how to improve them in the future.

        To opt-out of telemetry, set the environment variable "FUEL_NO_TELEMETRY" e.g:

            export FUEL_NO_TELEMETRY=1
    "#});

    exit(1);
}
