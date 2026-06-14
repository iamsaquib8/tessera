require 'set'

module Sample
  class UserService
    def find_by_id(id)
      render(load_user(id))
    end

    def render_user(id)
      find_by_id(id)
    end

    private

    def load_user(id)
      "user##{id}"
    end

    def render(user)
      "[#{user}]"
    end
  end
end
