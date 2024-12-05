use crate::{collector::Collector, config::Config};
use nix::{
    sys::{
        stat::{fstat, futimens},
        time::TimeSpec,
    },
    time::ClockId,
};
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::PathBuf,
};
use sysinfo::System;

const TOUCH_FILENAME: &str = "system_info.touch";
const LOG_INTERVAL: i64 = 86400;

// The `SystemInfo` collector collects system information metrics
#[derive(Default)]
pub struct SystemInfo {
    // `SystemInfo` configuration (it's actually a `Collector` configuration)
    config: Config<Collector>,

    // In-memory tracking of the time between logging
    last_logged: Option<TimeSpec>,

    // `sysinfo` instance used to get the actual system information
    sysinfo: Option<System>,

    // Filesystem tracking of the time between logging
    //
    // The touchfile handle saves us from hitting the filesystem
    touch_filename: Option<String>,
    touch_filehandle: Option<File>,
}

impl SystemInfo {
    //
    // Accessors
    //

    // Get the filename of the touchfile
    pub fn touch_filename(&mut self) -> String {
        if self.touch_filename.is_none() {
            // touch_filename = FUELUP_TMP_DIR/system_info.touch
            self.touch_filename = Some(
                PathBuf::from(&self.config.tmp_dir())
                    .join(TOUCH_FILENAME)
                    .to_str()
                    .expect("Error building system info touch filename")
                    .to_string(),
            );
        }

        self.touch_filename
            .as_ref()
            .expect("Error getting system info touch filename")
            .to_string()
    }

    // Get the filehandle of the touchfile
    //
    // The touchfile is opened the first time this method it called. Also, the
    // file itself is created if it doesn't already exist
    pub fn touch_filehandle(&mut self) -> File {
        if self.touch_filehandle.is_none() {
            self.touch_filehandle = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.touch_filename())
                    .expect("Error opening system info touch file"),
            );
        }

        self.touch_filehandle
            .as_ref()
            .expect("Error getting system info touch file")
            .try_clone()
            .expect("Error cloning system info touch file")
    }

    // Get the `sysinfo` instance
    //
    // We only want a single instance of `sysinfo`, so we use a getter and setter
    // to ensure that we only have one instance at a time
    pub fn get_sysinfo(&mut self) -> System {
        if self.sysinfo.is_none() {
            self.sysinfo = Some(System::new_with_specifics(
                sysinfo::RefreshKind::nothing()
                // TODO: add more types here
            ));
        }

        self.sysinfo.take().expect("Error getting system info")
    }

    // Sets the `sysinfo` instance
    //
    // We only want a single instance of `sysinfo', so we use a getter and setter
    // to ensure that we only have one instance at a time
    pub fn set_sysinfo(&mut self, sysinfo: System) {
        self.sysinfo = Some(sysinfo);
    }

    //
    // Methods and helper functions
    //

    // Collect system info metrics if it's time to do so
    pub fn collect(&mut self) {
        let now = ClockId::CLOCK_REALTIME.now().expect("Error getting time");

        if self.should_log(&now) {
            self.update_touchfile_timestamp(&now);
            self.last_logged = Some(now);

            let sysinfo = self.get_sysinfo();

            // TODO: refresh `sysinfo` to get updated metrics

            // TODO: log metrics

            // Save the updated `sysinfo` instance
            self.set_sysinfo(sysinfo);
        }
    }

    // Checks if now is the time to log metrics
    fn should_log(&mut self, now: &TimeSpec) -> bool {
        if let Some(last_logged) = self.last_logged {
            if now.tv_sec() < last_logged.tv_sec() + LOG_INTERVAL {
                return false;
            }
        }

        // Instead of storing the last time we logged in the touchfile itself,
        // we can instead use the file's modified time attribute
        let stat = fstat(self.touch_filehandle().as_raw_fd()).expect("Error getting touchfile stat");
        now.tv_sec() >= stat.st_mtime + LOG_INTERVAL
    }

    // Update the touchfile's modified time attribute to the supplied time
    fn update_touchfile_timestamp(&mut self, now: &TimeSpec) {
        futimens(self.touch_filehandle().as_raw_fd(), &TimeSpec::UTIME_OMIT, now)
            .expect("Error setting touchfile modified time");
    }
}
