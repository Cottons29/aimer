mod paragraph;
mod rich_text;
mod selection;
mod selection_area;
mod text;
mod text_button;
pub mod text_span;
mod text_source;

pub use rich_text::{LinkCallback, RawRichText, RichText};
pub use selection_area::{SelectionArea, SelectionAreaElement};
pub use selection::TextSelection;
pub use text::Text;
pub use text::raw_text::RawTextWidget;
pub use text_button::TextButton;
pub use text_span::{SpanStyle, TextSpan};
pub use text_source::TextSource;
pub use aimer_std::read_only::ShareRef;
