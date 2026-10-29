//! 手柄偏好与触觉配置, 与界面和设备后端无关.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod idle;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Rumble,
    Audio,
    TriggerRumble,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Curve {
    Linear,
    #[default]
    Smooth,
    Exponential,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modulation {
    #[default]
    Flat,
    Sine,
    Pulse,
    Triangle,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    SoftSquare,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TriggerMode {
    #[default]
    Off,
    Resistance,
    Vibration,
    Weapon,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    pub gain: f64,
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub hold: f64,
    pub release: f64,
    pub delay: f64,
    pub curve: Curve,
    pub modulation: Modulation,
    pub rate: f64,
    pub depth: f64,
    pub duty: f64,
    pub phase: f64,
    pub carrier: f64,
    pub end_frequency: f64,
    pub carrier_phase: f64,
    pub wave: Wave,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            gain: 0.2,
            attack: 12.0,
            decay: 35.0,
            sustain: 0.3,
            hold: 25.0,
            release: 90.0,
            delay: 0.0,
            curve: Curve::Smooth,
            modulation: Modulation::Flat,
            rate: 8.0,
            depth: 1.0,
            duty: 0.5,
            phase: 0.0,
            carrier: 100.0,
            end_frequency: 100.0,
            carrier_phase: 0.0,
            wave: Wave::Sine,
        }
    }
}
impl Channel {
    pub fn duration(&self) -> f64 {
        self.delay + self.attack + self.decay + self.hold + self.release
    }
    pub fn sample(&self, milliseconds: f64) -> f64 {
        let t = milliseconds - self.delay;
        if t < 0.0 || t >= self.duration() - self.delay {
            return 0.0;
        }
        let ramp = |x: f64| match self.curve {
            Curve::Linear => x,
            Curve::Smooth => x * x * (3.0 - 2.0 * x),
            Curve::Exponential => ((4.0 * x).exp() - 1.0) / (4.0_f64.exp() - 1.0),
        };
        let envelope = if t < self.attack {
            ramp(t / self.attack)
        } else if t < self.attack + self.decay {
            1.0 - (1.0 - self.sustain) * ramp((t - self.attack) / self.decay)
        } else if t < self.attack + self.decay + self.hold {
            self.sustain
        } else {
            self.sustain * (1.0 - ramp((t - self.attack - self.decay - self.hold) / self.release))
        };
        let phase = (t * self.rate / 1000.0 + self.phase / 360.0).fract();
        let modulation = match self.modulation {
            Modulation::Flat => 1.0,
            Modulation::Sine => (1.0 - (phase * std::f64::consts::TAU).cos()) / 2.0,
            Modulation::Pulse => f64::from(phase < self.duty),
            Modulation::Triangle => 1.0 - (phase * 2.0 - 1.0).abs(),
        };
        (self.gain * envelope * (1.0 - self.depth + self.depth * modulation)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Trigger {
    pub mode: TriggerMode,
    pub zones: [u8; 10],
    pub frequency: u8,
    pub start: u8,
    pub end: u8,
    pub strength: u8,
}
impl Default for Trigger {
    fn default() -> Self {
        Self {
            mode: TriggerMode::Off,
            zones: [0; 10],
            frequency: 30,
            start: 2,
            end: 5,
            strength: 2,
        }
    }
}
impl Trigger {
    pub fn bytes(&self) -> [u8; 11] {
        // 分区编码遵循 Nielk1 的 MIT 协议研究.
        let mut data = [0_u8; 11];
        data[0] = 0x05;
        if self.mode == TriggerMode::Weapon && self.strength > 0 {
            data[0] = 0x25;
            data[1..3]
                .copy_from_slice(&((1_u16 << self.start) | (1_u16 << self.end)).to_le_bytes());
            data[3] = self.strength - 1;
        } else if matches!(self.mode, TriggerMode::Resistance | TriggerMode::Vibration)
            && self.zones.iter().any(|&s| s > 0)
        {
            let mut active = 0_u16;
            let mut force = 0_u32;
            for (i, &strength) in self.zones.iter().enumerate().filter(|(_, s)| **s > 0) {
                active |= 1 << i;
                force |= u32::from(strength - 1) << (3 * i);
            }
            data[0] = if self.mode == TriggerMode::Resistance {
                0x21
            } else {
                0x26
            };
            data[1..3].copy_from_slice(&active.to_le_bytes());
            data[3..7].copy_from_slice(&force.to_le_bytes());
            if self.mode == TriggerMode::Vibration {
                data[9] = self.frequency;
            }
        }
        data
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub schema: String,
    pub name: String,
    pub mode: Mode,
    pub channels: [Channel; 2],
    pub triggers: [Trigger; 2],
    pub repeat: u8,
    pub gap: f64,
    pub trigger_ms: f64,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            schema: "oddity.haptics.v1".into(),
            name: "确认".into(),
            mode: Mode::Rumble,
            channels: [
                Channel::default(),
                Channel {
                    gain: 0.12,
                    delay: 12.0,
                    carrier: 180.0,
                    end_frequency: 180.0,
                    ..Channel::default()
                },
            ],
            triggers: [Trigger::default(), Trigger::default()],
            repeat: 1,
            gap: 120.0,
            trigger_ms: 0.0,
        }
    }
}
impl Profile {
    pub fn cycle(&self) -> f64 {
        self.channels
            .iter()
            .map(Channel::duration)
            .fold(self.trigger_ms, f64::max)
    }
    pub fn duration(&self) -> f64 {
        self.cycle() * f64::from(self.repeat) + self.gap * f64::from(self.repeat - 1)
    }
    pub fn sample(&self, ms: f64) -> [f64; 2] {
        if ms < 0.0 || ms >= self.duration() {
            return [0.0; 2];
        }
        let local = ms % (self.cycle() + self.gap);
        if local >= self.cycle() {
            return [0.0; 2];
        }
        self.channels
            .each_ref()
            .map(|channel| channel.sample(local))
    }
    pub fn validate(&self) -> Result<(), String> {
        let range =
            |value: f64, low: f64, high: f64| value.is_finite() && value >= low && value <= high;
        if self.schema != "oddity.haptics.v1"
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || !(1..=12).contains(&self.repeat)
            || !range(self.gap, 0.0, 3000.0)
            || !range(self.trigger_ms, 0.0, 10000.0)
        {
            return Err("手柄反馈名称或时序无效.".into());
        }
        for c in &self.channels {
            if ![c.gain, c.sustain, c.depth]
                .iter()
                .all(|&v| range(v, 0.0, 1.0))
                || ![c.attack, c.decay, c.release, c.delay]
                    .iter()
                    .all(|&v| range(v, 0.0, 2000.0))
                || !range(c.hold, 0.0, 5000.0)
                || !range(c.rate, 0.2, 30.0)
                || !range(c.duty, 0.05, 0.95)
                || !range(c.phase, 0.0, 360.0)
                || !range(c.carrier_phase, 0.0, 360.0)
                || !range(c.carrier, 20.0, 400.0)
                || !range(c.end_frequency, 20.0, 400.0)
            {
                return Err("手柄反馈通道参数超出范围.".into());
            }
        }
        for t in &self.triggers {
            if t.zones.iter().any(|&v| v > 8)
                || t.frequency == 0
                || !(2..=7).contains(&t.start)
                || !(3..=8).contains(&t.end)
                || t.end <= t.start
                || t.strength > 8
            {
                return Err("手柄扳机参数超出范围.".into());
            }
        }
        if !range(self.cycle(), 1.0, 30000.0) || self.duration() > 30000.0 {
            return Err("单次手柄反馈须为 1–30000ms.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub enabled: bool,
    pub icons: String,
    pub dead_zone: f32,
    pub vibration: bool,
    pub strength: u8,
    pub profiles: BTreeMap<String, Profile>,
}
impl Default for Options {
    fn default() -> Self {
        let confirm = Profile::default();
        let mut complete = Profile {
            name: "完成".into(),
            ..Profile::default()
        };
        complete.channels[0].hold = 90.0;
        complete.channels[0].release = 180.0;
        let error = Profile {
            name: "错误".into(),
            repeat: 2,
            gap: 90.0,
            ..Profile::default()
        };
        Self {
            enabled: true,
            icons: "auto".into(),
            dead_zone: 0.28,
            vibration: true,
            strength: 100,
            profiles: [
                ("confirm".into(), confirm),
                ("complete".into(), complete),
                ("error".into(), error),
            ]
            .into_iter()
            .collect(),
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !["auto", "dualsense", "ds4", "switch", "xbox"].contains(&self.icons.as_str())
            || !self.dead_zone.is_finite()
            || !(0.1..=0.75).contains(&self.dead_zone)
            || self.strength > 100
            || self.profiles.len() != 3
        {
            return Err("手柄设置无效.".into());
        }
        for event in ["confirm", "complete", "error"] {
            self.profiles
                .get(event)
                .ok_or("缺少手柄反馈配置.")?
                .validate()?;
        }
        Ok(())
    }
}
