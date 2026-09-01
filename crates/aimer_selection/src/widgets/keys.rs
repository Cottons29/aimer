use std::rc::Rc;

use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::base::{BuildContext, Vec2d};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, PortableWidget,
    Rebuildable, RequiredChild, VisitorElement, Widget,
};

use crate::Key;

/// Maps a platform named key onto the selection-control key vocabulary.
pub(crate) fn map_named_key(key: &NamedKey) -> Option<Key> {
    match key {
        NamedKey::Enter => Some(Key::Enter),
        NamedKey::Escape => Some(Key::Escape),
        NamedKey::Tab => Some(Key::Tab),
        NamedKey::Home => Some(Key::Home),
        NamedKey::End => Some(Key::End),
        NamedKey::ArrowUp => Some(Key::ArrowUp),
        NamedKey::ArrowDown => Some(Key::ArrowDown),
        NamedKey::ArrowLeft => Some(Key::ArrowLeft),
        NamedKey::ArrowRight => Some(Key::ArrowRight),
        NamedKey::Other(name)
            if name.eq_ignore_ascii_case("space") || name == " " || name == "Space" =>
        {
            Some(Key::Space)
        }
        _ => None,
    }
}

/// Forwards focused keyboard events into a choice-control handler.
pub(crate) struct KeyRelay<W = RequiredChild> {
    child: W,
    on_key: Rc<dyn Fn(Key) -> bool>,
}

impl KeyRelay {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            child: RequiredChild,
            on_key: Rc::new(|_| false),
        }
    }

    #[inline]
    pub(crate) fn on_key(mut self, on_key: impl Fn(Key) -> bool + 'static) -> Self {
        self.on_key = Rc::new(on_key);
        self
    }

    #[inline]
    pub(crate) fn child<C: Widget>(self, child: C) -> KeyRelay<C> {
        KeyRelay {
            child,
            on_key: self.on_key,
        }
    }
}

impl<W: Widget + 'static> PortableWidget for KeyRelay<W> {}

impl<W: Widget + 'static> Widget for KeyRelay<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawKeyRelay {
            child: self.child.to_element(ctx),
            on_key: self.on_key,
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "KeyRelay"
    }
}

struct RawKeyRelay {
    child: AnyElement,
    on_key: Rc<dyn Fn(Key) -> bool>,
}

impl VisitorElement for RawKeyRelay {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "KeyRelay"
    }
}

impl Rebuildable for RawKeyRelay {
    fn is_carry_state(&self) -> bool {
        self.child.is_carry_state()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.child.with_rebuild_context(ctx, callback);
    }
}

impl Drawable for RawKeyRelay {
    fn draw(&self, ctx: &BuildContext) {
        self.child.draw(ctx);
    }
}

impl LayoutElement for RawKeyRelay {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
}

impl EventElement for RawKeyRelay {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let key = match event {
            ElementEvent::KeyInput { key, action, .. }
                if matches!(action, KeyAction::Pressed | KeyAction::Repeat) =>
            {
                map_named_key(key)
            }
            ElementEvent::CharInput { ch: ' ', action, .. }
                if matches!(action, KeyAction::Pressed | KeyAction::Repeat) =>
            {
                Some(Key::Space)
            }
            ElementEvent::TextInput { text, action, .. }
                if text == " "
                    && matches!(action, KeyAction::Pressed | KeyAction::Repeat) =>
            {
                Some(Key::Space)
            }
            _ => None,
        };
        match key {
            Some(key) if (self.on_key)(key) => EventResult::consumed(),
            _ => EventResult::ignored(),
        }
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}
