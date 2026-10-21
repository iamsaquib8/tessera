class User < ApplicationRecord
  has_many :posts

  def display_name
    NameFormatter.format(name)
  end
end
