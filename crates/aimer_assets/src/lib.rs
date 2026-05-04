
pub mod img_widget;

use std::fmt::{Debug, Display};
pub use img_widget::image_widget::Image;
pub use img_widget::network_image::NetworkImage;
use aimer_widget::base::BuildContext;


#[derive(Debug, Clone, PartialEq)]
pub enum ImageResult {
    Loading,
    Success(u32),
    Error(String),
}

pub type LoadingResult = Result<u32, &'static str>;

///
/// A trait for providing images based on a given context.
///
/// The `ImageProvider` trait defines a common interface for structures
/// that can return an image representation, identified by a `u32`,
/// based on a provided context (`BuildContext`). The trait requires
/// implementors to be `Clone`.
///
/// # Required Methods
///
/// - `get_image(&self, ctx: &BuildContext) -> LoadingResult`
///
/// This method takes a reference to a `BuildContext` and returns a
/// `LoadingResult` (which is `Result<u32, &'static str>`).
/// The implementation of this method can define how the image is determined
/// from the given context.
///
/// # Example
/// To implement the `ImageProvider` trait:
///
/// ```rust
/// #[derive(Clone)]
/// struct MyImageProvider;
///
/// impl ImageProvider for MyImageProvider {
///     fn get_image(&self, ctx: &BuildContext) -> LoadingResult {
///         // Custom logic to provide an image ID based on the context
///         Ok(42) // Example image ID
///     }
/// }
/// ```
///
/// # Notes
/// - The `ImageProvider` trait requires types to implement the `Clone` trait.
/// - The `BuildContext` parameter represents the context used to determine the image
///   and is expected to be provided by the caller.
///
pub trait ImageProvider: Clone + Debug {
    fn get_image(&self, ctx: &BuildContext) -> ImageResult;
}
