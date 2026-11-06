; Slim framework routes: $app->get(path, handler), $group->post(...), etc.
; Docs: slimframework.com/docs/v4/objects/routing.html

; Grouped routes with prefix: $app->group('/api', function (RouteCollectorProxy $group) { ... })
(member_call_expression
  name: (name) @group_fn
  arguments: (arguments
    (argument [(string) (encapsed_string)] @prefix)
    (argument
      (anonymous_function
        body: (compound_statement
          (expression_statement
            (member_call_expression
              name: (name) @method
              arguments: (arguments
                (argument [(string) (encapsed_string)] @path)
                (argument
                  [(string (string_content) @handler)
                   (name) @handler
                   (array_creation_expression
                     (array_element_initializer)
                     (array_element_initializer (string (string_content) @handler)))
                   (_)]))) @route)))))
  (#eq? @group_fn "group")
  (#match? @method "^(get|post|put|patch|delete|options|any)$")
  (#set! priority "1"))

; Grouped routes with arrow function: $app->group('/api', fn ($group) => $group->get(...))
(member_call_expression
  name: (name) @group_fn
  arguments: (arguments
    (argument [(string) (encapsed_string)] @prefix)
    (argument
      (arrow_function
        body: (member_call_expression
          name: (name) @method
          arguments: (arguments
            (argument [(string) (encapsed_string)] @path)
            (argument
              [(string (string_content) @handler)
               (name) @handler
               (array_creation_expression
                 (array_element_initializer)
                 (array_element_initializer (string (string_content) @handler)))
               (_)]))) @route)))
  (#eq? @group_fn "group")
  (#match? @method "^(get|post|put|patch|delete|options|any)$")
  (#set! priority "1"))

; Top-level routes: $app->get('/users/{id}', handler)
(member_call_expression
  object: [(variable_name (name) @object)
           (member_call_expression name: (name) @object)
           (member_access_expression name: (name) @object)]
  name: (name) @method
  arguments: (arguments
    (argument [(string) (encapsed_string)] @path)
    (argument
      [(string (string_content) @handler)
       (name) @handler
       (array_creation_expression
         (array_element_initializer)
         (array_element_initializer (string (string_content) @handler)))
       (_)]))
  (#match? @object "^(app|router|collector|routeCollector|api|server|routes?)$")
  (#match? @method "^(get|post|put|patch|delete|options|any)$")) @route
