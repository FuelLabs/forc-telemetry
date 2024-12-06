use dirs::home_dir;
use std::{env::var_os, fs::create_dir_all, marker::PhantomData, path::PathBuf};

// Environment variables
pub const FUELUP_HOME: &str = "FUELUP_HOME";
pub const FUELUP_LOG: &str = "FUELUP_LOG";
pub const FUELUP_TMP: &str = "FUELUP_TMP";

// Directory names
pub const FUELUP_HOME_DIR: &str = ".fuelup";
pub const FUELUP_LOG_DIR: &str = "log";
pub const FUELUP_TMP_DIR: &str = "tmp";

// A trait to provide module-specific configuration. By implementing this trait,
// `Default::default()` will set module configuration automatically
//
// As an example, we can create a `Supervisor` without having to implement the
// the `log_filename()` method ourselves:
//
// ```rust
// Supervisor::default().config.log_filename();
// ```
pub trait ModuleConfig {
    fn log_filename() -> &'static str;
}

// By each module having its own `Config` struct, it can provide module-specific
// configuration values
pub struct Config<T: ModuleConfig> {
    // Module-specific log filename
    log_filename: String,

    // The temporary directory to use for temporary files
    tmp_dir: String,

    // T is the implementing module
    _phantom: PhantomData<T>,
}

// General accessors for configuration items
impl<T: ModuleConfig> Config<T> {
    pub fn log_filename(&self) -> &str {
        &self.log_filename
    }

    pub fn tmp_dir(&self) -> &str {
        &self.tmp_dir
    }
}

// `Default` for `Config` which also sets module-specific configuration
impl<T: ModuleConfig> Default for Config<T> {
    fn default() -> Self {
        // fuelup_home_dir = FUELUP_HOME || $HOME/.fuelup
        let fuelup_home_dir = var_os(FUELUP_HOME)
            .unwrap_or_else(|| {
                home_dir()
                    .expect("Error getting home directory")
                    .join(FUELUP_HOME_DIR)
                    .into_os_string()
            })
            .into_string()
            .expect("Error getting FUELUP_HOME directory");

        // log_filename = FUELUP_LOG || $HOME/.fuelup/log/<module::log_filename()>.log
        let log_filename = {
            let fuelup_log_dir = var_os(FUELUP_LOG)
                .unwrap_or_else(|| {
                    PathBuf::from(&fuelup_home_dir)
                        .join(FUELUP_LOG_DIR)
                        .into_os_string()
                })
                .into_string()
                .expect("Error getting FUELUP_LOG directory");

            // Ensure the FUELUP_LOG directory exists
            create_dir_all(&fuelup_log_dir).expect("Error creating FUELUP_LOG directory");

            // Note: the filename is pulled from the implementing module
            PathBuf::from(fuelup_log_dir)
                .join(T::log_filename())
                .to_str()
                .expect("Error getting log filename")
                .to_string()
        };

        // tmp_dir = FUELUP_TMP || $HOME/.fuelup/tmp
        let tmp_dir = {
            let fuelup_tmp_dir = var_os(FUELUP_TMP)
                .unwrap_or_else(|| {
                    PathBuf::from(&fuelup_home_dir)
                        .join(FUELUP_TMP_DIR)
                        .into_os_string()
                })
                .into_string()
                .expect("Error getting FUELUP_TMP directory");

            // Ensure the FUELUP_TMP directory exists
            create_dir_all(&fuelup_tmp_dir).expect("Error creating FUELUP_TMP directory");

            fuelup_tmp_dir
        };

        Self {
            log_filename,
            tmp_dir,
            _phantom: PhantomData,
        }
    }
}
