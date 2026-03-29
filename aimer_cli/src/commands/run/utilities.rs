use colored::Colorize;
use crossterm::style::Stylize;

pub trait LogStyling {
    fn process_log(self) -> String;
}

impl LogStyling for String {
    fn process_log(self) -> String {
        if self.contains("[ERROR]") {
            self.red().to_string()
        } else if self.contains("[WARN]") {
            self.yellow().to_string()
        } else if self.contains("[DEBUG]") {
            self.green().to_string()
        } else if self.contains("[INFO]") {
            self.bright_cyan().to_string()
        }else {
            self
        }
    }
}