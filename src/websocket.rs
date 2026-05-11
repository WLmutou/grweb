use std::io::{Read, Write};
use std::net::TcpStream;
use sha1::{Sha1, Digest};

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

const FIN_BIT: u8 = 0x80;
const MASK_BIT: u8 = 0x80;

pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close(Option<(u16, String)>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
}

pub struct WebSocket {
    stream: TcpStream,
}

impl WebSocket {
    pub fn accept(mut stream: TcpStream, key: &str) -> Option<Self> {
        let accept_key = compute_accept_key(key);
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\r\n",
            accept_key
        );
        if stream.write_all(response.as_bytes()).is_err() {
            return None;
        }
        let _ = stream.flush();
        Some(WebSocket { stream })
    }

    pub fn read_message(&mut self) -> Option<Message> {
        let mut frame_data = Vec::new();
        let mut opcode = 0u8;

        loop {
            let (fin, op, _mask, payload) = read_frame(&mut self.stream)?;

            if opcode == 0 {
                opcode = op;
            }

            frame_data.extend_from_slice(&payload);

            if fin {
                break;
            }
        }

        match opcode {
            OP_TEXT => {
                String::from_utf8(frame_data).ok().map(Message::Text)
            }
            OP_BINARY => Some(Message::Binary(frame_data)),
            OP_CLOSE => {
                let code = if frame_data.len() >= 2 {
                    Some((
                        u16::from_be_bytes([frame_data[0], frame_data[1]]),
                        String::from_utf8_lossy(&frame_data[2..]).to_string(),
                    ))
                } else {
                    None
                };
                Some(Message::Close(code))
            }
            OP_PING => Some(Message::Ping(frame_data)),
            OP_PONG => Some(Message::Pong(frame_data)),
            _ => None,
        }
    }

    pub fn send_text(&mut self, text: &str) -> bool {
        send_frame(&mut self.stream, OP_TEXT, text.as_bytes())
    }

    pub fn send_binary(&mut self, data: &[u8]) -> bool {
        send_frame(&mut self.stream, OP_BINARY, data)
    }

    pub fn send_ping(&mut self, data: &[u8]) -> bool {
        send_frame(&mut self.stream, OP_PING, data)
    }

    pub fn send_pong(&mut self, data: &[u8]) -> bool {
        send_frame(&mut self.stream, OP_PONG, data)
    }

    pub fn send_close(&mut self, code: u16, reason: &str) -> bool {
        let mut payload = Vec::with_capacity(2 + reason.len());
        payload.extend_from_slice(&code.to_be_bytes());
        payload.extend_from_slice(reason.as_bytes());
        send_frame(&mut self.stream, OP_CLOSE, &payload)
    }
}

fn compute_accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let hash = hasher.finalize();
    base64_encode(&hash)
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

fn read_frame(stream: &mut TcpStream) -> Option<(bool, u8, bool, Vec<u8>)> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).ok()?;

    let fin = (header[0] & FIN_BIT) != 0;
    let opcode = header[0] & 0x0F;
    let masked = (header[1] & MASK_BIT) != 0;
    let mut payload_len = (header[1] & 0x7F) as u64;

    if payload_len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).ok()?;
        payload_len = u16::from_be_bytes(ext) as u64;
    } else if payload_len == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext).ok()?;
        payload_len = u64::from_be_bytes(ext);
    }

    let mut mask_key = [0u8; 4];
    if masked {
        stream.read_exact(&mut mask_key).ok()?;
    }

    let mut payload = vec![0u8; payload_len as usize];
    if payload_len > 0 {
        stream.read_exact(&mut payload).ok()?;
    }

    if masked {
        for i in 0..payload.len() {
            payload[i] ^= mask_key[i % 4];
        }
    }

    Some((fin, opcode, masked, payload))
}

fn send_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> bool {
    let mut frame = Vec::with_capacity(10 + payload.len());

    frame.push(FIN_BIT | opcode);

    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(payload);

    stream.write_all(&frame).is_ok() && stream.flush().is_ok()
}