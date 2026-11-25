import { describe, expect, it } from "vitest";
import { filterIcons, ICON_THEMES, iconThemeClass, nerdGlyph } from "./ModlistIcon";
import { NERD_ICONS } from "./nerdIcons";

describe("modlist icons", () => {
  it("only bundles Font Awesome and Material Design glyphs, without duplicates", () => {
    const names = NERD_ICONS.map(([name]) => name);
    expect(names.every((n) => /^(fa|md)-[a-z0-9_]+$/.test(n))).toBe(true);
    expect(new Set(names).size).toBe(names.length);
    expect(NERD_ICONS.every(([, , cp]) => cp >= 0xe000)).toBe(true);
  });

  it("looks up and searches icons", () => {
    expect(nerdGlyph("md-sword")?.label).toBe("Sword");
    expect(nerdGlyph("not-an-icon")).toBeNull();
    expect(nerdGlyph(null)).toBeNull();
    const swords = filterIcons("sword").map(([name]) => name);
    expect(swords).toContain("md-sword");
    expect(swords).toContain("md-sword_cross");
    expect(filterIcons("battle axe").map(([name]) => name)).toEqual(["md-axe_battle"]);
    expect(filterIcons("").length).toBe(NERD_ICONS.length);
  });

  it("offers ten color themes and ignores unknown ones", () => {
    expect(ICON_THEMES).toHaveLength(10);
    expect(iconThemeClass("sage")).toBe(" icon-theme-sage");
    expect(iconThemeClass("neon")).toBe("");
    expect(iconThemeClass(null)).toBe("");
  });
});
