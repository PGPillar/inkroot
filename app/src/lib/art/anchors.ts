/**
 * Points inside each illustration's own viewBox, for laying one drawing over another.
 * Kept out of index.ts so the art index stays a list of illustrations.
 */
export interface Point { x: number; y: number }

export const VIEWBOX = {
  cable: { w: 280, h: 96 },
  hand: { w: 96, h: 140 },
  tablet: { w: 144, h: 200 },
  press: { w: 96, h: 96 },
  pressTimer: { w: 96, h: 124 },
} as const;

/** Hand: the pointing fingertip. */
export const HAND_TIP: Point = { x: 48, y: 134 };
/** DeepFlashCable: centre of the round button (the box spans x 118-174, y 34-74). */
export const CABLE_BUTTON: Point = { x: 146, y: 54 };
/** DeepFlashCable: the tip of the USB-C plug, on the cable's centre line. */
export const CABLE_USB_C_TIP: Point = { x: 270, y: 54 };
/** Tablet: the outer (top) edge of the power button, centred. */
export const TABLET_POWER_EDGE: Point = { x: 101, y: 19 };
/** Tablet: the mouth of the USB-C port on the bottom edge, centred. The port faces down. */
export const TABLET_PORT: Point = { x: 72, y: 178 };
/** PressIndicator: the press point the ripples circle (ripples reach r = 36). */
export const PRESS_POINT: Point = { x: 48, y: 48 };
