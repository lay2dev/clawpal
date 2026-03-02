use clawpal::runtime::types::RuntimeErrorCode;

#[test]
fn runtime_error_codes_cover_core_recovery_paths() {
    assert_eq!(RuntimeErrorCode::ConfigMissing.as_str(), "CONFIG_MISSING");
    assert_eq!(
        RuntimeErrorCode::ModelUnavailable.as_str(),
        "MODEL_UNAVAILABLE"
    );
}
