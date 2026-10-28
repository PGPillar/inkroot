//! USB presence probe for the Qualcomm EDL (9008) device.
//!
//! nusb only lists devices that have a driver. On Windows a 9008 device with no driver yet
//! (PnP status Error, no class) is invisible to it, so the probe falls back to SetupAPI: a
//! present device node whose hardware ID is `USB\VID_05C6&PID_9008` is reported with no
//! driver, which makes the flow offer the driver install.

const QUALCOMM_VID: u16 = 0x05c6;
const EDL_PID: u16 = 0x9008;
/// The hardware ID prefix Windows gives the EDL device.
pub const EDL_HARDWARE_ID: &str = r"USB\VID_05C6&PID_9008";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdlUsbInfo {
    /// Name of the bound driver (Windows only); `None` elsewhere or when unbound.
    pub driver: Option<String>,
}

pub trait UsbProbe: Send + Sync {
    fn edl_device(&self) -> Option<EdlUsbInfo>;
}

/// One Windows device node for the EDL hardware ID, as SetupAPI/cfgmgr32 report it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EdlDevNode {
    /// `SPDRP_SERVICE`: the driver service bound to the node (`WinUSB`), if any.
    pub service: Option<String>,
    /// Plugged in right now (SetupAPI `DIGCF_PRESENT`).
    pub present: bool,
    /// cfgmgr32 reports a problem (e.g. no driver installed: code 28).
    pub has_problem: bool,
}

/// Splits a REG_MULTI_SZ buffer (UTF-16, NUL-separated, double-NUL-terminated).
pub fn parse_multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// True when one of a device's hardware IDs is the EDL device's (`USB\VID_05C6&PID_9008`,
/// optionally followed by `&REV_xxxx` etc.).
pub fn is_edl_hardware_id(ids: &[String]) -> bool {
    ids.iter().any(|id| {
        let id = id.to_ascii_uppercase();
        id.strip_prefix(EDL_HARDWARE_ID)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('&'))
    })
}

/// The EDL device as the flow sees it, from the device nodes for its hardware ID: the first
/// present node, with its driver only when the node is working (a node with a problem has no
/// usable driver).
pub fn edl_info_from_nodes(nodes: &[EdlDevNode]) -> Option<EdlUsbInfo> {
    nodes.iter().find(|n| n.present).map(|n| EdlUsbInfo {
        driver: if n.has_problem {
            None
        } else {
            n.service.clone()
        },
    })
}

/// True when WinUSB has been bound to the EDL hardware ID on this computer before (present
/// now or not): Windows will bind it again when the tablet enters EDL.
pub fn winusb_bound_before(nodes: &[EdlDevNode]) -> bool {
    nodes
        .iter()
        .any(|n| crate::host::setup::driver_is_winusb(n.service.as_deref()))
}

pub struct NusbProbe;

impl UsbProbe for NusbProbe {
    fn edl_device(&self) -> Option<EdlUsbInfo> {
        if let Ok(devices) = nusb::list_devices() {
            for d in devices {
                if d.vendor_id() == QUALCOMM_VID && d.product_id() == EDL_PID {
                    #[cfg(windows)]
                    let driver = d.driver().map(str::to_string);
                    #[cfg(not(windows))]
                    let driver = None;
                    return Some(EdlUsbInfo { driver });
                }
            }
        }
        #[cfg(windows)]
        {
            win::edl_nodes(true).and_then(|n| edl_info_from_nodes(&n))
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

/// SetupAPI enumeration of the EDL device nodes (Windows only).
#[cfg(windows)]
pub mod win {
    use super::{EdlDevNode, is_edl_hardware_id, parse_multi_sz};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Status, CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT, DN_HAS_PROBLEM,
        HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_HARDWAREID, SPDRP_SERVICE,
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceRegistryPropertyW,
    };
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    struct DevInfoSet(HDEVINFO);
    impl Drop for DevInfoSet {
        fn drop(&mut self) {
            // SAFETY: a valid set from SetupDiGetClassDevsW, destroyed once.
            unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    /// A string or multi-string device registry property as UTF-16 code units.
    fn property(
        set: HDEVINFO,
        data: &SP_DEVINFO_DATA,
        prop: SETUP_DI_REGISTRY_PROPERTY,
    ) -> Option<Vec<u16>> {
        let mut buf = vec![0u16; 512];
        for _ in 0..2 {
            let mut needed = 0u32;
            let bytes = (buf.len() * 2) as u32;
            // SAFETY: `buf` holds `bytes` writable bytes; `data` comes from SetupDiEnumDeviceInfo.
            let ok = unsafe {
                SetupDiGetDeviceRegistryPropertyW(
                    set,
                    data,
                    prop,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    bytes,
                    &mut needed,
                )
            };
            if ok != 0 {
                let len = (needed as usize / 2).min(buf.len());
                buf.truncate(len);
                return Some(buf);
            }
            if needed as usize / 2 > buf.len() {
                buf = vec![0u16; needed as usize / 2 + 1];
            } else {
                return None;
            }
        }
        None
    }

    /// Every device node (present only, or also remembered ones) whose hardware ID is the EDL
    /// device's. `None` if SetupAPI couldn't be asked.
    pub fn edl_nodes(present_only: bool) -> Option<Vec<EdlDevNode>> {
        let enumerator: Vec<u16> = "USB".encode_utf16().chain(Some(0)).collect();
        let flags = DIGCF_ALLCLASSES | if present_only { DIGCF_PRESENT } else { 0 };
        // SAFETY: plain FFI call; the enumerator string is NUL-terminated.
        let set = unsafe {
            SetupDiGetClassDevsW(
                std::ptr::null(),
                enumerator.as_ptr(),
                std::ptr::null_mut(),
                flags,
            )
        };
        if set == INVALID_HANDLE_VALUE as HDEVINFO {
            return None;
        }
        let set = DevInfoSet(set);
        let mut nodes = Vec::new();
        for index in 0u32.. {
            // SAFETY: zeroed POD with cbSize set, as SetupDiEnumDeviceInfo requires.
            let mut data: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
            data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) } == 0 {
                break;
            }
            let ids = property(set.0, &data, SPDRP_HARDWAREID)
                .map(|b| parse_multi_sz(&b))
                .unwrap_or_default();
            if !is_edl_hardware_id(&ids) {
                continue;
            }
            let service = property(set.0, &data, SPDRP_SERVICE)
                .map(|b| parse_multi_sz(&b).into_iter().next().unwrap_or_default())
                .filter(|s| !s.is_empty());
            let (mut status, mut problem) = (0u32, 0u32);
            // SAFETY: out-pointers to locals; DevInst from SetupDiEnumDeviceInfo.
            let cr = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0) };
            // A remembered (unplugged) node has no live status: CR_NO_SUCH_DEVNODE.
            let present = cr == CR_SUCCESS;
            nodes.push(EdlDevNode {
                service,
                present,
                has_problem: present && (status & DN_HAS_PROBLEM != 0 || problem != 0),
            });
        }
        Some(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        let _ = NusbProbe.edl_device();
    }

    fn wide(parts: &[&str]) -> Vec<u16> {
        let mut v = Vec::new();
        for p in parts {
            v.extend(p.encode_utf16());
            v.push(0);
        }
        v.push(0);
        v
    }

    #[test]
    fn multi_sz_parses() {
        assert_eq!(
            parse_multi_sz(&wide(&[
                r"USB\VID_05C6&PID_9008&REV_0000",
                r"USB\VID_05C6&PID_9008"
            ])),
            [r"USB\VID_05C6&PID_9008&REV_0000", r"USB\VID_05C6&PID_9008"]
        );
        assert_eq!(parse_multi_sz(&wide(&["WinUSB"])), ["WinUSB"]);
        assert!(parse_multi_sz(&[0, 0]).is_empty());
        assert!(parse_multi_sz(&[]).is_empty());
        // No terminator at all (truncated buffer): still the one string.
        let raw: Vec<u16> = "WinUSB".encode_utf16().collect();
        assert_eq!(parse_multi_sz(&raw), ["WinUSB"]);
    }

    #[test]
    fn hardware_id_match() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_edl_hardware_id(&ids(&[
            r"USB\VID_05C6&PID_9008&REV_0000",
            r"USB\VID_05C6&PID_9008"
        ])));
        assert!(is_edl_hardware_id(&ids(&[r"usb\vid_05c6&pid_9008"])));
        assert!(!is_edl_hardware_id(&ids(&[r"USB\VID_05C6&PID_90080"])));
        assert!(!is_edl_hardware_id(&ids(&[r"USB\VID_05C6&PID_9091&MI_00"])));
        assert!(!is_edl_hardware_id(&ids(&[r"USB\VID_18D1&PID_4EE7"])));
        assert!(!is_edl_hardware_id(&[]));
    }

    #[test]
    fn driverless_node_reports_no_driver() {
        // What Windows shows before the driver is installed: Status Error, no class/service.
        let n = EdlDevNode {
            service: None,
            present: true,
            has_problem: true,
        };
        assert_eq!(edl_info_from_nodes(&[n]), Some(EdlUsbInfo { driver: None }));
        let bound = EdlDevNode {
            service: Some("WinUSB".into()),
            present: true,
            has_problem: false,
        };
        assert_eq!(
            edl_info_from_nodes(std::slice::from_ref(&bound)),
            Some(EdlUsbInfo {
                driver: Some("WinUSB".into())
            })
        );
        // Bound but failing: treated as no usable driver.
        let broken = EdlDevNode {
            has_problem: true,
            ..bound.clone()
        };
        assert_eq!(
            edl_info_from_nodes(&[broken]),
            Some(EdlUsbInfo { driver: None })
        );
        // Only remembered (unplugged) nodes: no device.
        let gone = EdlDevNode {
            present: false,
            ..bound
        };
        assert_eq!(edl_info_from_nodes(&[gone]), None);
        assert_eq!(edl_info_from_nodes(&[]), None);
    }

    #[test]
    fn winusb_bound_before_reads_any_node() {
        let node = |service: Option<&str>, present| EdlDevNode {
            service: service.map(str::to_string),
            present,
            has_problem: false,
        };
        assert!(!winusb_bound_before(&[]));
        assert!(!winusb_bound_before(&[node(None, true)]));
        assert!(!winusb_bound_before(&[node(Some("qcusbser"), false)]));
        assert!(winusb_bound_before(&[
            node(None, true),
            node(Some("WinUSB"), false)
        ]));
    }
}
