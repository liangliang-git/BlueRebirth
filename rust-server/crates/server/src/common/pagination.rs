use super::error::GameError;

pub const MAX_PAGE_SIZE: i32 = 100;

pub fn page_range(offset: i32, limit: i32) -> Result<(usize, usize), GameError> {
    if offset < 0 || !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(GameError::InvalidRequest("pagination"));
    }
    Ok((offset as usize, limit as usize))
}
