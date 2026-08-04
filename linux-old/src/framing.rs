// Framing layer: turns a byte stream into a sequence of Messages and back.
//
// The codec (decode) wants exactly one complete frame body. A socket hands
// out arbitrary lumps, so this layer reads the u32 length prefix, reads
// exactly that many more bytes (read_exact loops internally until it has
// them), guards MAX_FRAME so a bad length can't trigger a huge allocation,
// then decodes. Writing is trivial: encode already produces a full frame.

use crate::protocol::{DecodeError, Message, MAX_FRAME};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub enum FrameError {
    Io(std::io::Error),
    TooLarge(usize),
    Decode(DecodeError),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
			FrameError::Io(e) => match e.kind() {
                std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe => write!(f, "sender disconnected"),
                _ => write!(f, "io error: {e}"),
            },
            FrameError::TooLarge(n) => write!(f, "frame too large: {n}"),
            FrameError::Decode(e) => write!(f, "decode: {e:?}"),
        }
    }
}
impl std::error::Error for FrameError {}

impl From<std::io::Error> for FrameError {
    fn from(e: std::io::Error) -> Self {
        FrameError::Io(e)
    }
}
impl From<DecodeError> for FrameError {
    fn from(e: DecodeError) -> Self {
        FrameError::Decode(e)
    }
}

// Reads one frame. Ok(None) means a clean EOF at a frame boundary (the peer
// closed between frames). An EOF partway through a frame is an error.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Message>, FrameError> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(FrameError::Io(e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 {
        return Err(FrameError::Decode(DecodeError::Empty));
    }
    if 4 + len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?; // UnexpectedEof here = truncated frame = error
    Ok(Some(Message::decode(&body)?))
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W,
    m: &Message,
) -> Result<(), FrameError> {
    w.write_all(&m.encode()).await?;
    w.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Role;

    #[tokio::test]
    async fn reads_three_frames_then_clean_eof() {
        let msgs = [
            Message::Hello { version: 1, role: Role::Sender, name: "takara".into() },
            Message::Credit { file_id: 1, credit: 8 },
            Message::Bye,
        ];
        let mut buf = Vec::new();
        for m in &msgs {
            buf.extend_from_slice(&m.encode());
        }

        let mut cur: &[u8] = &buf;
        for expected in &msgs {
            let got = read_frame(&mut cur).await.unwrap().unwrap();
            assert_eq!(&got, expected);
        }
        assert!(read_frame(&mut cur).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn oversized_length_is_rejected_without_allocating() {
        let mut cur: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0x01];
        match read_frame(&mut cur).await {
            Err(FrameError::TooLarge(_)) => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn truncated_body_is_an_error_not_eof() {
        let mut cur: &[u8] = &[0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x00];
        match read_frame(&mut cur).await {
            Err(FrameError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
            other => panic!("expected truncated Io error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let m = Message::Accept { file_id: 7, quality_id: 0, credit: 32 };
        let mut buf = Vec::new();
        write_frame(&mut buf, &m).await.unwrap();
        let mut cur: &[u8] = &buf;
        assert_eq!(read_frame(&mut cur).await.unwrap().unwrap(), m);
    }
}
