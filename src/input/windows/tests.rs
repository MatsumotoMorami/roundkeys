use super::*;
use crate::model::{Config, Model};
use std::sync::{Arc, Mutex};

fn engine() -> Engine {
    Engine {
        shared: Arc::new(Mutex::new(Model::new(Config::default()))),
        devices: HashMap::new(),
        expiry: HashMap::new(),
        keyboard_down: HashSet::new(),
        raw_keys: HashSet::new(),
        last_hid_status: Instant::now(),
    }
}

#[test]
fn quick_taps_and_overlapping_keyboards_reach_the_overlay() {
    let mut e = engine();
    let code = b'S' as u16;
    e.keyboard_event(1, code, true);
    e.keyboard_event(1, code, false);
    e.keyboard_event(1, code, true);
    e.keyboard_event(1, code, true); // OS autorepeat
    e.keyboard_event(2, code, true);
    e.keyboard_event(1, code, false);
    assert!(e.shared.lock().unwrap().pressed()[1]);
    e.keyboard_event(2, code, false);
    let mut model = e.shared.lock().unwrap();
    assert!(!model.pressed()[1]);
    assert_eq!(model.snapshot().counts[1], 2);
}

#[test]
fn raw_modifiers_support_sided_capture_and_existing_generic_bindings() {
    let key = RAWKEYBOARD {
        VKey: VK_CONTROL,
        Flags: RI_KEY_E0 as u16,
        ..Default::default()
    };
    let right = raw_key_code(&key).unwrap();
    assert_eq!(right, VK_RCONTROL);
    assert_eq!(
        raw_key_code(&RAWKEYBOARD {
            VKey: VK_SHIFT,
            MakeCode: 0x36,
            ..Default::default()
        }),
        Some(VK_RSHIFT)
    );
    let mut e = engine();
    e.shared.lock().unwrap().config.lanes[1].bindings =
        vec![Binding::Keyboard { code: VK_CONTROL }];
    e.keyboard_event(1, right, true);
    e.keyboard_event(1, VK_LCONTROL, true);
    e.keyboard_event(1, right, false);
    assert!(e.shared.lock().unwrap().pressed()[1]);
    e.keyboard_event(1, VK_LCONTROL, false);
    assert!(!e.shared.lock().unwrap().pressed()[1]);
    e.shared.lock().unwrap().capture = Some((2, 0));
    e.keyboard_event(1, right, true);
    assert_eq!(
        e.shared.lock().unwrap().config.lanes[2].bindings[0],
        Binding::Keyboard { code: right }
    );
}

#[test]
fn hid_taps_survive_a_silent_source_and_delayed_copies() {
    use ReportSource::{Direct, Raw};
    for (first, second) in [(Direct, Raw), (Raw, Direct)] {
        let mut s = ReportStreams::default();
        let now = Instant::now();
        for value in [0, 1, 0] {
            assert!(s.accept(first, &[value], now));
        }
        for value in [0, 1, 0] {
            assert!(!s.accept(second, &[value], now));
        }
        assert!(s.accept(second, &[1], now));
        assert!(!s.accept(first, &[1], now));
        assert!(s.accept(second, &[0], now));
        // When the other transport resumes after an idle gap, its first report
        // may equal its own previous sample but still change the current state.
        assert!(s.accept(first, &[1], now + Duration::from_secs(1)));
    }
}
