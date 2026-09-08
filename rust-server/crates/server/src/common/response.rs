use blueoath_protocol::{
    CopyInfoCodec, CopyInfoPayload, GameLoginCodec, PlayerUserCodec, TMessageCodec, TResponse,
    TRetLogin, UserInfo, UserInfoCodec, UserListCodec, UserLoginCodec,
};

use super::error::GameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub method: String,
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePayload {
    User(Box<UserResponse>),
    Battle(BattleResponse),
    Raw(Vec<u8>),
}

impl ResponsePayload {
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::User(payload) => payload.into_bytes(),
            Self::Battle(payload) => payload.into_bytes(),
            Self::Raw(payload) => payload,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::User(payload) => payload.encoded_len(),
            Self::Battle(payload) => payload.encoded_len(),
            Self::Raw(payload) => payload.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl PartialEq<Vec<u8>> for ResponsePayload {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.len() == other.len() && self.clone().into_bytes() == *other
    }
}

/// Typed user responses. Encoding stays at response/wire boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserResponse {
    PlayerLogin(TRetLogin),
    UserList(Vec<UserInfo>),
    Player(UserInfo),
    Info(UserInfo),
    Login(UserLoginResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserLoginResponse {
    pub ret: String,
    pub ban_msg: String,
    pub ban_time: i32,
}

impl UserResponse {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::PlayerLogin(response) => GameLoginCodec::encode_response(&response),
            Self::UserList(users) => UserListCodec::encode(&users),
            Self::Player(user) => PlayerUserCodec::encode(&user),
            Self::Info(user) => UserInfoCodec::encode(&user),
            Self::Login(response) => {
                UserLoginCodec::encode_response(&response.ret, &response.ban_msg, response.ban_time)
            }
        }
    }

    fn encoded_len(&self) -> usize {
        self.clone().into_bytes().len()
    }
}

/// Battle response category. `Raw` marks routes whose protocol DTO migration
/// is still pending; callers must opt into it explicitly with `battle_bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleResponse {
    CopyInfo(CopyInfoPayload),
    Raw(Vec<u8>),
}

impl BattleResponse {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::CopyInfo(payload) => CopyInfoCodec::encode_payload(&payload),
            Self::Raw(payload) => payload,
        }
    }

    fn encoded_len(&self) -> usize {
        match self {
            Self::CopyInfo(payload) => CopyInfoCodec::encode_payload(payload).len(),
            Self::Raw(payload) => payload.len(),
        }
    }
}

impl Response {
    pub fn new(method: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            method: method.into(),
            payload: ResponsePayload::Raw(payload),
        }
    }

    pub fn encode(self, callback_handler: u32, token: String, time: u32) -> Vec<u8> {
        self.encode_with_error(callback_handler, token, time, 0, String::new())
    }

    pub fn encode_with_error(
        self,
        callback_handler: u32,
        token: String,
        time: u32,
        error_code: i32,
        error_message: String,
    ) -> Vec<u8> {
        TMessageCodec::encode_response(&TResponse {
            err: error_code,
            err_msg: error_message,
            method: self.method,
            ret: Some(self.payload.into_bytes()),
            callback_handler,
            token,
            time,
            is_response: 1,
            ..TResponse::default()
        })
    }

    pub fn encode_push(self, time: u32) -> Vec<u8> {
        TMessageCodec::encode_response(&TResponse {
            method: self.method,
            ret: Some(self.payload.into_bytes()),
            time,
            ..TResponse::default()
        })
    }

    pub fn raw(method: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(method, payload.into())
    }

    pub fn from_payload(method: impl Into<String>, payload: ResponsePayload) -> Self {
        Self {
            method: method.into(),
            payload,
        }
    }

    pub fn user(method: impl Into<String>, payload: UserResponse) -> Self {
        Self::from_payload(method, ResponsePayload::User(Box::new(payload)))
    }

    pub fn battle(method: impl Into<String>, payload: BattleResponse) -> Self {
        Self::from_payload(method, ResponsePayload::Battle(payload))
    }

    pub fn battle_bytes(method: impl Into<String>, payload: Vec<u8>) -> Self {
        Self::battle(method, BattleResponse::Raw(payload))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerResult {
    Reply(Response),
    PushOnly,
    Empty,
    Error(GameError),
}

impl HandlerResult {
    pub fn into_response(self, method: impl Into<String>) -> Option<Response> {
        match self {
            Self::Reply(response) => Some(response),
            Self::PushOnly | Self::Empty => None,
            // Errors still complete request callback with empty payload. The
            // outer session carries client error code/message separately.
            Self::Error(_) => Some(Response::raw(method, Vec::new())),
        }
    }
}

#[derive(Debug, Default)]
pub struct ResponseEffects {
    pre: Vec<Response>,
    post: Vec<Response>,
    error: Option<GameError>,
}

impl ResponseEffects {
    pub fn push_pre(&mut self, response: Response) {
        self.pre.push(response);
    }

    pub fn push_post(&mut self, response: Response) {
        self.post.push(response);
    }

    pub fn fail(&mut self, error: GameError) {
        self.error = Some(error);
    }

    pub fn fail_invalid(&mut self, message: &'static str) {
        self.fail(GameError::InvalidRequest(message));
    }

    pub fn client_error(&self) -> Option<(i32, String)> {
        self.error
            .as_ref()
            .map(|error| (error.client_code(), error.to_string()))
    }

    pub fn error(&self) -> Option<&GameError> {
        self.error.as_ref()
    }

    pub fn into_parts(self) -> (Vec<Response>, Vec<Response>, Option<GameError>) {
        (self.pre, self.post, self.error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BattleResponse, HandlerResult, Response, ResponseEffects, UserLoginResponse, UserResponse,
    };
    use blueoath_protocol::TMessageCodec;

    #[test]
    fn response_effects_map_domain_failure_to_client_fields() {
        let mut effects = ResponseEffects::default();
        effects.fail_invalid("copy id");
        assert_eq!(
            effects.client_error(),
            Some((1, "invalid request: copy id".to_owned()))
        );
    }

    #[test]
    fn raw_response_preserves_method_and_payload() {
        assert_eq!(
            Response::raw("user.GetInfo", [1, 2, 3].to_vec()),
            Response::new("user.GetInfo", vec![1, 2, 3])
        );
    }

    #[test]
    fn typed_user_response_encodes_only_at_wire_boundary() {
        let user = blueoath_protocol::UserInfo {
            uid: 7,
            uname: "Captain".to_owned(),
            ..blueoath_protocol::UserInfo::default()
        };
        let response = Response::user("user.GetUserInfo", UserResponse::Info(user.clone()));
        assert_eq!(
            response.payload.len(),
            blueoath_protocol::UserInfoCodec::encode(&user).len()
        );
        assert_eq!(
            response.payload.clone().into_bytes(),
            blueoath_protocol::UserInfoCodec::encode(&user)
        );
    }

    #[test]
    fn typed_login_response_preserves_protocol_fields() {
        let response = Response::user(
            "user.UserLogin",
            UserResponse::Login(UserLoginResponse {
                ret: "ok".to_owned(),
                ban_msg: String::new(),
                ban_time: 0,
            }),
        );
        assert_eq!(
            response.payload.into_bytes(),
            blueoath_protocol::UserLoginCodec::encode_response("ok", "", 0)
        );
    }

    #[test]
    fn battle_raw_transition_is_explicit() {
        let response = Response::battle_bytes("battle.End", vec![1, 2]);
        assert_eq!(
            response.payload,
            super::ResponsePayload::Battle(BattleResponse::Raw(vec![1, 2]))
        );
    }

    #[test]
    fn push_response_uses_server_push_envelope() {
        let encoded = Response::raw("user.UpdateUserInfo", [1, 2, 3].to_vec()).encode_push(42);
        let decoded = TMessageCodec::decode_response(&encoded).expect("push response");
        assert_eq!(decoded.method, "user.UpdateUserInfo");
        assert_eq!(decoded.ret, Some(vec![1, 2, 3]));
        assert_eq!(decoded.time, 42);
        assert_eq!(decoded.callback_handler, 0);
    }

    #[test]
    fn handler_error_still_completes_callback_with_empty_payload() {
        assert_eq!(
            HandlerResult::Error(super::GameError::AccountUnavailable)
                .into_response("user.GetUserInfo")
                .map(|response| response.payload.into_bytes()),
            Some(Vec::new())
        );
    }
}
