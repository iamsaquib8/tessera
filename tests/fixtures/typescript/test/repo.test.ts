import { loadUser } from "../src/users/repo";

describe("loadUser", () => {
  it("returns the user", () => {
    expect(loadUser("a")).toBe("a");
  });
});
