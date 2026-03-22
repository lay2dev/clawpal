use crate::recipe_action_catalog::{find_recipe_action, list_recipe_actions};

#[test]
fn catalog_non_empty() {
    assert!(!list_recipe_actions().is_empty());
}

#[test]
fn catalog_unique_kinds() {
    let actions = list_recipe_actions();
    let mut kinds: Vec<&str> = actions.iter().map(|e| e.kind.as_str()).collect();
    let original_len = kinds.len();
    kinds.sort();
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        original_len,
        "duplicate action kinds in catalog"
    );
}

#[test]
fn catalog_all_have_required_fields() {
    for entry in list_recipe_actions() {
        assert!(!entry.kind.is_empty(), "empty kind");
        assert!(!entry.title.is_empty(), "empty title for {}", entry.kind);
        assert!(!entry.group.is_empty(), "empty group for {}", entry.kind);
        assert!(
            !entry.category.is_empty(),
            "empty category for {}",
            entry.kind
        );
        assert!(
            !entry.backend.is_empty(),
            "empty backend for {}",
            entry.kind
        );
        assert!(
            !entry.description.is_empty(),
            "empty description for {}",
            entry.kind
        );
    }
}

#[test]
fn find_known_action() {
    assert!(find_recipe_action("create_agent").is_some());
    assert!(find_recipe_action("bind_agent").is_some());
}

#[test]
fn find_unknown_action_returns_none() {
    assert!(find_recipe_action("nonexistent_action_xyz").is_none());
}

#[test]
fn legacy_aliases_point_to_existing_kinds() {
    let actions = list_recipe_actions();
    let kinds: Vec<&str> = actions.iter().map(|e| e.kind.as_str()).collect();
    for entry in &actions {
        if let Some(ref alias_of) = entry.legacy_alias_of {
            assert!(
                kinds.contains(&alias_of.as_str()),
                "legacy_alias_of '{}' on '{}' does not reference an existing action kind",
                alias_of,
                entry.kind,
            );
        }
    }
}

#[test]
fn read_only_actions_have_no_capabilities() {
    for entry in list_recipe_actions() {
        if entry.read_only {
            assert!(
                entry.capabilities.is_empty(),
                "read-only action '{}' should not declare capabilities",
                entry.kind,
            );
        }
    }
}
