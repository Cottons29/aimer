pub mod log;

pub struct ExecTimes;

impl ExecTimes {

    pub fn no_param<T>(label: &str, f: impl FnOnce() -> T) -> T {
        let start = chrono::Local::now();
        let res = f();
        let delta = chrono::Local::now().signed_duration_since(start);
        debug!("<{label}> time consume: {:?}", delta);
        res
    }

    pub fn with_param<T, P>(label: &str, param: P, f: impl FnOnce(P) -> T) -> T {
        let start = chrono::Local::now();
        let res = f(param);
        let delta = chrono::Local::now().signed_duration_since(start);
        debug!("<{label}> time consume: {:?}", delta);
        res
    }
    
}



