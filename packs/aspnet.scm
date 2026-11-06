; ASP.NET Core minimal API routes: app.MapGet(path, handler), MapPost, etc.
; Docs: learn.microsoft.com/aspnet/core/fundamentals/minimal-apis

; Chained MapGroup: app.MapGroup("/api").MapDelete("/orders/{id}", DeleteOrder)
(invocation_expression
  function: (member_access_expression
    expression: (invocation_expression
      function: (member_access_expression
        name: (identifier) @group_fn)
      arguments: (argument_list (argument [(string_literal) (raw_string_literal)] @prefix)))
    name: (identifier) @method)
  arguments: (argument_list
    (argument [(string_literal) (raw_string_literal)] @path)
    . (argument [(identifier) @handler
                 (member_access_expression name: (identifier) @handler)
                 (lambda_expression)
                 (_)]))
  (#eq? @group_fn "MapGroup")
  (#match? @method "^(MapGet|MapPost|MapPut|MapPatch|MapDelete)$")
  (#set! priority "1")) @route

; Group variable in top-level statements: var api = app.MapGroup("/api"); api.MapDelete(...)
(compilation_unit
  (global_statement
    (local_declaration_statement
      (variable_declaration
        (variable_declarator
          name: (identifier) @group
          (invocation_expression
            function: (member_access_expression name: (identifier) @group_fn)
            arguments: (argument_list (argument [(string_literal) (raw_string_literal)] @prefix)))))))
  (global_statement
    (expression_statement
      (invocation_expression
        function: (member_access_expression
          expression: (identifier) @object
          name: (identifier) @method)
        arguments: (argument_list
          (argument [(string_literal) (raw_string_literal)] @path)
          . (argument [(identifier) @handler
                       (member_access_expression name: (identifier) @handler)
                       (lambda_expression)
                       (_)]))) @route))
  (#eq? @group @object)
  (#eq? @group_fn "MapGroup")
  (#match? @method "^(MapGet|MapPost|MapPut|MapPatch|MapDelete)$")
  (#set! priority "1"))

; Group variable in a block / method body: var api = app.MapGroup("/api"); api.MapDelete(...)
(block
  (local_declaration_statement
    (variable_declaration
      (variable_declarator
        name: (identifier) @group
        (invocation_expression
          function: (member_access_expression name: (identifier) @group_fn)
          arguments: (argument_list (argument [(string_literal) (raw_string_literal)] @prefix))))))
  (expression_statement
    (invocation_expression
      function: (member_access_expression
        expression: (identifier) @object
        name: (identifier) @method)
      arguments: (argument_list
        (argument [(string_literal) (raw_string_literal)] @path)
        . (argument [(identifier) @handler
                     (member_access_expression name: (identifier) @handler)
                     (lambda_expression)
                     (_)]))) @route)
  (#eq? @group @object)
  (#eq? @group_fn "MapGroup")
  (#match? @method "^(MapGet|MapPost|MapPut|MapPatch|MapDelete)$")
  (#set! priority "1"))

; Top-level minimal API routes: app.MapGet("/users/{id}", ...)
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @object
    name: (identifier) @method)
  arguments: (argument_list
    (argument [(string_literal) (raw_string_literal)] @path)
    . (argument [(identifier) @handler
                 (member_access_expression name: (identifier) @handler)
                 (lambda_expression)
                 (_)]))
  (#match? @object "^(app|builder|routes?|endpoints?|server)$")
  (#match? @method "^(MapGet|MapPost|MapPut|MapPatch|MapDelete)$")) @route
