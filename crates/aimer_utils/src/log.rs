use std::panic::Location;
#[cfg(not(target_arch = "wasm32"))]
use colored::Colorize;

#[cfg(target_arch = "wasm32")]
mod console {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);
        #[wasm_bindgen(js_namespace = console)]
        pub fn warn(s: &str);
        #[wasm_bindgen(js_namespace = console)]
        pub fn error(s: &str);
        #[wasm_bindgen]
        pub fn eval(s: &str);
    }
}

#[allow(dead_code)]
fn extract_location(locat: &Location, log: &str, namespace: &str) -> String {
    let file_line  = format!("{}:{}", locat.file(), locat.line());
    format!(r#"
//# sourceURL={file_line}
console.{namespace}(`{log}`);
"#,)
}



#[track_caller]
pub fn log(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "INFO ".bold().bright_cyan();
        let colored_msg = msg.bright_cyan();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        #[cfg(debug_assertions)]
        {
            let fmt = format!("[INFO]  {}", msg);
            let location = extract_location(Location::caller(), &fmt, "log");
            console::eval(&location);
        }
        #[cfg(not(debug_assertions))]
        {
            let fmt = format!("[INFO]  {}", msg);
            console::log(&fmt);
        }
    }
}

#[track_caller]
pub fn warn(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "WARN ".bold().yellow();
        let colored_msg = msg.yellow();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        #[cfg(debug_assertions)]
        {
            let fmt = format!("[WARN]  {}", msg);
            let location = extract_location(Location::caller(), &fmt, "warn");
            console::eval(&location);
        }
        #[cfg(not(debug_assertions))]
        {
            let fmt = format!("[WARN]  {}", msg);
            console::warn(&fmt);
        }
    }
}

#[track_caller]
pub fn error(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "ERROR".bold().red();
        let colored_msg = msg.red();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        #[cfg(debug_assertions)]
        {
            let fmt = format!("[ERROR] {}", msg);
            let location = extract_location(Location::caller(), &fmt, "error");
            console::eval(&location);
        }
        #[cfg(not(debug_assertions))]
        {
            let fmt = format!("[ERROR] {}", msg);
            console::error(&fmt);
        }
    }
}

#[track_caller]
pub fn debug(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let label = "DEBUG".bold().green();
        let colored_msg = msg.bright_green();
        println!("[{}] {}", label, colored_msg);
    }
    #[cfg(target_arch = "wasm32")]
    {
        #[cfg(debug_assertions)]
        {
            let fmt = format!("[DEBUG] {}", msg);
            let location = extract_location(Location::caller(), &fmt, "log");
            console::eval(&location);
        }
        #[cfg(not(debug_assertions))]
        {
            let fmt = format!("[DEBUG] {}", msg);
            console::log(&fmt);
        }
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
