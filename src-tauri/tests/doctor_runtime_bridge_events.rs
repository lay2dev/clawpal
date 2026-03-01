use clawpal::doctor_runtime_bridge::map_runtime_event_name;
use clawpal::runtime::types::RuntimeEvent;

#[test]
fn doctor_event_mapping_is_stable() {
    assert_eq!(
        map_runtime_event_name(&RuntimeEvent::chat_delta("x".into())),
        "doctor:chat-delta"
    );
    assert_eq!(
        map_runtime_event_name(&RuntimeEvent::chat_final("x".into())),
        "doctor:chat-final"
    );
}
