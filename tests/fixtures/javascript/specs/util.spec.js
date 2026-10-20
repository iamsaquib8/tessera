const { slugify } = require("../lib/util");

describe("slugify", () => {
  it("lowercases", () => {
    expect(slugify("A")).toBe("a");
  });
});
