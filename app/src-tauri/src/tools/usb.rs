//! USB presence probe for the Qualcomm EDL (9008) device.

const QUALCOMM_VID: u16 = 0x05c6;
const EDL_PID: u16 = 0x9008;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdlUsbInfo {
    /// Name of the bound driver (Windows only); `None` elsewhere or when unbound.
    pub driver: Option<String>,
}

pub trait UsbProbe: Send + Sync {
    fn edl_device(&self) -> Option<EdlUsbInfo>;
}

pub struct NusbProbe;

impl UsbProbe for NusbProbe {
    fn edl_device(&self) -> Option<EdlUsbInfo> {
        let devices = nusb::list_devices().ok()?;
        for d in devices {
            if d.vendor_id() == QUALCOMM_VID && d.product_id() == EDL_PID {
                #[cfg(windows)]
                let driver = d.driver().map(str::to_string);
                #[cfg(not(windows))]
                let driver = None;
                return Some(EdlUsbInfo { driver });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        let _ = NusbProbe.edl_device();
    }
}
