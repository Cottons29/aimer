use std::env;

fn main() {
    println!("cargo:rustc-link-lib=c++");
    let target = env::var("TARGET").unwrap();
    if target.contains("apple-ios") {
        println!("cargo:rustc-link-lib=framework=SwiftUI");
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
    } else if target.contains("apple-darwin") {
         println!("cargo:rustc-link-lib=framework=SwiftUI");
         println!("cargo:rustc-link-lib=framework=AppKit");
         println!("cargo:rustc-link-lib=framework=CoreGraphics");
    }
}
