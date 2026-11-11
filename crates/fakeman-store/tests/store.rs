use fakeman_store::{BodyFormat, EndpointRecord, Error, FieldRecord, HeaderRecord, Store};
use rusqlite::Connection;
use serde_json::json;

fn field(name: &str, key: &str, params: serde_json::Value, enabled: bool) -> FieldRecord {
    FieldRecord {
        name: name.into(),
        generator_key: key.into(),
        params,
        enabled,
    }
}

fn header(key: &str, value: &str, enabled: bool) -> HeaderRecord {
    HeaderRecord {
        key: key.into(),
        value: value.into(),
        enabled,
    }
}

fn filled(store: &mut Store, name: &str) -> EndpointRecord {
    let mut endpoint = store.create(name).unwrap();
    endpoint.url = "https://example.com/users".into();
    endpoint.body_format = BodyFormat::Form;
    endpoint.run_count = 10;
    endpoint.delay_ms = 500;
    endpoint.fields = vec![
        field("firstname", "person.firstName", json!({}), true),
        field("address.city", "location.city", json!({}), false),
        field("age", "number.int", json!({ "min": 18, "max": 99 }), true),
        field(
            "note",
            "future.unknownGenerator",
            json!({ "a": [1, "x", null] }),
            true,
        ),
    ];
    endpoint.headers = vec![
        header("Authorization", "Bearer abc", true),
        header("X-Debug", "1", false),
    ];
    store.save(&endpoint).unwrap();
    endpoint
}

fn names(store: &Store) -> Vec<String> {
    store
        .list_endpoints()
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

fn count(conn: &Connection, table: &str, id: i64) -> i64 {
    conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE endpoint_id = ?1"),
        [id],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn create_uses_defaults() {
    let mut store = Store::open_in_memory().unwrap();
    let endpoint = store.create("New request").unwrap();
    assert_eq!(endpoint.name, "New request");
    assert_eq!(endpoint.method, "POST");
    assert_eq!(endpoint.url, "");
    assert_eq!(endpoint.body_format, BodyFormat::Json);
    assert_eq!(endpoint.run_count, 1);
    assert_eq!(endpoint.delay_ms, 0);
    assert!(endpoint.fields.is_empty() && endpoint.headers.is_empty());
    assert!(!endpoint.created_at.is_empty());
}

#[test]
fn save_round_trips_fields_and_headers_in_order() {
    let mut store = Store::open_in_memory().unwrap();
    let saved = filled(&mut store, "Signup");
    let loaded = store.get_endpoint(saved.id).unwrap();
    assert_eq!(
        loaded,
        EndpointRecord {
            updated_at: loaded.updated_at.clone(),
            ..saved
        }
    );
}

#[test]
fn save_replaces_fields_and_headers() {
    let mut store = Store::open_in_memory().unwrap();
    let mut endpoint = filled(&mut store, "Signup");
    endpoint.fields = vec![
        field("email", "internet.email", json!({}), true),
        field("firstname", "person.firstName", json!({}), true),
    ];
    endpoint.headers.clear();
    endpoint.name = "Signup v2".into();
    store.save(&endpoint).unwrap();

    let loaded = store.get_endpoint(endpoint.id).unwrap();
    assert_eq!(loaded.name, "Signup v2");
    assert_eq!(loaded.fields, endpoint.fields);
    assert!(loaded.headers.is_empty());
}

#[test]
fn failed_save_changes_nothing() {
    let mut store = Store::open_in_memory().unwrap();
    let original = filled(&mut store, "Signup");
    let mut broken = original.clone();
    broken.run_count = 0;
    broken.fields.clear();
    assert!(matches!(store.save(&broken), Err(Error::Sqlite(_))));

    let loaded = store.get_endpoint(original.id).unwrap();
    assert_eq!(loaded.run_count, 10);
    assert_eq!(loaded.fields, original.fields);
}

#[test]
fn missing_endpoints_are_not_found() {
    let mut store = Store::open_in_memory().unwrap();
    let ghost = store.create("Ghost").unwrap();
    store.delete(ghost.id).unwrap();
    assert!(matches!(
        store.get_endpoint(ghost.id),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(store.save(&ghost), Err(Error::NotFound(_))));
    assert!(matches!(
        store.rename(ghost.id, "x"),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(store.duplicate(ghost.id), Err(Error::NotFound(_))));
    assert!(matches!(store.delete(ghost.id), Err(Error::NotFound(_))));
}

#[test]
fn rename_changes_only_the_name() {
    let mut store = Store::open_in_memory().unwrap();
    let endpoint = filled(&mut store, "Signup");
    store.rename(endpoint.id, "Register").unwrap();
    let loaded = store.get_endpoint(endpoint.id).unwrap();
    assert_eq!(loaded.name, "Register");
    assert_eq!(loaded.fields, endpoint.fields);
    assert_eq!(loaded.url, endpoint.url);
}

#[test]
fn list_follows_creation_then_reorder() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store.create("A").unwrap().id;
    let b = store.create("B").unwrap().id;
    let c = store.create("C").unwrap().id;
    assert_eq!(names(&store), ["A", "B", "C"]);

    store.reorder(&[c, a, b]).unwrap();
    assert_eq!(names(&store), ["C", "A", "B"]);
    let orders: Vec<i64> = store
        .list_endpoints()
        .unwrap()
        .iter()
        .map(|e| e.sort_order)
        .collect();
    assert_eq!(orders, [0, 1, 2]);
}

#[test]
fn reorder_rejects_incomplete_or_unknown_ids() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store.create("A").unwrap().id;
    let b = store.create("B").unwrap().id;
    for bad in [vec![b], vec![b, b], vec![b, a, 999], vec![b, 999]] {
        assert!(matches!(store.reorder(&bad), Err(Error::InvalidOrder)));
    }
    assert_eq!(names(&store), ["A", "B"]);
}

#[test]
fn duplicate_is_a_deep_copy_after_the_original() {
    let mut store = Store::open_in_memory().unwrap();
    store.create("First").unwrap();
    let original = filled(&mut store, "Signup");
    store.create("Last").unwrap();

    let copy = store.duplicate(original.id).unwrap();
    assert_ne!(copy.id, original.id);
    assert_eq!(copy.name, "Signup copy");
    assert_eq!(copy.url, original.url);
    assert_eq!(copy.body_format, original.body_format);
    assert_eq!((copy.run_count, copy.delay_ms), (10, 500));
    assert_eq!(copy.fields, original.fields);
    assert_eq!(copy.headers, original.headers);
    assert_eq!(names(&store), ["First", "Signup", "Signup copy", "Last"]);

    let mut edited = copy.clone();
    edited.fields.truncate(1);
    edited.headers.clear();
    store.save(&edited).unwrap();
    let reloaded = store.get_endpoint(original.id).unwrap();
    assert_eq!(reloaded.fields, original.fields);
    assert_eq!(reloaded.headers, original.headers);
}

#[test]
fn delete_cascades_to_fields_and_headers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fakeman.db");
    let mut store = Store::open(&path).unwrap();
    let doomed = filled(&mut store, "Doomed");
    let kept = filled(&mut store, "Kept");

    let raw = Connection::open(&path).unwrap();
    assert_eq!(count(&raw, "fields", doomed.id), 4);
    assert_eq!(count(&raw, "headers", doomed.id), 2);

    store.delete(doomed.id).unwrap();
    assert_eq!(count(&raw, "fields", doomed.id), 0);
    assert_eq!(count(&raw, "headers", doomed.id), 0);
    assert_eq!(count(&raw, "fields", kept.id), 4);
    assert_eq!(count(&raw, "headers", kept.id), 2);
}

#[test]
fn file_database_uses_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fakeman.db");
    let store = Store::open(&path).unwrap();
    let raw = Connection::open(&path).unwrap();
    let mode: String = raw
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(store);
}

#[test]
fn first_launch_seeds_one_sample_endpoint_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fakeman.db");

    let store = Store::open(&path).unwrap();
    let list = store.list_endpoints().unwrap();
    assert_eq!(list.len(), 1);
    let sample = store.get_endpoint(list[0].id).unwrap();
    assert_eq!(sample.name, "Create user");
    assert_eq!(sample.method, "POST");
    assert_eq!(sample.url, "https://httpbin.org/post");
    assert_eq!(sample.body_format, BodyFormat::Json);
    assert_eq!((sample.run_count, sample.delay_ms), (1, 0));
    assert_eq!(
        sample.fields,
        [
            field("firstname", "person.firstName", json!({}), true),
            field("lastname", "person.lastName", json!({}), true),
            field("email", "internet.email", json!({}), true),
        ]
    );
    assert!(sample.headers.is_empty());

    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(names(&store), ["Create user"]);

    store.delete(sample.id).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.list_endpoints().unwrap().is_empty());
}

#[test]
fn in_memory_database_is_not_seeded() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.list_endpoints().unwrap().is_empty());
}

#[test]
fn opens_a_v1_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fakeman.db");
    {
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch(include_str!("../migrations/0001_initial.sql"))
            .unwrap();
        raw.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO endpoints (id, name, url, body_format, run_count, delay_ms, sort_order, created_at, updated_at)
             VALUES (7, 'Legacy', 'http://localhost/x', 'form', 3, 250, 0, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
             INSERT INTO fields (endpoint_id, position, name, generator_key) VALUES (7, 0, 'id', 'datatype.uuid');
             INSERT INTO headers (endpoint_id, position, key, value, enabled) VALUES (7, 0, 'Accept', '*/*', 0);",
        )
        .unwrap();
    }

    let store = Store::open(&path).unwrap();
    assert_eq!(names(&store), ["Legacy"]);
    let legacy = store.get_endpoint(7).unwrap();
    assert_eq!(legacy.method, "POST");
    assert_eq!(legacy.body_format, BodyFormat::Form);
    assert_eq!((legacy.run_count, legacy.delay_ms), (3, 250));
    assert_eq!(
        legacy.fields,
        [field("id", "datatype.uuid", json!({}), true)]
    );
    assert_eq!(legacy.headers, [header("Accept", "*/*", false)]);
}

#[test]
fn default_path_is_in_the_os_data_directory() {
    let path = fakeman_store::default_db_path().unwrap();
    assert!(path.ends_with("fakeman/fakeman.db"));
}
