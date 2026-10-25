; NestJS GraphQL resolvers: docs.nestjs.com/graphql/resolvers

; NestJS: @Query(() => User) user() { … }, renamed by { name: "…" }.
(class_body
  (decorator
    (call_expression
      function: (identifier) @root
      arguments: (arguments) @args)) @resolver
  .
  (method_definition name: (property_identifier) @field)
  (#match? @root "^(Query|Mutation|Subscription)$"))
