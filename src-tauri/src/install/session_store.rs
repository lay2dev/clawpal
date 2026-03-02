use super::types::InstallSession;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct InstallSessionStore {
    sessions: Mutex<HashMap<String, InstallSession>>,
}

impl InstallSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, session: InstallSession) -> Result<(), String> {
        let mut guard = self
            .sessions
            .lock()
            .map_err(|_| "install session store lock poisoned".to_string())?;
        guard.insert(session.id.clone(), session);
        Ok(())
    }

    pub fn upsert(&self, session: InstallSession) -> Result<(), String> {
        self.insert(session)
    }

    pub fn get(&self, session_id: &str) -> Result<Option<InstallSession>, String> {
        let guard = self
            .sessions
            .lock()
            .map_err(|_| "install session store lock poisoned".to_string())?;
        Ok(guard.get(session_id).cloned())
    }
}
