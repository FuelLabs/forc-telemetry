mod system_info;

use crate::{
    collector::system_info::SystemInfo,
    config::{Config, ModuleConfig},
    detach_process, flush_stdio, setup_filesystem, setup_stdio,
};

use nix::{
    errno::Errno,
    fcntl::{Flock, FlockArg},
};
use std::{
    env::var,
    fs::{read_dir, remove_file, File, OpenOptions},
    io::Read,
    process::exit,
    thread::sleep,
    time::Duration,
};

const LOG_FILENAME: &str = "collector.log";
const SLEEP_INTERVAL: Duration = Duration::from_secs(60);

// The`Collector` pushes metrics files to the metrics server
#[derive(Default)]
pub struct Collector {
    // `Collector` configuration
    config: Config<Collector>,

    // Lockfile for the `Collector` to enforce a singleton
    logfile_lock: Option<Flock<File>>,

    // A `SystemInfo` instance for the `Collector`
    system_info: SystemInfo,
}

impl ModuleConfig for Collector {
    fn log_filename() -> &'static str {
        LOG_FILENAME
    }
}

impl Collector {
    // Start the `Collector` daemon. This will perform the "double-fork" method
    // to daemonise, then will loop to collect system info metrics and send
    // all stored metrics to the metrics server
    pub fn start(&mut self) {
        // Allow the user to opt-out of telemetry
        if var("FUEL_NO_TELEMETRY").is_ok() {
            return;
        }

        // Flush stdio so that we don't have any buffered output
        flush_stdio();

        // Perform the "double-fork" method to detach the process
        detach_process();

        // Setup the filesystem and stdio for the `Collector`
        setup_stdio(self.config.log_filename());
        setup_filesystem();

        loop {
            // Make sure only one `Collector` is running at one time
            self.enforce_singleton();

            // Collect system info then send all metrics to the metrics server
            self.system_info.collect();
            self.send_metrics_files();

            // Ensure the `Collector` isn't spinning
            sleep(SLEEP_INTERVAL);
        }
    }

    // Enforce a singleton `Collector` by locking the logfile
    fn enforce_singleton(&mut self) {
        // Since these are advisory locks, we can't trust that the lockfile
        // hasn't been cleaned up from under us, so check every time
        self.logfile_lock = None;

        let logfile = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.config.log_filename())
            .expect("Error opening logfile for locking");

        self.logfile_lock = match Flock::lock(logfile, FlockArg::LockExclusiveNonblock) {
            Ok(logfile_lock) => Some(logfile_lock),
            Err((_, Errno::EWOULDBLOCK)) => {
                // Silently exit as another Collector is already running
                exit(0);
            }
            Err((_, err)) => {
                eprintln!("Error locking logfile: {err}");
                exit(1);
            }
        };
    }

    // Send all metrics files to the metrics server
    fn send_metrics_files(&self) {
        // It would be nice to use something like inotify rather than polling
        // the directory each time, however nothing is portable without being
        // buggy in non-native environments like docker, WSL, etc

        for entry in read_dir(self.config.tmp_dir()).expect("Error reading directory") {
            let path = entry.expect("Error reading directory entry").path();
            let filename = path
                .file_name()
                .expect("Error getting filename")
                .to_str()
                .expect("Error converting filename to string");

            // We only care about files of the form "FUELUP_TMP_DIR/metrics*"
            if filename.starts_with("metrics") {
                let file = OpenOptions::new()
                    .read(true)
                    .open(filename)
                    .expect("Error opening file");

                // Skip locked files as another process is still writing metrics
                let mut locked_file = match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                    Ok(locked_file) => locked_file,
                    Err((_, Errno::EWOULDBLOCK)) => continue,
                    Err((_, e)) => {
                        eprintln!("Error locking metrics file: {e}");
                        exit(1);
                    }
                };

                let mut buf = String::new();
                locked_file
                    .read_to_string(&mut buf)
                    .expect("Error reading file");

                // TODO: send metrics to server

                remove_file(filename).expect("Error removing metrics file");
            }
        }
    }
}

// TODO: tests will need to be serialized since the Collector is a singleton
