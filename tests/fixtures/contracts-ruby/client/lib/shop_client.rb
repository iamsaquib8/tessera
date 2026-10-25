require "net/http"
require "faraday"
require "httparty"

class ShopClient
  BASE = "http://shop:3000"

  def user(id)
    Net::HTTP.get(URI("http://shop:3000/users/#{id}"))
  end

  def create_user(body)
    Faraday.post("#{BASE}/users", body)
  end

  def order(id)
    HTTParty.get("http://shop:3000/orders/#{id}")
  end

  def cart(id)
    conn = Faraday.new(url: BASE)
    conn.delete("/api/carts/#{id}")
  end

  def reviews
    Net::HTTP.get_response(URI.parse("http://shop:3000/reviews"))
  end

  def lookalike(cache)
    cache.get("/api/status") || ENV.fetch("/orders/new", nil)
  end
end
