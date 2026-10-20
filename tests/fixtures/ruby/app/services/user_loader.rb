require_relative "name_formatter"

class UserLoader
  def load(id)
    user = User.new(id: id)
    user.display_name
  end

  def call
    load 1
  end
end
