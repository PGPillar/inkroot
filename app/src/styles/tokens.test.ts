import { expect, test } from "vitest";
import tokens from "./tokens.css?raw";
import global from "./global.css?raw";

test("panel palette tokens", () => {
  expect(tokens).toMatch(/--panel:\s*#E8E9E4/i);
  expect(tokens).toMatch(/--ink:\s*#111111/i);
  expect(tokens).toMatch(/--muted:\s*#555555/i);
  expect(tokens).toMatch(/prefers-color-scheme:\s*dark/);
});

test("no remote fonts, radius or shadows", () => {
  for (const css of [tokens, global]) {
    expect(css).not.toMatch(/https?:\/\//);
    expect(css).not.toMatch(/box-shadow:\s*(?!none)/);
  }
  expect(global).toMatch(
    /@keyframes eink\{0%\{filter:invert\(1\) contrast\(3\)\}30%\{filter:brightness\(0\)\}55%\{filter:brightness\(3\) grayscale\(1\)\}80%\{filter:none\}\}/,
  );
  expect(global).toMatch(/prefers-reduced-motion:\s*reduce/);
});
