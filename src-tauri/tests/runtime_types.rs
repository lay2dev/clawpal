use clawpal::runtime::types::{RuntimeDomain, RuntimeEvent, RuntimeSessionKey};

#[test]
fn runtime_session_key_contains_instance_scope() {
    let key = RuntimeSessionKey::new(
        "zeroclaw",
        RuntimeDomain::Doctor,
        "docker:local",
        "main",
        "s1",
    );
    assert_eq!(key.instance_id, "docker:local");
}

#[test]
fn runtime_event_has_stable_kinds() {
    let ev = RuntimeEvent::chat_final("hello".into());
    assert_eq!(ev.kind(), "chat-final");
}

#[test]
fn runtime_session_storage_key_includes_instance_id() {
    let key = RuntimeSessionKey::new("zeroclaw", RuntimeDomain::Doctor, "ssh:vm1", "main", "s1");
    assert_eq!(key.storage_key(), "zeroclaw:doctor:ssh:vm1:main:s1");
}
