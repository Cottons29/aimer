use constructor::Constructor;

#[derive(Constructor, Debug)]
pub struct TestStruct {
    name : String,
    score : f64
}

#[derive(Constructor, Debug)]
pub struct OtherStruct {
    name: String,
    test: TestStruct
}

#[cfg(test)]
mod tests {
    use super::TestStruct;

    #[test]
    fn constructs_with_macro() {
        let value = TestStruct! { name: "my_name".to_string(), score: 12.2 };

        assert_eq!(value.name, "my_name");
        assert_eq!(value.score, 12.2);
    }
}


