use crate::Element;

pub trait VisitorElement {
    #[allow(unused_variables)]
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {}
    fn debug_name(&self) -> &'static str;
}
