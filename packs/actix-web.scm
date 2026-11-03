; Actix-web routes in Rust: attribute macros and .route() call chains.
; Docs: actix.rs/docs/url-dispatch/

; Macro routes: #[get("/path")] async fn handler(...)
(source_file
  (attribute_item
    (attribute
      [(identifier) @method (scoped_identifier name: (identifier) @method)]
      arguments: (token_tree . [(string_literal) (raw_string_literal)] @path))) @route
  .
  (function_item
    name: (identifier) @handler)
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./"))

(declaration_list
  (attribute_item
    (attribute
      [(identifier) @method (scoped_identifier name: (identifier) @method)]
      arguments: (token_tree . [(string_literal) (raw_string_literal)] @path))) @route
  .
  (function_item
    name: (identifier) @handler)
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./"))

; Macro routes with intervening attribute: #[post("/path")] #[tracing::instrument] async fn handler(...)
(source_file
  (attribute_item
    (attribute
      [(identifier) @method (scoped_identifier name: (identifier) @method)]
      arguments: (token_tree . [(string_literal) (raw_string_literal)] @path))) @route
  .
  (attribute_item)
  .
  (function_item
    name: (identifier) @handler)
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./"))

(declaration_list
  (attribute_item
    (attribute
      [(identifier) @method (scoped_identifier name: (identifier) @method)]
      arguments: (token_tree . [(string_literal) (raw_string_literal)] @path))) @route
  .
  (attribute_item)
  .
  (function_item
    name: (identifier) @handler)
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./"))

; Chained .route("/path", web::get().to(handler))
(call_expression
  function: (field_expression
    field: (field_identifier) @route)
  arguments: (arguments
    . [(string_literal) (raw_string_literal)] @path
    . (call_expression
        function: (field_expression
          value: (call_expression
            function: [(scoped_identifier name: (identifier) @method) (identifier) @method])
          field: (field_identifier) @to_fn)
        arguments: (arguments
          [(identifier) (scoped_identifier)] @handler)))
  (#eq? @route "route")
  (#eq? @to_fn "to")
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./"))

; Scoped routes: web::scope("/prefix").route("/path", web::post().to(handler))
(call_expression
  function: (field_expression
    value: (call_expression
      function: [(scoped_identifier name: (identifier) @scope_fn) (identifier) @scope_fn]
      arguments: (arguments . [(string_literal) (raw_string_literal)] @prefix))
    field: (field_identifier) @route)
  arguments: (arguments
    . [(string_literal) (raw_string_literal)] @path
    . (call_expression
        function: (field_expression
          value: (call_expression
            function: [(scoped_identifier name: (identifier) @method) (identifier) @method])
          field: (field_identifier) @to_fn)
        arguments: (arguments
          [(identifier) (scoped_identifier)] @handler)))
  (#eq? @scope_fn "scope")
  (#eq? @route "route")
  (#eq? @to_fn "to")
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./")
  (#set! priority "1"))

; Scoped routes (second in chain): web::scope("/prefix").route(...).route("/path", web::post().to(handler))
(call_expression
  function: (field_expression
    value: (call_expression
      function: (field_expression
        value: (call_expression
          function: [(scoped_identifier name: (identifier) @scope_fn) (identifier) @scope_fn]
          arguments: (arguments . [(string_literal) (raw_string_literal)] @prefix))
        field: (field_identifier) @prev_route_fn))
    field: (field_identifier) @route)
  arguments: (arguments
    . [(string_literal) (raw_string_literal)] @path
    . (call_expression
        function: (field_expression
          value: (call_expression
            function: [(scoped_identifier name: (identifier) @method) (identifier) @method])
          field: (field_identifier) @to_fn)
        arguments: (arguments
          [(identifier) (scoped_identifier)] @handler)))
  (#eq? @scope_fn "scope")
  (#eq? @prev_route_fn "route")
  (#eq? @route "route")
  (#eq? @to_fn "to")
  (#match? @method "^(get|post|put|patch|delete|head)$")
  (#match? @path "^./")
  (#set! priority "1"))
