; Go HTTP servers: net/http (with Go 1.22 method patterns), chi, gin, echo.
; Docs: pkg.go.dev/net/http#ServeMux, go-chi.io, gin-gonic.com/docs,
; echo.labstack.com/docs/routing
; A path that starts with "/" (or "GET /") and a handler after it tell a
; route from cache.Get("k") and header lookups.

; mux.HandleFunc("GET /users/{id}", getUser), http.Handle("/metrics", h)
(call_expression
  function: (selector_expression
    operand: (identifier)
    field: (field_identifier) @fn)
  arguments: (argument_list
    . [(interpreted_string_literal) (raw_string_literal)] @path
    . (_))
  (#match? @fn "^(HandleFunc|Handle)$")
  (#match? @path "^.([A-Z]+ +)?/")) @route

; chi: r.Get("/users/{id}", h); gin and echo: r.GET("/users/:id", h)
(call_expression
  function: (selector_expression
    operand: (identifier) @object
    field: (field_identifier) @method)
  arguments: (argument_list
    . [(interpreted_string_literal) (raw_string_literal)] @path
    . (_))
  (#match? @object "^(r|router|mux|api|app|e|g|s|srv|server|engine|v[0-9]+|[a-zA-Z]*Router|[a-zA-Z]*Mux|[a-zA-Z]*Group)$")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Head|Options|Connect|Trace)$")
  (#match? @path "^./")) @route

(call_expression
  function: (selector_expression
    operand: (identifier)
    field: (field_identifier) @method)
  arguments: (argument_list
    . [(interpreted_string_literal) (raw_string_literal)] @path
    . (_))
  (#match? @method "^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|Any)$")
  (#match? @path "^./")) @route

; chi: r.Route("/api", func(r chi.Router) { r.Post("/carts", h) })
(call_expression
  function: (selector_expression field: (field_identifier) @route_fn)
  arguments: (argument_list
    . [(interpreted_string_literal) (raw_string_literal)] @prefix
    . (func_literal
      body: (block
        (statement_list
          (expression_statement
            (call_expression
              function: (selector_expression field: (field_identifier) @method)
              arguments: (argument_list
                . [(interpreted_string_literal) (raw_string_literal)] @path
                . (_))) @route)))))
  (#eq? @route_fn "Route")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Head|Options|Handle|HandleFunc)$")
  (#match? @path "^./"))

; gin and echo groups: api := r.Group("/api"); api.POST("/orders", h)
(statement_list
  (short_var_declaration
    left: (expression_list . (identifier) @group)
    right: (expression_list
      . (call_expression
        function: (selector_expression field: (field_identifier) @group_fn)
        arguments: (argument_list
          . [(interpreted_string_literal) (raw_string_literal)] @prefix))))
  (expression_statement
    (call_expression
      function: (selector_expression
        operand: (identifier) @object
        field: (field_identifier) @method)
      arguments: (argument_list
        . [(interpreted_string_literal) (raw_string_literal)] @path
        . (_))) @route)
  (#eq? @group @object)
  (#eq? @group_fn "Group")
  (#match? @method "^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|Any|Get|Post|Put|Patch|Delete|Head|Options|Handle|HandleFunc)$")
  (#match? @path "^./"))
