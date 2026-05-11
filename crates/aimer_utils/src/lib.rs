pub mod log;
mod time_cost;

pub use time_cost::ExecTimes;

#[cfg(test)]
mod tests {
    use crate::time_cost;

    #[test]
    fn test_time_cost() {
        let res = time_cost!("test_op", || {
            let mut sum = 0;
            for i in 0..1000 {
                sum += i;
            }
            sum
        });
        assert_eq!(res, 499500);
    }
}
