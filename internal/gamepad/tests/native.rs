use super::*;
use sdl3::sys::joystick;
use std::ffi::{c_void, CString};

unsafe extern "C" fn rumble(_: *mut c_void, _: u16, _: u16) -> bool {
    true
}
unsafe extern "C" fn effect(_: *mut c_void, _: *const c_void, size: i32) -> bool {
    size == 47
}

#[test]
fn virtual_controllers_poll_feedback_stream_and_profile_files() {
    let mut env = crate::tests::EnvGuard::new();
    env.set("SDL_AUDIO_DRIVER", "dummy");
    env.set("SDL_JOYSTICK_HIDAPI", "0");
    let mut input = Input::new().unwrap();
    let options = Options::default();
    input.poll(false, &options).unwrap();
    assert_eq!(input.matched_audio, Some(None));
    let cached_audio = input
        .audio
        .as_ref()
        .unwrap()
        .audio_playback_device_ids()
        .unwrap()[0];
    input.matched_audio = Some(Some(cached_audio));
    input.poll(false, &options).unwrap();
    assert_eq!(input.audio_device(), Some(cached_audio));
    for (event_type, capture) in [
        (sdl3::sys::events::SDL_EVENT_AUDIO_DEVICE_ADDED, true),
        (sdl3::sys::events::SDL_EVENT_AUDIO_DEVICE_ADDED, false),
        (sdl3::sys::events::SDL_EVENT_AUDIO_DEVICE_REMOVED, false),
    ] {
        input.matched_audio = Some(Some(cached_audio));
        let mut event = sdl3::sys::events::SDL_Event {
            adevice: sdl3::sys::events::SDL_AudioDeviceEvent {
                r#type: event_type,
                which: cached_audio.id(),
                recording: capture,
                ..Default::default()
            },
        };
        assert!(unsafe { sdl3::sys::events::SDL_PushEvent(&mut event) });
        input.poll(false, &options).unwrap();
        assert_eq!(input.audio_device(), capture.then_some(cached_audio));
    }
    input.matched_audio = Some(Some(cached_audio));
    let mut attached = Vec::new();
    for (vendor, product, name) in [
        (0x054c, 0x0ce6, "PS5"),
        (0x054c, 0x05c4, "PS4"),
        (0x045e, 0x028e, "Xbox360"),
        (0x045e, 0x0b13, "XboxOne"),
        (0x057e, 0x2009, "Switch"),
        (1, 1, "Generic"),
    ] {
        let label = CString::new(name).unwrap();
        let desc = joystick::SDL_VirtualJoystickDesc {
            r#type: joystick::SDL_JOYSTICK_TYPE_GAMEPAD.0 as u16,
            vendor_id: vendor,
            product_id: product,
            naxes: 6,
            nbuttons: 26,
            button_mask: (1 << 26) - 1,
            axis_mask: 63,
            name: label.as_ptr(),
            Rumble: Some(rumble),
            RumbleTriggers: Some(rumble),
            SendEffect: Some(effect),
            ..Default::default()
        };
        let raw = unsafe { joystick::SDL_AttachVirtualJoystick(&desc) };
        assert_ne!(raw.0, 0);
        let stick = unsafe { joystick::SDL_OpenJoystick(raw) };
        assert!(!stick.is_null());
        attached.push((raw, stick));
        input.error = Some("触觉播放失败".into());
        let snapshot = input.poll(true, &options).unwrap();
        assert_eq!(input.matched_audio, Some(None));
        assert_eq!(snapshot.error.as_deref(), Some("触觉播放失败"));
        assert!(!snapshot.devices.is_empty());
        let id = JoystickId::new(raw.0);
        input.active = Some(id);
        let device = input.device(id, false);
        assert_eq!(
            device.family,
            match name {
                "PS5" => "dualsense",
                "PS4" => "ds4",
                "Switch" => "switch",
                "Generic" => "generic",
                _ => "xbox",
            }
        );
        assert!(device.rumble);
        assert!(device.trigger_rumble);
        for mode in [Mode::Rumble, Mode::TriggerRumble] {
            let profile = Profile {
                mode,
                ..Profile::default()
            };
            input.feedback(raw.0, profile, &options).unwrap();
            assert!(input.playback.is_some());
            input.tick_feedback().unwrap();
            input.playback.as_mut().unwrap().started = Instant::now() - Duration::from_secs(1);
            input.tick_feedback().unwrap();
            assert!(input.playback.is_none());
            assert!(input.poll(true, &options).unwrap().error.is_none());
        }
        assert!(input
            .feedback(raw.0 + 10000, Profile::default(), &options)
            .is_err());
        let mut disabled = options.clone();
        disabled.strength = 0;
        input
            .feedback(raw.0, Profile::default(), &disabled)
            .unwrap();
        let audio = Profile {
            mode: Mode::Audio,
            ..Profile::default()
        };
        assert!(input.feedback(raw.0, audio, &options).is_err());
        let mut trigger = Profile::default();
        trigger.triggers[0].mode = TriggerMode::Resistance;
        trigger.triggers[0].zones = [3; 10];
        if device.adaptive {
            input.feedback(raw.0, trigger, &options).unwrap();
        } else {
            assert!(input.feedback(raw.0, trigger, &options).is_err());
        }
        unsafe {
            joystick::SDL_SetJoystickVirtualButton(stick, 0, true);
            joystick::SDL_SetJoystickVirtualAxis(stick, 0, 24000);
        }
        let snapshot = input.poll(true, &options).unwrap();
        assert_eq!(snapshot.device.unwrap().id, raw.0);
        assert!(snapshot.buttons.contains(&0));
        assert!(snapshot.axes[0] > 0.5);
        input.poll(false, &options).unwrap();
        input.stop();
    }
    input.audio_device();
    if let Some(audio) = input.audio.clone() {
        let id = audio.audio_playback_device_ids().unwrap()[0];
        for wave in [Wave::Sine, Wave::Triangle, Wave::SoftSquare] {
            let stream = AudioDevice::new(id, audio.clone())
                .open_device_stream(Some(&AudioSpec {
                    freq: Some(48000),
                    channels: Some(4),
                    format: Some(AudioFormat::s16_sys()),
                }))
                .unwrap();
            let mut profile = Profile {
                mode: Mode::Audio,
                ..Profile::default()
            };
            profile.channels[0].wave = wave;
            profile.channels[1].wave = wave;
            input.playback = Some(Playback {
                device: input.active.unwrap(),
                profile,
                started: Instant::now(),
                gain: 0.4,
                stream: Some(stream),
                rendered: 0,
            });
            input.tick_feedback().unwrap();
            assert!(input.playback.as_ref().unwrap().rendered > 0);
            input.stop();
        }
    }
    input.matched_audio = Some(Some(cached_audio));
    for (raw, stick) in attached {
        unsafe {
            joystick::SDL_CloseJoystick(stick);
            joystick::SDL_DetachVirtualJoystick(raw);
        }
    }
    input.poll(true, &options).unwrap();
    assert_eq!(input.matched_audio, Some(None));
    input.poll(false, &options).unwrap();
    let dir = crate::tests::TempDir::new("gamepad-profiles");
    let path = dir.join("profile.json").display().to_string();
    gamepad_profile_save(path.clone(), Profile::default()).unwrap();
    assert_eq!(gamepad_profile_load(path.clone()).unwrap().name, "确认");
    gamepad_profile_validate(Profile::default()).unwrap();
    std::fs::write(&path, "invalid").unwrap();
    assert!(gamepad_profile_load(path.clone()).is_err());
    std::fs::write(&path, vec![0; 65537]).unwrap();
    assert!(gamepad_profile_load(path.clone()).is_err());
    assert!(gamepad_profile_load(format!("{path}.missing")).is_err());
    let invalid = Profile {
        repeat: 0,
        ..Profile::default()
    };
    assert!(gamepad_profile_validate(invalid.clone()).is_err());
    assert!(gamepad_profile_save(path, invalid).is_err());
    INPUT.with_borrow_mut(|slot| *slot = Some(Ok(input)));
    stop();
    INPUT.with_borrow_mut(|slot| *slot = None);
}
