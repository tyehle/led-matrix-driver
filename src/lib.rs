#![no_std]

pub fn go() -> i32 {
    42
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_go() {
        assert_eq!(go(), 42);
    }
}