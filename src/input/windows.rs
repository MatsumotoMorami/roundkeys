use super::*;
use crate::model::Binding;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::*,
    Foundation::*,
    Storage::FileSystem::*,
    System::{IO::*, LibraryLoader::GetModuleHandleW, Threading::*},
    UI::{
        Input::{
            KeyboardAndMouse::{
                GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU,
                VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT,
            },
            *,
        },
        WindowsAndMessaging::*,
    },
};

#[cfg(test)]
mod tests;

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
    reader: Option<HidReader>,
    reader_error: Option<String>,
    reports: u64,
    streams: ReportStreams,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportSource {
    Direct,
    Raw,
}

// Windows can deliver the same ordered reports through both APIs, with one queue
// a few frames behind the other. Consume matching copies without replaying edges.
#[derive(Default)]
struct ReportStreams {
    leader: Option<ReportSource>,
    unmatched: VecDeque<(Instant, Vec<u8>)>,
    last_direct: Option<(Instant, Vec<u8>)>,
    last_raw: Option<(Instant, Vec<u8>)>,
}
impl ReportStreams {
    fn accept(&mut self, source: ReportSource, report: &[u8], now: Instant) -> bool {
        let last = match source {
            ReportSource::Direct => &mut self.last_direct,
            ReportSource::Raw => &mut self.last_raw,
        };
        let (at, previous) = last.get_or_insert_with(|| (now, Vec::new()));
        let recent = now.duration_since(*at) <= Duration::from_millis(100);
        *at = now;
        if recent && previous == report {
            return false;
        }
        previous.clear();
        previous.extend_from_slice(report);
        while self
            .unmatched
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > Duration::from_millis(100))
        {
            self.unmatched.pop_front();
        }
        if self.leader != Some(source) {
            if let Some(i) = self.unmatched.iter().position(|(_, r)| r == report) {
                self.unmatched.drain(..=i);
                return false;
            }
            self.unmatched.clear();
            self.leader = Some(source);
        }
        if self.unmatched.len() == 128 {
            self.unmatched.pop_front();
        }
        self.unmatched.push_back((now, report.to_vec()));
        true
    }
}
struct Engine {
    shared: Shared,
    devices: HashMap<usize, Device>,
    expiry: HashMap<Binding, f64>,
    sampled_keys: HashSet<u16>,
    recent_raw: HashMap<u16, Instant>,
    raw_keys: HashSet<(usize, u16)>,
    last_hid_status: Instant,
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
        // Register the keyboard separately so controller input survives a keyboard registration failure.
        let inputs: Vec<_> = [4, 5, 8]
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
        let keyboard = RAWINPUTDEVICE {
            usUsagePage: 1,
            usUsage: 6,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        };
        let raw_keyboard =
            RegisterRawInputDevices(&keyboard, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0;
        let status = if raw_keyboard {
            "后台 Raw Input 键盘 + HID 监听".into()
        } else {
            format!(
                "键盘 Raw Input 注册失败，回退到采样：{}",
                std::io::Error::last_os_error()
            )
        };
        let mut engine = Engine {
            shared: shared.clone(),
            devices: HashMap::new(),
            expiry: HashMap::new(),
            sampled_keys: HashSet::new(),
            recent_raw: HashMap::new(),
            raw_keys: HashSet::new(),
            last_hid_status: Instant::now(),
        };
        engine.discover();
        ENGINE.with(|e| *e.borrow_mut() = Some(engine));
        shared.lock().unwrap().input_status = status;
        let mut message: MSG = zeroed();
        loop {
            // A busy Raw Input queue must still give direct HID reads and axis releases a turn.
            let mut processed = 0;
            while processed < 128 && PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                processed += 1;
                if message.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            ENGINE.with(|e| {
                if let Some(e) = e.borrow_mut().as_mut() {
                    e.poll_hid();
                    e.poll_keyboard(raw_keyboard);
                    e.expire();
                }
            });
            if processed < 128 {
                std::thread::sleep(Duration::from_millis(2));
            }
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
                if w == GIDC_REMOVAL as usize {
                    let removed: Vec<_> = e
                        .raw_keys
                        .iter()
                        .filter(|(device, _)| *device == l as usize)
                        .copied()
                        .collect();
                    for (device, code) in removed {
                        e.keyboard_event(device, code, false);
                    }
                }
                e.discover();
            }
        }),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
fn raw_key_code(key: &RAWKEYBOARD) -> Option<u16> {
    if !(1..255).contains(&key.VKey) || key.MakeCode == 0xff {
        return None;
    }
    let extended = key.Flags as u32 & RI_KEY_E0 != 0;
    Some(match key.VKey {
        VK_SHIFT if extended => return None, // synthetic Shift prefix of an extended key
        VK_SHIFT => {
            if key.MakeCode == 0x36 {
                VK_RSHIFT
            } else {
                VK_LSHIFT
            }
        }
        VK_CONTROL => {
            if extended {
                VK_RCONTROL
            } else {
                VK_LCONTROL
            }
        }
        VK_MENU => {
            if extended {
                VK_RMENU
            } else {
                VK_LMENU
            }
        }
        code => code,
    })
}
fn modifier(code: u16) -> Option<u16> {
    match code {
        VK_LSHIFT | VK_RSHIFT => Some(VK_SHIFT),
        VK_LCONTROL | VK_RCONTROL => Some(VK_CONTROL),
        VK_LMENU | VK_RMENU => Some(VK_MENU),
        _ => None,
    }
}
impl Engine {
    fn keyboard_event(&mut self, device: usize, code: u16, down: bool) {
        let alias = modifier(code);
        let mut m = self.shared.lock().unwrap();
        let bound = |key| {
            m.config
                .lanes
                .iter()
                .any(|lane| lane.bindings.contains(&Binding::Keyboard { code: key }))
        };
        let primary = bound(code) || m.capture.is_some();
        let generic = alias.filter(|&key| bound(key));
        if down && !primary && generic.is_none() {
            return;
        }
        let changed = if down {
            self.raw_keys.insert((device, code))
        } else {
            self.raw_keys.remove(&(device, code))
        };
        let mut took_sample = false;
        for key in [Some(code), alias].into_iter().flatten() {
            if changed || self.sampled_keys.contains(&key) {
                self.recent_raw.insert(key, Instant::now());
                took_sample |= self.sampled_keys.remove(&key);
            }
        }
        if !changed && !took_sample {
            return;
        }
        let now = m.now();
        for key in [Some(code), alias].into_iter().flatten() {
            let binding = Binding::Keyboard { code: key };
            let held = self
                .raw_keys
                .iter()
                .any(|&(_, c)| c == key || modifier(c) == Some(key));
            if held != m.down.contains(&binding)
                && (!held || (key == code && primary) || generic == Some(key))
            {
                m.transition(binding, held, now);
            }
        }
    }
    fn poll_hid(&mut self) {
        for d in self.devices.values_mut() {
            for _ in 0..32 {
                let Some(reader) = d.reader.as_mut() else {
                    break;
                };
                match reader.poll() {
                    Ok(Some(report)) => {
                        d.reports += 1;
                        if d.streams
                            .accept(ReportSource::Direct, &report, Instant::now())
                        {
                            parse_report(d, &self.shared, &mut self.expiry, &report);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        d.reader_error = Some(error);
                        d.reader = None;
                        self.shared.lock().unwrap().controller_status = format!(
                            "{}：直接读取失败，使用 Raw Input；{}",
                            d.name,
                            d.reader_error.as_deref().unwrap_or("")
                        );
                        break;
                    }
                }
            }
        }
        if self.last_hid_status.elapsed() >= Duration::from_secs(1) {
            self.last_hid_status = Instant::now();
            let direct = self.devices.values().filter(|d| d.reader.is_some()).count();
            let reports: u64 = self.devices.values().map(|d| d.reports).sum();
            let errors = self
                .devices
                .values()
                .filter_map(|d| d.reader_error.as_deref())
                .collect::<Vec<_>>()
                .join("；");
            self.shared.lock().unwrap().controller_status = format!(
                "HID 直接后台读取 {}/{} · 报告 {} {}",
                direct,
                self.devices.len(),
                reports,
                errors
            );
        }
    }
    fn poll_keyboard(&mut self, raw_keyboard: bool) {
        self.poll_keyboard_with(raw_keyboard, |code| unsafe {
            GetAsyncKeyState(code as i32) < 0
        });
    }
    fn poll_keyboard_with(&mut self, raw_keyboard: bool, is_down: impl Fn(u16) -> bool) {
        let mut m = self.shared.lock().unwrap();
        let now = m.now();
        self.recent_raw
            .retain(|_, at| at.elapsed() < Duration::from_millis(12));
        let mut candidates: HashSet<u16> = m
            .config
            .lanes
            .iter()
            .flat_map(|lane| lane.bindings.iter())
            .filter_map(|b| match b {
                Binding::Keyboard { code } => Some(*code),
                _ => None,
            })
            .chain(self.sampled_keys.iter().copied())
            .collect();
        // Only bound keys are sampled normally; binding capture temporarily enables all keys.
        if !raw_keyboard && m.capture.is_some() {
            candidates.extend(8..=254);
        }
        for code in candidates {
            // High bit is current state. Do not consume the unreliable "recently pressed" bit.
            if self.recent_raw.contains_key(&code) {
                continue;
            }
            if is_down(code) {
                if self.sampled_keys.insert(code) && !m.down.contains(&Binding::Keyboard { code }) {
                    m.transition(Binding::Keyboard { code }, true, now);
                }
            } else if self.sampled_keys.remove(&code) {
                // Only a key observed down by the sampler can be released by it.
                // A zero from an inaccessible keyboard state must not cancel a raw press.
                self.raw_keys
                    .retain(|&(_, c)| c != code && modifier(c) != Some(code));
                m.transition(Binding::Keyboard { code }, false, now);
            }
        }
    }
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
            if raw.header.dwType == RIM_TYPEKEYBOARD
                && size >= header + size_of::<RAWKEYBOARD>() as u32
            {
                let key = &raw.data.keyboard;
                if let Some(code) = raw_key_code(key) {
                    self.keyboard_event(
                        raw.header.hDevice as usize,
                        code,
                        key.Flags as u32 & RI_KEY_BREAK == 0,
                    );
                }
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
                    d.reports += 1;
                    if d.reader.is_none()
                        || d.streams.accept(ReportSource::Raw, report, Instant::now())
                    {
                        parse_report(d, &self.shared, &mut self.expiry, report);
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
fn parse_report(
    d: &mut Device,
    shared: &Shared,
    expiry: &mut HashMap<Binding, f64>,
    report: &[u8],
) {
    if report.is_empty() {
        return;
    }
    unsafe {
        let mut m = shared.lock().unwrap();
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
            let value = if a.cap.LogicalMin < 0 && a.cap.BitSize > 0 && a.cap.BitSize < 32 {
                ((v << (32 - a.cap.BitSize)) as i32 >> (32 - a.cap.BitSize)) as f64
            } else {
                v as f64
            };
            let range = (a.cap.LogicalMax as f64 - a.cap.LogicalMin as f64 + 1.0).max(1.0);
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
                    expiry.remove(&reverse);
                    expiry.insert(binding, now + m.config.scratch_release_ms);
                }
            }
            a.last = Some(normalized);
        }
    }
}

// A shared read-only handle receives reports independently of window focus.
// OVERLAPPED and its buffer stay allocated until completion or cancellation.
struct HidReader {
    file: HANDLE,
    overlap: Box<OVERLAPPED>,
    buffer: Vec<u8>,
    pending: bool,
}
impl HidReader {
    unsafe fn open(path: &[u16], size: usize) -> Result<Self, String> {
        unsafe {
            let file = CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            );
            if file == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let event = CreateEventW(null(), 1, 0, null());
            if event.is_null() {
                let error = std::io::Error::last_os_error().to_string();
                CloseHandle(file);
                return Err(error);
            }
            let mut overlap: Box<OVERLAPPED> = Box::new(zeroed());
            overlap.hEvent = event;
            Ok(Self {
                file,
                overlap,
                buffer: vec![0; size],
                pending: false,
            })
        }
    }
    fn poll(&mut self) -> Result<Option<Vec<u8>>, String> {
        unsafe {
            let mut bytes = 0;
            if !self.pending {
                ResetEvent(self.overlap.hEvent);
                self.overlap.Internal = 0;
                self.overlap.InternalHigh = 0;
                let ok = ReadFile(
                    self.file,
                    self.buffer.as_mut_ptr(),
                    self.buffer.len() as u32,
                    null_mut(),
                    self.overlap.as_mut(),
                );
                if ok == 0 && GetLastError() != ERROR_IO_PENDING {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                self.pending = true;
            }
            if GetOverlappedResult(self.file, self.overlap.as_mut(), &mut bytes, 0) == 0 {
                if GetLastError() == ERROR_IO_INCOMPLETE {
                    return Ok(None);
                }
                self.pending = false;
                return Err(std::io::Error::last_os_error().to_string());
            }
            self.pending = false;
            Ok(Some(
                self.buffer[..(bytes as usize).min(self.buffer.len())].to_vec(),
            ))
        }
    }
}
impl Drop for HidReader {
    fn drop(&mut self) {
        unsafe {
            if self.pending {
                CancelIoEx(self.file, self.overlap.as_mut());
                let mut bytes = 0;
                GetOverlappedResult(self.file, self.overlap.as_mut(), &mut bytes, 1);
            }
            CloseHandle(self.overlap.hEvent);
            CloseHandle(self.file);
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
            let (reader, reader_error) =
                match HidReader::open(&path, caps.InputReportByteLength as usize) {
                    Ok(reader) => (Some(reader), None),
                    Err(error) => (None, Some(error)),
                };
            Some(Self {
                reader,
                reader_error,
                reports: 0,
                streams: ReportStreams::default(),
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
