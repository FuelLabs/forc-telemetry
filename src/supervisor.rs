use crate::{
    config::{Config, ModuleConfig},
    detach_process, flush_stdio, follow_parent, setup_filesystem, setup_stdio,
};

use nix::{
    sys::{
        resource::{getrusage, UsageWho},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::getppid,
};
use std::{env::var, process::exit, thread::sleep, time::Duration};
use sysinfo::{Pid, System};

pub const LOG_FILENAME: &str = "supervisor.log";
pub const SLEEP_INTERVAL: Duration = Duration::from_secs(60);

// A `Supervisor` monitors and reports metrics on child and parent processes
#[derive(Default)]
pub struct Supervisor {
    // `Supervisor` configuration
    config: Config<Supervisor>,
}

impl ModuleConfig for Supervisor {
    fn log_filename() -> &'static str {
        LOG_FILENAME
    }
}

impl Supervisor {
    // Supervise a child process. This will fork then waitpid() on the child
    // process. Once the child exits, metrics will be record via getrusage()
    //
    // Note: this method is intended to be called from within Rust source code
    /*
    ```mermaid
    graph TD
        A(("supervise_child_process()")) --> B["var('FUEL_NO_TELEMETRY').is_ok()"]
        B --"true"--> C["return"]
        B --"false"--> D["flush_stdio()"]
        D --> E["follow_parent()"]
        E --> F{match}
        F --"Parent"--> G["setup_stdio(self.config.log_filename())"]
        G --> H["setup_filesystem()"]
        H --> I["waitpid(child_pid, Some(WaitPidFlag::WEXITED))"]
        I --> J{match}
        J --"Exited"--> K["status"]
        J --"Signaled"--> L["signal as i32"]
        J --"Unknown"--> M["eprintln!('Child exited with unknown status')"]
        M --> N["exit(1)"]
        K --> O["getrusage(UsageWho::RUSAGE_CHILDREN)"]
        O --> P["store metrics"]
        P --> Q["exit(0)"]
        L --> O
        F --"Child"--> R["return"]
    ```
     */
    pub fn supervise_child_process(&self) {
        // Allow the user to opt-out of telemetry
        if var("FUEL_NO_TELEMETRY").is_ok() {
            return;
        }

        // Flush stdio so that we don't have any buffered output
        flush_stdio();

        // Fork off the `Supervisor` to monitor the child process
        let Some(child_pid) = follow_parent() else {
            // We're the child process that the parent monitors and reports
            // metrics on, so return here and let the child continue unimpeded
            return;
        };

        // Setup the filesystem and stdio for the `Supervisor`
        setup_stdio(self.config.log_filename());
        setup_filesystem();

        // Block until the child process exits
        let _status_code = match waitpid(child_pid, Some(WaitPidFlag::WEXITED)) {
            Ok(WaitStatus::Exited(_, status)) => status,
            Ok(WaitStatus::Signaled(_, signal, _)) => signal as i32,
            _ => {
                eprintln!("Child exited with unknown status");
                exit(1);
            }
        };

        // Get the resource usage of the child process
        let _usage = getrusage(UsageWho::RUSAGE_CHILDREN).expect("Error getting resource usage");

        // TODO: log metrics
        //
        // Values are different on different systems, so we need to normalise them
        // https://stackoverflow.com/questions/59913657/strange-values-of-get-rusage-maxrss-on-macos-and-linux

        exit(0);
    }

    // Supervise the parent process. This will fork then wait for the parent to
    // exit. Once the parent exits, metrics will be recorded via `sysinfo`
    //
    // Note: this method is intended to be called from the `forc-telemetry`
    // binary
    /*
    ```mermaid
    flowchart
        A(("supervise_parent_process()")) --> B{"var('FUEL_NO_TELEMETRY')<br />.is_ok()"}
        B --"true"--> C["return"]
        B --"false"--> D["getppid().as_raw().try_into()"]
        D --> E{match}
        E --"Ok(parent_pid)"--> F["flush_stdio()"]
        E --"Err"--> G["eprintln!('Error getting parent process pid')"]
        G --> H["exit(1)"]
        F --> I["detach_process()"]
        I --> J["setup_stdio(self.config.log_filename())"]
        J --> K["setup_filesystem()"]
        K --> KK["sysinfo = System::new_with_specifics(...)"]
        KK --> loop

        subgraph loop
            direction TB
            LL["refresh sysinfo"]
            LL --> M{"Some(parent) <br />= sysinfo.process(parent_pid)"}
            M --"false"--> P["store final metrics"]
            M --"true"--> N["store metrics"]
            N --> O["sleep(SLEEP_INTERVAL)"]
            O --> LL
            P --> Q["exit(0)"]
        end
    ```
     */
    pub fn supervise_parent_process(&self) {
        if var("FUEL_NO_TELEMETRY").is_ok() {
            return;
        }

        // Get the pid of the parent process
        let Ok(parent_pid): Result<u32, _> = getppid().as_raw().try_into() else {
            // There was an error getting the parent process pid, but we want
            // to silently exit here as to not impede the calling process
            eprintln!("Error getting parent process pid");
            exit(1);
        };

        // Flush stdio so that we don't have any buffered output
        flush_stdio();

        // Perform the "double-fork" method to detach the process
        detach_process();

        // Setup the filesystem and stdio for the `Supervisor`
        setup_stdio(self.config.log_filename());
        setup_filesystem();

        // Get a `sysinfo` instance to collect parent process metrics
        let sysinfo = System::new_with_specifics(
            sysinfo::RefreshKind::nothing(), // TODO: add more types here
        );

        loop {
            // TODO: refresh `sysinfo` to get updated metrics

            if let Some(_parent_process) = sysinfo.process(Pid::from_u32(parent_pid)) {
                // TODO: log metrics of the parent process

                // Ensure the `Supervisor` isn't spinning
                sleep(SLEEP_INTERVAL);
                continue;
            };

            // TODO: log the final metrics of the parent process

            exit(0);
        }
    }
}
