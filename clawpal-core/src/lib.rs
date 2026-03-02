pub mod backup;
pub mod config;
pub mod connect;
pub mod cron;
pub mod discovery;
pub mod doctor;
pub mod health;
pub mod install;
pub mod instance;
pub mod openclaw;
pub mod precheck;
pub mod profile;
pub mod sessions;
pub mod shell;
pub mod ssh;
pub mod watchdog;

#[cfg(test)]
pub mod test_support {
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
