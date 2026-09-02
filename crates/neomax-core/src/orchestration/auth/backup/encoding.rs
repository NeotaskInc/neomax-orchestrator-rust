use base64::Engine as _;

use crate::{Error, Result};

pub(super) fn encode(bytes: Option<&[u8]>) -> Option<String> {
    bytes.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub(super) fn decode(value: Option<&str>) -> Result<Option<Vec<u8>>> {
    value
        .map(|value| {
            base64::engine::general_purpose::STANDARD
                .decode(value)
                .map_err(|error| Error::InvalidArgument(format!("invalid backup data: {error}")))
        })
        .transpose()
}
