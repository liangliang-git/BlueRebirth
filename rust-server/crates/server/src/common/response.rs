use blueoath_protocol::{TMessageCodec, TResponse};

use super::error::GameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub method: String,
    pub payload: Vec<u8>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerResult {
    Reply(Response),
    PushOnly,
    Empty,
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

    pub fn error(&self) -> Option<&GameError> {
        self.error.as_ref()
    }

    pub fn into_parts(self) -> (Vec<Response>, Vec<Response>, Option<GameError>) {
        (self.pre, self.post, self.error)
    }
}
