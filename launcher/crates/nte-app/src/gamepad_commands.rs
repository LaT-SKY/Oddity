//! SDL 设备、事件泵与触觉流由 Tauri 主线程持有.
use nte_core::gamepad::idle::{ActivityInhibitor, Status as IdleStatus};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use nte_core::gamepad::{Mode, Options, Profile, Trigger, TriggerMode, Wave};
use sdl3::audio::{AudioDevice, AudioDeviceID, AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, Gamepad, GamepadType};
use sdl3::joystick::JoystickId;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    id: u32,
    name: String,
    family: &'static str,
    rumble: bool,
    trigger_rumble: bool,
    adaptive: bool,
    audio: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    devices: Vec<Device>,
    device: Option<Device>,
    buttons: Vec<u8>,
    pressed: Vec<u8>,
    axes: [f32; 6],
    focused: bool,
    error: Option<String>,
    idle: IdleStatus,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Feedback {
    Confirm,
    Complete,
    Error,
    Stop,
}

struct Playback {
    device: JoystickId,
    profile: Profile,
    started: Instant,
    gain: f64,
    stream: Option<AudioStreamOwner>,
    rendered: usize,
}
struct Input {
    _sdl: sdl3::Sdl,
    audio: Option<sdl3::AudioSubsystem>,
    events: sdl3::EventPump,
    subsystem: sdl3::GamepadSubsystem,
    pads: BTreeMap<JoystickId, Gamepad>,
    active: Option<JoystickId>,
    playback: Option<Playback>,
    focused: bool,
    error: Option<String>,
    idle: ActivityInhibitor,
    idle_reported: Option<Instant>,
}
thread_local! {
    static INPUT: RefCell<Option<Result<Input, String>>> = const { RefCell::new(None) };
}

impl Input {
    fn new() -> Result<Self, String> {
        sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
        sdl3::hint::set("SDL_JOYSTICK_HIDAPI_PS5", "1");
        let sdl = sdl3::init().map_err(|e| e.to_string())?;
        let audio = sdl.audio().ok();
        let subsystem = sdl.gamepad().map_err(|e| e.to_string())?;
        let events = sdl.event_pump().map_err(|e| e.to_string())?;
        Ok(Self {
            _sdl: sdl,
            audio,
            events,
            subsystem,
            pads: BTreeMap::new(),
            active: None,
            playback: None,
            focused: false,
            error: None,
            idle: ActivityInhibitor::new(Duration::from_secs(60)),
            idle_reported: None,
        })
    }
    fn audio_device(&self) -> Option<AudioDeviceID> {
        let audio = self.audio.as_ref()?;
        // 只在唯一的 Sony 设备可以匹配四声道音频时开放音频触觉.
        if self
            .pads
            .values()
            .filter(|p| p.vendor_id() == Some(0x054c))
            .count()
            != 1
        {
            return None;
        }
        audio
            .audio_playback_device_ids()
            .ok()?
            .into_iter()
            .find(|&id| {
                let device = AudioDevice::new(id, audio.clone());
                device
                    .name()
                    .is_ok_and(|name| name.to_lowercase().contains("dualsense"))
                    && device
                        .format()
                        .is_ok_and(|(spec, _)| spec.channels == Some(4))
            })
    }
    fn device(&self, id: JoystickId, audio: bool) -> Device {
        let pad = &self.pads[&id];
        let family = match pad.r#type() {
            GamepadType::PS4 => "ds4",
            GamepadType::PS5 => "dualsense",
            GamepadType::Xbox360 | GamepadType::XboxOne => "xbox",
            GamepadType::NintendoSwitchPro
            | GamepadType::NintendoSwitchJoyconLeft
            | GamepadType::NintendoSwitchJoyconRight
            | GamepadType::NintendoSwitchJoyconPair => "switch",
            _ => "generic",
        };
        Device {
            id: id.raw(),
            name: pad.name().unwrap_or_else(|| "手柄".into()),
            family,
            rumble: unsafe { pad.has_rumble() },
            trigger_rumble: unsafe { pad.has_rumble_triggers() },
            adaptive: family == "dualsense",
            audio: family == "dualsense" && audio,
        }
    }
    fn effects(pad: &mut Gamepad, triggers: &[Trigger; 2]) -> Result<(), String> {
        let mut packet = [0_u8; 47];
        packet[0] = 0x0c;
        packet[10..21].copy_from_slice(&triggers[1].bytes());
        packet[21..32].copy_from_slice(&triggers[0].bytes());
        pad.send_effect(&packet).map_err(|e| e.to_string())
    }
    fn stop(&mut self) {
        self.playback = None;
        for pad in self.pads.values_mut() {
            let _ = pad.set_rumble(0, 0, 0);
            if unsafe { pad.has_rumble_triggers() } {
                let _ = pad.set_rumble_triggers(0, 0, 0);
            }
            if pad.r#type() == GamepadType::PS5 {
                let _ = Self::effects(pad, &[Trigger::default(), Trigger::default()]);
            }
        }
    }
    fn tick_feedback(&mut self) -> Result<(), String> {
        let Some(playback) = self.playback.as_mut() else {
            return Ok(());
        };
        let ms = playback.started.elapsed().as_secs_f64() * 1000.0;
        let profile = &playback.profile;
        if let Some(stream) = &playback.stream {
            let total = (profile.duration() * 48.0).ceil() as usize;
            while playback.rendered < total
                && stream.queued_bytes().map_err(|e| e.to_string())? < 480 * 8 * 4
            {
                let count = (total - playback.rendered).min(480);
                let mut pcm = Vec::with_capacity(count * 4);
                for offset in 0..count {
                    let time = (playback.rendered + offset) as f64 / 48.0;
                    let gains = profile.sample(time);
                    let local = time % (profile.cycle() + profile.gap);
                    pcm.extend([0_i16; 2]);
                    for (channel, gain) in profile.channels.iter().zip(gains) {
                        let t = ((local - channel.delay) / 1000.0).max(0.0);
                        let length = ((channel.duration() - channel.delay) / 1000.0).max(0.001);
                        let phase = std::f64::consts::TAU
                            * (channel.carrier * t
                                + (channel.end_frequency - channel.carrier) * t * t
                                    / (2.0 * length))
                            + channel.carrier_phase.to_radians();
                        let sine = phase.sin();
                        let wave = match channel.wave {
                            Wave::Sine => sine,
                            Wave::Triangle => 2.0 / std::f64::consts::PI * sine.asin(),
                            Wave::SoftSquare => (3.0 * sine).tanh() / 3.0_f64.tanh(),
                        };
                        pcm.push((wave * gain * playback.gain * 32767.0).round() as i16);
                    }
                }
                stream.put_data_i16(&pcm).map_err(|e| e.to_string())?;
                playback.rendered += count;
            }
            if playback.rendered >= total
                && stream.queued_bytes().map_err(|e| e.to_string())? == 0
                && ms >= profile.duration() + 60.0
            {
                self.stop();
            }
        } else if ms >= profile.duration() {
            self.stop();
        } else if let Some(pad) = self.pads.get_mut(&playback.device) {
            let [low, high] = profile
                .sample(ms)
                .map(|value| (value * playback.gain * 65535.0).round() as u16);
            if profile.mode == Mode::TriggerRumble {
                pad.set_rumble_triggers(low, high, 80)
            } else {
                pad.set_rumble(low, high, 80)
            }
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn poll(&mut self, focused: bool, options: &Options) -> Result<Snapshot, String> {
        let previous_active = self.active;
        let mut pressed = Vec::new();
        for event in self.events.poll_iter() {
            match event {
                Event::GamepadButtonDown { which, button, .. } if focused && self.focused => {
                    self.active = Some(which);
                    pressed.push((which, button as u8));
                }
                Event::GamepadAxisMotion { which, value, .. }
                    if focused
                        && self.focused
                        && i32::from(value).abs() > (options.dead_zone * 32767.0) as i32 =>
                {
                    self.active = Some(which);
                }
                _ => {}
            }
        }
        let disconnected = self.pads.iter().any(|(_, pad)| !pad.connected());
        if disconnected {
            self.idle.clear();
            self.idle_reported = None;
            self.stop();
        }
        self.pads.retain(|_, pad| pad.connected());
        let mut open_error = None;
        for id in self.subsystem.gamepads().map_err(|e| e.to_string())? {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.pads.entry(id) {
                match self.subsystem.open(id) {
                    Ok(pad) => {
                        entry.insert(pad);
                    }
                    Err(error) => open_error = Some(error.to_string()),
                }
            }
        }
        if self.active.is_none_or(|id| !self.pads.contains_key(&id)) {
            self.active = self.pads.keys().next().copied();
            pressed.clear();
        }
        if previous_active != self.active {
            self.stop();
        }
        let pressed: Vec<u8> = pressed
            .into_iter()
            .filter(|(id, _)| Some(*id) == self.active)
            .map(|(_, button)| button)
            .collect();
        if !focused && self.focused {
            self.idle.clear();
            self.idle_reported = None;
            self.stop();
        }
        self.focused = focused;
        if let Err(error) = self.tick_feedback() {
            self.stop();
            self.error = Some(error);
        }
        let audio = self.audio_device().is_some();
        let devices = self.pads.keys().map(|&id| self.device(id, audio)).collect();
        let device = self.active.map(|id| self.device(id, audio));
        let (buttons, axes) = self
            .active
            .and_then(|id| self.pads.get(&id))
            .map(|pad| {
                let buttons = (0..26)
                    .filter_map(|index| {
                        Button::from_ll(sdl3::sys::gamepad::SDL_GamepadButton(index))
                            .filter(|&button| pad.button(button))
                            .map(|_| index as u8)
                    })
                    .collect::<Vec<_>>();
                let axes = [
                    Axis::LeftX,
                    Axis::LeftY,
                    Axis::RightX,
                    Axis::RightY,
                    Axis::TriggerLeft,
                    Axis::TriggerRight,
                ]
                .map(|axis| f32::from(pad.axis(axis)) / 32767.0);
                (buttons, axes)
            })
            .unwrap_or_default();
        if focused
            && (!buttons.is_empty()
                || !pressed.is_empty()
                || axes.iter().any(|value| value.abs() > options.dead_zone))
            && self
                .idle_reported
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(10))
        {
            self.idle.activity();
            self.idle_reported = Some(Instant::now());
        }
        Ok(Snapshot {
            devices,
            device,
            buttons: if focused { buttons } else { Vec::new() },
            pressed,
            axes: if focused { axes } else { [0.0; 6] },
            focused,
            error: open_error.or_else(|| self.error.clone()),
            idle: self.idle.status(),
        })
    }
    fn feedback(&mut self, device: u32, profile: Profile, options: &Options) -> Result<(), String> {
        self.stop();
        if !options.vibration || options.strength == 0 {
            return Ok(());
        }
        let id = JoystickId::new(device);
        if self.active != Some(id) {
            return Err("操作手柄已切换, 请重试.".into());
        }
        let audio_id = self.audio_device();
        let capabilities = self.device(id, audio_id.is_some());
        if profile.mode == Mode::Rumble && !capabilities.rumble {
            return Err("当前手柄未提供普通振动.".into());
        }
        if profile.mode == Mode::TriggerRumble && !capabilities.trigger_rumble {
            return Err("当前连接未提供扳机振动.".into());
        }
        if profile
            .triggers
            .iter()
            .any(|trigger| trigger.mode != TriggerMode::Off)
            && !capabilities.adaptive
        {
            return Err("自适应扳机仅适用于 DualSense.".into());
        }
        let stream = if profile.mode == Mode::Audio {
            let id = audio_id
                .filter(|_| capabilities.audio)
                .ok_or("未找到可匹配的四声道 DualSense 音频设备.")?;
            let audio = self.audio.as_ref().ok_or("系统音频不可用.")?;
            let device = AudioDevice::new(id, audio.clone());
            let stream = device
                .open_device_stream(Some(&AudioSpec {
                    freq: Some(48000),
                    channels: Some(4),
                    format: Some(AudioFormat::s16_sys()),
                }))
                .map_err(|e| e.to_string())?;
            stream.resume().map_err(|e| e.to_string())?;
            Some(stream)
        } else {
            None
        };
        let pad = self.pads.get_mut(&id).ok_or("手柄已断开连接.")?;
        if capabilities.adaptive {
            Self::effects(pad, &profile.triggers)?;
        }
        self.error = None;
        self.playback = Some(Playback {
            device: id,
            profile,
            started: Instant::now(),
            gain: f64::from(options.strength) / 100.0,
            stream,
            rendered: 0,
        });
        self.tick_feedback()
    }
}

pub fn stop() {
    INPUT.with_borrow_mut(|slot| {
        if let Some(Ok(input)) = slot {
            input.stop();
            input.idle.clear();
            input.idle_reported = None;
        }
    });
}

async fn on_main<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&mut Input, &AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = INPUT.with_borrow_mut(|slot| {
            slot.get_or_insert_with(Input::new)
                .as_mut()
                .map_err(|error| error.clone())
                .and_then(|input| action(input, &handle))
        });
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn gamepad_poll(app: AppHandle) -> Result<Snapshot, String> {
    on_main(app, |input, app| {
        let options = app
            .state::<crate::AppState>()
            .settings
            .lock()
            .map_err(|e| e.to_string())?
            .gamepad
            .clone();
        let focused = options.enabled
            && app.get_webview_window("main").is_some_and(|window| {
                window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
            });
        input.poll(focused, &options)
    })
    .await
}
#[tauri::command]
pub async fn gamepad_feedback(
    app: AppHandle,
    device_id: u32,
    effect: Feedback,
) -> Result<(), String> {
    on_main(app, move |input, app| {
        if matches!(effect, Feedback::Stop) {
            input.stop();
            return Ok(());
        }
        let options = app
            .state::<crate::AppState>()
            .settings
            .lock()
            .map_err(|e| e.to_string())?
            .gamepad
            .clone();
        if !options.enabled
            || !app
                .get_webview_window("main")
                .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
        {
            return Err("请先切回启动器窗口.".into());
        }
        let event = match effect {
            Feedback::Confirm => "confirm",
            Feedback::Complete => "complete",
            Feedback::Error => "error",
            Feedback::Stop => unreachable!(),
        };
        let profile = options.profiles[event].clone();
        input.feedback(device_id, profile, &options)
    })
    .await
}
#[tauri::command]
pub fn gamepad_profile_validate(profile: Profile) -> Result<Profile, String> {
    profile.validate()?;
    Ok(profile)
}

#[tauri::command]
pub fn gamepad_profile_load(path: String) -> Result<Profile, String> {
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("反馈配置不得超过 64KB.".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let profile: Profile =
        serde_json::from_str(&text).map_err(|e| format!("反馈配置格式无效: {e}"))?;
    profile.validate()?;
    Ok(profile)
}

#[tauri::command]
pub fn gamepad_profile_save(path: String, profile: Profile) -> Result<(), String> {
    profile.validate()?;
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gamepad_release_idle(app: AppHandle) -> Result<(), String> {
    on_main(app, |input, _| {
        input.idle.clear();
        input.idle_reported = None;
        Ok(())
    })
    .await
}

#[cfg(test)]
#[path = "../../../../internal/gamepad/tests/native.rs"]
mod tests;
