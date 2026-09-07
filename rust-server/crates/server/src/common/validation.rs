use super::error::GameError;

pub fn positive_i32(value: i32, field: &'static str) -> Result<i32, GameError> {
    if value > 0 {
        Ok(value)
    } else {
        Err(GameError::InvalidRequest(field))
    }
}

pub fn bounded_i32(value: i32, field: &'static str, min: i32, max: i32) -> Result<i32, GameError> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(GameError::InvalidRequest(field))
    }
}

pub fn bounded_len<T>(values: &[T], field: &'static str, max: usize) -> Result<(), GameError> {
    if values.len() <= max {
        Ok(())
    } else {
        Err(GameError::InvalidRequest(field))
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_i32, positive_i32};

    #[test]
    fn rejects_invalid_boundary_values() {
        assert!(positive_i32(0, "id").is_err());
        assert!(bounded_i32(8, "level", 1, 7).is_err());
        assert_eq!(bounded_i32(7, "level", 1, 7).unwrap(), 7);
    }
}
