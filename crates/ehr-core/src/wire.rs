//! EHR protocol frame: length (u32, big-endian) + JSON, with a size limit.
use crate::consts::MAX_WIRE_MESSAGE_BYTES;
use crate::error::EhrError;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use serde::{Serialize, de::DeserializeOwned};

pub(crate) async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    value: &impl Serialize,
) -> Result<(), EhrError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_WIRE_MESSAGE_BYTES {
        return Err(EhrError::Invalid("message too large".into()));
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

pub(crate) async fn read_frame<T: DeserializeOwned, S: AsyncRead + Unpin>(
    stream: &mut S,
) -> Result<T, EhrError> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_WIRE_MESSAGE_BYTES {
        return Err(EhrError::Invalid("message too large".into()));
    }
    let mut bytes = vec![0u8; length];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
