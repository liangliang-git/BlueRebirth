use blueoath_protocol::TRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub method: String,
    pub args: Vec<u8>,
    pub callback_handler: u32,
    pub token: String,
}

impl From<TRequest> for RequestContext {
    fn from(request: TRequest) -> Self {
        Self {
            method: request.method,
            args: request.args.unwrap_or_default(),
            callback_handler: request.callback_handler,
            token: request.token,
        }
    }
}
