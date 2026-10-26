import { expect, test } from 'vitest';
import type { DeviceInfo, DeviceStatus, FlowState, Gate } from './ipc';
import { NOTE_MAX } from './mock/fixtures';
import { resolveScreen, type Route, type ScreenId } from './screen';

const adb = (o: Partial<DeviceInfo> = {}, gate: Gate | null = null): DeviceStatus => ({ kind: 'adb', info: { ...NOTE_MAX, ...o }, gate });
const runningFlow: FlowState = {
  flowId: 'f1', kind: 'root', step: 'write', stepIndex: 3, stepCount: 4, status: 'running',
  prompt: null, error: null, result: null, writing: true,
};
const home: Route = { screen: 'home' };

test.each<[DeviceStatus, ScreenId]>([
  [{ kind: 'none' }, 'gate-none'],
  [{ kind: 'unauthorized' }, 'gate-unauthorized'],
  [{ kind: 'edl' }, 'gate-edl'],
  [adb({}, 'unknownModel'), 'gate-unknownModel'],
  [adb({}, 'unknownFirmware'), 'gate-unknownFirmware'],
  [adb({}, 'lowBattery'), 'gate-lowBattery'],
  [adb(), 'home'],
])('device %o → %s', (device, id) => expect(resolveScreen({ device, flow: null, route: home })).toBe(id));

test('flow outranks device gates and routes', () => {
  expect(resolveScreen({ device: { kind: 'none' }, flow: runningFlow, route: { screen: 'about' } })).toBe('flow');
});

test('a terminal flow stays on screen until dismissed', () => {
  const done: FlowState = { ...runningFlow, status: 'done', writing: false, step: 'done', result: {} };
  expect(resolveScreen({ device: adb(), flow: done, route: home })).toBe('flow');
});

test('risk, about, backups and rescuePick outrank device gates', () => {
  const then = { kind: 'root' as const, options: {} };
  expect(resolveScreen({ device: { kind: 'edl' }, flow: null, route: { screen: 'risk', then } })).toBe('risk');
  expect(resolveScreen({ device: { kind: 'none' }, flow: null, route: { screen: 'about' } })).toBe('about');
  expect(resolveScreen({ device: adb({}, 'lowBattery'), flow: null, route: { screen: 'backups' } })).toBe('backups');
});

test('usbGuide auto-advances when adb appears', () => {
  expect(resolveScreen({ device: { kind: 'none' }, flow: null, route: { screen: 'usbGuide' } })).toBe('usbGuide');
  expect(resolveScreen({ device: { kind: 'unauthorized' }, flow: null, route: { screen: 'usbGuide' } })).toBe('usbGuide');
  expect(resolveScreen({ device: adb(), flow: null, route: { screen: 'usbGuide' } })).toBe('home');
  expect(resolveScreen({ device: { kind: 'edl' }, flow: null, route: { screen: 'usbGuide' } })).toBe('gate-edl');
});

test('usbGuide opened from home with keepOpen stays while adb is connected', () => {
  const route = { screen: 'usbGuide', keepOpen: true } as const;
  expect(resolveScreen({ device: adb(), flow: null, route })).toBe('usbGuide');
  expect(resolveScreen({ device: { kind: 'none' }, flow: null, route })).toBe('usbGuide');
  expect(resolveScreen({ device: { kind: 'edl' }, flow: null, route })).toBe('gate-edl');
});

test('alreadyRooted only when rooted', () => {
  const route: Route = { screen: 'alreadyRooted' };
  expect(resolveScreen({ device: adb({ rooted: false }), flow: null, route })).toBe('home');
  expect(resolveScreen({ device: adb({ rooted: true }), flow: null, route })).toBe('alreadyRooted');
  expect(resolveScreen({ device: { kind: 'none' }, flow: null, route })).toBe('gate-none');
});

test('rescuePick reachable with no device', () => {
  expect(resolveScreen({ device: { kind: 'none' }, flow: null, route: { screen: 'rescuePick' } })).toBe('rescuePick');
});
