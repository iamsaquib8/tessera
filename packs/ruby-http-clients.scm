; Ruby HTTP clients: Net::HTTP with a URI, Faraday and HTTParty, and a
; Faraday connection. Docs: docs.ruby-lang.org/en/master/Net/HTTP.html,
; lostisland.github.io/faraday, github.com/jnunemaker/httparty

; Net::HTTP.get(URI("http://shop/users/#{id}")), Net::HTTP.post(URI.parse(...), body)
(call
  receiver: (scope_resolution scope: (constant) @ns name: (constant) @cls)
  method: (identifier) @method
  arguments: (argument_list
    . (call
      method: (constant) @uri
      arguments: (argument_list . (string) @path)))
  (#eq? @ns "Net")
  (#eq? @cls "HTTP")
  (#eq? @uri "URI")
  (#match? @method "^(get|get_response|post|post_form|put|patch|delete|head)$")) @client

(call
  receiver: (scope_resolution scope: (constant) @ns name: (constant) @cls)
  method: (identifier) @method
  arguments: (argument_list
    . (call
      receiver: (constant) @uri
      method: (identifier) @parse
      arguments: (argument_list . (string) @path)))
  (#eq? @ns "Net")
  (#eq? @cls "HTTP")
  (#eq? @uri "URI")
  (#eq? @parse "parse")
  (#match? @method "^(get|get_response|post|post_form|put|patch|delete|head)$")) @client

; Faraday.get(url), HTTParty.post(url, body: …)
(call
  receiver: (constant) @lib
  method: (identifier) @method
  arguments: (argument_list . (string) @path)
  (#match? @lib "^(Faraday|HTTParty|RestClient|HTTPX)$")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")) @client

; conn = Faraday.new(url: BASE); conn.get("/api/carts/#{id}")
(call
  receiver: (identifier) @object
  method: (identifier) @method
  arguments: (argument_list . (string) @path)
  (#match? @object "^(conn|connection|faraday|http|client|[a-z_]*_client|[a-z_]*_conn|[a-z_]*_connection)$")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")
  (#match? @path "^[\"']([^\"']*/)")) @client
