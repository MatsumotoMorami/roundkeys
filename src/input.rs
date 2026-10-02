use crate::Shared;
pub fn start(shared: Shared) {
    #[cfg(windows)]
    std::thread::spawn(move || windows::run(shared));
    #[cfg(not(windows))]
    {
        shared.lock().unwrap().input_status = "当前版本只支持 Windows 输入".into();
    }
}
pub fn axis_delta(previous: f64, current: f64) -> f64 {
    let d = current - previous;
    if d > 0.5 {
        d - 1.0
    } else if d < -0.5 {
        d + 1.0
    } else {
        d
    }
}
#[cfg(windows)]
mod windows;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turntable_wrap_is_not_a_large_reverse() {
        assert!((axis_delta(0.99, 0.01) - 0.02).abs() < 1e-9);
        assert!((axis_delta(0.01, 0.99) + 0.02).abs() < 1e-9);
    }
}
