
pub enum KeyState {
    Pressed,
    Released,
    Repeat
}

pub enum AimerAppEvent {
    KeyboardEvent{state: KeyState, key: char},
}