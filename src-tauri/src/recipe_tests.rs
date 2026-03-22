use serde_json::{json, Map, Value};

use crate::recipe::{
    build_candidate_config_from_template, collect_change_paths, render_template_string,
    render_template_value, step_references_empty_param, validate, validate_recipe_source,
    RecipeParam, RecipeStep,
};

fn make_param(id: &str, required: bool) -> RecipeParam {
    RecipeParam {
        id: id.into(),
        label: id.into(),
        kind: "string".into(),
        required,
        pattern: None,
        min_length: None,
        max_length: None,
        placeholder: None,
        depends_on: None,
        default_value: None,
        options: None,
    }
}

fn make_recipe(params: Vec<RecipeParam>) -> crate::recipe::Recipe {
    crate::recipe::Recipe {
        id: "test".into(),
        name: "test".into(),
        description: "test".into(),
        version: "1.0.0".into(),
        tags: vec![],
        difficulty: "easy".into(),
        presentation: None,
        params,
        steps: vec![],
        clawpal_preset_maps: None,
        bundle: None,
        execution_spec_template: None,
    }
}

fn make_recipe_json(id: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "description": "test",
        "version": "1.0.0",
        "tags": [],
        "difficulty": "easy",
        "params": [],
        "steps": []
    })
}

// --- validate() ---

#[test]
fn validate_missing_required_param() {
    let recipe = make_recipe(vec![make_param("name", true)]);
    let errors = validate(&recipe, &Map::new());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("missing required param: name"));
}

#[test]
fn validate_optional_param_absent_ok() {
    let recipe = make_recipe(vec![make_param("name", false)]);
    assert!(validate(&recipe, &Map::new()).is_empty());
}

#[test]
fn validate_param_min_length() {
    let mut p = make_param("name", true);
    p.min_length = Some(3);
    let recipe = make_recipe(vec![p]);
    let mut params = Map::new();
    params.insert("name".into(), Value::String("ab".into()));
    assert!(validate(&recipe, &params)[0].contains("too short"));
}

#[test]
fn validate_param_max_length() {
    let mut p = make_param("name", true);
    p.max_length = Some(5);
    let recipe = make_recipe(vec![p]);
    let mut params = Map::new();
    params.insert("name".into(), Value::String("toolong".into()));
    assert!(validate(&recipe, &params)[0].contains("too long"));
}

#[test]
fn validate_param_pattern_mismatch() {
    let mut p = make_param("email", true);
    p.pattern = Some(r"^[a-z]+$".into());
    let recipe = make_recipe(vec![p]);
    let mut params = Map::new();
    params.insert("email".into(), Value::String("ABC123".into()));
    assert!(validate(&recipe, &params)
        .iter()
        .any(|e| e.contains("not match pattern")));
}

#[test]
fn validate_param_non_string_rejected() {
    let recipe = make_recipe(vec![make_param("count", true)]);
    let mut params = Map::new();
    params.insert("count".into(), json!(42));
    assert!(validate(&recipe, &params)
        .iter()
        .any(|e| e.contains("must be string")));
}

// --- render_template_string() ---

#[test]
fn render_template_simple() {
    let mut p = Map::new();
    p.insert("name".into(), Value::String("Alice".into()));
    assert_eq!(
        render_template_string("Hello {{name}}!", &p),
        "Hello Alice!"
    );
}

#[test]
fn render_template_missing_key_unchanged() {
    assert_eq!(
        render_template_string("Hello {{name}}!", &Map::new()),
        "Hello {{name}}!"
    );
}

#[test]
fn render_template_multiple() {
    let mut p = Map::new();
    p.insert("a".into(), Value::String("1".into()));
    p.insert("b".into(), Value::String("2".into()));
    assert_eq!(render_template_string("{{a}}-{{b}}", &p), "1-2");
}

// --- render_template_value() ---

#[test]
fn render_value_string_interpolation() {
    let mut p = Map::new();
    p.insert("x".into(), Value::String("val".into()));
    assert_eq!(
        render_template_value(&json!("prefix-{{x}}"), &p, None),
        json!("prefix-val")
    );
}

#[test]
fn render_value_exact_placeholder_preserves_type() {
    let mut p = Map::new();
    p.insert("x".into(), json!(42));
    assert_eq!(render_template_value(&json!("{{x}}"), &p, None), json!(42));
}

#[test]
fn render_value_array() {
    let mut p = Map::new();
    p.insert("a".into(), Value::String("1".into()));
    assert_eq!(
        render_template_value(&json!(["{{a}}", "static"]), &p, None),
        json!(["1", "static"])
    );
}

#[test]
fn render_value_object() {
    let mut p = Map::new();
    p.insert("k".into(), Value::String("val".into()));
    assert_eq!(
        render_template_value(&json!({"key": "{{k}}"}), &p, None),
        json!({"key": "val"})
    );
}

#[test]
fn render_value_preset_map() {
    let mut p = Map::new();
    p.insert("provider".into(), Value::String("openai".into()));
    let mut pm = Map::new();
    pm.insert(
        "provider".into(),
        json!({"openai": {"url": "https://api.openai.com"}}),
    );
    assert_eq!(
        render_template_value(&json!("{{presetMap:provider}}"), &p, Some(&pm)),
        json!({"url": "https://api.openai.com"})
    );
}

#[test]
fn render_value_preset_map_missing_selection_returns_empty() {
    let mut p = Map::new();
    p.insert("provider".into(), Value::String("unknown".into()));
    let mut pm = Map::new();
    pm.insert("provider".into(), json!({"openai": "yes"}));
    assert_eq!(
        render_template_value(&json!("{{presetMap:provider}}"), &p, Some(&pm)),
        json!("")
    );
}

#[test]
fn render_value_non_string_passthrough() {
    let p = Map::new();
    assert_eq!(render_template_value(&json!(42), &p, None), json!(42));
    assert_eq!(render_template_value(&json!(true), &p, None), json!(true));
    assert_eq!(render_template_value(&json!(null), &p, None), json!(null));
}

// --- validate_recipe_source() ---

#[test]
fn validate_recipe_source_valid() {
    let src = serde_json::to_string(&make_recipe_json("r1")).unwrap();
    let d = validate_recipe_source(&src).unwrap();
    assert!(d.errors.is_empty());
}

#[test]
fn validate_recipe_source_invalid_json() {
    let d = validate_recipe_source("not json {{{").unwrap();
    assert!(!d.errors.is_empty());
    assert_eq!(d.errors[0].category, "parse");
}

#[test]
fn validate_recipe_source_empty() {
    let d = validate_recipe_source("").unwrap();
    assert!(!d.errors.is_empty());
}

// --- load_recipes_from_source_text() ---

#[test]
fn load_source_text_empty_error() {
    assert!(crate::recipe::load_recipes_from_source_text("").is_err());
}

#[test]
fn load_source_text_single() {
    let src = serde_json::to_string(&make_recipe_json("r")).unwrap();
    let r = crate::recipe::load_recipes_from_source_text(&src).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].id, "r");
}

#[test]
fn load_source_text_list() {
    let src =
        serde_json::to_string(&json!([make_recipe_json("a"), make_recipe_json("b")])).unwrap();
    assert_eq!(
        crate::recipe::load_recipes_from_source_text(&src)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn load_source_text_wrapped() {
    let src = serde_json::to_string(&json!({"recipes": [make_recipe_json("x")]})).unwrap();
    assert_eq!(
        crate::recipe::load_recipes_from_source_text(&src)
            .unwrap()
            .len(),
        1
    );
}

// --- builtin_recipes() ---

#[test]
fn builtin_recipes_non_empty_unique_ids() {
    let recipes = crate::recipe::builtin_recipes();
    assert!(!recipes.is_empty());
    let mut ids: Vec<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
    let original_len = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), original_len, "duplicate recipe IDs");
}

// --- step_references_empty_param() ---

#[test]
fn step_refs_empty_param_true() {
    let step = RecipeStep {
        action: "test".into(),
        label: "test".into(),
        args: {
            let mut m = Map::new();
            m.insert("cmd".into(), json!("run {{name}}"));
            m
        },
    };
    let mut p = Map::new();
    p.insert("name".into(), Value::String("".into()));
    assert!(step_references_empty_param(&step, &p));
}

#[test]
fn step_refs_nonempty_param_false() {
    let step = RecipeStep {
        action: "test".into(),
        label: "test".into(),
        args: {
            let mut m = Map::new();
            m.insert("cmd".into(), json!("run {{name}}"));
            m
        },
    };
    let mut p = Map::new();
    p.insert("name".into(), Value::String("alice".into()));
    assert!(!step_references_empty_param(&step, &p));
}

// --- build_candidate_config_from_template() ---

#[test]
fn candidate_config_adds_new_key() {
    let mut p = Map::new();
    p.insert("val".into(), Value::String("hello".into()));
    let (merged, changes) = build_candidate_config_from_template(
        &json!({"existing": true}),
        r#"{"newKey": "{{val}}"}"#,
        &p,
    )
    .unwrap();
    assert_eq!(merged["newKey"], "hello");
    assert_eq!(merged["existing"], true);
    assert!(changes.iter().any(|c| c.op == "add"));
}

#[test]
fn candidate_config_replaces_existing() {
    let (merged, changes) =
        build_candidate_config_from_template(&json!({"k": "old"}), r#"{"k": "new"}"#, &Map::new())
            .unwrap();
    assert_eq!(merged["k"], "new");
    assert!(changes.iter().any(|c| c.op == "replace"));
}

// --- collect_change_paths() ---

#[test]
fn change_paths_identical_empty() {
    assert!(collect_change_paths(&json!({"a": 1}), &json!({"a": 1})).is_empty());
}

#[test]
fn change_paths_different_returns_root() {
    let c = collect_change_paths(&json!({"a": 1}), &json!({"a": 2}));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].path, "root");
}
