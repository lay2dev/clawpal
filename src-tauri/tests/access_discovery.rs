use clawpal::access_discovery::probe_engine::build_probe_plan_for_local;
use clawpal::access_discovery::store::AccessDiscoveryStore;
use clawpal::access_discovery::types::{CapabilityProfile, ExecutionExperience};
use clawpal::commands::ensure_access_profile_for_test;

#[test]
fn capability_profile_roundtrip() {
    let profile = CapabilityProfile::example_local("local");
    let text = serde_json::to_string(&profile).expect("serialize profile");
    let parsed: CapabilityProfile = serde_json::from_str(&text).expect("deserialize profile");
    assert_eq!(parsed.instance_id, "local");
    assert_eq!(parsed.transport, "local");
    assert_eq!(parsed.working_chain, vec!["openclaw", "--version"]);
}

#[test]
fn capability_profile_store_roundtrip() {
    let test_dir =
        std::env::temp_dir().join(format!("clawpal-access-discovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).expect("create test dir");
    let store = AccessDiscoveryStore::from_path(&test_dir);
    let profile = CapabilityProfile::example_local("docker:local");

    store.save_profile(&profile).expect("save profile");
    let loaded = store
        .load_profile("docker:local")
        .expect("load profile")
        .expect("profile should exist");

    assert_eq!(loaded.instance_id, "docker:local");
    assert_eq!(loaded.transport, "local");
    assert_eq!(loaded.probes.len(), 1);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn probe_plan_has_fallbacks() {
    let plan = build_probe_plan_for_local();
    assert!(!plan.is_empty());
    assert!(plan.iter().any(|p| p.contains("--version")));
}

#[test]
fn execution_experience_store_roundtrip() {
    let test_dir = std::env::temp_dir().join(format!(
        "clawpal-access-discovery-exp-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).expect("create test dir");
    let store = AccessDiscoveryStore::from_path(&test_dir);

    let count = store
        .save_experience(ExecutionExperience {
            instance_id: "docker:local".to_string(),
            goal: "install:docker".to_string(),
            transport: "docker_local".to_string(),
            method: "docker".to_string(),
            commands: vec!["docker compose up -d".to_string()],
            successful_chain: vec!["openclaw".to_string(), "--version".to_string()],
            recorded_at: 1,
        })
        .expect("save experience");
    assert_eq!(count, 1);

    let all = store
        .load_experiences("docker:local")
        .expect("load experiences");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].goal, "install:docker");
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn execution_experience_keeps_recent_five() {
    let test_dir = std::env::temp_dir().join(format!(
        "clawpal-access-discovery-exp-cap-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).expect("create test dir");
    let store = AccessDiscoveryStore::from_path(&test_dir);

    for idx in 0..7u64 {
        let count = store
            .save_experience(ExecutionExperience {
                instance_id: "docker:local".to_string(),
                goal: format!("install:docker:{idx}"),
                transport: "docker_local".to_string(),
                method: "docker".to_string(),
                commands: vec![format!("cmd-{idx}")],
                successful_chain: vec!["openclaw".to_string(), "--version".to_string()],
                recorded_at: idx,
            })
            .expect("save experience");
        assert!(count <= 5);
    }

    let all = store
        .load_experiences("docker:local")
        .expect("load experiences");
    assert_eq!(all.len(), 5);
    assert_eq!(
        all.first().map(|e| e.goal.as_str()),
        Some("install:docker:2")
    );
    assert_eq!(
        all.last().map(|e| e.goal.as_str()),
        Some("install:docker:6")
    );
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn execution_experience_updates_same_goal_instead_of_duplicating() {
    let test_dir = std::env::temp_dir().join(format!(
        "clawpal-access-discovery-exp-update-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).expect("create test dir");
    let store = AccessDiscoveryStore::from_path(&test_dir);

    store
        .save_experience(ExecutionExperience {
            instance_id: "local".to_string(),
            goal: "install:docker".to_string(),
            transport: "docker_local".to_string(),
            method: "docker".to_string(),
            commands: vec!["cmd-old".to_string()],
            successful_chain: vec!["openclaw".to_string()],
            recorded_at: 1,
        })
        .expect("save first");
    let count = store
        .save_experience(ExecutionExperience {
            instance_id: "local".to_string(),
            goal: "install:docker".to_string(),
            transport: "docker_local".to_string(),
            method: "docker".to_string(),
            commands: vec!["cmd-new".to_string()],
            successful_chain: vec!["openclaw".to_string(), "--version".to_string()],
            recorded_at: 2,
        })
        .expect("save updated");

    assert_eq!(count, 1);
    let all = store.load_experiences("local").expect("load experiences");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].commands, vec!["cmd-new".to_string()]);
    assert_eq!(all[0].recorded_at, 2);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[tokio::test]
async fn ensure_access_profile_falls_back_when_probe_fails() {
    let result = ensure_access_profile_for_test("local")
        .await
        .expect("ensure access profile should return result");
    assert!(result.used_legacy_fallback || !result.working_chain.is_empty());
}
