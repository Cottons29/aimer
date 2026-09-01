#[path = "../src/assets_media_example.rs"]
mod assets_media_example;

#[test]
fn assets_media_example_exposes_bounded_asset_cache_icon_and_media_fallback() {
    let snapshot = assets_media_example::AssetsMediaExample::new().snapshot();
    assert!(snapshot.asset_is_ready());
    assert_eq!(snapshot.cache_entries(), 1);
    assert_eq!(snapshot.icon_size(), 24);
    assert!(snapshot.media_is_unsupported());
}

#[test]
fn assets_media_example_builds_a_public_widget() {
    fn assert_widget(_: impl aimer::Widget) {}

    assert_widget(assets_media_example::assets_media_example());
}
