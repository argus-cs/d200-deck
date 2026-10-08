use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Context, Result};
use hidapi::{HidApi, HidDevice};
use serde_json::Value;

use crate::layout::{build_zip, KeyView};
use crate::protocol::{
    brightness_payload, packets, parse, small_window_payload, Command, Incoming, WindowMode,
    PACKET_SIZE, PRODUCT_ID, USAGE_PAGE, VENDOR_ID,
};

static LAYOUT_NONCE: AtomicU64 = AtomicU64::new(0);

pub struct D200 {
    dev: HidDevice,
    /// Windows expects a leading report ID byte (0 for this device) on writes.
    report_id_prefix: bool,
}

impl D200 {
    pub fn open(api: &HidApi) -> Result<Self> {
        let info = api
            .device_list()
            .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID && d.usage_page() == USAGE_PAGE)
            .ok_or_else(|| anyhow!("D200 not found (VID 2207, PID 0019, usage page 0x0C)"))?;
        let dev = info.open_device(api).context("could not open the D200 HID interface")?;
        Ok(Self { dev, report_id_prefix: cfg!(windows) })
    }

    pub fn set_report_id_prefix(&mut self, on: bool) {
        self.report_id_prefix = on;
    }

    pub fn set_brightness(&self, percent: u8) -> Result<()> {
        self.send(Command::SetBrightness, &brightness_payload(percent))
    }

    pub fn set_small_window(&self, mode: WindowMode, cpu: u8, mem: u8, gpu: u8) -> Result<()> {
        let time = chrono::Local::now().format("%H:%M:%S").to_string();
        self.send(Command::SetSmallWindow, &small_window_payload(mode, cpu, mem, &time, gpu))
    }

    pub fn set_label_style(&self, style: &Value) -> Result<()> {
        self.send(Command::SetLabelStyle, &serde_json::to_vec(style)?)
    }

    /// `partial` sends only the given keys (command 0x000D) instead of the whole page.
    pub fn set_layout(&self, keys: &BTreeMap<usize, KeyView>, partial: bool) -> Result<usize> {
        let zip = build_zip(keys, LAYOUT_NONCE.fetch_add(1, Ordering::Relaxed))?;
        let command = if partial { Command::UpdateButtons } else { Command::SetButtons };
        self.send(command, &zip)?;
        Ok(zip.len())
    }

    pub fn read(&self, timeout_ms: i32) -> Result<Option<(Incoming, Vec<u8>)>> {
        let mut buf = [0u8; PACKET_SIZE + 1];
        let n = self.dev.read_timeout(&mut buf, timeout_ms)?;
        if n == 0 {
            return Ok(None);
        }
        Ok(parse(&buf[..n]).map(|msg| (msg, buf[..n].to_vec())))
    }

    fn send(&self, command: Command, payload: &[u8]) -> Result<()> {
        for packet in packets(command, payload) {
            self.write_packet(&packet)?;
        }
        Ok(())
    }

    fn write_packet(&self, packet: &[u8; PACKET_SIZE]) -> Result<()> {
        if self.report_id_prefix {
            let mut buf = [0u8; PACKET_SIZE + 1];
            buf[1..].copy_from_slice(packet);
            self.dev.write(&buf)?;
        } else {
            self.dev.write(packet)?;
        }
        Ok(())
    }
}
