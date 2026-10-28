import { describe, expect, test } from 'vitest';
import { ALL_ERROR_CODES, ALL_PROMPT_KINDS, ALL_STEP_IDS, FLOW_KINDS, FLOW_STEPS, FLOW_STEP_COUNT } from './flows';
import {
  CABLE_GUIDE, CANCELLED_COPY, DONE_COPY, ERROR_HEADINGS, GATE_COPY, GROUP_LABELS, HOME_COPY, PROMPT_COPY,
  RESCUE_DONE, RESCUE_WAIT, RISK_COPY, STEP_COPY, WRITING_NOTE, fill, stepLabel,
} from './copy';

const stripTech = (o: unknown): unknown => JSON.parse(JSON.stringify(o, (k, v) => (k === 'tech' ? undefined : v)));

test('ALL_* lists match the contract unions', () => {
  expect(new Set(ALL_STEP_IDS).size).toBe(20);
  expect(ALL_PROMPT_KINDS).toHaveLength(10);
  expect(ALL_ERROR_CODES).toHaveLength(19);
  expect(new Set([...Object.values(FLOW_STEPS).flat(), 'done'])).toEqual(new Set(ALL_STEP_IDS));
});
const pairs = FLOW_KINDS.flatMap((k) => FLOW_STEPS[k].map((s) => [k, s] as const));
test.each(pairs)('copy exists for %s · %s', (k, s) => {
  if (k === 'rescue' && (s === 'cableGuide' || s === 'waitEdl')) return expect(CABLE_GUIDE.length).toBe(6);
  const c = STEP_COPY[k][s]!;
  expect(c.heading.length).toBeGreaterThan(0);
  expect(c.body.length).toBeGreaterThan(0);
});
test.each(FLOW_KINDS)('group labels cover stepCount for %s', (k) => expect(GROUP_LABELS[k]).toHaveLength(FLOW_STEP_COUNT[k]));
test.each(FLOW_KINDS)('done copy for %s', (k) => expect(DONE_COPY[k].heading).toBeTruthy());
test.each(ALL_PROMPT_KINDS)('prompt copy for %s labels every contract option', (k) => {
  const ids = {
    cableCheck: ['have', 'whereToGet', 'continueWithout'], cableCheckConfirm: ['confirmContinue', 'back'], chooseSlot: ['a', 'b'],
    keepWaitingOrRescue: ['keepWaiting', 'goToRescue'], installDriver: ['install', 'cancel'], installUdev: ['install', 'cancel'],
    edlNotFound: ['retry', 'cancel'], retryWrite: ['retry', 'cancel'], stillInEdl: ['retry', 'heldPower'],
    tabletStarting: ['homeScreen'],
  }[k];
  expect(Object.keys(PROMPT_COPY[k].options).sort()).toEqual([...ids].sort());
});
test.each(ALL_ERROR_CODES)('error heading for %s', (c) => expect(ERROR_HEADINGS[c]).toBeTruthy());
test('stepLabel', () => {
  expect(stepLabel('root', 2, 4)).toBe('STEP 2 OF 4 · SAFETY COPY');
  expect(stepLabel('root', 9, 4)).toBe('STEP 9 OF 4');
});
test('fill leaves unknown keys visible', () => expect(fill('a {x} {y}', { x: 1 })).toBe('a 1 {y}'));
test('main-path copy has no jargon', () => {
  const main = JSON.stringify([STEP_COPY, DONE_COPY, PROMPT_COPY, GATE_COPY, HOME_COPY, RESCUE_WAIT, RESCUE_DONE].map(stripTech));
  for (const w of ['adb', 'EDL', '9008', 'fastboot', 'partition', 'SHA', 'verity', 'sideload']) expect(main).not.toMatch(new RegExp(`\\b${w}\\b`));
});
describe('voice anchors', () => {
  test('anchors', () => {
    expect(STEP_COPY.root.write!.body).toBe("The screen may stay blank, or keep showing the last thing it displayed. That's normal. Don't unplug it.");
    expect(GATE_COPY.unknownModel.heading).toBe('Inkroot only supports the BOOX Note Max right now.');
    expect(RISK_COPY.checkbox).toBe("I understand and accept these risks; I'm responsible for my device");
  });
});

test('cancelled and writing copy', () => {
  expect(CANCELLED_COPY).toEqual({
    label: 'Stopped',
    heading: 'Stopped.',
    body: "Nothing else will happen. You can start again whenever you're ready.",
    back: 'Back to start',
  });
  expect(WRITING_NOTE).toBe('writing · please keep it plugged in');
});

test('rescue ending copy', () => {
  expect(RESCUE_WAIT.checklist).toEqual([
    '✓ Written to your tablet',
    '✓ Checked — it matches your safety copy',
    '… Waiting for your tablet to start (up to 5 min)',
  ]);
  expect(RESCUE_WAIT.done).toBe('Done — I can see my home screen');
  expect(RESCUE_DONE.tabletBack).toEqual({
    heading: 'Your tablet is back ✓',
    body: 'We wrote your safety copy, checked it, and your tablet has started up again.',
  });
  expect(RESCUE_DONE.notSeen.heading).toBe('Written and checked ✓');
  expect(RESCUE_DONE.notSeen.body).toBe(
    "Your tablet should be starting up now. Once you see its home screen, you're done. If it's still on the ONYX logo after 5 minutes, hold the power button for up to 30 seconds until it restarts, or run Rescue again. Your screen may still show the last thing it displayed. E-ink keeps its picture even when the tablet is off or in rescue mode. That's normal.",
  );
  expect(RESCUE_DONE.notSeen.note).toMatch(/USB debugging is off/);
  expect(PROMPT_COPY.tabletStarting.options.homeScreen).toBe(RESCUE_WAIT.done);
});
