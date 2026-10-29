#[cfg(test)]
mod tests {
    use nte_core::gamepad::{
        Channel, Curve, Mode, Modulation, Options, Profile, Trigger, TriggerMode,
    };
    #[test]
    fn defaults_round_trip_and_legacy_settings_default_gamepad() {
        let settings: nte_core::settings::Settings = serde_json::from_str("{}").unwrap();
        settings.gamepad.validate().unwrap();
        let json = serde_json::to_string(&settings.gamepad).unwrap();
        serde_json::from_str::<Options>(&json)
            .unwrap()
            .validate()
            .unwrap();
        for name in ["confirm", "complete", "error"] {
            assert!(settings.gamepad.profiles[name].duration() > 0.0);
        }
    }
    #[test]
    fn exported_lab_profile_is_accepted_without_parameter_loss() {
        let text = include_str!("../../tests/profile.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let profile: Profile = serde_json::from_value(value.clone()).unwrap();
        profile.validate().unwrap();
        let exported = serde_json::to_value(profile).unwrap();
        fn normalized(value: serde_json::Value) -> serde_json::Value {
            match value {
                serde_json::Value::Number(value) => serde_json::json!(value.as_f64().unwrap()),
                serde_json::Value::Array(values) => values.into_iter().map(normalized).collect(),
                serde_json::Value::Object(values) => values
                    .into_iter()
                    .map(|(key, value)| (key, normalized(value)))
                    .collect(),
                value => value,
            }
        }
        assert_eq!(normalized(value), normalized(exported));
    }
    #[test]
    fn envelope_delay_attack_decay_hold_release_and_repeat_gap() {
        let channel = Channel {
            gain: 1.0,
            attack: 100.0,
            decay: 100.0,
            sustain: 0.5,
            hold: 100.0,
            release: 100.0,
            delay: 50.0,
            curve: Curve::Linear,
            ..Channel::default()
        };
        for (time, expected) in [
            (0.0, 0.0),
            (50.0, 0.0),
            (100.0, 0.5),
            (150.0, 1.0),
            (200.0, 0.75),
            (300.0, 0.5),
            (400.0, 0.25),
            (450.0, 0.0),
        ] {
            assert!((channel.sample(time) - expected).abs() < 1e-9);
        }
        let profile = Profile {
            channels: [channel.clone(), channel],
            repeat: 2,
            gap: 100.0,
            ..Profile::default()
        };
        assert_eq!(profile.duration(), 1000.0);
        assert_eq!(profile.sample(500.0), [0.0; 2]);
        assert_eq!(profile.sample(650.0), [0.5; 2]);
        assert_eq!(profile.sample(1000.0), [0.0; 2]);
    }
    #[test]
    fn modulation_and_zero_duration_stages_stay_finite() {
        for modulation in [
            Modulation::Flat,
            Modulation::Sine,
            Modulation::Pulse,
            Modulation::Triangle,
        ] {
            let channel = Channel {
                attack: 0.0,
                decay: 0.0,
                release: 0.0,
                hold: 100.0,
                modulation,
                ..Channel::default()
            };
            for time in 0..120 {
                let value = channel.sample(time as f64);
                assert!(value.is_finite() && (0.0..=1.0).contains(&value));
            }
        }
    }
    #[test]
    fn trigger_packet_encodes_ten_zones_and_clears_effects() {
        let trigger = Trigger {
            mode: TriggerMode::Resistance,
            zones: [1, 2, 3, 4, 5, 6, 7, 8, 1, 2],
            ..Trigger::default()
        };
        let packet = trigger.bytes();
        assert_eq!(packet[0], 0x21);
        assert_eq!(&packet[1..3], &1023_u16.to_le_bytes());
        let force = u32::from_le_bytes(packet[3..7].try_into().unwrap());
        for (index, strength) in trigger.zones.iter().enumerate() {
            assert_eq!((force >> (index * 3)) & 7, u32::from(strength - 1));
        }
        assert_eq!(
            Trigger::default().bytes(),
            [5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let trigger = Trigger {
            mode: TriggerMode::Vibration,
            zones: [8; 10],
            frequency: 123,
            ..Trigger::default()
        };
        assert_eq!(trigger.bytes()[9], 123);
    }
    #[test]
    fn invalid_imports_are_rejected_at_boundary() {
        let mut profile = Profile {
            repeat: 0,
            ..Profile::default()
        };
        assert!(profile.validate().is_err());
        profile = Profile::default();
        profile.channels[0].gain = f64::NAN;
        assert!(profile.validate().is_err());
        profile = Profile::default();
        profile.triggers[0].zones[9] = 9;
        assert!(profile.validate().is_err());
        profile = Profile::default();
        profile.mode = Mode::Audio;
        profile.repeat = 12;
        profile.channels[0].hold = 5000.0;
        assert!(profile.validate().is_err());
        let options = Options {
            strength: 101,
            ..Options::default()
        };
        assert!(options.validate().is_err());
    }
}
