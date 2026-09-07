use blueoath_protocol::{TMessageCodec, TResponse};

use super::error::GameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub method: String,
    pub payload: Vec<u8>,
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
}

impl Response {
    pub fn new(method: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            method: method.into(),
            payload,
        }
    }

    pub fn encode(self, callback_handler: u32, token: String, time: u32) -> Vec<u8> {
        TMessageCodec::encode_response(&TResponse {
            method: self.method,
            ret: Some(self.payload),
            callback_handler,
            token,
            time,
            is_response: 1,
            ..TResponse::default()
        })
    }

    pub fn raw(method: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(method, payload.into())
    }

    pub fn from_payload(method: impl Into<String>, payload: ResponsePayload) -> Self {
        Self::new(method, payload.into_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerResult {
    Reply(Response),
    PushOnly,
    Empty,
}

impl HandlerResult {
    pub fn into_payload(self) -> Option<Vec<u8>> {
        match self {
            Self::Reply(response) => Some(response.payload),
            Self::PushOnly | Self::Empty => None,
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
    use super::{Response, ResponseEffects};

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
}
