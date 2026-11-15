use crate::error::BitswapError;
use crate::models::BitswapMessage;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message;
use unsigned_varint::{aio, io::ReadError};

impl BitswapMessage {
    /// Ramka na strumieniu: długość jako varint, potem protobuf.
    pub fn to_frame(&self) -> Vec<u8> {
        self.encode_length_delimited_to_vec()
    }

    pub fn from_frame(frame: &[u8]) -> Result<Self, BitswapError> {
        Ok(Self::decode_length_delimited(frame)?)
    }

    pub fn is_empty(&self) -> bool {
        self.wantlist.is_none()
            && self.blocks.is_empty()
            && self.payload.is_empty()
            && self.block_presences.is_empty()
    }
}

pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<BitswapMessage>, BitswapError> {
    let length = match aio::read_usize(&mut *reader).await {
        Ok(length) => length,
        Err(ReadError::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(ReadError::Io(error)) => return Err(error.into()),
        Err(error) => return Err(BitswapError::Stream(error.to_string())),
    };
    if length > max_bytes {
        return Err(BitswapError::MessageTooLarge {
            actual: length,
            limit: max_bytes,
        });
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(BitswapMessage::decode(body.as_slice())?))
}

pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &BitswapMessage,
    max_bytes: usize,
) -> Result<(), BitswapError> {
    let length = message.encoded_len();
    if length > max_bytes {
        return Err(BitswapError::MessageTooLarge {
            actual: length,
            limit: max_bytes,
        });
    }
    writer.write_all(&message.to_frame()).await?;
    writer.flush().await?;
    Ok(())
}
