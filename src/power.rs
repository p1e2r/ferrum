// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Battery state, for energy saver.
//!
//! - Linux: `/sys/class/power_supply` (mains `online`, battery `capacity`).
//! - macOS: `pmset -g batt` ("AC Power" / "Battery Power", "85%").
//! - Windows: `GetSystemPowerStatus`.
//!
//! Read on a background thread once a minute; desktops without a battery
//! report "plugged in".

use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Power {
    /// Running on battery (unplugged).
    pub on_battery: bool,
    /// Battery charge in percent, if there is a battery.
    pub percent: Option<u8>,
}

/// Start watching the battery. `changed` is called when the state changes.
pub fn watch(changed: Arc<dyn Fn() + Send + Sync>) -> Arc<Mutex<Power>> {
    let slot = Arc::new(Mutex::new(read()));
    let out = slot.clone();
    let _ = std::thread::Builder::new().name("power".into()).spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let now = read();
            let differs = out.lock().map(|mut p| std::mem::replace(&mut *p, now) != now).unwrap_or(false);
            if differs {
                changed();
            }
        }
    });
    slot
}

#[cfg(target_os = "linux")]
pub fn read() -> Power {
    let mut on_mains = None;
    let mut percent = None;
    let mut discharging = false;
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for e in entries.flatten() {
            let dir = e.path();
            let get = |f: &str| std::fs::read_to_string(dir.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
            match get("type").as_str() {
                "Mains" | "USB" => {
                    if get("online") == "1" {
                        on_mains = Some(true);
                    } else if on_mains.is_none() {
                        on_mains = Some(false);
                    }
                },
                "Battery" if get("scope") != "Device" => {
                    percent = get("capacity").parse().ok().or(percent);
                    discharging |= get("status") == "Discharging";
                },
                _ => {},
            }
        }
    }
    let on_battery = percent.is_some() && (on_mains == Some(false) || (on_mains.is_none() && discharging));
    Power { on_battery, percent }
}

#[cfg(target_os = "macos")]
pub fn read() -> Power {
    let out = std::process::Command::new("pmset").args(["-g", "batt"]).output().ok();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    parse_pmset(&text)
}

/// `Now drawing from 'Battery Power'\n -InternalBattery-0 (id=…)\t85%; discharging; …`
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_pmset(text: &str) -> Power {
    let on_battery = text.contains("'Battery Power'");
    let percent = text.split_whitespace().find_map(|w| w.trim_end_matches(';').strip_suffix('%').and_then(|n| n.parse().ok()));
    Power { on_battery, percent }
}

#[cfg(target_os = "windows")]
pub fn read() -> Power {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return Power::default();
    }
    // BatteryFlag 128: no battery. ACLineStatus 0: unplugged.
    let has_battery = status.BatteryFlag != 128 && status.BatteryFlag != 255;
    Power {
        on_battery: has_battery && status.ACLineStatus == 0,
        percent: (has_battery && status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn read() -> Power {
    Power::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pmset() {
        let batt = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1234)\t18%; discharging; 1:02 remaining present: true\n";
        assert_eq!(parse_pmset(batt), Power { on_battery: true, percent: Some(18) });
        let ac = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1234)\t100%; charged; 0:00 remaining present: true\n";
        assert_eq!(parse_pmset(ac), Power { on_battery: false, percent: Some(100) });
        assert_eq!(parse_pmset("Now drawing from 'AC Power'\n"), Power { on_battery: false, percent: None });
    }
}
