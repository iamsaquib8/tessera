; TypeORM entities: @Entity("users"), @Entity({ name: "users" }) or @Entity().
; Docs: typeorm.io/entities

(class_declaration
  decorator: (decorator
    (call_expression
      function: (identifier) @decorator
      arguments: (arguments) @table_args))
  name: (type_identifier) @name
  (#eq? @decorator "Entity")) @entity

(export_statement
  decorator: (decorator
    (call_expression
      function: (identifier) @decorator
      arguments: (arguments) @table_args))
  declaration: (class_declaration name: (type_identifier) @name)
  (#eq? @decorator "Entity")) @entity
