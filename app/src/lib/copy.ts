import type { ErrorCode, FlowError, FlowKind, Gate, PromptKind, StepId } from './ipc';

export const FALLBACK_ERROR_MESSAGE = 'Something went wrong.';

/** Strings composed by format.ts: the device chip, the backup line and the reveal button. */
export const FORMAT_COPY = {
  chipNone: '○ NO TABLET',
  chipUnauthorized: '◐ WAITING FOR ALLOW',
  chipEdl: '● RESCUE MODE',
  chipUnsupported: 'UNSUPPORTED',
  chipRoot: 'ROOT',
  backupStock: 'backup ✓',
  backupNotOriginal: 'backup · not original ·',
  noBackup: "no backup yet · we'll make one first",
  reveal: { macos: 'Show in Finder', windows: 'Show in Explorer', linux: 'Show in folder' },
} as const;

// ---------------------------------------------------------------------------
// Flow copy. `{name}` marks an interpolation; fill() replaces it.
// ---------------------------------------------------------------------------

export function fill(t: string, vars: Record<string, string | number>): string {
  return t.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

export interface StepCopy { heading: string; body: string; tech?: string }
export interface PromptCopy { label: string; heading: string; body: string; options: Record<string, string>; bullets?: string[]; checkbox?: string }

export const GROUP_LABELS: Record<FlowKind, string[]> = {
  backup: ['Check', 'Get ready', 'Copy', 'Verify'],
  importBackup: ['Open', 'Check', 'Save'],
  root: ['Check', 'Safety copy', 'Rooting', 'Restart'],
  rescue: ['Your file', 'Cable', 'Writing', 'Starting up'],
  unroot: ['Check', 'Restoring', 'Restart'],
  reinstallBootService: ['Check', 'Reinstall'],
};
export const STEP_DONE_LABEL = 'Done';
export const STEP_FAILED_LABEL = 'Stopped';

export function stepLabel(kind: FlowKind, stepIndex: number, stepCount: number): string {
  const g = GROUP_LABELS[kind][stepIndex - 1];
  return g ? `STEP ${stepIndex} OF ${stepCount} · ${g.toUpperCase()}` : `STEP ${stepIndex} OF ${stepCount}`;
}

const CHECK: StepCopy = {
  heading: 'Checking your tablet',
  body: "Making sure it's a Note Max on supported software.",
  tech: 'adb getprop: model, build id, fingerprint, slot, battery',
};
const REBOOT: StepCopy = {
  heading: 'Restarting your tablet',
  body: "It'll be back in a minute or two.",
  tech: 'edl reset → waiting for adb',
};
const WRITE: StepCopy = {
  heading: 'Writing to your tablet',
  body: "The screen may stay blank, or keep showing the last thing it displayed. That's normal. Don't unplug it.",
  tech: 'edl --memory=ufs w boot_{slot}',
};
const VERIFY_WRITE: StepCopy = {
  heading: 'Double-checking',
  body: "Reading it back to make sure every byte landed. Still don't unplug it.",
  tech: 'read-back SHA-256 compare',
};
const RESET: StepCopy = { heading: 'Restarting your tablet', body: 'Almost there.', tech: 'edl reset' };
const CHOOSE_SLOT_Q = 'One quick question.';

export const STEP_COPY: Record<FlowKind, Partial<Record<StepId, StepCopy>>> = {
  backup: {
    checkDevice: CHECK,
    enterEdl: {
      heading: 'Getting your tablet ready',
      body: "Your tablet will restart into a special mode. The screen may go blank, or keep showing the last thing it displayed. That's normal.",
      tech: 'adb reboot edl → waiting for USB 05c6:9008',
    },
    readPartitions: {
      heading: 'Making a safety copy',
      body: "We're copying your tablet's startup software to this computer, so it can always be put back. Keep it plugged in.",
      tech: 'edl --memory=ufs r boot_a, boot_b, vbmeta_a, vbmeta_b',
    },
    verifyBackup: {
      heading: 'Checking the copy',
      body: 'Reading it a second time to make sure the copy is perfect.',
      tech: 'second read of the active boot slot, SHA-256 compare, ANDROID! magic',
    },
    rebootToSystem: REBOOT,
  },
  importBackup: {
    pickImage: {
      heading: 'Opening your file',
      body: "Checking that it's a startup image from a Note Max.",
      tech: 'ANDROID! magic, size 100,663,296 bytes',
    },
    stockCheck: {
      heading: "Checking it's original",
      body: 'We only use untouched, original copies for rooting. This takes a few seconds.',
      tech: 'known stock SHA-256 list, else magiskboot cpio test',
    },
    chooseSlot: { heading: 'Which slot is it from?', body: CHOOSE_SLOT_Q },
    saveImport: {
      heading: 'Saving it to your library',
      body: 'Adding it to your safety copies.',
      tech: 'manifest.json source: imported',
    },
  },
  root: {
    checkDevice: CHECK,
    enterEdl: {
      heading: 'Getting your tablet ready',
      body: "Your tablet will restart into a special mode. The screen may stay blank, or keep showing the last thing it displayed. That's normal. Don't unplug it.",
      tech: 'adb reboot edl → 05c6:9008',
    },
    readPartitions: {
      heading: 'Saving your lifeline',
      body: "Before we change anything, we copy your tablet's original startup software so it can always be put back.",
      tech: 'edl --memory=ufs r boot_a/boot_b/vbmeta_a/vbmeta_b',
    },
    verifyBackup: {
      heading: 'Checking the copy',
      body: 'Making sure your safety copy is perfect before we go on.',
      tech: 'stock backup verified (SHA-256 re-read)',
    },
    rebootToSystem: REBOOT,
    patch: {
      heading: 'Preparing root',
      body: 'Your tablet is building its rooted startup software from your safety copy. Nothing is changed yet.',
      tech: 'Magisk 30.7 boot_patch.sh, KEEPVERITY=true KEEPFORCEENCRYPT=true, + overlay.d/init.bootsvc.rc',
    },
    cableCheck: { heading: 'Before we write', body: 'One quick question first.' },
    write: WRITE,
    verifyWrite: VERIFY_WRITE,
    reset: RESET,
    waitBoot: {
      heading: 'Waiting for your tablet to start',
      body: 'The first start after rooting can take a couple of minutes. The ONYX logo may stay up for a while. That\'s normal.',
      tech: 'waiting for adb (up to ~3 min)',
    },
    verifyRoot: {
      heading: 'Checking root works',
      body: 'Making sure Magisk and the boot service are both running.',
      tech: '/debug_ramdisk/magisk -c = 30.7; bootsvc.log uid=0 u:r:magisk:s0',
    },
  },
  rescue: {
    pickImage: {
      heading: 'Checking your file',
      body: "Making sure it's a good startup image for your tablet.",
      tech: 'ANDROID! magic, size check',
    },
    chooseSlot: { heading: 'Which slot should we fix?', body: CHOOSE_SLOT_Q },
    write: WRITE,
    verifyWrite: VERIFY_WRITE,
    reset: RESET,
    waitBoot: {
      heading: 'Written and checked ✓',
      body: 'Your tablet is starting up. This can take a few minutes.',
      tech: 'watching adb devices quietly (up to 5 min); 9008 again = back in rescue mode',
    },
  },
  unroot: {
    checkDevice: CHECK,
    enterEdl: {
      heading: 'Getting your tablet ready',
      body: "Your tablet will restart into a special mode. The screen may stay blank, or keep showing the last thing it displayed. That's normal. Don't unplug it.",
      tech: 'adb reboot edl → 05c6:9008',
    },
    write: {
      heading: 'Putting the original back',
      body: "The screen may stay blank, or keep showing the last thing it displayed. That's normal. Don't unplug it.",
      tech: 'edl --memory=ufs w boot_{slot} (stock)',
    },
    verifyWrite: VERIFY_WRITE,
    reset: RESET,
    waitBoot: {
      heading: 'Waiting for your tablet to start',
      body: "Then we'll check that root is gone.",
      tech: 'waiting for adb; expect magisk -c to fail',
    },
  },
  reinstallBootService: {
    checkDevice: CHECK,
    pushScripts: {
      heading: 'Reinstalling the boot service',
      body: "Copying the boot service back onto your tablet. Your startup software isn't touched.",
      tech: 'push bootsvc.sh + service.sh → /data/local/tmp',
    },
    waitBoot: {
      heading: 'Restarting your tablet',
      body: 'The boot service starts on its own when your tablet comes back.',
      tech: 'adb reboot; waiting for adb',
    },
    verifyRoot: {
      heading: 'Checking root works',
      body: 'Making sure Magisk and the boot service are both running.',
      tech: '/debug_ramdisk/magisk -c = 30.7; bootsvc.log uid=0 u:r:magisk:s0',
    },
  },
};

export const FALLBACK_STEP: StepCopy = {
  heading: 'Working on it',
  body: 'This can take a minute. Keep your tablet plugged in.',
};

export const DONE_COPY: Record<FlowKind, StepCopy & { checks?: string[] }> = {
  backup: {
    heading: 'Your safety copy is saved.',
    body: "It's in {path}. Keep a second copy somewhere safe, like a USB stick.",
    checks: ['Startup software copied', 'Copy checked twice'],
  },
  importBackup: {
    heading: 'Your backup is imported.',
    body: 'Inkroot can now use it to root, rescue or restore your tablet.',
  },
  root: {
    heading: 'Your Note Max is rooted.',
    body: 'Magisk {magiskVersion} is running, and so is the boot service. Your backup is in Documents/Inkroot. Keep it.',
    checks: ['Magisk daemon alive', 'Boot service running as root', 'Backup kept safe'],
  },
  rescue: {
    heading: 'Written and checked ✓',
    body: "Your tablet should be starting up now. Once you see its home screen, you're done. If it's still on the ONYX logo after 5 minutes, hold the power button for up to 30 seconds until it restarts, or run Rescue again. Your screen may still show the last thing it displayed. E-ink keeps its picture even when the tablet is off or in rescue mode. That's normal.",
  },
  unroot: {
    heading: 'Your Note Max is back to stock.',
    body: 'Root is removed and the original startup software is back. Your safety copies are still in Documents/Inkroot.',
    checks: ['Original startup software restored', 'Root removed'],
  },
  reinstallBootService: {
    heading: 'The boot service is running again.',
    body: 'It starts as root every time your tablet starts.',
    checks: ['Boot service running as root'],
  },
};
export const DONE_IMPORT_RESCUE_ONLY: StepCopy = {
  heading: 'Saved for rescue only.',
  body: "We couldn't confirm this copy is original, so it can rescue a stuck tablet but can't be used for rooting.",
};
export const ROOT_TIP: StepCopy = {
  heading: 'One more thing',
  body: 'Onyx puts apps it thinks are idle to sleep, which can stop root apps from working. On your tablet, open Settings → Battery → App freeze, turn off "Freeze new apps", and allow your apps to start automatically.',
};
export const ROOT_MORE = 'What can I do with root? →';

/** A flow the user stopped (Cancel, Stop or Not now). */
export const CANCELLED_COPY = {
  label: 'Stopped',
  heading: 'Stopped.',
  body: "Nothing else will happen. You can start again whenever you're ready.",
  back: 'Back to start',
} as const;
/** Shown where Cancel would be while the tablet is being written. */
export const WRITING_NOTE = 'writing · please keep it plugged in';
/** Root's safety-copy checklist (readPartitions / verifyBackup). */
export const SAFETY_CHECKS = {
  firmware: 'Firmware {firmwareLabel} recognised',
  slot: 'Active slot: {S}',
  saved: 'Startup software saved',
  checked: 'Copy checked',
} as const;
/** Rescue done, not seen during the wait, then spotted by the device poller. */
export const RESCUE_BACK = { found: "It's back ✓ Your tablet started." } as const;
/** Rescue step 4: the copy is written and checked; Inkroot watches for the tablet to start. */
export const RESCUE_WAIT = {
  checklist: [
    '✓ Written to your tablet',
    '✓ Checked — it matches your safety copy',
    '… Waiting for your tablet to start (up to 5 min)',
  ],
  done: 'Done — I can see my home screen',
} as const;
/** How a rescue ended (FlowResult.rescueOutcome). */
export const RESCUE_DONE = {
  tabletBack: {
    heading: 'Your tablet is back ✓',
    body: 'We wrote your safety copy, checked it, and your tablet has started up again.',
  },
  homeScreenSeen: {
    heading: 'Your tablet is back ✓',
    body: "We wrote your safety copy, checked it, and you've seen your tablet start up again.",
  },
  notSeen: {
    heading: 'Written and checked ✓',
    body: "Your tablet should be starting up now. Once you see its home screen, you're done. If it's still on the ONYX logo after 5 minutes, hold the power button for up to 30 seconds until it restarts, or run Rescue again. Your screen may still show the last thing it displayed. E-ink keeps its picture even when the tablet is off or in rescue mode. That's normal.",
    note: "Inkroot can't see your tablet when USB debugging is off, which is common after a rescue. That's okay.",
  },
  backInEdl: {
    heading: 'Your tablet went back to rescue mode',
    body: "We wrote your safety copy and checked it, but your tablet restarted into rescue mode instead of starting up. Hold the power button for up to 30 seconds until it restarts. Your screen may still show the last thing it displayed. E-ink keeps its picture even when the tablet is off or in rescue mode. That's normal. If it lands in rescue mode again, run Rescue again with a different saved copy.",
  },
  rooted: "It's rooted.",
  notRooted: "It's not rooted.",
  again: 'Run Rescue again',
} as const;

export const PROMPT_COPY: Record<PromptKind, PromptCopy> = {
  cableCheck: {
    label: 'Before we write',
    heading: 'Do you have a rescue cable?',
    body: 'If anything goes wrong, getting back needs a Qualcomm deep-flash cable and a computer with a USB-A port. Do you have one?',
    options: { have: 'Yes, I have one', whereToGet: 'Show me where to get one', continueWithout: 'Continue without one' },
  },
  cableCheckConfirm: {
    label: 'Are you sure?',
    heading: 'Continue without a rescue cable?',
    body: "Rooting almost always goes fine. If it doesn't, your tablet could stay stuck until you get a cable. Your safety copy will be ready either way.",
    options: { confirmContinue: 'Continue without a cable', back: 'Go back' },
    checkbox: 'I understand my tablet could be stuck until I get a cable.',
  },
  chooseSlot: {
    label: 'One question',
    heading: 'Which slot is this copy from?',
    body: 'Your tablet keeps two copies of its startup software, called A and B. {hint}',
    options: { a: 'Slot A', b: 'Slot B' },
  },
  keepWaitingOrRescue: {
    label: 'Still waiting',
    heading: "Your tablet hasn't restarted yet",
    body: "That's okay. The first start sometimes takes longer. If it's still on the ONYX logo after a few more minutes, Rescue can put your safety copy back.",
    options: { keepWaiting: 'Keep waiting', goToRescue: 'Go to Rescue' },
  },
  installDriver: {
    label: 'One-time setup',
    heading: 'One quick setup step',
    body: 'Windows needs a small driver to talk to your tablet in rescue mode. Windows will ask for permission once.',
    options: { install: 'Install driver', cancel: 'Not now' },
  },
  installUdev: {
    label: 'One-time setup',
    heading: 'One quick setup step',
    body: "Linux needs permission to talk to your tablet in rescue mode. You'll be asked for your password once. Afterwards, unplug the tablet and plug it back in.",
    options: { install: 'Allow access', cancel: 'Not now' },
  },
  edlNotFound: {
    label: 'Not found yet',
    heading: "We can't see your tablet yet",
    body: '',
    bullets: [
      "Check it's the deep-flash cable, not a normal one.",
      'Use a USB-A port on the computer itself, not an adapter or hub.',
      'Try again and keep holding the cable button a bit longer, about 15 seconds. The screen might not change.',
      "Make sure the cable's button isn't pressed, then try again.",
      "A black screen, or a screen stuck on the last thing it showed, is normal. E-ink keeps its picture even in rescue mode.",
    ],
    options: { retry: 'Try again', cancel: 'Stop' },
  },
  retryWrite: {
    label: "Didn't check out",
    heading: "That write didn't check out",
    body: "Nothing is broken. The copy on your tablet didn't match, so we stopped before restarting it. Unplug the cable, plug it back in firmly. Make sure the cable's button isn't pressed, then try again.",
    options: { retry: 'Try again', cancel: 'Stop' },
  },
  connectionLostAfterWrite: {
    label: 'Connection dropped',
    heading: 'The connection dropped while checking',
    body: "Your tablet may already be fixed. Hold the power button for up to 30 seconds to restart it. If it starts normally, you're done. If not, run Rescue again.",
    options: { restartedOk: 'It started normally', runAgain: 'Run Rescue again', cancel: 'Stop' },
  },
  stillInEdl: {
    label: 'Still restarting',
    heading: 'Your tablet is still in rescue mode',
    body: "Nothing is wrong with it. Inkroot asked it to restart a few times, but it stayed where it was. You can restart it yourself.",
    bullets: [
      'Hold the power button for up to 30 seconds until it restarts. The screen might not change until it does.',
      "Then let go and choose “I've restarted it”.",
      'Or choose “Try restarting again” to let Inkroot have another go.',
    ],
    options: { retry: 'Try restarting again', heldPower: "I've restarted it" },
  },
  // Rendered by the rescue waiting screen (RescueWait), not PromptView.
  tabletStarting: {
    label: 'Starting up',
    heading: 'Written and checked ✓',
    body: 'Your tablet is starting up. This can take a few minutes.',
    options: { homeScreen: 'Done — I can see my home screen' },
  },
};
/** edlNotFound for every flow except rescue. */
export const EDL_NOT_FOUND_AUTO: PromptCopy = {
  label: 'Not found yet',
  heading: "Your tablet didn't switch modes in time",
  body: "Check the cable is firmly plugged in at both ends and that the cable's button isn't pressed, then try again. Nothing has been changed on your tablet.",
  options: { retry: 'Try again', cancel: 'Stop' },
};
/** chooseSlot {hint} variants. */
export const CHOOSE_SLOT_HINTS = {
  fromName: "The file name says {S}, so we've picked that.",
  active: 'Your tablet is using {S} right now.',
  none: '',
} as const;

export const CABLE_BUY = {
  label: 'Rescue cable',
  heading: 'Getting a rescue cable',
  body: 'Search for "Qualcomm deep flash cable" or "EDL cable". It\'s a USB cable with a small button on it, and it costs a few dollars. A normal cable won\'t work.',
  copy: 'Copy search terms',
  copyValue: 'Qualcomm deep flash cable',
  back: 'Back',
} as const;

export const ERROR_HEADINGS: Record<ErrorCode, string> = {
  noDevice: 'We lost your tablet',
  unauthorized: 'Tap Allow on your tablet',
  unknownModel: "This tablet isn't supported",
  unknownFirmware: "This software version isn't supported yet",
  lowBattery: 'Charge your tablet first',
  alreadyRooted: 'Your tablet is already rooted',
  notRooted: "Your tablet isn't rooted",
  noStockBackup: 'We need an original safety copy',
  badImage: "That file can't be used",
  notStock: "That copy isn't original",
  edlTimeout: "Your tablet didn't switch modes",
  hashMismatch: "That write didn't check out",
  patchFailed: "Preparing root didn't work",
  bootTimeout: "Your tablet hasn't restarted yet",
  verifyFailed: "Root didn't start",
  toolMissing: 'Inkroot is missing a part',
  driverMissing: 'A driver is missing',
  busy: 'Inkroot is busy with another job',
  internal: 'Something went wrong',
};

export const ACTION_LABELS: Record<NonNullable<FlowError['suggestedAction']>, string> = {
  retry: 'Try again',
  goToRescue: 'Go to Rescue',
  reconnect: "I've reconnected it",
  chargeDevice: "OK, I'll charge it",
  importBackup: 'Import a backup',
};

/** Why the home screen can't show yet: no usable tablet, or a supported-device check failed. */
export type GateId = 'none' | 'unauthorized' | 'edl' | Gate;
export interface GateCopy {
  label: string;
  heading: string;
  body: string;
  primary?: string;
  secondary?: string;
  link?: string;
  meta?: string;
}

export const GATE_COPY: Record<GateId, GateCopy> = {
  none: {
    label: 'Connect',
    heading: 'Plug in your Note Max',
    body: "Use a normal USB-C cable. Then turn on USB debugging on your tablet. It's a hidden setting, so we'll walk you through it.",
    primary: 'Show me how',
    link: 'Tablet stuck on the ONYX logo? Rescue it →',
    meta: 'waiting for tablet…',
  },
  unauthorized: {
    label: 'Almost there',
    heading: 'Tap Allow on your tablet',
    body: 'Your tablet is asking "Allow USB debugging?". Tick "Always allow from this computer", then tap Allow.',
    meta: "Don't see it? Unplug the cable and plug it back in.",
    link: 'Show me how',
  },
  edl: {
    label: 'Rescue mode',
    heading: 'Your tablet is in rescue mode',
    body: "It's connected in a special mode used for repairs. Its screen may still show the last thing it displayed, which is normal for e-ink. If it's stuck, Rescue can put a good copy back.",
    primary: 'Rescue my tablet',
  },
  unknownModel: {
    label: 'Not supported',
    heading: 'Inkroot only supports the BOOX Note Max right now.',
    body: 'We found a {model}. Other tablets need their own testing before Inkroot can work with them safely. Nothing on your tablet was changed.',
  },
  unknownFirmware: {
    label: 'Not supported yet',
    heading: "This software version isn't supported yet",
    body: "Your Note Max is running a software version Inkroot hasn't been tested on. Using it could leave your tablet unable to start, so we've stopped here. Nothing on your tablet was changed.",
    secondary: 'Ask for support on GitHub',
    meta: 'fw {displayId} · build {incremental}',
  },
  lowBattery: {
    label: 'Charge first',
    heading: 'Charge to at least 30 % first',
    body: "Your tablet is at {battery} %. Rooting takes a while, and your tablet mustn't run out of power halfway. This screen moves on by itself once it's charged enough.",
    meta: 'battery {battery} % · need 30 %',
  },
};

export interface GuideStep {
  id: string;
  heading: string;
  body: string;
  header?: string;
  next?: string;
  primary?: string;
  secondary?: string;
  /** panel id the primary / secondary button jumps to */
  primaryTo?: string;
  secondaryTo?: string;
  links?: string[];
}

export const USB_GUIDE_LABEL = 'Turn on USB debugging · {i} of {n}';
export const USB_GUIDE_LIVE = {
  none: 'waiting for tablet…',
  unauthorized: 'tablet found · tap Allow on it',
  found: 'Found it ✓',
} as const;
export const USB_GUIDE_BACK = 'Back';
export const USB_GUIDE_CLOSE = 'Close';
export const USB_GUIDE: GuideStep[] = [
  { id: 'a1', header: 'Method A · Onyx menu', heading: 'Open Settings, then Apps', body: 'On your tablet, open Settings and tap Apps.', next: 'Next' },
  { id: 'a2', heading: 'Tap ⋮ in the top-right corner', body: "It's the menu with three dots.", next: 'Next' },
  {
    id: 'a3', heading: 'Turn on USB Debug Mode',
    body: 'If you see it, switch it on. Then plug your tablet into this computer.',
    primary: "It's on", primaryTo: 'allow', secondary: "I can't find it", secondaryTo: 'b1',
  },
  {
    id: 'b1', header: "Method B · If you can't find it", heading: 'Get Activity Launcher',
    body: "It's a free, open-source app that opens Android's hidden settings. Install it from F-Droid or Google Play. On a BOOX you may need to turn on Google Play first, in Settings → Apps.",
    primary: 'Next', links: ['F-Droid', 'Google Play'],
  },
  {
    id: 'b2', heading: 'Open Android Settings',
    body: 'In Activity Launcher, search for "Settings" and open the one with com.android.settings written under it.',
    next: 'Next',
  },
  {
    id: 'b3', heading: 'Tap Build number 7 times',
    body: "Go to About tablet and tap Build number seven times, until it says you're a developer.",
    next: 'Next',
  },
  {
    id: 'b4', heading: 'Turn on USB debugging',
    body: 'Go back, open System → Developer options, and switch on USB debugging. Then plug your tablet into this computer.',
    primary: "It's on", primaryTo: 'allow',
  },
  {
    id: 'allow', heading: 'Tap Allow on your tablet',
    body: 'When your tablet asks "Allow USB debugging?", tick "Always allow from this computer" and tap Allow.',
  },
];

export interface CableStep {
  id: string;
  heading: string;
  body: string;
  note?: string;
  art: string;
  primary?: string;
  secondary?: string;
  next?: string;
}
export const CABLE_GUIDE_LABEL = 'Step {i} of 4 · Cable · {p} of 6';
export const CABLE_GUIDE_FOOT = {
  reassurance: "Your screen may still show the last thing it displayed. E-ink keeps its picture even when the tablet is off or in rescue mode. That's normal, and your tablet isn't broken.",
  waiting: 'Waiting for your tablet…',
  found: 'Found it ✓',
} as const;
/** Cable guide controls: Back, the hold timers and the copy-search-terms feedback. */
export const CABLE_GUIDE_NAV = {
  back: 'Back',
  timer: '{n} s · keep holding',
  timerDone: "Time's up ✓",
} as const;
export const CABLE_GUIDE: CableStep[] = [
  {
    id: 'c1', heading: 'You need a deep-flash cable',
    body: 'It\'s a USB cable with a small button on it. Search for "Qualcomm deep flash cable" or "EDL cable". They cost a few dollars. A normal USB cable won\'t work.',
    art: 'DeepFlashCable, button highlighted', primary: 'I have one', secondary: 'Copy search terms',
  },
  {
    id: 'c2', heading: 'Use a real USB-A port',
    body: "Plug the cable's big end into a USB-A port on the computer itself. USB-C adapters and hubs often don't work.",
    note: 'No USB-A port? Borrow a computer that has one, like a desktop or an older laptop, and run Inkroot there. There are versions for Windows and Linux.',
    art: 'UsbAPort + crossed-out adapter', next: 'Next',
  },
  {
    id: 'c3', heading: "Hold the cable's button",
    body: "Press the small button on the cable and keep holding it. Don't let go yet.",
    art: 'Hand + PressIndicator on cable', next: 'Next',
  },
  {
    id: 'c4', heading: 'Plug it into your tablet',
    body: 'Still holding the button, plug the cable into your tablet.',
    art: 'Tablet + cable', next: 'Next',
  },
  {
    id: 'c5', heading: 'Hold the power button for 15 seconds',
    body: "Keep holding both buttons for about 15 seconds. The screen might not change. E-ink keeps showing the last thing it displayed, and that's normal.",
    art: 'Tablet power button + PressIndicator', secondary: 'Start a 15 s timer', next: 'Next',
  },
  {
    id: 'c6', heading: 'Keep holding the cable button 10 more seconds',
    body: "Let go of the power button. Keep holding the cable button for about 10 more seconds. Let go of the button and don't touch it again until Inkroot says it's done — pressing it during the transfer interrupts the connection.",
    art: 'Hand on cable', secondary: 'Start a 10 s timer',
  },
];

export const RISK_COPY = {
  label: 'Before you start',
  heading: 'Please read this first',
  bullets: [
    "Changing your tablet's software can damage it permanently or leave it unable to start.",
    'It may void your warranty and could cause data loss.',
    'Inkroot is free community software, provided as-is with no warranty. Its authors accept no liability for any damage.',
    "Inkroot isn't affiliated with or endorsed by Onyx International / BOOX, Qualcomm, Google or the Magisk project.",
    'Only use it on a tablet you own. Rooting may be restricted where you live, and checking is up to you.',
  ],
  checkbox: "I understand and accept these risks; I'm responsible for my device",
  link: 'Read the full disclaimer',
  continue: 'Continue',
  back: 'Back',
} as const;

export const HOME_COPY = {
  heading: 'Pick a job.',
  root: { title: 'Root', meta: 'magisk 30.7', metaRooted: 'rooted ✓' },
  rescue: { title: 'Rescue', meta: 'stuck / bootloop' },
  stock: { title: 'Stock', meta: 'undo root', metaNotRooted: 'not rooted', metaNoStock: 'no original copy' },
  backupsLink: 'Backups →',
  helpLink: 'Trouble connecting? USB debugging help',
} as const;

export const ROOTED_COPY = {
  label: 'Rooted',
  heading: 'Already rooted ✓',
  body: 'Magisk {magiskVersion} is running{svc}.',
  svcRunning: ', and so is the boot service',
  svcStopped: ", but the boot service isn't",
  reinstall: { label: 'Reinstall boot service', desc: 'Fixes the boot service if it stopped.' },
  stock: { label: 'Return to stock', desc: 'Puts the original startup software back.' },
  noStock: "You don't have an original safety copy on file. A copy made now would include root, so it can't be used to return to stock. If you saved a stock boot image when you first rooted, import it.",
  importButton: 'Import a backup',
  back: 'Back',
} as const;

export const BACKUPS_COPY = {
  label: 'Library',
  heading: 'Your safety copies',
  body: 'Saved on this computer in {backupRoot}. Inkroot never uploads them.',
  rowMeta: '{serial} · {date} · slot {S} · {original} · {verified}{imported}',
  original: 'original ✓',
  notOriginal: 'not original',
  verified: 'verified',
  notVerified: 'not verified',
  imported: ' · imported',
  backUpNow: 'Back up now',
  importButton: 'Import a backup…',
  back: 'Back',
  empty: 'No safety copies yet. Back up your tablet, or import a copy you made before.',
  rootedNote: "Your tablet is rooted, so a new copy won't be original. It can still help with Rescue.",
  backUpDisabled: 'Connect your Note Max to make a new copy.',
} as const;

export const RESCUE_PICK_COPY = {
  label: 'Rescue',
  heading: 'Which copy should we put back?',
  body: 'A tablet stuck on the ONYX logo is almost always fixable. Pick the safety copy you made of this tablet, or choose the file if you saved it somewhere else.',
  chooseFile: 'Choose a file…',
  continue: 'Continue',
  back: 'Back',
  empty: 'There are no safety copies in Inkroot yet. If you have a boot image file, choose it below.',
} as const;

export const ABOUT_COPY = {
  label: 'About',
  heading: 'Inkroot',
  lines: [
    'Version {version}',
    'One-click root and rescue for the ONYX BOOX Note Max.',
    'Inkroot collects no data. Nothing leaves your computer.',
    'Not affiliated with, endorsed by or sponsored by Onyx International / BOOX, Qualcomm, Google or the Magisk project. Product names are used only to describe compatibility.',
    'Free software under the MIT license. Magisk and edl are GPLv3 and ship as separate programs.',
    'Safety copies: {backupRoot}',
  ],
  links: { disclaimer: 'Disclaimer', licenses: 'Licenses & third-party notices', source: 'Source code' },
  demoChip: 'DEMO MODE',
} as const;

export const CLOSE_GUARD_COPY = {
  heading: 'Inkroot is writing to your tablet',
  body: "Closing now could leave your tablet unable to start. It'll be done in a few minutes.",
  keep: 'Keep Inkroot open',
  closeAnyway: 'Close anyway',
} as const;

/** Shown while the app connects to the backend, before anything else can render. */
export const STARTING = 'starting…';
/** Shown instead of STARTING when the app couldn't connect to its backend at all. */
export const START_FAILED = "Inkroot couldn't start. Try closing it and opening it again.";

/** Shown next to a link the system browser couldn't open. */
export const OPEN_FAILED = "Couldn't open your browser. The link is in Details.";

/** Lines Inkroot adds to Details itself (technical, so plain English isn't required here). */
export const LOG_NOTES = {
  openFailed: "Couldn't open {url} in the browser: {error}",
  revealFailed: "Couldn't show {path}: {error}",
  pickFailed: "Couldn't open the file picker: {error}",
} as const;

export const COMMON = {
  cancel: 'Cancel',
  finish: 'Finish',
  backToStart: 'Back to start',
  details: 'Details',
  copyLog: 'Copy log',
  copied: 'Copied ✓',
  copyFailed: "Couldn't copy. Select the text instead.",
  backups: 'Backups',
  about: 'About',
  wordmark: 'INKROOT',
  failedFoot: 'The full details are in Details below if you want to report this.',
} as const;
