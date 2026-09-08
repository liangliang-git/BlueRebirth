use blueoath_protocol::{
    ChangeNameRequest, Decode, ProtocolError, SetHeadFrameRequest, SetHeadRequest,
    SetMessageRequest, SetSecretaryRequest,
};

/// User routes decoded once at feature boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserRequest {
    SetSecretary(SetSecretaryRequest),
    ChangeName(ChangeNameRequest),
    SetMessage(SetMessageRequest),
    SetHeadFrame(SetHeadFrameRequest),
    SetHead(SetHeadRequest),
}

impl UserRequest {
    pub(crate) fn decode(method: &str, payload: &[u8]) -> Result<Option<Self>, ProtocolError> {
        let request = match method {
            "user.SetUserSecretary" => Self::SetSecretary(SetSecretaryRequest::decode(payload)?),
            "user.ChangeName" => Self::ChangeName(ChangeNameRequest::decode(payload)?),
            "user.SetMessage" => Self::SetMessage(SetMessageRequest::decode(payload)?),
            "user.SetPlayerHeadFrame" => Self::SetHeadFrame(SetHeadFrameRequest::decode(payload)?),
            "user.SetHead" => Self::SetHead(SetHeadRequest::decode(payload)?),
            _ => return Ok(None),
        };
        Ok(Some(request))
    }
}

#[cfg(test)]
mod tests {
    use super::UserRequest;

    #[test]
    fn decodes_only_user_routes() {
        assert!(
            UserRequest::decode("user.SetMessage", &[0x0A, 0x02, b'h', b'i'])
                .unwrap()
                .is_some()
        );
        assert!(UserRequest::decode("user.Unknown", &[]).unwrap().is_none());
    }

    #[test]
    fn rejects_malformed_user_request() {
        assert!(UserRequest::decode("user.SetHead", &[]).is_err());
    }
}
