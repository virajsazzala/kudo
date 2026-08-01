// kudo wire protocol codec.
//
// Pure logic: turns protocol messages into frame bytes and back. No sockets,
// no Bluetooth. The socket layer reads the u32 length prefix, reads that many
// bytes, and hands the frame body (type + payload) to `Message::decode`.
// `Message::encode` produces the full frame including the length prefix.

pub const MAGIC: u32 = 0x4B55_444F; // "KUDO"
pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 131_072; // 128 KiB safety cap, above a 64 KiB data chunk + header

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Sender,
    Receiver,
}

impl Role {
    fn to_u8(self) -> u8 {
        match self {
            Role::Sender => 1,
            Role::Receiver => 2,
        }
    }
    fn from_u8(b: u8) -> Result<Role, DecodeError> {
        match b {
            1 => Ok(Role::Sender),
            2 => Ok(Role::Receiver),
            _ => Err(DecodeError::BadField("role")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quality {
    pub quality_id: u8,
    pub bitrate: u32, // bits per second, 0 if unknown
    pub size: u64,    // total bytes at this quality
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello {
        version: u16,
        role: Role,
        name: String,
    },
    Offer {
        file_id: u32,
        total_size: u64,
        chunk_size: u32,
        sha256: [u8; 32],
        mime: String,
        name: String,
        qualities: Vec<Quality>,
    },
    Accept {
        file_id: u32,
        quality_id: u8,
        credit: u32,
    },
    Decline {
        file_id: u32,
    },
    Chunk {
        file_id: u32,
        index: u32,
        data: Vec<u8>,
    },
    Credit {
        file_id: u32,
        credit: u32,
    },
    Complete {
        file_id: u32,
        status: u8, // 0 = hash verified, 1 = hash mismatch
    },
	Cancel {
        file_id: u32,
    },
    Error {
        code: u16,
        msg: String,
    },
    Bye,
}

// Message type tags on the wire.
const T_HELLO: u8 = 0x01;
const T_OFFER: u8 = 0x02;
const T_ACCEPT: u8 = 0x03;
const T_DECLINE: u8 = 0x04;
const T_CHUNK: u8 = 0x05;
const T_CREDIT: u8 = 0x06;
const T_COMPLETE: u8 = 0x07;
const T_CANCEL: u8 = 0x0A;
const T_ERROR: u8 = 0x08;
const T_BYE: u8 = 0x09;

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Empty,               // frame body had no type byte
    UnknownType(u8),     // maps to ERROR code 2
    BadMagic,            // HELLO magic wrong
    Truncated,           // ran off the end of the payload
    BadField(&'static str),
    BadUtf8,
}

impl Message {
    // Full frame: len(u32) | type(u8) | payload. len counts type + payload.
    pub fn encode(&self) -> Vec<u8> {
        let (tag, payload) = self.encode_body();
        let len = (payload.len() + 1) as u32; // +1 for the type byte
        let mut out = Vec::with_capacity(payload.len() + 5);
        out.extend_from_slice(&len.to_be_bytes());
        out.push(tag);
        out.extend_from_slice(&payload);
        out
    }

    fn encode_body(&self) -> (u8, Vec<u8>) {
        let mut p = Vec::new();
        let tag = match self {
            Message::Hello { version, role, name } => {
                p.extend_from_slice(&MAGIC.to_be_bytes());
                p.extend_from_slice(&version.to_be_bytes());
                p.push(role.to_u8());
                let nb = name.as_bytes();
                p.push(nb.len() as u8);
                p.extend_from_slice(nb);
                T_HELLO
            }
            Message::Offer {
                file_id, total_size, chunk_size, sha256, mime, name, qualities,
            } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                p.extend_from_slice(&total_size.to_be_bytes());
                p.extend_from_slice(&chunk_size.to_be_bytes());
                p.extend_from_slice(sha256);
                let mb = mime.as_bytes();
                p.push(mb.len() as u8);
                p.extend_from_slice(mb);
                let nb = name.as_bytes();
                p.extend_from_slice(&(nb.len() as u16).to_be_bytes());
                p.extend_from_slice(nb);
                p.push(qualities.len() as u8);
                for q in qualities {
                    p.push(q.quality_id);
                    p.extend_from_slice(&q.bitrate.to_be_bytes());
                    p.extend_from_slice(&q.size.to_be_bytes());
                }
                T_OFFER
            }
            Message::Accept { file_id, quality_id, credit } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                p.push(*quality_id);
                p.extend_from_slice(&credit.to_be_bytes());
                T_ACCEPT
            }
            Message::Decline { file_id } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                T_DECLINE
            }
            Message::Chunk { file_id, index, data } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                p.extend_from_slice(&index.to_be_bytes());
                p.extend_from_slice(data);
                T_CHUNK
            }
            Message::Credit { file_id, credit } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                p.extend_from_slice(&credit.to_be_bytes());
                T_CREDIT
            }
            Message::Complete { file_id, status } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                p.push(*status);
                T_COMPLETE
            }
            Message::Error { code, msg } => {
                p.extend_from_slice(&code.to_be_bytes());
                let mb = msg.as_bytes();
                p.extend_from_slice(&(mb.len() as u16).to_be_bytes());
                p.extend_from_slice(mb);
                T_ERROR
            }
			Message::Cancel { file_id } => {
                p.extend_from_slice(&file_id.to_be_bytes());
                T_CANCEL
            }
            Message::Bye => T_BYE,
        };
        (tag, p)
    }

    // Decode a frame body (type byte + payload), length prefix already stripped.
    pub fn decode(body: &[u8]) -> Result<Message, DecodeError> {
        let (tag, payload) = body.split_first().ok_or(DecodeError::Empty)?;
        let mut r = Reader::new(payload);
        let msg = match *tag {
            T_HELLO => {
                let magic = r.u32()?;
                if magic != MAGIC {
                    return Err(DecodeError::BadMagic);
                }
                let version = r.u16()?;
                let role = Role::from_u8(r.u8()?)?;
                let n = r.u8()? as usize;
                let name = r.utf8(n)?;
                Message::Hello { version, role, name }
            }
            T_OFFER => {
                let file_id = r.u32()?;
                let total_size = r.u64()?;
                let chunk_size = r.u32()?;
                let sha256 = r.array32()?;
                let ml = r.u8()? as usize;
                let mime = r.utf8(ml)?;
                let nl = r.u16()? as usize;
                let name = r.utf8(nl)?;
                let nq = r.u8()? as usize;
                let mut qualities = Vec::with_capacity(nq);
                for _ in 0..nq {
                    qualities.push(Quality {
                        quality_id: r.u8()?,
                        bitrate: r.u32()?,
                        size: r.u64()?,
                    });
                }
                Message::Offer {
                    file_id, total_size, chunk_size, sha256, mime, name, qualities,
                }
            }
            T_ACCEPT => Message::Accept {
                file_id: r.u32()?,
                quality_id: r.u8()?,
                credit: r.u32()?,
            },
            T_DECLINE => Message::Decline { file_id: r.u32()? },
            T_CHUNK => {
                let file_id = r.u32()?;
                let index = r.u32()?;
                let data = r.rest().to_vec();
                Message::Chunk { file_id, index, data }
            }
            T_CREDIT => Message::Credit {
                file_id: r.u32()?,
                credit: r.u32()?,
            },
            T_COMPLETE => Message::Complete {
                file_id: r.u32()?,
                status: r.u8()?,
            },
            T_ERROR => {
                let code = r.u16()?;
                let ml = r.u16()? as usize;
                let msg = r.utf8(ml)?;
                Message::Error { code, msg }
            },
			T_CANCEL => Message::Cancel { file_id: r.u32()? },
            T_BYE => Message::Bye,
            other => return Err(DecodeError::UnknownType(other)),
        };
        Ok(msg)
    }
}

// Bounds-checked big-endian reader over a payload slice.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        if end > self.buf.len() {
            return Err(DecodeError::Truncated);
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }
    fn array32(&mut self) -> Result<[u8; 32], DecodeError> {
        let b = self.take(32)?;
        let mut a = [0u8; 32];
        a.copy_from_slice(b);
        Ok(a)
    }
    fn utf8(&mut self, n: usize) -> Result<String, DecodeError> {
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| DecodeError::BadUtf8)
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }
}


// tests

#[cfg(test)]
mod tests {
    use super::*;

    fn body(frame: &[u8]) -> &[u8] {
        &frame[4..]
    }

    #[test]
    fn hello_vector_exact_bytes() {
        let m = Message::Hello { version: 1, role: Role::Sender, name: "takara".to_string() };
        let expected = vec![
            0x00, 0x00, 0x00, 0x0F, 0x01,
            0x4B, 0x55, 0x44, 0x4F, 0x00, 0x01, 0x01, 0x06,
            0x74, 0x61, 0x6B, 0x61, 0x72, 0x61,
        ];
        assert_eq!(m.encode(), expected);
    }

    #[test]
    fn credit_vector_exact_bytes() {
        let m = Message::Credit { file_id: 1, credit: 8 };
        let expected = vec![
            0x00, 0x00, 0x00, 0x09, 0x06,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08,
        ];
        assert_eq!(m.encode(), expected);
    }

    #[test]
    fn hello_roundtrip() {
        let m = Message::Hello { version: 1, role: Role::Sender, name: "takara".to_string() };
        assert_eq!(Message::decode(body(&m.encode())).unwrap(), m);
    }

    #[test]
    fn offer_roundtrip() {
        let m = Message::Offer {
            file_id: 42, total_size: 381_000_000, chunk_size: 65_536,
            sha256: [0xAB; 32], mime: "video/mp4".to_string(),
            name: "big-buck-bunny.mp4".to_string(),
            qualities: vec![Quality { quality_id: 0, bitrate: 1_200_000, size: 381_000_000 }],
        };
        assert_eq!(Message::decode(body(&m.encode())).unwrap(), m);
    }

    #[test]
    fn chunk_roundtrip() {
        let m = Message::Chunk { file_id: 1, index: 5810, data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x7F] };
        assert_eq!(Message::decode(body(&m.encode())).unwrap(), m);
    }

    #[test]
    fn complete_and_bye_roundtrip() {
        for m in [
            Message::Complete { file_id: 1, status: 0 },
            Message::Complete { file_id: 1, status: 1 },
            Message::Bye,
        ] {
            assert_eq!(Message::decode(body(&m.encode())).unwrap(), m);
        }
    }

    #[test]
    fn error_roundtrip_empty_and_nonempty() {
        for m in [
            Message::Error { code: 4, msg: String::new() },
            Message::Error { code: 1, msg: "version unsupported".to_string() },
        ] {
            assert_eq!(Message::decode(body(&m.encode())).unwrap(), m);
        }
    }

    #[test]
    fn unknown_type_is_reported() {
        assert_eq!(Message::decode(&[0xFF]), Err(DecodeError::UnknownType(0xFF)));
    }

    #[test]
    fn bad_magic_rejected() {
        let mut f = Message::Hello { version: 1, role: Role::Sender, name: "x".to_string() }.encode();
        f[5] = 0x00;
        assert_eq!(Message::decode(body(&f)), Err(DecodeError::BadMagic));
    }

    #[test]
    fn truncated_payload_rejected() {
        assert_eq!(Message::decode(&[T_ACCEPT, 0x00, 0x00, 0x00]), Err(DecodeError::Truncated));
    }

    #[test]
    fn empty_body_rejected() {
        assert_eq!(Message::decode(&[]), Err(DecodeError::Empty));
    }

	#[test]
    fn cancel_vector_exact_bytes() {
        let m = Message::Cancel { file_id: 1 };
        assert_eq!(m.encode(), vec![0x00, 0x00, 0x00, 0x05, 0x0A, 0x00, 0x00, 0x00, 0x01]);
    }
}
