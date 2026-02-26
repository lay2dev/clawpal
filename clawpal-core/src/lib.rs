pub mod connect;
pub mod health;
pub mod install;
pub mod instance;
pub mod openclaw;
pub mod profile;
pub mod ssh;

#[cfg(test)]
pub mod test_support {
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
