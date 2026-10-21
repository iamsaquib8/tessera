; NestJS controllers: @Controller(prefix) on the class, @Get(path) and
; friends on its methods. Docs: docs.nestjs.com/controllers

(class_declaration
  decorator: (decorator
    (call_expression
      function: (identifier) @controller
      arguments: (arguments) @prefix_args))
  body: (class_body
    (decorator
      (call_expression
        function: (identifier) @method
        arguments: (arguments) @args) @route))
  (#eq? @controller "Controller")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Head|Options|All)$"))

(export_statement
  decorator: (decorator
    (call_expression
      function: (identifier) @controller
      arguments: (arguments) @prefix_args))
  declaration: (class_declaration
    body: (class_body
      (decorator
        (call_expression
          function: (identifier) @method
          arguments: (arguments) @args) @route)))
  (#eq? @controller "Controller")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Head|Options|All)$"))
