pub mod simd_traits;
pub mod number;
pub mod backend;
pub mod kernels;
pub mod cpu_features;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}


pub fn simd_add() {
    // let add
}
