//! The blog content compiled into the Worker.
//!
//! Both halves of the content directory are embedded: the index describing the
//! published posts and the markdown bodies gathered by `build.rs`. Nothing here
//! is validated — [`crate::BlogStore`] does that when it is built.

/// The published blog index, as written in `content/blogs/index.json`.
pub(crate) static INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/content/blogs/index.json"
));

include!(concat!(env!("OUT_DIR"), "/embedded_markdown.rs"));
