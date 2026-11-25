import { sanitizeHtml } from "./html";

describe("sanitizeHtml", () => {
  it("keeps layout styles and drops dangerous ones", () => {
    const out = sanitizeHtml(
      '<img style="width: 22px; height: 22px; position: fixed; z-index: 99; background-image: url(x.png)" src="https://a/b.png">',
    );
    expect(out).toContain("width: 22px");
    expect(out).toContain("height: 22px");
    expect(out).not.toMatch(/position|z-index|url\(/);
  });

  it("keeps text colour only alongside a solid background", () => {
    expect(sanitizeHtml('<span style="color: black">x</span>')).toBe("<span>x</span>");
    expect(sanitizeHtml('<div style="background-color: white">x</div>')).toBe("<div>x</div>");
    expect(sanitizeHtml('<span style="color: black; background-color: transparent">x</span>')).toBe("<span>x</span>");
    const boxed = sanitizeHtml('<span style="color: black; background-color: white">x</span>');
    expect(boxed).toContain("color: black");
    expect(boxed).toContain("background-color: white");
  });

  it("removes scripts, handlers and frames", () => {
    const out = sanitizeHtml('<p onclick="x()">a</p><script>x()</script><iframe src="https://e"></iframe>');
    expect(out).toBe("<p>a</p>");
  });
});
