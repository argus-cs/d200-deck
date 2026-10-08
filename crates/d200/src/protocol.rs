//! Wire format: every packet is 1024 bytes. The first packet of a message
//! starts with an 8-byte header (`7C 7C`, command as big-endian u16, total
//! length as little-endian u32) followed by up to 1016 bytes of payload.
//! Longer payloads (the layout zip) continue in raw 1024-byte packets.

pub const VENDOR_ID: u16 = 0x2207;
pub const PRODUCT_ID: u16 = 0x0019;
/// The protocol runs on the consumer-control interface (MI_00 on Windows).
pub const USAGE_PAGE: u16 = 0x000C;

pub const PACKET_SIZE: usize = 1024;
pub const HEADER_SIZE: usize = 8;
pub const PAYLOAD_SIZE: usize = PACKET_SIZE - HEADER_SIZE;
const MAGIC: [u8; 2] = [0x7C, 0x7C];

/// 13 keys plus the wide status window, which also reports presses.
pub const KEY_COUNT: usize = 14;
pub const COLS: usize = 5;
pub const ICON_SIZE: u32 = 196;

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SetButtons = 0x0001,
    SetSmallWindow = 0x0006,
    SetBrightness = 0x000A,
    SetLabelStyle = 0x000B,
    UpdateButtons = 0x000D,
}

const IN_BUTTON: u16 = 0x0101;
const IN_BUTTON_ALT: u16 = 0x0102;
const IN_DEVICE_INFO: u16 = 0x0303;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Stats = 0,
    Clock = 1,
    Background = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    Button { index: u8, pressed: bool, state: u8 },
    DeviceInfo(String),
    Unknown { command: u16, length: u32 },
}

pub fn packet(command: Command, payload: &[u8], total_length: u32) -> [u8; PACKET_SIZE] {
    let mut out = [0u8; PACKET_SIZE];
    out[..2].copy_from_slice(&MAGIC);
    out[2..4].copy_from_slice(&(command as u16).to_be_bytes());
    out[4..8].copy_from_slice(&total_length.to_le_bytes());
    let n = payload.len().min(PAYLOAD_SIZE);
    out[HEADER_SIZE..HEADER_SIZE + n].copy_from_slice(&payload[..n]);
    out
}

/// Splits a payload of any size into the header packet plus raw continuation packets.
pub fn packets(command: Command, payload: &[u8]) -> Vec<[u8; PACKET_SIZE]> {
    let first = payload.len().min(PAYLOAD_SIZE);
    let mut out = vec![packet(command, &payload[..first], payload.len() as u32)];
    for chunk in payload[first..].chunks(PACKET_SIZE) {
        let mut raw = [0u8; PACKET_SIZE];
        raw[..chunk.len()].copy_from_slice(chunk);
        out.push(raw);
    }
    out
}

pub fn parse(buf: &[u8]) -> Option<Incoming> {
    // Windows may hand the report back with a leading zero report ID.
    let buf = match buf {
        [0, 0x7C, 0x7C, ..] => &buf[1..],
        _ => buf,
    };
    if buf.len() < HEADER_SIZE || buf[..2] != MAGIC {
        return None;
    }
    let command = u16::from_be_bytes([buf[2], buf[3]]);
    let length = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let body = &buf[HEADER_SIZE..];
    match command {
        IN_BUTTON | IN_BUTTON_ALT if body.len() >= 4 => Some(Incoming::Button {
            state: body[0],
            index: body[1],
            pressed: body[3] == 0x01,
        }),
        IN_DEVICE_INFO => {
            let end = body.iter().position(|&b| b == 0).unwrap_or(body.len());
            Some(Incoming::DeviceInfo(String::from_utf8_lossy(&body[..end]).into_owned()))
        }
        _ => Some(Incoming::Unknown { command, length }),
    }
}

pub fn brightness_payload(percent: u8) -> Vec<u8> {
    percent.min(100).to_string().into_bytes()
}

/// `mode|cpu|mem|HH:MM:SS|gpu`, e.g. `1|9|64|16:23:04|0`.
pub fn small_window_payload(mode: WindowMode, cpu: u8, mem: u8, time: &str, gpu: u8) -> Vec<u8> {
    format!("{}|{}|{}|{}|{}", mode as u8, cpu, mem, time, gpu).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout() {
        let p = packet(Command::SetBrightness, b"80", 2);
        assert_eq!(&p[..8], &[0x7C, 0x7C, 0x00, 0x0A, 0x02, 0x00, 0x00, 0x00]);
        assert_eq!(&p[8..10], b"80");
        assert!(p[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn long_payload_continues_in_raw_packets() {
        let data: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let out = packets(Command::SetButtons, &data);
        assert_eq!(out.len(), 3);
        assert_eq!(u32::from_le_bytes(out[0][4..8].try_into().unwrap()), 3000);
        assert_eq!(&out[0][8..], &data[..PAYLOAD_SIZE]);
        assert_eq!(&out[1][..], &data[PAYLOAD_SIZE..PAYLOAD_SIZE + PACKET_SIZE]);
        let tail = &data[PAYLOAD_SIZE + PACKET_SIZE..];
        assert_eq!(&out[2][..tail.len()], tail);
    }

    #[test]
    fn parses_button_event() {
        let mut buf = [0u8; 16];
        buf[..8].copy_from_slice(&[0x7C, 0x7C, 0x01, 0x01, 0x04, 0, 0, 0]);
        buf[8..12].copy_from_slice(&[0x00, 0x06, 0x01, 0x01]);
        assert_eq!(parse(&buf), Some(Incoming::Button { index: 6, pressed: true, state: 0 }));
    }

    #[test]
    fn parses_with_leading_report_id() {
        let mut buf = [0u8; 17];
        buf[1..9].copy_from_slice(&[0x7C, 0x7C, 0x01, 0x01, 0x04, 0, 0, 0]);
        buf[9..13].copy_from_slice(&[0x00, 0x02, 0x01, 0x00]);
        assert_eq!(parse(&buf), Some(Incoming::Button { index: 2, pressed: false, state: 0 }));
    }
}
