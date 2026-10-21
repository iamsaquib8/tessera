import { makeService } from "../src/services/users";

describe("UserService", () => {
  it("names a user", () => {
    expect(makeService().name("1")).toBeDefined();
  });
});
