use crate::CaptureSpan;

pub trait SyntaxHighlight {
    fn as_span(&self) -> CaptureSpan;
}
