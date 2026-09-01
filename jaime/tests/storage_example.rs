#[path = "../src/storage_example.rs"]
mod storage_example;

use aimer::storage::StorageError;

#[test]
fn storage_example_uses_namespaced_memory_fallback() {
    assert_eq!(
        storage_example::memory_storage_round_trip().unwrap(),
        Some(b"dark".to_vec())
    );
}

#[test]
fn storage_example_exposes_a_typed_invalid_key_fallback() {
    assert_eq!(
        storage_example::memory_storage_invalid_key_fallback(),
        Err(StorageError::InvalidKey)
    );
}

#[test]
fn storage_example_exposes_a_constructible_widget() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(storage_example::storage_example());
}
