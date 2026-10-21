require "spec_helper"

RSpec.describe UserLoader do
  it "loads" do
    expect(UserLoader.new.load(1)).to be_truthy
  end
end
