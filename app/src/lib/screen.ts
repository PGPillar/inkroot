import type { DeviceStatus, FlowKind, FlowOptions, FlowState, Slot } from './ipc';

export type Route =
  | { screen: 'home' }
  | { screen: 'about' }
  | { screen: 'backups' }
  /** keepOpen: opened from Home with the tablet already connected, so there is nothing to auto-advance to. */
  | { screen: 'usbGuide'; keepOpen?: true }
  | { screen: 'alreadyRooted' }
  | { screen: 'rescuePick'; preselect?: { backupId?: string; slot?: Slot } }
  | { screen: 'risk'; then: { kind: FlowKind; options: FlowOptions } };

export type ScreenId =
  | 'flow' | 'risk' | 'about' | 'backups' | 'rescuePick' | 'usbGuide' | 'alreadyRooted' | 'home'
  | 'gate-none' | 'gate-unauthorized' | 'gate-edl'
  | 'gate-unknownModel' | 'gate-unknownFirmware' | 'gate-lowBattery';

/** Which screen to show. First match wins; a flow (even a finished one) outranks everything until dismissed. */
export function resolveScreen(i: { device: DeviceStatus; flow: FlowState | null; route: Route }): ScreenId {
  const { device, flow, route } = i;
  if (flow !== null) return 'flow';
  switch (route.screen) {
    case 'risk':
    case 'about':
    case 'backups':
    case 'rescuePick':
      return route.screen;
  }
  // The USB guide stays up until adb authorises; then it falls through (auto-advance).
  // Opened as help from Home (keepOpen), it also stays while adb is connected, until closed.
  if (route.screen === 'usbGuide') {
    const waiting = device.kind === 'none' || device.kind === 'unauthorized';
    if (waiting || (route.keepOpen && device.kind === 'adb')) return 'usbGuide';
  }
  if (device.kind !== 'adb') return `gate-${device.kind}`;
  if (device.gate !== null) return `gate-${device.gate}`;
  if (route.screen === 'alreadyRooted' && device.info.rooted) return 'alreadyRooted';
  return 'home';
}
