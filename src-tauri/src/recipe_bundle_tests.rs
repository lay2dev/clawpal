use crate::recipe_bundle::parse_recipe_bundle;

#[test]
fn recipe_bundle_rejects_unknown_execution_kind() {
    let raw = r#"apiVersion: strategy.platform/v1
kind: StrategyBundle
execution: { supportedKinds: [workflow] }"#;

    assert!(parse_recipe_bundle(raw).is_err());
}

#[test]
fn parse_valid_bundle_json() {
    let raw = r#"{
        "apiVersion": "strategy.platform/v1",
        "kind": "StrategyBundle",
        "execution": { "supportedKinds": ["job"] }
    }"#;
    let bundle = parse_recipe_bundle(raw).unwrap();
    assert_eq!(bundle.kind, "StrategyBundle");
    assert_eq!(bundle.execution.supported_kinds, vec!["job"]);
}

#[test]
fn parse_valid_bundle_yaml() {
    let raw = "apiVersion: strategy.platform/v1\nkind: StrategyBundle\nexecution:\n  supportedKinds: [service]";
    let bundle = parse_recipe_bundle(raw).unwrap();
    assert_eq!(bundle.execution.supported_kinds, vec!["service"]);
}

#[test]
fn parse_bundle_wrong_kind_rejected() {
    let raw = r#"{"apiVersion": "v1", "kind": "WrongKind"}"#;
    let err = parse_recipe_bundle(raw).unwrap_err();
    assert!(err.contains("unsupported document kind"), "{}", err);
}

#[test]
fn parse_bundle_invalid_syntax() {
    assert!(parse_recipe_bundle("not valid {{").is_err());
}

#[test]
fn parse_bundle_empty_execution_kinds_ok() {
    let raw = r#"{"apiVersion": "v1", "kind": "StrategyBundle"}"#;
    let bundle = parse_recipe_bundle(raw).unwrap();
    assert!(bundle.execution.supported_kinds.is_empty());
}

use crate::recipe_bundle::validate_recipe_bundle;
use crate::recipe_bundle::RecipeBundle;

#[test]
fn validate_bundle_rejects_wrong_kind() {
    let bundle = RecipeBundle {
        kind: "NotABundle".into(),
        ..Default::default()
    };
    assert!(validate_recipe_bundle(&bundle).is_err());
}

#[test]
fn validate_bundle_rejects_unknown_execution_kind_in_struct() {
    let bundle = RecipeBundle {
        kind: "StrategyBundle".into(),
        execution: crate::recipe_bundle::BundleExecution {
            supported_kinds: vec!["fantasy".into()],
        },
        ..Default::default()
    };
    assert!(validate_recipe_bundle(&bundle).is_err());
}
