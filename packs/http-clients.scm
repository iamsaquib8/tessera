; HTTP clients in JavaScript and TypeScript: fetch, axios and axios-like
; instances, ky, SWR.

; fetch("/api/x")
(call_expression
  function: (identifier) @fn
  arguments: (arguments . [(string) (template_string) (binary_expression)] @path .)
  (#eq? @fn "fetch")
  (#set! method "GET")) @client

; fetch("/api/x", { method: "POST" })
(call_expression
  function: (identifier) @fn
  arguments: (arguments
    . [(string) (template_string) (binary_expression)] @path
    . (object (pair key: (property_identifier) @method_key value: (string) @method)))
  (#eq? @fn "fetch")
  (#eq? @method_key "method")) @client

; fetch("/api/x", options)
(call_expression
  function: (identifier) @fn
  arguments: (arguments . [(string) (template_string) (binary_expression)] @path . (_))
  (#eq? @fn "fetch")) @client

; axios.get("/api/x"), api.post(`/api/${id}`, body), this.http.delete(url)
(call_expression
  function: (member_expression
    object: [(identifier) @object
             (member_expression property: (property_identifier) @object)]
    property: (property_identifier) @method)
  arguments: (arguments . [(string) (template_string) (binary_expression)] @path)
  (#match? @object "^(axios|ky|http|httpClient|api|apiClient|client|request|\\$http|\\$api|instance)$")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")) @client

; axios({ method: "post", url: "/api/x" })
(call_expression
  function: (identifier) @fn
  arguments: (arguments . (object
    (pair key: (property_identifier) @method_key value: (string) @method)
    (pair key: (property_identifier) @url_key value: [(string) (template_string)] @path)))
  (#eq? @fn "axios")
  (#eq? @method_key "method")
  (#eq? @url_key "url")) @client

(call_expression
  function: (identifier) @fn
  arguments: (arguments . (object
    (pair key: (property_identifier) @url_key value: [(string) (template_string)] @path)))
  (#eq? @fn "axios")
  (#eq? @url_key "url")) @client

; useSWR("/api/x", fetcher)
(call_expression
  function: (identifier) @fn
  arguments: (arguments . [(string) (template_string)] @path)
  (#match? @fn "^(useSWR|useSWRImmutable|useSWRInfinite)$")
  (#set! method "GET")) @client
