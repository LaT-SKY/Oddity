export interface Channel {
  gain: number;
  attack: number;
  decay: number;
  sustain: number;
  hold: number;
  release: number;
  delay: number;
  curve: "linear" | "smooth" | "exponential";
  modulation: "flat" | "sine" | "pulse" | "triangle";
  rate: number;
  depth: number;
  duty: number;
  phase: number;
  carrier: number;
  end_frequency: number;
  carrier_phase: number;
  wave: "sine" | "triangle" | "soft-square";
}
export interface Trigger {
  mode: "off" | "resistance" | "vibration" | "weapon";
  zones: number[];
  frequency: number;
  start: number;
  end: number;
  strength: number;
}
export interface Profile {
  schema: "oddity.haptics.v1";
  name: string;
  mode: "rumble" | "audio" | "trigger-rumble";
  channels: Channel[];
  triggers: Trigger[];
  repeat: number;
  gap: number;
  trigger_ms: number;
}
export const channel = (gain = 0.3): Channel => ({
  gain,
  attack: 12,
  decay: 35,
  sustain: 0.3,
  hold: 25,
  release: 90,
  delay: 0,
  curve: "smooth",
  modulation: "flat",
  rate: 8,
  depth: 1,
  duty: 0.5,
  phase: 0,
  carrier: 100,
  end_frequency: 100,
  carrier_phase: 0,
  wave: "sine",
});
export const trigger = (): Trigger => ({
  mode: "off",
  zones: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  frequency: 30,
  start: 2,
  end: 5,
  strength: 2,
});
export const defaultProfile = (): Profile => ({
  schema: "oddity.haptics.v1",
  name: "确认",
  mode: "rumble",
  channels: [
    channel(0.2),
    { ...channel(0.12), delay: 12, carrier: 180, end_frequency: 180 },
  ],
  triggers: [trigger(), trigger()],
  repeat: 1,
  gap: 120,
  trigger_ms: 0,
});

export type Feedback = "confirm" | "complete" | "error";
export interface GamepadOptions {
  enabled: boolean;
  icons: "auto" | "dualsense" | "ds4" | "switch" | "xbox";
  deadZone: number;
  vibration: boolean;
  strength: number;
  profiles: Record<Feedback, Profile>;
}
export interface GamepadDevice {
  id: number;
  name: string;
  family: "dualsense" | "ds4" | "switch" | "xbox" | "generic";
  rumble: boolean;
  triggerRumble: boolean;
  adaptive: boolean;
  audio: boolean;
}
export interface GamepadSnapshot {
  idle: { requested: boolean; error: string | null };
  devices: GamepadDevice[];
  device: GamepadDevice | null;
  buttons: number[];
  pressed: number[];
  axes: [number, number, number, number, number, number];
  focused: boolean;
  error: string | null;
}
export function defaultGamepadOptions(): GamepadOptions {
  const confirm = defaultProfile();
  const complete = defaultProfile();
  complete.name = "完成";
  complete.channels[0]!.hold = 90;
  complete.channels[0]!.release = 180;
  const error = defaultProfile();
  error.name = "错误";
  error.repeat = 2;
  error.gap = 90;
  return {
    enabled: true,
    icons: "auto",
    deadZone: 0.28,
    vibration: true,
    strength: 100,
    profiles: { confirm, complete, error },
  };
}

export type GamepadButton =
  | "confirm"
  | "back"
  | "west"
  | "north"
  | "lb"
  | "rb"
  | "lt"
  | "rt"
  | "move"
  | "r3";
export interface GamepadHint {
  id: string;
  buttons: GamepadButton[];
  label: string;
}
