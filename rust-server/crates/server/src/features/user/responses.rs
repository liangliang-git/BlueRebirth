use crate::common::response::{UserLoginResponse, UserResponse};
use blueoath_protocol::{TRetLogin, UserInfo};

pub(crate) fn player_login(profile_id: impl Into<String>) -> UserResponse {
    UserResponse::PlayerLogin(TRetLogin {
        ret: "ok".to_owned(),
        feign_role_id: profile_id.into(),
        err_code: 0,
    })
}

pub(crate) fn user_list(user: UserInfo) -> UserResponse {
    UserResponse::UserList(vec![user])
}

pub(crate) fn player(user: UserInfo) -> UserResponse {
    UserResponse::Player(user)
}

pub(crate) fn info(user: UserInfo) -> UserResponse {
    UserResponse::Info(user)
}

pub(crate) fn login() -> UserResponse {
    UserResponse::Login(UserLoginResponse {
        ret: "ok".to_owned(),
        ban_msg: String::new(),
        ban_time: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::{info, login, player_login};
    use crate::common::response::ResponsePayload;

    #[test]
    fn response_adapters_keep_wire_payload_typed() {
        let info = info(blueoath_protocol::UserInfo::default());
        assert!(matches!(info, super::UserResponse::Info(_)));
        assert!(matches!(login(), super::UserResponse::Login(_)));
        assert!(matches!(
            player_login("profile"),
            super::UserResponse::PlayerLogin(_)
        ));
        let _ = ResponsePayload::Raw(Vec::new());
    }
}
