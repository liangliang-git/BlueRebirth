use blueoath_domain::{AccountState, NewAccountFactory, ProfileId};
use blueoath_protocol::TRequest;

pub fn request(method: impl Into<String>, args: Vec<u8>) -> TRequest {
    TRequest {
        method: method.into(),
        args: Some(args),
        ..TRequest::default()
    }
}

pub fn new_account(profile_id: &str) -> AccountState {
    NewAccountFactory::create(
        ProfileId::new(profile_id).expect("test profile id must be valid"),
        profile_id,
    )
}

pub fn varint_field(field: u8, value: u64) -> Vec<u8> {
    let mut output = Vec::new();
    write_varint(&mut output, u64::from(field) << 3);
    write_varint(&mut output, value);
    output
}

pub fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_valid_account_fixture_and_varint_request() {
        let account = new_account("fixture");
        assert!(account.validate().is_ok());
        let request = request("user.GetUserInfo", varint_field(1, 7));
        assert_eq!(request.method, "user.GetUserInfo");
        assert_eq!(request.args, Some(vec![0x08, 0x07]));
    }
}
