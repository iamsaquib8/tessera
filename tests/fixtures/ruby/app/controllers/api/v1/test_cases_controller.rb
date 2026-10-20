class TestCasesController
  def index
    UserLoader.new.load(1)
  end
end
