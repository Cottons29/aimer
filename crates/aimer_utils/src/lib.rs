pub mod log;


pub fn time_consume<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = chrono::Local::now();
    let res = f();
    let delta = chrono::Local::now().signed_duration_since(start);
    debug!("<{label}> time consume: {:?}", delta);
    res
}