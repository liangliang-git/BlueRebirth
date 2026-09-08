use blueoath_protocol::{TMessageCodec, TResponse};

use super::error::GameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub method: String,
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePayload {
    User(Vec<u8>),
    Battle(Vec<u8>),
    Raw(Vec<u8>),
}

impl ResponsePayload {
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::User(payload) | Self::Battle(payload) | Self::Raw(payload) => payload,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::User(payload) | Self::Battle(payload) | Self::Raw(payload) => payload,
        }
    }

    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

impl AsRef<[u8]> for ResponsePayload {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl std::ops::Deref for ResponsePayload {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl PartialEq<Vec<u8>> for ResponsePayload {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_bytes() == other.as_slice()
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

    pub fn user(method: impl Into<String>, payload: Vec<u8>) -> Self {
        Self::from_payload(method, ResponsePayload::User(payload))
    }

    pub fn battle(method: impl Into<String>, payload: Vec<u8>) -> Self {
        Self::from_payload(method, ResponsePayload::Battle(payload))
    }
}

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
    use super::{HandlerResult, Response, ResponseEffects};
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
