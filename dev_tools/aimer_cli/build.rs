use chrono::Local;

fn main() {
    let build_date = Local::now()
        .format("%d-%m-%Y (%H:%M:%S)")
        .to_string();
    println!("cargo:rustc-env=AIMER_BUILD_TIME={build_date}");
}
