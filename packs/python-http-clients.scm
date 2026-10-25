; Python HTTP clients: requests, httpx (module functions, Client and
; AsyncClient), and sessions. Docs: requests.readthedocs.io,
; www.python-httpx.org/api/
; A client-like receiver and a URL with a "/" tell a request from
; cache.get("k") and dict lookups.

; requests.get(url), httpx.post(url, json=...), client.get(f"/api/{id}")
(call
  function: (attribute
    object: (identifier) @object
    attribute: (identifier) @method)
  arguments: (argument_list . [(string) (binary_operator)] @path)
  (#match? @object "^(requests|httpx|client|session|http|sess|[a-z_]*_client|[a-z_]*_session)$")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")
  (#match? @path "/")) @client

; httpx.Client().get(url), requests.Session().post(url)
(call
  function: (attribute
    object: (call
      function: (attribute
        object: (identifier) @lib
        attribute: (identifier) @ctor))
    attribute: (identifier) @method)
  arguments: (argument_list . [(string) (binary_operator)] @path)
  (#match? @lib "^(requests|httpx)$")
  (#match? @ctor "^(Client|AsyncClient|Session)$")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")) @client

; requests.request("GET", url)
(call
  function: (attribute
    object: (identifier) @object
    attribute: (identifier) @fn)
  arguments: (argument_list . (string) @method . [(string) (binary_operator)] @path)
  (#match? @object "^(requests|httpx|client|session|[a-z_]*_client|[a-z_]*_session)$")
  (#eq? @fn "request")) @client
