use openclaw_gateway_client::auth_store::{AuthStore, DeviceTokenRecord, FileAuthStore};
use tempfile::tempdir;

#[test]
fn saves_and_loads_device_token_by_device_and_role() {
    let dir = tempdir().expect("tempdir");
    let store = FileAuthStore::new(dir.path().to_path_buf());
    let record = DeviceTokenRecord {
        token: "device-token".into(),
        scopes: vec!["operator.read".into()],
    };

    store
        .store("device-1", "node", &record)
        .expect("store token");

    let loaded = store.load("device-1", "node").expect("load token");
    assert_eq!(loaded, Some(record));
}

#[test]
fn clears_device_token() {
    let dir = tempdir().expect("tempdir");
    let store = FileAuthStore::new(dir.path().to_path_buf());
    let record = DeviceTokenRecord {
        token: "device-token".into(),
        scopes: vec![],
    };

    store
        .store("device-1", "node", &record)
        .expect("store token");
    store.clear("device-1", "node").expect("clear token");

    let loaded = store.load("device-1", "node").expect("load token");
    assert_eq!(loaded, None);
}
