#[cfg(not(target_arch = "wasm32"))]
use colored::Colorize;

#[cfg(target_arch = "wasm32")]
mod console {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);
        pub fn warn(s: &str);
        pub fn error(s: &str);
    }
}

pub fn log(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "INFO".bold().blue();
        let colored_msg = msg.bright_green();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let fmt = format!("[INFO] {}", msg);
        console::log(&fmt);
    }
}

pub fn warn(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "WARN".bold().yellow();
        let colored_msg = msg.yellow();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!("[WARN] {}", msg);
        console::warn(&msg);
    }
}

pub fn error(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "ERROR".bold().red();
        let colored_msg = msg.red();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!("[ERROR] {}", msg);
        console::error(&msg);
    }
}

pub fn debug(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "DEBUG".bold().green();
        let colored_msg = msg.bright_green();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!("[DEBUG] {}", msg);
        console::log(&msg);
    }
}


#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::log::log(&format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        $crate::log::warn(&format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        $crate::log::error(&format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        // #[cfg(debug_assertions)]
        $crate::log::debug(&format!($($arg)*));
    }};
}
