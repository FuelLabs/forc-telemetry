pub mod collector;
pub mod config;
pub mod supervisor;

use collector::Collector;
use supervisor::Supervisor;

use nix::sys::stat;
use nix::unistd::{chdir, close, dup2, fork, setsid, sysconf, ForkResult, Pid, SysconfVar};
use std::fs;
use std::os::fd::AsRawFd;
use std::process::exit;
use std::{io, io::Write};

//
// API to the outside world
//

// Supervise a child process. This will fork then waitpid() on the child
// process. Once the child exits, metrics will be record via getrusage()
//
// Also starts a `Collector` so that metrics are pushed to the metrics server
//
// Note: this method is intended to be called from within Rust source code
pub fn supervise_child_process() {
    // Note: ordering of running the `Collector` and `Supervisor` is important
    //
    // To supervise a child process (e.g `forc-telemetry` is used as a library
    // from within Rust source code), then the `Collector` must be started
    // before the `Supervisor` is run. This is because the `Supervisor` needs to
    // report metrics on the forked off child.

    Collector::default().start();
    Supervisor::default().supervise_child_process();
}

// Supervise the parent process. This will fork then wait for the parent to
// exit. Once the parent exits, metrics will be recorded via `sysinfo`
//
// Also starts a `Collector` so that metrics are pushed to the metrics server
//
// Note: this method is intended to be called from the `forc-telemetry` binary
pub fn supervise_parent_process() {
    // Note: ordering of running the `Collector` and `Supervisor` is important
    //
    // To supervise a parent process (e.g `forc-telemetry` is used as a binary
    // called from within another program), then the `Supervisor` must be run
    // before the `Collector` is started. This is because the `Supervisor` needs
    // to report metrics on the direct parent process.

    if follow_parent().is_some() {
        // We're the `Supervisor` so we monitor the parent process
        Supervisor::default().supervise_parent_process();
    }
    else {
        // We're the child of the `Supervisor` so we start the `Collector`
        Collector::default().start();
    }

    // In case of any errors from either side of the fork, exit silently so we
    // don't impede the parent process
    exit(1);
}

//
// Functions for process management
//

// As part of daemonising a proces, we need to perform the "double-fork" method
// to safely detach the final process from its current parent and session
pub(crate) fn detach_process() {
    // To prevent us from becoming a zombie when we die, we kill the parent to
    // become the child so that we are automatically reaped by init or systemd
    //
    // Also, doing this guarantees that we are not the group leader, which is
    // required to create a new session (i.e setsid() will fail otherwise)
    kill_parent();

    // Creating a new session means we won't receive signals to the original
    // group or session (e.g. hitting CTRL-C to break a command pipeline)
    setsid().expect("Error creating new session");

    // As session leader, we now fork then follow the child again to guarantee
    // we cannot re-acquire a terminal
    kill_parent();
}

// Convenience function to fork then return the child PID
pub(crate) fn follow_parent() -> Option<Pid> {
    match unsafe { fork().expect("Error forking to follow parent") } {
        ForkResult::Parent { child } => Some(child),
        ForkResult::Child => None,
    }
}

// Convenience function to fork then kill the parent process
pub(crate) fn kill_parent() {
    match unsafe { fork().expect("Error forking to kill parent") } {
        ForkResult::Parent { .. } => exit(0),
        ForkResult::Child => {}
    }
}

//
// Functions for filesystem management
//

// As part of daemonising a process, we need to prepare the filesystem
// environment for safe access
pub(crate) fn setup_filesystem() {
    // The current working directory needs to be set to root so that we don't
    // prevent any unmounting of the filesystem leading up to the directory we
    // started in
    chdir("/").expect("Error changing directory to root");

    // We close all file descriptors since any currently opened were inherited
    // from the parent process which we don't care about. Not doing so leaks
    // open file descriptors which could lead to exhaustion
    //
    // We skip the first three because we deal with stdio later.

    // 1024 is a safe value i.e MIN(Legacy Linux, MacOS)
    let max_fd = sysconf(SysconfVar::OPEN_MAX)
        .expect("Error getting max open file descriptors system config")
        .unwrap_or(1024) as i32;

    for fd in 3..=max_fd {
        let _ = close(fd);
    }

    // Clear the umask so that files we create aren't too permission-restricive
    stat::umask(stat::Mode::empty());
}

//
// Functions for stdio management
//

// As part of daemonising a process, we need to flush stdout and stderr so that
// no buffered output is duplicated when we fork, which is a common problem
// when daemonising a process
pub(crate) fn flush_stdio() {
    io::stdout().flush().expect("Error flushing stdout");
    io::stderr().flush().expect("Error flushing stderr");
}

// As part of daemonising a process, we need to redirect stdio so that stderr
// is captured to a log file while stdio is completely discarded
pub(crate) fn setup_stdio(log_filename: &str) {
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_filename)
        .expect("Error opening log file");

    dup2(log_file.as_raw_fd(), 2).expect("Error redirecting stderr to supervisor log file");

    let dev_null = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .expect("Error opening /dev/null");

    let dev_null_fd = dev_null.as_raw_fd();
    dup2(dev_null_fd, 0).expect("Error redirecting stdin to /dev/null");
    dup2(dev_null_fd, 1).expect("Error redirecting stdout to /dev/null");
}
