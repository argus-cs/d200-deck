//! Builds the zip the D200 expects for key images and labels.
//!
//! Layout inside the archive: `dummy.txt` (padding), `Images/*.png`,
//! `manifest.json` keyed by `"col_row"`, then an empty `sentinel.txt`.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::protocol::{COLS, PACKET_SIZE, PAYLOAD_SIZE};

#[derive(Clone, Debug, Default)]
pub struct KeyView {
    pub text: String,
    /// 196×196 PNG; `None` leaves the key without an image.
    pub png: Option<Vec<u8>>,
}

/// A continuation packet that starts with one of these bytes breaks the
/// transfer, so the zip is re-padded until no packet boundary lands on them.
const BAD_LEAD_BYTES: [u8; 2] = [0x00, 0x7C];
const MAX_ATTEMPTS: usize = 1000;

/// `nonce` goes into the image names: the device caches icons by file name.
pub fn build_zip(keys: &BTreeMap<usize, KeyView>, nonce: u64) -> Result<Vec<u8>> {
    let mut rng = XorShift::seeded(nonce);
    for attempt in 0..MAX_ATTEMPTS {
        let padding = rng.alphanumeric(attempt * 64);
        let zip = write_zip(keys, nonce, &padding)?;
        if is_transfer_safe(&zip) {
            return Ok(zip);
        }
    }
    bail!("could not build a layout zip the D200 accepts after {MAX_ATTEMPTS} attempts")
}

pub fn is_transfer_safe(zip: &[u8]) -> bool {
    (PAYLOAD_SIZE..zip.len())
        .step_by(PACKET_SIZE)
        .all(|i| !BAD_LEAD_BYTES.contains(&zip[i]))
}

fn write_zip(keys: &BTreeMap<usize, KeyView>, nonce: u64, padding: &str) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("dummy.txt", opts)?;
    zip.write_all(padding.as_bytes())?;

    let mut manifest = Map::new();
    for (&index, key) in keys {
        let icon = match &key.png {
            Some(png) => {
                let name = format!("Images/{nonce}_{index}.png");
                zip.start_file(name.as_str(), opts)?;
                zip.write_all(png)?;
                name
            }
            None => String::new(),
        };
        manifest.insert(
            format!("{}_{}", index % COLS, index / COLS),
            json!({ "State": 0, "ViewParam": [{ "Text": key.text, "Icon": icon }] }),
        );
    }

    zip.start_file("manifest.json", opts)?;
    zip.write_all(serde_json::to_string(&Value::Object(manifest))?.as_bytes())?;
    zip.start_file("sentinel.txt", opts)?;
    Ok(zip.finish()?.into_inner())
}

struct XorShift(u64);

impl XorShift {
    fn seeded(seed: u64) -> Self {
        let clock = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self((seed ^ clock) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn alphanumeric(&mut self, len: usize) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        (0..len)
            .map(|_| CHARS[(self.next() % CHARS.len() as u64) as usize] as char)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_never_puts_bad_bytes_on_packet_boundaries() {
        let keys: BTreeMap<usize, KeyView> = (0..14)
            .map(|i| (i, KeyView { text: format!("{}", i + 1), png: Some(vec![i as u8; 4000]) }))
            .collect();
        for nonce in 0..20 {
            let zip = build_zip(&keys, nonce).unwrap();
            assert!(is_transfer_safe(&zip));
        }
    }

    #[test]
    fn manifest_uses_col_row_keys() {
        let mut keys = BTreeMap::new();
        keys.insert(7, KeyView { text: "Oi".into(), png: None });
        let zip = build_zip(&keys, 1).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        let mut manifest = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("manifest.json").unwrap(), &mut manifest).unwrap();
        let v: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(v["2_1"]["ViewParam"][0]["Text"], "Oi");
    }
}
