#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeDomain {
    Doctor,
    Install,
}

impl RuntimeDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::Install => "install",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionKey {
    pub engine: String,
    pub domain: RuntimeDomain,
    pub instance_id: String,
    pub agent_id: String,
    pub session_id: String,
}

impl RuntimeSessionKey {
    pub fn new(
        engine: impl Into<String>,
        domain: RuntimeDomain,
        instance_id: impl Into<String>,
        agent_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            engine: engine.into(),
            domain,
            instance_id: instance_id.into(),
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        }
    }

    pub fn storage_key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.engine,
            self.domain.as_str(),
            self.instance_id,
            self.agent_id,
            self.session_id
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorCode {
    RuntimeUnreachable,
    ConfigMissing,
    ModelUnavailable,
    SessionInvalid,
    TargetUnreachable,
    AuthExpired,
    AuthMisconfigured,
    RegistryCorrupt,
    InstanceOrphaned,
    TransportStale,
    Unknown,
}

impl RuntimeErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RuntimeUnreachable => "RUNTIME_UNREACHABLE",
            Self::ConfigMissing => "CONFIG_MISSING",
            Self::ModelUnavailable => "MODEL_UNAVAILABLE",
            Self::SessionInvalid => "SESSION_INVALID",
            Self::TargetUnreachable => "TARGET_UNREACHABLE",
            Self::AuthExpired => "AUTH_EXPIRED",
            Self::AuthMisconfigured => "AUTH_MISCONFIGURED",
            Self::RegistryCorrupt => "REGISTRY_CORRUPT",
            Self::InstanceOrphaned => "INSTANCE_ORPHANED",
            Self::TransportStale => "TRANSPORT_STALE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub message: String,
    pub action_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    ChatDelta { text: String },
    ChatFinal { text: String },
    Invoke { payload: Value },
    DiagnosisReport { items: Value },
    Error { error: RuntimeError },
    Status { text: String },
}

impl RuntimeEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ChatDelta { .. } => "chat-delta",
            Self::ChatFinal { .. } => "chat-final",
            Self::Invoke { .. } => "invoke",
            Self::DiagnosisReport { .. } => "diagnosis-report",
            Self::Error { .. } => "error",
            Self::Status { .. } => "status",
        }
    }

    pub fn chat_delta(text: String) -> Self {
        Self::ChatDelta { text }
    }

    pub fn chat_final(text: String) -> Self {
        Self::ChatFinal { text }
    }

    pub fn diagnosis_report(items: Value) -> Self {
        Self::DiagnosisReport { items }
    }
}

pub trait RuntimeAdapter {
    fn engine_name(&self) -> &'static str;
    fn start(
        &self,
        _key: &RuntimeSessionKey,
        _message: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError>;
    fn send(
        &self,
        _key: &RuntimeSessionKey,
        _message: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError>;
}
use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_error_codes_have_correct_string_repr() {
        assert_eq!(RuntimeErrorCode::AuthExpired.as_str(), "AUTH_EXPIRED");
        assert_eq!(
            RuntimeErrorCode::AuthMisconfigured.as_str(),
            "AUTH_MISCONFIGURED"
        );
        assert_eq!(
            RuntimeErrorCode::RegistryCorrupt.as_str(),
            "REGISTRY_CORRUPT"
        );
        assert_eq!(
            RuntimeErrorCode::InstanceOrphaned.as_str(),
            "INSTANCE_ORPHANED"
        );
        assert_eq!(RuntimeErrorCode::TransportStale.as_str(), "TRANSPORT_STALE");
    }
}
