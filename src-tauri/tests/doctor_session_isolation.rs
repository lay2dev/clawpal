use clawpal::runtime::zeroclaw::session::{append_history, history_len, reset_history};

#[test]
fn doctor_sessions_are_isolated_by_instance_id() {
    let k1 = "zeroclaw:doctor:local:main:s1";
    let k2 = "zeroclaw:doctor:docker:local:main:s1";

    reset_history(k1);
    reset_history(k2);
    append_history(k1, "user", "hello local");
    append_history(k1, "assistant", "ok local");
    append_history(k2, "user", "hello docker");

    assert_eq!(history_len(k1), 2);
    assert_eq!(history_len(k2), 1);
}
