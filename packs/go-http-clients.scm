; Go HTTP clients: net/http's package functions, an *http.Client's methods,
; and http.NewRequest. Docs: pkg.go.dev/net/http#Get

; http.Get(url), http.Post(url, contentType, body), client.Get(url)
(call_expression
  function: (selector_expression
    operand: (identifier) @object
    field: (field_identifier) @method)
  arguments: (argument_list . [(interpreted_string_literal) (raw_string_literal) (binary_expression)] @path)
  (#match? @object "^(http|client|httpClient|[a-zA-Z]*Client)$")
  (#match? @method "^(Get|Head|Post|PostForm)$")
  (#match? @path "/")) @client

; http.Get(fmt.Sprintf("%s/users/%d", base, id))
(call_expression
  function: (selector_expression
    operand: (identifier) @object
    field: (field_identifier) @method)
  arguments: (argument_list
    . (call_expression
      function: (selector_expression operand: (identifier) @fmt field: (field_identifier) @sprintf)
      arguments: (argument_list . (interpreted_string_literal) @path)))
  (#match? @object "^(http|client|httpClient|[a-zA-Z]*Client)$")
  (#match? @method "^(Get|Head|Post|PostForm)$")
  (#eq? @fmt "fmt")
  (#eq? @sprintf "Sprintf")) @client

; http.NewRequest("POST", url, body), method a string or http.MethodPost
(call_expression
  function: (selector_expression operand: (identifier) @pkg field: (field_identifier) @fn)
  arguments: (argument_list
    . [(interpreted_string_literal) (selector_expression)] @method
    . [(interpreted_string_literal) (raw_string_literal) (binary_expression)] @path)
  (#eq? @pkg "http")
  (#eq? @fn "NewRequest")) @client

(call_expression
  function: (selector_expression operand: (identifier) @pkg field: (field_identifier) @fn)
  arguments: (argument_list
    . [(interpreted_string_literal) (selector_expression)] @method
    . (call_expression
      function: (selector_expression operand: (identifier) @fmt field: (field_identifier) @sprintf)
      arguments: (argument_list . (interpreted_string_literal) @path)))
  (#eq? @pkg "http")
  (#eq? @fn "NewRequest")
  (#eq? @fmt "fmt")
  (#eq? @sprintf "Sprintf")) @client

; http.NewRequestWithContext(ctx, "GET", url, body)
(call_expression
  function: (selector_expression operand: (identifier) @pkg field: (field_identifier) @fn)
  arguments: (argument_list
    . (_)
    . [(interpreted_string_literal) (selector_expression)] @method
    . [(interpreted_string_literal) (raw_string_literal) (binary_expression)] @path)
  (#eq? @pkg "http")
  (#eq? @fn "NewRequestWithContext")) @client

(call_expression
  function: (selector_expression operand: (identifier) @pkg field: (field_identifier) @fn)
  arguments: (argument_list
    . (_)
    . [(interpreted_string_literal) (selector_expression)] @method
    . (call_expression
      function: (selector_expression operand: (identifier) @fmt field: (field_identifier) @sprintf)
      arguments: (argument_list . (interpreted_string_literal) @path)))
  (#eq? @pkg "http")
  (#eq? @fn "NewRequestWithContext")
  (#eq? @fmt "fmt")
  (#eq? @sprintf "Sprintf")) @client
