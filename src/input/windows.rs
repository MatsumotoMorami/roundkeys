use super::*;
use crate::model::Binding;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::*,
    Foundation::*,
    Storage::FileSystem::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::*, WindowsAndMessaging::*},
};

struct Axis {
    cap: HIDP_VALUE_CAPS,
    usage: u16,
    last: Option<f64>,
    accum: f64,
}
struct Device {
    id: u32,
    name: String,
    prep: Vec<u64>,
    buttons: Vec<HIDP_BUTTON_CAPS>,
    axes: Vec<Axis>,
    down: HashSet<u16>,
}
struct Engine {
    shared: Shared,
    devices: HashMap<usize, Device>,
    expiry: HashMap<Binding, f64>,
}
thread_local! { static ENGINE:RefCell<Option<Engine>>=const{RefCell::new(None)}; }
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn run(shared: Shared) {
    unsafe {
        let class = wide("RoundKeysInputSink");
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            shared.lock().unwrap().input_status =
                format!("输入窗口创建失败：{}", std::io::Error::last_os_error());
            return;
        }
        let inputs: Vec<_> = [6, 4, 5, 8]
            .iter()
            .map(|&usage| RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: usage,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            })
            .collect();
        if RegisterRawInputDevices(
            inputs.as_ptr(),
            inputs.len() as u32,
            size_of::<RAWINPUTDEVICE>() as u32,
        ) == 0
        {
            shared.lock().unwrap().input_status =
                format!("后台输入注册失败：{}", std::io::Error::last_os_error());
            return;
        }
        let mut engine = Engine {
            shared: shared.clone(),
            devices: HashMap::new(),
            expiry: HashMap::new(),
        };
        engine.discover();
        ENGINE.with(|e| *e.borrow_mut() = Some(engine));
        shared.lock().unwrap().input_status = "Raw Input 后台监听 · 不占用手台".into();
        let mut message: MSG = zeroed();
        loop {
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            ENGINE.with(|e| {
                if let Some(e) = e.borrow_mut().as_mut() {
                    e.expire();
                }
            });
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => ENGINE.with(|e| {
            if let Some(e) = e.borrow_mut().as_mut() {
                e.read(l as HRAWINPUT);
            }
        }),
        WM_INPUT_DEVICE_CHANGE => ENGINE.with(|e| {
            if let Some(e) = e.borrow_mut().as_mut() {
                e.discover();
            }
        }),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
impl Engine {
    fn discover(&mut self) {
        unsafe {
            let mut count = 0;
            GetRawInputDeviceList(
                null_mut(),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );
            let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
            if GetRawInputDeviceList(
                list.as_mut_ptr(),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            ) == u32::MAX
            {
                return;
            }
            list.truncate(count as usize);
            let alive: HashSet<_> = list.iter().map(|d| d.hDevice as usize).collect();
            let removed: Vec<_> = self
                .devices
                .keys()
                .filter(|h| !alive.contains(h))
                .copied()
                .collect();
            for h in removed {
                if let Some(d) = self.devices.remove(&h) {
                    let mut m = self.shared.lock().unwrap();
                    let now = m.now();
                    let keys: Vec<_> = m
                        .down
                        .iter()
                        .filter(|b| match b {
                            Binding::ControllerButton { device, .. }
                            | Binding::ControllerAxis { device, .. } => *device == d.id,
                            _ => false,
                        })
                        .copied()
                        .collect();
                    for b in keys {
                        m.transition(b, false, now);
                    }
                }
            }
            for d in list {
                if d.dwType == RIM_TYPEHID
                    && !self.devices.contains_key(&(d.hDevice as usize))
                    && let Some(device) = Device::load(d.hDevice)
                {
                    self.devices.insert(d.hDevice as usize, device);
                }
            }
            let mut m = self.shared.lock().unwrap();
            m.devices = self
                .devices
                .values()
                .map(|d| (d.id, d.name.clone()))
                .collect();
            m.devices.sort_by_key(|d| d.0);
            m.controller_status = if m.devices.is_empty() {
                "未检测到手台；键盘仍可使用".into()
            } else {
                format!("已连接 {} 个 HID 控制器", m.devices.len())
            };
        }
    }
    fn read(&mut self, handle: HRAWINPUT) {
        unsafe {
            let header = size_of::<RAWINPUTHEADER>() as u32;
            let mut size = 0;
            if GetRawInputData(handle, RID_INPUT, null_mut(), &mut size, header) == u32::MAX
                || size < header
            {
                return;
            }
            // u64 allocation keeps the RAWINPUT and HID preparsed buffers aligned.
            let mut buf = vec![0u64; (size as usize).max(size_of::<RAWINPUT>()).div_ceil(8)];
            if GetRawInputData(
                handle,
                RID_INPUT,
                buf.as_mut_ptr().cast(),
                &mut size,
                header,
            ) == u32::MAX
            {
                return;
            }
            let raw = &*(buf.as_ptr().cast::<RAWINPUT>());
            if raw.header.dwType == RIM_TYPEKEYBOARD {
                if size < (header as usize + size_of::<RAWKEYBOARD>()) as u32 {
                    return;
                }
                let k = raw.data.keyboard;
                let mut code = k.VKey;
                code = match code {
                    16 => {
                        if k.MakeCode == 0x36 {
                            161
                        } else {
                            160
                        }
                    }
                    17 => {
                        if k.Flags & 2 != 0 {
                            163
                        } else {
                            162
                        }
                    }
                    18 => {
                        if k.Flags & 2 != 0 {
                            165
                        } else {
                            164
                        }
                    }
                    _ => code,
                };
                if code == 255 {
                    return;
                }
                let mut m = self.shared.lock().unwrap();
                let now = m.now();
                m.transition(Binding::Keyboard { code }, k.Flags & 1 == 0, now);
            } else if raw.header.dwType == RIM_TYPEHID {
                if size < header + 8 {
                    return;
                }
                let h = raw.header.hDevice as usize;
                if let std::collections::hash_map::Entry::Vacant(entry) = self.devices.entry(h)
                    && let Some(d) = Device::load(raw.header.hDevice)
                {
                    entry.insert(d);
                }
                let Some(d) = self.devices.get_mut(&h) else {
                    return;
                };
                let hid = &raw.data.hid;
                let start = std::ptr::addr_of!(hid.bRawData).cast::<u8>();
                let offset = start as usize - buf.as_ptr() as usize;
                let total = (hid.dwSizeHid as usize).checked_mul(hid.dwCount as usize);
                if total.is_none_or(|t| offset.saturating_add(t) > size as usize)
                    || hid.dwSizeHid == 0
                {
                    return;
                }
                for n in 0..hid.dwCount as usize {
                    let report = std::slice::from_raw_parts(
                        start.add(n * hid.dwSizeHid as usize),
                        hid.dwSizeHid as usize,
                    );
                    let mut m = self.shared.lock().unwrap();
                    let now = m.now();
                    let prep = d.prep.as_ptr() as PHIDP_PREPARSED_DATA;
                    // Reports must not release buttons belonging to another report ID.
                    for cap in &d.buttons {
                        if cap.ReportID != 0 && report[0] != cap.ReportID {
                            continue;
                        }
                        let mut usages = [0u16; 128];
                        let mut count = 128;
                        if HidP_GetUsages(
                            HidP_Input,
                            cap.UsagePage,
                            cap.LinkCollection,
                            usages.as_mut_ptr(),
                            &mut count,
                            prep,
                            report.as_ptr() as *mut u8,
                            report.len() as u32,
                        ) != HIDP_STATUS_SUCCESS
                        {
                            continue;
                        }
                        let (min, max) = if cap.IsRange {
                            (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.UsageMax)
                        } else {
                            let u = cap.Anonymous.NotRange.Usage;
                            (u, u)
                        };
                        for usage in min..=max.min(128) {
                            if usage == 0 {
                                continue;
                            }
                            let pressed = usages[..count.min(128) as usize].contains(&usage);
                            let was = d.down.contains(&usage);
                            if pressed != was {
                                if pressed {
                                    d.down.insert(usage);
                                } else {
                                    d.down.remove(&usage);
                                }
                                m.transition(
                                    Binding::ControllerButton {
                                        device: d.id,
                                        button: (usage - 1) as u8,
                                    },
                                    pressed,
                                    now,
                                );
                            }
                        }
                    }
                    for a in &mut d.axes {
                        if a.cap.ReportID != 0 && report[0] != a.cap.ReportID {
                            continue;
                        }
                        let mut v = 0;
                        if HidP_GetUsageValue(
                            HidP_Input,
                            a.cap.UsagePage,
                            a.cap.LinkCollection,
                            a.usage,
                            &mut v,
                            prep,
                            report.as_ptr(),
                            report.len() as u32,
                        ) != HIDP_STATUS_SUCCESS
                        {
                            continue;
                        }
                        let value =
                            if a.cap.LogicalMin < 0 && a.cap.BitSize > 0 && a.cap.BitSize < 32 {
                                ((v << (32 - a.cap.BitSize)) as i32 >> (32 - a.cap.BitSize)) as f64
                            } else {
                                v as f64
                            };
                        let range =
                            (a.cap.LogicalMax as f64 - a.cap.LogicalMin as f64 + 1.0).max(1.0);
                        let normalized = (value - a.cap.LogicalMin as f64) / range;
                        if let Some(previous) = a.last {
                            let delta = super::axis_delta(previous, normalized);
                            if delta.signum() != a.accum.signum() {
                                a.accum = 0.0;
                            }
                            a.accum += delta;
                            if a.accum.abs() >= m.config.scratch_threshold {
                                let direction = if a.accum > 0.0 { 1 } else { -1 };
                                a.accum = 0.0;
                                let binding = Binding::ControllerAxis {
                                    device: d.id,
                                    axis: (a.usage - 0x30) as u8,
                                    direction,
                                };
                                let reverse = Binding::ControllerAxis {
                                    device: d.id,
                                    axis: (a.usage - 0x30) as u8,
                                    direction: -direction,
                                };
                                m.transition(binding, true, now);
                                m.transition(reverse, false, now);
                                self.expiry.remove(&reverse);
                                self.expiry
                                    .insert(binding, now + m.config.scratch_release_ms);
                            }
                        }
                        a.last = Some(normalized);
                    }
                }
            }
        }
    }
    fn expire(&mut self) {
        let mut m = self.shared.lock().unwrap();
        let now = m.now();
        let expired: Vec<_> = self
            .expiry
            .iter()
            .filter(|(_, time)| now >= **time)
            .map(|(b, _)| *b)
            .collect();
        for b in expired {
            self.expiry.remove(&b);
            m.transition(b, false, now);
        }
    }
}
impl Device {
    unsafe fn load(handle: HANDLE) -> Option<Self> {
        unsafe {
            let mut size = 0;
            if GetRawInputDeviceInfoW(handle, RIDI_PREPARSEDDATA, null_mut(), &mut size) == u32::MAX
                || size == 0
            {
                return None;
            }
            let mut prep = vec![0u64; (size as usize).div_ceil(8)];
            if GetRawInputDeviceInfoW(
                handle,
                RIDI_PREPARSEDDATA,
                prep.as_mut_ptr().cast(),
                &mut size,
            ) == u32::MAX
            {
                return None;
            }
            let pp = prep.as_ptr() as PHIDP_PREPARSED_DATA;
            let mut caps: HIDP_CAPS = zeroed();
            if HidP_GetCaps(pp, &mut caps) != HIDP_STATUS_SUCCESS
                || caps.UsagePage != 1
                || ![4, 5, 8].contains(&caps.Usage)
            {
                return None;
            }
            let mut len = 0;
            GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, null_mut(), &mut len);
            let mut path = vec![0u16; len as usize + 1];
            GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, path.as_mut_ptr().cast(), &mut len);
            let pathstr = String::from_utf16_lossy(&path[..len as usize]);
            let id = pathstr.bytes().fold(2166136261u32, |h, b| {
                (h ^ (b as u32)).wrapping_mul(16777619)
            });
            let mut product = [0u16; 256];
            let file = CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            );
            let name = if file != INVALID_HANDLE_VALUE {
                let ok = HidD_GetProductString(file, product.as_mut_ptr().cast(), 512);
                CloseHandle(file);
                if ok {
                    String::from_utf16_lossy(
                        &product[..product.iter().position(|x| *x == 0).unwrap_or(256)],
                    )
                } else {
                    format!("HID {:04X}", id & 0xffff)
                }
            } else {
                format!("HID {:04X}", id & 0xffff)
            };
            let mut button_count = caps.NumberInputButtonCaps;
            let mut buttons = vec![HIDP_BUTTON_CAPS::default(); button_count as usize];
            if button_count > 0 {
                HidP_GetButtonCaps(HidP_Input, buttons.as_mut_ptr(), &mut button_count, pp);
            }
            buttons.truncate(button_count as usize);
            buttons.retain(|c| c.UsagePage == 9);
            let mut value_count = caps.NumberInputValueCaps;
            let mut values = vec![HIDP_VALUE_CAPS::default(); value_count as usize];
            if value_count > 0 {
                HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &mut value_count, pp);
            }
            values.truncate(value_count as usize);
            let mut axes = vec![];
            for cap in values {
                if cap.UsagePage != 1 {
                    continue;
                }
                let (min, max) = if cap.IsRange {
                    (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.UsageMax)
                } else {
                    let u = cap.Anonymous.NotRange.Usage;
                    (u, u)
                };
                for usage in min.max(0x30)..=max.min(0x38) {
                    axes.push(Axis {
                        cap,
                        usage,
                        last: None,
                        accum: 0.0,
                    });
                }
            }
            Some(Self {
                id,
                name,
                prep,
                buttons,
                axes,
                down: HashSet::new(),
            })
        }
    }
}
