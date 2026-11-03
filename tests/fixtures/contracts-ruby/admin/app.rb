require "sinatra/base"

class AdminApp < Sinatra::Base
  get "/admin/stats" do
    "stats"
  end

  post "/admin/users" do
    "created"
  end

  put "/admin/users/:id" do
    "updated"
  end

  patch "/admin/users/:id" do
    "patched"
  end

  delete "/admin/users/:id" do
    "deleted"
  end

  head "/admin/health" do
    "ok"
  end

  options "/admin/health" do
    "options"
  end
end

# Top-level Sinatra routes:
get "/ping" do
  "pong"
end

# Lookalike: plain get call or receiver call that must not match as a route:
class Lookalike
  def check(cache)
    cache.get "/admin/stats" do
      # not a sinatra route
    end
    get "/admin/stats"
  end
end
